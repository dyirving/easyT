//! QwenSession 状态机
//!
//! 维护 Qwen 登录态：
//! - LoggedOut：本地无凭证或已显式注销
//! - LoggingIn：正在登录（已创建登录窗口，watcher 运行中）
//! - Ready：本地存在凭证（不保证实时有效；首次 401/403 时转 Expired）
//! - Expired：凭证曾被验证过但已失效，需要重新登录
//!
//! QwenSession 内部使用 std::sync::Mutex 保护短时内存状态。
//! 不得在持有 MutexGuard 时执行：
//! - 创建 WebView
//! - 读取 Cookie
//! - 文件 I/O
//! - await
//! - HTTP 请求
//!
//! 需要先在锁内完成状态判断和状态切换，然后释放锁，再执行外部操作。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::translation_backend::error::BackendError;

use super::account::{AccountId, PersistedLogin};
use crate::translation_backend::web_gateway::credential_store::{self, TicketSecret};

/// 登录态阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum QwenSessionPhase {
    /// 本地无凭证或已显式注销
    #[default]
    LoggedOut,
    /// 正在登录（已创建登录窗口，watcher 运行中）
    LoggingIn,
    /// 本地存在凭证（不保证实时有效）
    Ready,
    /// 凭证已被验证过但已失效（首次 401/403 时转 Expired）
    Expired,
}

/// QwenSession 当前状态快照（前端可观察）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QwenSessionStatus {
    pub phase: QwenSessionPhase,
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub updated_at: Option<u64>,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug)]
struct SessionState {
    phase: QwenSessionPhase,
    /// 进入 LoggingIn 前的状态，用于取消或失败时恢复。
    phase_before_login: Option<QwenSessionPhase>,
    message: Option<String>,
    message_code: Option<String>,
    updated_at: Option<u64>,
    /// 登录 watcher 是否应该继续运行
    watcher_should_run: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            phase: QwenSessionPhase::LoggedOut,
            phase_before_login: None,
            message: None,
            message_code: None,
            updated_at: Some(now_unix()),
            watcher_should_run: false,
        }
    }
}

/// QwenSession：管理登录态与凭证
///
/// 不可在锁内执行外部 I/O；锁内只做状态判断与切换
#[derive(Debug)]
pub struct QwenSession {
    state: Mutex<SessionState>,
    account_id: Option<AccountId>,
    account_dir: Option<PathBuf>,
}

impl QwenSession {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(SessionState::default()),
            account_id: None,
            account_dir: None,
        }
    }

    pub fn for_account(
        account_id: AccountId,
        account_dir: PathBuf,
        persisted_login: PersistedLogin,
    ) -> Self {
        Self {
            state: Mutex::new(SessionState {
                phase: match persisted_login {
                    PersistedLogin::LoggedOut => QwenSessionPhase::LoggedOut,
                    PersistedLogin::Ready => QwenSessionPhase::Ready,
                    PersistedLogin::Expired => QwenSessionPhase::Expired,
                },
                ..SessionState::default()
            }),
            account_id: Some(account_id),
            account_dir: Some(account_dir),
        }
    }

    pub fn account_id(&self) -> Option<&AccountId> {
        self.account_id.as_ref()
    }

    pub fn account_dir(&self) -> Option<&Path> {
        self.account_dir.as_deref()
    }

    /// 启动时从凭证文件恢复状态
    /// - 文件不存在：LoggedOut
    /// - 文件格式有效：Ready
    /// - 文件损坏：CredentialCorrupted（前端显示需要重新登录）
    pub fn restore_from_storage(&self, app_data: &std::path::Path) {
        let new_phase = match self.load_ticket(app_data) {
            Ok(Some(_)) => QwenSessionPhase::Ready,
            Ok(None) => QwenSessionPhase::LoggedOut,
            Err(BackendError::CredentialCorrupted) => {
                // 凭证损坏：标记为 LoggedOut，让用户重新登录
                log::warn!("Qwen 凭证文件已损坏，需要重新登录");
                QwenSessionPhase::LoggedOut
            }
            Err(e) => {
                log::warn!("恢复 Qwen 凭证失败: {:?}", e);
                QwenSessionPhase::LoggedOut
            }
        };
        let mut g = self.lock();
        g.phase = new_phase;
        g.phase_before_login = None;
        g.message = None;
        g.updated_at = Some(now_unix());
    }

    /// 返回当前状态快照（前端可见）
    pub fn status(&self) -> QwenSessionStatus {
        let g = self.lock();
        QwenSessionStatus {
            phase: g.phase,
            message: g.message.clone(),
            code: g.message_code.clone(),
            updated_at: g.updated_at,
        }
    }

    /// 尝试开始登录：仅当当前不是 LoggingIn 时返回 true
    ///
    /// 调用方在锁外创建 WebView 与启动 watcher
    pub fn try_begin_login(&self) -> bool {
        let mut g = self.lock();
        if g.phase == QwenSessionPhase::LoggingIn {
            return false;
        }
        g.phase_before_login = Some(g.phase);
        g.phase = QwenSessionPhase::LoggingIn;
        g.message = Some("正在登录...".to_string());
        g.message_code = None;
        g.updated_at = Some(now_unix());
        g.watcher_should_run = true;
        true
    }

    /// Login watchers must reject the ticket already stored before re-login.
    /// This avoids accepting a profile's stale Cookie as a new authentication.
    pub fn is_fresh_login_ticket(&self, ticket: &TicketSecret) -> bool {
        let current = self.load_ticket_for_account();
        match current {
            Ok(Some(existing)) => !constant_time_ticket_eq(existing.as_str(), ticket.as_str()),
            Ok(None) => true,
            Err(_) => true,
        }
    }

    /// Watcher 检查是否应继续运行
    pub fn watcher_should_run(&self) -> bool {
        self.lock().watcher_should_run
    }

    /// 取消 watcher（用户关闭登录窗口、超时、应用退出时调用）
    pub fn cancel_watcher(&self) {
        let mut g = self.lock();
        g.watcher_should_run = false;
    }

    /// 登录成功：保存凭证并切换为 Ready
    pub fn complete_login(
        &self,
        app_data: &std::path::Path,
        ticket: &str,
    ) -> Result<(), BackendError> {
        match &self.account_dir {
            Some(account_dir) => credential_store::save_ticket_at(
                &credential_store::account_credentials_path(account_dir),
                ticket,
            )?,
            None => credential_store::save_ticket(app_data, ticket)?,
        }
        let mut g = self.lock();
        g.phase = QwenSessionPhase::Ready;
        g.phase_before_login = None;
        g.message = None;
        g.message_code = None;
        g.updated_at = Some(now_unix());
        g.watcher_should_run = false;
        Ok(())
    }

    /// Finishes an already persisted account login without another file write.
    pub fn mark_login_complete(&self) {
        let mut g = self.lock();
        g.phase = QwenSessionPhase::Ready;
        g.phase_before_login = None;
        g.message = None;
        g.updated_at = Some(now_unix());
        g.watcher_should_run = false;
    }

    /// Updates only the in-memory session after the pool has durably committed logout.
    pub fn mark_logged_out(&self) {
        let mut g = self.lock();
        g.phase = QwenSessionPhase::LoggedOut;
        g.phase_before_login = None;
        g.message = None;
        g.message_code = None;
        g.updated_at = Some(now_unix());
        g.watcher_should_run = false;
    }

    /// 登录被用户取消：恢复到登录前状态
    ///
    /// - 旧凭证 Ready → 保持 Ready（用户原本就有可用凭证）
    /// - 旧凭证 Expired → 保持 Expired
    /// - 旧凭证 LoggedOut → 保持 LoggedOut
    pub fn cancel_login(&self) {
        self.fail_login("登录已取消");
    }

    /// 登录窗口创建、Cookie 读取或凭证保存失败。
    pub fn fail_login(&self, message: impl Into<String>) {
        let mut g = self.lock();
        g.watcher_should_run = false;
        if g.phase != QwenSessionPhase::LoggingIn {
            return;
        }
        g.phase = g
            .phase_before_login
            .take()
            .unwrap_or(QwenSessionPhase::LoggedOut);
        g.message = Some(message.into());
        g.message_code = None;
        g.updated_at = Some(now_unix());
    }

    pub fn fail_login_with_code(&self, message: impl Into<String>, code: &str) {
        let mut g = self.lock();
        g.watcher_should_run = false;
        if g.phase != QwenSessionPhase::LoggingIn {
            return;
        }
        g.phase = g
            .phase_before_login
            .take()
            .unwrap_or(QwenSessionPhase::LoggedOut);
        g.message = Some(message.into());
        g.message_code = Some(code.to_string());
        g.updated_at = Some(now_unix());
    }

    /// 设置为 Expired（首次收到 401/403 时调用）
    pub fn mark_expired(&self) {
        let mut g = self.lock();
        g.phase = QwenSessionPhase::Expired;
        g.phase_before_login = None;
        g.message = Some("登录状态已过期".to_string());
        g.message_code = None;
        g.updated_at = Some(now_unix());
    }

    /// 退出登录：删除凭证并切换为 LoggedOut
    pub fn logout(&self, app_data: &std::path::Path) -> Result<(), BackendError> {
        match &self.account_dir {
            Some(account_dir) => credential_store::delete_ticket_at(
                &credential_store::account_credentials_path(account_dir),
            )?,
            None => credential_store::delete_ticket(app_data)?,
        }
        // 不强制删除 profile（用户可能想保留浏览器缓存）
        // 由命令层显式调用 delete_qwen_profile 决定
        let mut g = self.lock();
        g.phase = QwenSessionPhase::LoggedOut;
        g.phase_before_login = None;
        g.message = None;
        g.message_code = None;
        g.updated_at = Some(now_unix());
        g.watcher_should_run = false;
        Ok(())
    }

    /// 取出短期使用的解密 ticket
    ///
    /// 仅在请求期间使用；调用方负责尽快 drop TicketSecret
    pub fn borrow_ticket(
        &self,
        app_data: &std::path::Path,
    ) -> Result<Option<TicketSecret>, BackendError> {
        let phase = {
            let g = self.lock();
            g.phase
        };
        match phase {
            QwenSessionPhase::Ready => self.load_ticket(app_data),
            QwenSessionPhase::LoggingIn => {
                // 登录中不允许借用（避免误用未完成的凭证）
                Err(BackendError::LoginRequired)
            }
            QwenSessionPhase::LoggedOut => Ok(None),
            QwenSessionPhase::Expired => Err(BackendError::SessionExpired),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(|poisoned| {
            log::warn!("QwenSession 锁曾发生 panic，继续使用锁内状态");
            poisoned.into_inner()
        })
    }

    fn load_ticket(&self, app_data: &Path) -> Result<Option<TicketSecret>, BackendError> {
        match &self.account_dir {
            Some(account_dir) => credential_store::load_ticket_at(
                &credential_store::account_credentials_path(account_dir),
            ),
            None => credential_store::load_ticket(app_data),
        }
    }

    fn load_ticket_for_account(&self) -> Result<Option<TicketSecret>, BackendError> {
        match &self.account_dir {
            Some(account_dir) => credential_store::load_ticket_at(
                &credential_store::account_credentials_path(account_dir),
            ),
            None => Ok(None),
        }
    }
}

fn constant_time_ticket_eq(left: &str, right: &str) -> bool {
    use zeroize::Zeroize;

    let mut left_hash = blake3::hash(left.as_bytes()).as_bytes().to_vec();
    let mut right_hash = blake3::hash(right.as_bytes()).as_bytes().to_vec();
    let mut difference = left_hash.len() ^ right_hash.len();
    for (left_byte, right_byte) in left_hash.iter().zip(right_hash.iter()) {
        difference |= usize::from(left_byte ^ right_byte);
    }
    left_hash.zeroize();
    right_hash.zeroize();
    difference == 0
}

impl Default for QwenSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "easyt-qwen-session-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn fresh_session_is_logged_out() {
        let session = QwenSession::new();
        assert_eq!(session.status().phase, QwenSessionPhase::LoggedOut);
    }

    #[test]
    fn try_begin_login_succeeds_when_idle() {
        let session = QwenSession::new();
        assert!(session.try_begin_login());
        assert_eq!(session.status().phase, QwenSessionPhase::LoggingIn);
    }

    #[test]
    fn try_begin_login_fails_when_already_logging_in() {
        let session = QwenSession::new();
        assert!(session.try_begin_login());
        assert!(!session.try_begin_login()); // 第二次失败
    }

    #[test]
    fn cancel_login_restores_expired_phase() {
        // 模拟从 Expired 状态启动登录
        let session = QwenSession::new();
        session.mark_expired();
        assert!(session.try_begin_login());
        assert_eq!(session.status().phase, QwenSessionPhase::LoggingIn);
        session.cancel_login();
        assert_eq!(session.status().phase, QwenSessionPhase::Expired);
        assert!(!session.watcher_should_run());
    }

    #[test]
    fn cancel_login_restores_logged_out_phase() {
        let session = QwenSession::new();
        assert!(session.try_begin_login());
        session.cancel_login();
        assert_eq!(session.status().phase, QwenSessionPhase::LoggedOut);
        assert!(!session.watcher_should_run());
    }

    #[test]
    fn mark_expired_transitions_from_ready() {
        let session = QwenSession::new();
        let dir = temp_dir();
        session.complete_login(&dir, "test-ticket").expect("login");
        assert_eq!(session.status().phase, QwenSessionPhase::Ready);
        session.mark_expired();
        assert_eq!(session.status().phase, QwenSessionPhase::Expired);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn closing_window_after_success_does_not_turn_success_into_cancel() {
        let session = QwenSession::new();
        let dir = temp_dir();
        assert!(session.try_begin_login());
        session.complete_login(&dir, "test-ticket").expect("login");
        session.cancel_login();
        let status = session.status();
        assert_eq!(status.phase, QwenSessionPhase::Ready);
        assert!(status.message.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn logout_clears_phase_and_deletes_file() {
        let session = QwenSession::new();
        let dir = temp_dir();
        session.complete_login(&dir, "test-ticket").expect("login");
        assert!(credential_store::credentials_path(&dir).exists());
        session.logout(&dir).expect("logout");
        assert!(!credential_store::credentials_path(&dir).exists());
        assert_eq!(session.status().phase, QwenSessionPhase::LoggedOut);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_from_storage_picks_up_existing_ticket_windows_only() {
        if cfg!(not(windows)) {
            return;
        }
        let dir = temp_dir();
        // 准备凭证文件
        credential_store::save_ticket(&dir, "test-ticket").expect("save");

        let session = QwenSession::new();
        session.restore_from_storage(&dir);
        assert_eq!(session.status().phase, QwenSessionPhase::Ready);

        // 借用应能取到 ticket
        let ticket = session.borrow_ticket(&dir).expect("borrow").expect("some");
        assert_eq!(ticket.as_str(), "test-ticket");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_from_storage_handles_missing_ticket() {
        let dir = temp_dir();
        let session = QwenSession::new();
        session.restore_from_storage(&dir);
        assert_eq!(session.status().phase, QwenSessionPhase::LoggedOut);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn borrow_ticket_returns_login_required_when_logging_in() {
        let session = QwenSession::new();
        let dir = temp_dir();
        assert!(session.try_begin_login());
        let result = session.borrow_ticket(&dir);
        assert!(matches!(result, Err(BackendError::LoginRequired)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn borrow_ticket_returns_session_expired_when_expired() {
        let session = QwenSession::new();
        let dir = temp_dir();
        session.mark_expired();
        let result = session.borrow_ticket(&dir);
        assert!(matches!(result, Err(BackendError::SessionExpired)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn account_session_rejects_the_preexisting_ticket_as_a_relogin_result() {
        let dir = temp_dir();
        let id = AccountId::new();
        let session = QwenSession::for_account(id, dir.clone(), PersistedLogin::Ready);
        session
            .complete_login(&dir, "old-ticket")
            .expect("seed ticket");

        let old = TicketSecret::new("old-ticket".to_string());
        let replacement = TicketSecret::new("new-ticket".to_string());
        assert!(!session.is_fresh_login_ticket(&old));
        assert!(session.is_fresh_login_ticket(&replacement));
        let _ = std::fs::remove_dir_all(dir);
    }
}
