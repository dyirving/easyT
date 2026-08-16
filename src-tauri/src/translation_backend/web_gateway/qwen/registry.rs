use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::account::DisplayName;
use super::account::{
    display_status, AccountId, AccountMoveDirection, PersistedAccount, PersistedHealth,
    PersistedLogin, QwenAccountPoolSnapshot, QwenAccountPoolWarning, QwenAccountSnapshot,
    MAXIMUM_ACCOUNTS,
};
use super::error::QwenError;

const REGISTRY_FILE_NAME: &str = "accounts.json";
const REGISTRY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    schema_version: u32,
    accounts: Vec<PersistedAccount>,
}

#[derive(Debug)]
pub struct AccountRegistry {
    qwen_root: PathBuf,
    accounts: Vec<PersistedAccount>,
    warning: Option<QwenAccountPoolWarning>,
}

impl AccountRegistry {
    pub fn open(qwen_root: &Path) -> Result<Self, QwenError> {
        let path = registry_path(qwen_root);
        let contents = fs::read_to_string(&path).map_err(QwenError::storage_read)?;
        let parsed: RegistryFile =
            serde_json::from_str(&contents).map_err(QwenError::storage_read)?;
        validate_registry(&parsed)?;
        Ok(Self {
            qwen_root: qwen_root.to_path_buf(),
            accounts: parsed.accounts,
            warning: None,
        })
    }

    #[cfg(test)]
    pub fn create_empty(qwen_root: &Path) -> Result<Self, QwenError> {
        let registry = Self {
            qwen_root: qwen_root.to_path_buf(),
            accounts: Vec::new(),
            warning: None,
        };
        registry.write_accounts(&registry.accounts)?;
        Ok(registry)
    }

    pub fn open_or_recover(qwen_root: &Path) -> Result<Self, QwenError> {
        let path = registry_path(qwen_root);
        if !path.exists() {
            return Ok(Self {
                qwen_root: qwen_root.to_path_buf(),
                accounts: Vec::new(),
                warning: None,
            });
        }
        match Self::open(qwen_root) {
            Ok(registry) => Ok(registry),
            Err(error) if error.is_recoverable_registry_corruption() => {
                Self::recover_corrupt_registry(qwen_root)
            }
            Err(error) => Err(error),
        }
    }

    pub fn accounts(&self) -> &[PersistedAccount] {
        &self.accounts
    }

    #[cfg(test)]
    pub fn warning(&self) -> Option<&QwenAccountPoolWarning> {
        self.warning.as_ref()
    }

    pub fn account_dir(&self, id: &AccountId) -> PathBuf {
        self.qwen_root.join("accounts").join(id.as_str())
    }

    pub fn qwen_root(&self) -> &Path {
        &self.qwen_root
    }

    pub fn snapshot(&self) -> QwenAccountPoolSnapshot {
        QwenAccountPoolSnapshot {
            accounts: self
                .accounts
                .iter()
                .enumerate()
                .map(|(order, account)| QwenAccountSnapshot {
                    account_id: account.account_id.clone(),
                    display_name: account.display_name.clone(),
                    enabled: account.enabled,
                    order,
                    status: display_status(account),
                    cooldown_remaining_seconds: None,
                    message: None,
                    message_code: None,
                    actions: super::account::QwenAccountActions {
                        can_rename: true,
                        can_toggle_enabled: true,
                        can_move_up: order > 0,
                        can_move_down: order + 1 < self.accounts.len(),
                        can_login: true,
                        can_logout: account.login_state != PersistedLogin::LoggedOut,
                        can_test: false,
                        can_delete: true,
                    },
                })
                .collect(),
            maximum_accounts: MAXIMUM_ACCOUNTS,
            login_account_id: None,
            warning: self.warning.clone(),
        }
    }

    #[cfg(test)]
    pub fn create_account(&mut self, display_name: &str) -> Result<PersistedAccount, QwenError> {
        if self.accounts.len() >= MAXIMUM_ACCOUNTS {
            return Err(QwenError::pool_limit());
        }
        let account = PersistedAccount::new(DisplayName::parse(display_name)?);
        let mut next = self.accounts.clone();
        next.push(account.clone());
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(account)
    }

    pub fn insert_account(&mut self, account: PersistedAccount) -> Result<(), QwenError> {
        account.validate()?;
        if self.accounts.len() >= MAXIMUM_ACCOUNTS {
            return Err(QwenError::pool_limit());
        }
        let mut next = self.accounts.clone();
        next.push(account);
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    pub fn restore_account_at(
        &mut self,
        index: usize,
        account: PersistedAccount,
    ) -> Result<(), QwenError> {
        if self.accounts.len() >= MAXIMUM_ACCOUNTS || index > self.accounts.len() {
            return Err(QwenError::storage_cleanup(
                "cannot restore account registry entry",
            ));
        }
        account.validate()?;
        let mut next = self.accounts.clone();
        next.insert(index, account);
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    pub fn set_login_state(
        &mut self,
        id: &AccountId,
        login_state: PersistedLogin,
        last_health: PersistedHealth,
    ) -> Result<(), QwenError> {
        let Some(index) = self
            .accounts
            .iter()
            .position(|account| &account.account_id == id)
        else {
            return Err(QwenError::account_not_found());
        };
        let mut next = self.accounts.clone();
        next[index].login_state = login_state;
        next[index].last_health = last_health;
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    pub fn set_health(&mut self, id: &AccountId, health: PersistedHealth) -> Result<(), QwenError> {
        let index = self.account_index(id)?;
        let mut next = self.accounts.clone();
        next[index].last_health = health;
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    pub fn rename_account(&mut self, id: &AccountId, display_name: &str) -> Result<(), QwenError> {
        let display_name = super::account::DisplayName::parse(display_name)?.into_inner();
        let index = self.account_index(id)?;
        let mut next = self.accounts.clone();
        next[index].display_name = display_name;
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &AccountId, enabled: bool) -> Result<(), QwenError> {
        let index = self.account_index(id)?;
        let mut next = self.accounts.clone();
        next[index].enabled = enabled;
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    pub fn move_account(
        &mut self,
        id: &AccountId,
        direction: AccountMoveDirection,
    ) -> Result<(), QwenError> {
        let index = self.account_index(id)?;
        let target = match direction {
            AccountMoveDirection::Up if index > 0 => index - 1,
            AccountMoveDirection::Down if index + 1 < self.accounts.len() => index + 1,
            _ => return Err(QwenError::invalid_account_order()),
        };
        let mut next = self.accounts.clone();
        next.swap(index, target);
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    pub fn remove_account(&mut self, id: &AccountId) -> Result<(), QwenError> {
        let index = self.account_index(id)?;
        let mut next = self.accounts.clone();
        next.remove(index);
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    fn account_index(&self, id: &AccountId) -> Result<usize, QwenError> {
        self.accounts
            .iter()
            .position(|account| &account.account_id == id)
            .ok_or_else(QwenError::account_not_found)
    }

    pub fn add_migrated_default(&mut self, id: AccountId) -> Result<(), QwenError> {
        let account_dir = self.account_dir(&id);
        let has_ticket = account_dir.join("credentials.bin").is_file();
        let default_account = PersistedAccount {
            account_id: id.clone(),
            display_name: "默认账号".to_string(),
            enabled: true,
            login_state: if has_ticket {
                PersistedLogin::Ready
            } else {
                PersistedLogin::LoggedOut
            },
            last_health: if has_ticket {
                PersistedHealth::Healthy
            } else {
                PersistedHealth::Unknown
            },
        };
        if let Some(index) = self
            .accounts
            .iter()
            .position(|account| account.account_id == id)
        {
            if self.accounts[index] == default_account {
                return Ok(());
            }
            let mut next = self.accounts.clone();
            next[index] = default_account;
            self.write_accounts(&next)?;
            self.accounts = next;
            return Ok(());
        }
        if self.accounts.len() >= MAXIMUM_ACCOUNTS {
            return Err(QwenError::storage_migration("account pool is full"));
        }
        let mut next = self.accounts.clone();
        next.push(default_account);
        self.write_accounts(&next)?;
        self.accounts = next;
        Ok(())
    }

    fn recover_corrupt_registry(qwen_root: &Path) -> Result<Self, QwenError> {
        let path = registry_path(qwen_root);
        let quarantine = next_quarantine_path(qwen_root)?;
        fs::rename(&path, &quarantine).map_err(|_| QwenError::storage_recovery_failed())?;

        let mut accounts = Vec::new();
        let accounts_root = qwen_root.join("accounts");
        if accounts_root.exists() {
            for entry in
                fs::read_dir(&accounts_root).map_err(|_| QwenError::storage_recovery_failed())?
            {
                let entry = entry.map_err(|_| QwenError::storage_recovery_failed())?;
                if !entry
                    .file_type()
                    .map_err(|_| QwenError::storage_recovery_failed())?
                    .is_dir()
                {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let Ok(account_id) = AccountId::parse(name) else {
                    continue;
                };
                let has_ticket = entry.path().join("credentials.bin").is_file();
                accounts.push(PersistedAccount {
                    account_id,
                    display_name: format!("Recovered account {}", accounts.len() + 1),
                    enabled: false,
                    login_state: if has_ticket {
                        PersistedLogin::Ready
                    } else {
                        PersistedLogin::LoggedOut
                    },
                    last_health: PersistedHealth::Unhealthy,
                });
            }
        }
        if accounts.len() > MAXIMUM_ACCOUNTS {
            return Err(QwenError::storage_recovery_failed());
        }
        let registry = Self {
            qwen_root: qwen_root.to_path_buf(),
            accounts,
            warning: Some(recovery_warning()),
        };
        registry.write_accounts(&registry.accounts)?;
        Ok(registry)
    }

    fn write_accounts(&self, accounts: &[PersistedAccount]) -> Result<(), QwenError> {
        let file = RegistryFile {
            schema_version: REGISTRY_SCHEMA_VERSION,
            accounts: accounts.to_vec(),
        };
        validate_registry(&file)?;
        let encoded = serde_json::to_vec_pretty(&file).map_err(QwenError::storage_write)?;
        write_atomic(&registry_path(&self.qwen_root), &encoded)
    }
}

fn recovery_warning() -> QwenAccountPoolWarning {
    let error = QwenError::storage_corrupted_recovered();
    QwenAccountPoolWarning {
        code: error.code().as_str().to_string(),
        message: error.safe_message().to_string(),
    }
}

fn validate_registry(registry: &RegistryFile) -> Result<(), QwenError> {
    if registry.schema_version != REGISTRY_SCHEMA_VERSION
        || registry.accounts.len() > MAXIMUM_ACCOUNTS
    {
        return Err(QwenError::storage_incompatible());
    }
    let mut ids = HashSet::new();
    for account in &registry.accounts {
        account
            .validate()
            .map_err(|_| QwenError::storage_read("invalid account metadata"))?;
        if !ids.insert(account.account_id.clone()) {
            return Err(QwenError::storage_read("duplicate account id"));
        }
    }
    Ok(())
}

pub fn registry_path(qwen_root: &Path) -> PathBuf {
    qwen_root.join(REGISTRY_FILE_NAME)
}

fn next_quarantine_path(qwen_root: &Path) -> Result<PathBuf, QwenError> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| QwenError::storage_recovery_failed())?
        .as_secs();
    for suffix in 0..1000 {
        let path = qwen_root.join(format!("accounts.corrupt.{timestamp}.{suffix}.json"));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err(QwenError::storage_recovery_failed())
}

pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> Result<(), QwenError> {
    let parent = path
        .parent()
        .ok_or_else(|| QwenError::storage_write("missing parent"))?;
    fs::create_dir_all(parent).map_err(QwenError::storage_write)?;
    let tmp = path.with_extension("json.tmp");
    let result = (|| {
        let mut file = fs::File::create(&tmp).map_err(QwenError::storage_write)?;
        file.write_all(data).map_err(QwenError::storage_write)?;
        file.sync_all().map_err(QwenError::storage_write)?;
        replace_file(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(windows)]
fn replace_file(tmp: &Path, target: &Path) -> Result<(), QwenError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let tmp_wide: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
    let target_wide: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(
            tmp_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(QwenError::storage_write("atomic replace failed"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(tmp: &Path, target: &Path) -> Result<(), QwenError> {
    fs::rename(tmp, target).map_err(QwenError::storage_write)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation_backend::web_gateway::qwen::test_support;

    #[test]
    fn registry_round_trips_with_atomic_replace() {
        let root = test_support::TestDir::new("registry-round-trip");
        let mut registry = AccountRegistry::create_empty(root.path()).unwrap();
        let account = registry.create_account("Default").unwrap();
        let loaded = AccountRegistry::open(root.path()).unwrap();

        assert_eq!(loaded.accounts(), &[account]);
        assert!(!root.path().join("accounts.json.tmp").exists());
    }

    #[test]
    fn registry_rejects_an_eleventh_account() {
        let root = test_support::TestDir::new("registry-limit");
        let mut registry = AccountRegistry::create_empty(root.path()).unwrap();
        for index in 0..MAXIMUM_ACCOUNTS {
            registry
                .create_account(&format!("Account {index}"))
                .unwrap();
        }

        let error = registry.create_account("Eleven").unwrap_err();
        assert_eq!(error.code().as_str(), "QW-POOL-002");
    }

    #[test]
    fn unknown_schema_is_not_overwritten() {
        let root = test_support::TestDir::new("registry-unknown-schema");
        std::fs::create_dir_all(root.path()).unwrap();
        let registry_path = root.path().join("accounts.json");
        let original = r#"{"schemaVersion":99,"accounts":[]}"#;
        std::fs::write(&registry_path, original).unwrap();

        let error = AccountRegistry::open_or_recover(root.path()).unwrap_err();
        assert_eq!(error.code().as_str(), "QW-STORAGE-004");
        assert_eq!(std::fs::read_to_string(registry_path).unwrap(), original);
    }

    #[test]
    fn corrupt_registry_is_quarantined_and_recovered_accounts_are_disabled() {
        let root = test_support::TestDir::new("registry-corrupt");
        let id = AccountId::new();
        let account_dir = root.path().join("accounts").join(id.as_str());
        std::fs::create_dir_all(&account_dir).unwrap();
        std::fs::write(root.path().join("accounts.json"), "not json").unwrap();

        let registry = AccountRegistry::open_or_recover(root.path()).unwrap();
        assert_eq!(registry.warning().unwrap().code, "QW-STORAGE-002");
        assert_eq!(registry.accounts().len(), 1);
        assert_eq!(registry.accounts()[0].account_id, id);
        assert!(!registry.accounts()[0].enabled);
        assert_eq!(
            registry.accounts()[0].last_health,
            PersistedHealth::Unhealthy
        );
        assert!(root.path().join("accounts.json").exists());
        assert!(std::fs::read_dir(root.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("accounts.corrupt.")
        }));
    }

    #[test]
    fn invalid_account_metadata_is_recovered_without_deleting_account_directories() {
        let root = test_support::TestDir::new("registry-invalid-account");
        let id = AccountId::new();
        let account_dir = root.path().join("accounts").join(id.as_str());
        std::fs::create_dir_all(&account_dir).unwrap();
        std::fs::write(
            root.path().join("accounts.json"),
            r#"{"schemaVersion":1,"accounts":[{"accountId":"not-a-uuid","displayName":"x","enabled":true,"loginState":"ready","lastHealth":"healthy"}]}"#,
        )
        .unwrap();

        let registry = AccountRegistry::open_or_recover(root.path()).unwrap();
        assert_eq!(registry.accounts().len(), 1);
        assert_eq!(registry.accounts()[0].account_id, id);
        assert!(!registry.accounts()[0].enabled);
        assert!(account_dir.exists());
    }
}
