use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::account::AccountId;
use super::error::QwenError;
use super::registry::{write_atomic, AccountRegistry};

const JOURNAL_FILE_NAME: &str = "legacy-migration.json";
const JOURNAL_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MigrationPhase {
    Prepared,
    CredentialsStaged,
    ProfileStaged,
    DirectoryPublished,
    RegistryCommitted,
}

impl MigrationPhase {
    #[cfg(test)]
    const ALL: [Self; 5] = [
        Self::Prepared,
        Self::CredentialsStaged,
        Self::ProfileStaged,
        Self::DirectoryPublished,
        Self::RegistryCommitted,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyMigrationJournal {
    schema_version: u32,
    account_id: AccountId,
    source_credential_path: String,
    source_profile_path: String,
    staging_path: String,
    target_path: String,
    phase: MigrationPhase,
}

pub fn reconcile_legacy_migration(qwen_root: &Path) -> Result<(), QwenError> {
    fs::create_dir_all(qwen_root).map_err(QwenError::storage_migration)?;
    let journal = if journal_path(qwen_root).exists() {
        read_journal(qwen_root)?
    } else {
        if !legacy_credentials_path(qwen_root).exists() && !legacy_profile_path(qwen_root).exists()
        {
            return Ok(());
        }
        let journal = new_journal(qwen_root, AccountId::new());
        write_journal(qwen_root, &journal)?;
        journal
    };

    complete_journal(qwen_root, journal)
}

fn complete_journal(
    qwen_root: &Path,
    mut journal: LegacyMigrationJournal,
) -> Result<(), QwenError> {
    validate_journal_paths(qwen_root, &journal)?;
    let staging = staging_dir(qwen_root, &journal.account_id);
    let target = target_dir(qwen_root, &journal.account_id);
    fs::create_dir_all(&staging).map_err(QwenError::storage_migration)?;

    move_source_if_needed(
        &legacy_credentials_path(qwen_root),
        &staging.join("credentials.bin"),
    )?;
    if journal.phase == MigrationPhase::Prepared {
        journal.phase = MigrationPhase::CredentialsStaged;
        write_journal(qwen_root, &journal)?;
    }

    move_source_if_needed(&legacy_profile_path(qwen_root), &staging.join("profile"))?;
    if matches!(
        journal.phase,
        MigrationPhase::Prepared | MigrationPhase::CredentialsStaged
    ) {
        journal.phase = MigrationPhase::ProfileStaged;
        write_journal(qwen_root, &journal)?;
    }

    if target.exists() && staging.exists() && staging_has_data(&staging)? {
        return Err(QwenError::storage_migration(
            "both staging and target exist",
        ));
    }
    if !target.exists() {
        if !staging_has_data(&staging)? {
            return Err(QwenError::storage_migration("staging data is missing"));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(QwenError::storage_migration)?;
        }
        fs::rename(&staging, &target).map_err(QwenError::storage_migration)?;
    }
    if journal.phase != MigrationPhase::DirectoryPublished {
        journal.phase = MigrationPhase::DirectoryPublished;
        write_journal(qwen_root, &journal)?;
    }

    let mut registry = AccountRegistry::open_or_recover(qwen_root)
        .map_err(|_| QwenError::storage_migration("registry unavailable"))?;
    registry.add_migrated_default(journal.account_id.clone())?;
    if journal.phase != MigrationPhase::RegistryCommitted {
        journal.phase = MigrationPhase::RegistryCommitted;
        write_journal(qwen_root, &journal)?;
    }
    fs::remove_file(journal_path(qwen_root)).map_err(QwenError::storage_migration)
}

fn move_source_if_needed(source: &Path, staged: &Path) -> Result<(), QwenError> {
    if !source.exists() {
        return Ok(());
    }
    if staged.exists() {
        return Err(QwenError::storage_migration(
            "source and staged data both exist",
        ));
    }
    if let Some(parent) = staged.parent() {
        fs::create_dir_all(parent).map_err(QwenError::storage_migration)?;
    }
    fs::rename(source, staged).map_err(QwenError::storage_migration)
}

fn staging_has_data(staging: &Path) -> Result<bool, QwenError> {
    Ok(staging.join("credentials.bin").exists() || staging.join("profile").exists())
}

fn new_journal(qwen_root: &Path, account_id: AccountId) -> LegacyMigrationJournal {
    LegacyMigrationJournal {
        schema_version: JOURNAL_SCHEMA_VERSION,
        account_id: account_id.clone(),
        source_credential_path: legacy_credentials_path(qwen_root)
            .to_string_lossy()
            .into_owned(),
        source_profile_path: legacy_profile_path(qwen_root)
            .to_string_lossy()
            .into_owned(),
        staging_path: staging_dir(qwen_root, &account_id)
            .to_string_lossy()
            .into_owned(),
        target_path: target_dir(qwen_root, &account_id)
            .to_string_lossy()
            .into_owned(),
        phase: MigrationPhase::Prepared,
    }
}

fn read_journal(qwen_root: &Path) -> Result<LegacyMigrationJournal, QwenError> {
    let raw = fs::read_to_string(journal_path(qwen_root)).map_err(QwenError::storage_migration)?;
    let journal = serde_json::from_str(&raw).map_err(QwenError::storage_migration)?;
    Ok(journal)
}

fn write_journal(qwen_root: &Path, journal: &LegacyMigrationJournal) -> Result<(), QwenError> {
    let encoded = serde_json::to_vec_pretty(journal).map_err(QwenError::storage_migration)?;
    write_atomic(&journal_path(qwen_root), &encoded)
        .map_err(|_| QwenError::storage_migration("journal write failed"))
}

fn validate_journal_paths(
    qwen_root: &Path,
    journal: &LegacyMigrationJournal,
) -> Result<(), QwenError> {
    if journal.schema_version != JOURNAL_SCHEMA_VERSION
        || journal.source_credential_path != legacy_credentials_path(qwen_root).to_string_lossy()
        || journal.source_profile_path != legacy_profile_path(qwen_root).to_string_lossy()
        || journal.staging_path != staging_dir(qwen_root, &journal.account_id).to_string_lossy()
        || journal.target_path != target_dir(qwen_root, &journal.account_id).to_string_lossy()
    {
        return Err(QwenError::storage_migration("journal paths are invalid"));
    }
    Ok(())
}

pub fn journal_path(qwen_root: &Path) -> PathBuf {
    qwen_root.join(JOURNAL_FILE_NAME)
}

fn legacy_credentials_path(qwen_root: &Path) -> PathBuf {
    qwen_root.join("credentials.bin")
}

fn legacy_profile_path(qwen_root: &Path) -> PathBuf {
    qwen_root.join("profile")
}

fn staging_dir(qwen_root: &Path, id: &AccountId) -> PathBuf {
    qwen_root.join(format!(".migration-staging-{}", id.as_str()))
}

fn target_dir(qwen_root: &Path, id: &AccountId) -> PathBuf {
    qwen_root.join("accounts").join(id.as_str())
}

#[cfg(test)]
fn seed_interrupted_migration(qwen_root: &Path, id: AccountId, phase: MigrationPhase) {
    fs::create_dir_all(qwen_root).unwrap();
    let journal = new_journal(qwen_root, id.clone());
    let staging = staging_dir(qwen_root, &id);
    let target = target_dir(qwen_root, &id);
    fs::write(legacy_credentials_path(qwen_root), "fake-ticket").unwrap();
    fs::create_dir_all(legacy_profile_path(qwen_root)).unwrap();
    match phase {
        MigrationPhase::Prepared => {}
        MigrationPhase::CredentialsStaged => {
            fs::create_dir_all(&staging).unwrap();
            fs::rename(
                legacy_credentials_path(qwen_root),
                staging.join("credentials.bin"),
            )
            .unwrap();
        }
        MigrationPhase::ProfileStaged => {
            fs::create_dir_all(&staging).unwrap();
            fs::rename(
                legacy_credentials_path(qwen_root),
                staging.join("credentials.bin"),
            )
            .unwrap();
            fs::rename(legacy_profile_path(qwen_root), staging.join("profile")).unwrap();
        }
        MigrationPhase::DirectoryPublished | MigrationPhase::RegistryCommitted => {
            fs::create_dir_all(&staging).unwrap();
            fs::rename(
                legacy_credentials_path(qwen_root),
                staging.join("credentials.bin"),
            )
            .unwrap();
            fs::rename(legacy_profile_path(qwen_root), staging.join("profile")).unwrap();
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::rename(staging, &target).unwrap();
        }
    }
    let mut journal = journal;
    journal.phase = phase;
    if phase == MigrationPhase::RegistryCommitted {
        let mut registry = AccountRegistry::create_empty(qwen_root).unwrap();
        registry.add_migrated_default(id).unwrap();
    }
    write_journal(qwen_root, &journal).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation_backend::web_gateway::qwen::test_support;

    #[test]
    fn legacy_migration_reconciles_every_interruption_boundary() {
        for phase in MigrationPhase::ALL {
            let root = test_support::TestDir::new("migration-boundary");
            let id = AccountId::new();
            seed_interrupted_migration(root.path(), id.clone(), phase);

            reconcile_legacy_migration(root.path()).unwrap();
            let registry = AccountRegistry::open(root.path()).unwrap();
            assert_eq!(registry.accounts().len(), 1, "phase={phase:?}");
            assert_eq!(registry.accounts()[0].account_id, id, "phase={phase:?}");
            assert!(registry.account_dir(&id).exists(), "phase={phase:?}");
            assert!(!journal_path(root.path()).exists(), "phase={phase:?}");
        }
    }

    #[test]
    fn repeated_migration_startup_is_idempotent() {
        let root = test_support::TestDir::new("migration-repeat");
        std::fs::create_dir_all(root.path()).unwrap();
        std::fs::write(root.path().join("credentials.bin"), "fake-ticket").unwrap();
        std::fs::create_dir_all(root.path().join("profile")).unwrap();

        reconcile_legacy_migration(root.path()).unwrap();
        reconcile_legacy_migration(root.path()).unwrap();

        assert_eq!(
            AccountRegistry::open(root.path()).unwrap().accounts().len(),
            1
        );
    }

    #[test]
    fn legacy_data_migrates_before_a_corrupt_registry_is_recovered() {
        let root = test_support::TestDir::new("migration-corrupt-registry");
        std::fs::create_dir_all(root.path()).unwrap();
        std::fs::write(root.path().join("credentials.bin"), "fake-ticket").unwrap();
        std::fs::write(root.path().join("accounts.json"), "not json").unwrap();

        reconcile_legacy_migration(root.path()).unwrap();

        let registry = AccountRegistry::open(root.path()).unwrap();
        assert_eq!(registry.accounts().len(), 1);
        assert!(registry
            .account_dir(&registry.accounts()[0].account_id)
            .join("credentials.bin")
            .exists());
    }
}
