//! Qwen 网页凭证明文存储。
//!
//! 持久化位置：
//! `easyT_Data/web_gateway/qwen/credentials.bin`
//!
//! 用户已明确选择明文持久化。该文件不写入 `config.json`，日志也不得输出其内容。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use zeroize::Zeroize;

use crate::translation_backend::error::BackendError;

const MAX_TICKET_BYTES: usize = 64 * 1024;

pub fn credentials_path(app_data: &Path) -> PathBuf {
    app_data
        .join("web_gateway")
        .join("qwen")
        .join("credentials.bin")
}

pub fn account_credentials_path(account_dir: &Path) -> PathBuf {
    account_dir.join("credentials.bin")
}

pub fn qwen_profile_path(app_data: &Path) -> PathBuf {
    app_data.join("web_gateway").join("qwen").join("profile")
}

pub fn account_profile_path(account_dir: &Path) -> PathBuf {
    account_dir.join("profile")
}

/// 将 ticket 直接以 UTF-8 明文写入凭证文件。
pub fn save_ticket(app_data: &Path, ticket: &str) -> Result<(), BackendError> {
    save_ticket_at(&credentials_path(app_data), ticket)
}

pub fn save_ticket_at(path: &Path, ticket: &str) -> Result<(), BackendError> {
    if ticket.is_empty() || ticket.len() > MAX_TICKET_BYTES {
        return Err(BackendError::CredentialCorrupted);
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| BackendError::Internal(format!("创建凭证目录失败: {e}")))?;
    }
    write_atomic(path, ticket.as_bytes())?;
    log::info!("Qwen 明文凭证已保存: bytes={}", ticket.len());
    Ok(())
}

/// Writes a replacement ticket beside the active credential without changing it.
/// The caller can discard this staging file if the accompanying registry update fails.
pub fn stage_ticket(path: &Path, ticket: &str) -> Result<PathBuf, BackendError> {
    if ticket.is_empty() || ticket.len() > MAX_TICKET_BYTES {
        return Err(BackendError::CredentialCorrupted);
    }
    let staged = path.with_extension("bin.login.tmp");
    if let Some(parent) = staged.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| BackendError::Internal(format!("创建凭证目录失败: {e}")))?;
    }
    let mut file = fs::File::create(&staged)
        .map_err(|e| BackendError::Internal(format!("创建凭证临时文件失败: {e}")))?;
    file.write_all(ticket.as_bytes())
        .map_err(|e| BackendError::Internal(format!("写入凭证失败: {e}")))?;
    file.sync_all()
        .map_err(|e| BackendError::Internal(format!("刷新凭证到磁盘失败: {e}")))?;
    Ok(staged)
}

pub fn commit_staged_ticket(staged: &Path, path: &Path) -> Result<(), BackendError> {
    replace_file(staged, path)
}

pub fn discard_staged_ticket(staged: &Path) -> Result<(), BackendError> {
    if staged.exists() {
        fs::remove_file(staged)
            .map_err(|e| BackendError::Internal(format!("清理凭证临时文件失败: {e}")))?;
    }
    Ok(())
}

pub fn load_ticket(app_data: &Path) -> Result<Option<TicketSecret>, BackendError> {
    let path = credentials_path(app_data);
    load_ticket_at(&path)
}

pub fn load_ticket_at(path: &Path) -> Result<Option<TicketSecret>, BackendError> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path).map_err(|e| BackendError::Internal(format!("读取凭证失败: {e}")))?;
    if bytes.is_empty() || bytes.len() > MAX_TICKET_BYTES {
        return Err(BackendError::CredentialCorrupted);
    }
    let ticket = String::from_utf8(bytes).map_err(|_| BackendError::CredentialCorrupted)?;
    Ok(Some(TicketSecret::new(ticket)))
}

pub fn delete_ticket(app_data: &Path) -> Result<(), BackendError> {
    delete_ticket_at(&credentials_path(app_data))
}

pub fn delete_ticket_at(path: &Path) -> Result<(), BackendError> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| BackendError::Internal(format!("删除凭证失败: {e}")))?;
    }
    Ok(())
}

pub fn delete_qwen_profile(app_data: &Path) -> Result<(), BackendError> {
    let profile = qwen_profile_path(app_data);
    if profile.exists() {
        fs::remove_dir_all(&profile)
            .map_err(|e| BackendError::Internal(format!("删除 Qwen profile 失败: {e}")))?;
    }
    Ok(())
}

pub fn delete_qwen_profile_at(account_dir: &Path) -> Result<(), BackendError> {
    let profile = account_profile_path(account_dir);
    if profile.exists() {
        fs::remove_dir_all(&profile)
            .map_err(|e| BackendError::Internal(format!("删除 Qwen profile 失败: {e}")))?;
    }
    Ok(())
}

/// 请求期间持有 ticket，释放时清理内存副本。
pub struct TicketSecret {
    inner: String,
}

impl TicketSecret {
    pub fn new(value: String) -> Self {
        Self { inner: value }
    }

    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl Drop for TicketSecret {
    fn drop(&mut self) {
        // SAFETY: 只覆盖 String 已初始化的字节，不改变长度或容量。
        unsafe { self.inner.as_bytes_mut() }.zeroize();
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), BackendError> {
    let tmp = path.with_extension("bin.tmp");
    let write_result = (|| {
        let mut file = fs::File::create(&tmp)
            .map_err(|e| BackendError::Internal(format!("创建凭证临时文件失败: {e}")))?;
        file.write_all(data)
            .map_err(|e| BackendError::Internal(format!("写入凭证失败: {e}")))?;
        file.sync_all()
            .map_err(|e| BackendError::Internal(format!("刷新凭证到磁盘失败: {e}")))?;
        replace_file(&tmp, path)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    write_result
}

#[cfg(windows)]
fn replace_file(tmp: &Path, target: &Path) -> Result<(), BackendError> {
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
        return Err(BackendError::Internal(format!(
            "提交凭证文件失败: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(tmp: &Path, target: &Path) -> Result<(), BackendError> {
    fs::rename(tmp, target).map_err(|e| BackendError::Internal(format!("提交凭证文件失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let counter = TEST_DIR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "easyt-credential-test-{}-{}-{}",
            std::process::id(),
            counter,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn plaintext_round_trip() {
        let dir = temp_dir();
        let ticket = "tongyi_test_ticket_value";
        save_ticket(&dir, ticket).expect("save");
        assert_eq!(fs::read_to_string(credentials_path(&dir)).unwrap(), ticket);
        let loaded = load_ticket(&dir).unwrap().unwrap();
        assert_eq!(loaded.as_str(), ticket);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_again_replaces_existing_credentials() {
        let dir = temp_dir();
        save_ticket(&dir, "old-ticket").expect("save old");
        save_ticket(&dir, "new-ticket").expect("replace old");
        let loaded = load_ticket(&dir).unwrap().unwrap();
        assert_eq!(loaded.as_str(), "new-ticket");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_plaintext_is_preserved_and_rejected() {
        let dir = temp_dir();
        let path = credentials_path(&dir);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(matches!(
            load_ticket(&dir),
            Err(BackendError::CredentialCorrupted)
        ));
        assert!(path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_ticket_is_idempotent() {
        let dir = temp_dir();
        delete_ticket(&dir).expect("delete missing");
        let _ = fs::remove_dir_all(&dir);
    }
}
