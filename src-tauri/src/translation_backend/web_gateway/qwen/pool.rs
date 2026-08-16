use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use crate::config::AppConfig;
use crate::translation_backend::error::BackendError;
use crate::translation_backend::models::{BackendRequest, BackendResult};
use crate::translation_backend::web_gateway::credential_store;
use crate::translation_backend::{connection_success_message, TranslationProgressReporter};

use super::account::{
    AccountId, AccountMoveDirection, PersistedAccount, PersistedHealth, PersistedLogin,
    QwenAccountActions, QwenAccountDisplayStatus, QwenAccountPoolSnapshot, QwenAccountSnapshot,
    MAXIMUM_ACCOUNTS,
};
use super::error::QwenError;
use super::executor::{QwenExecutionOptions, QwenRequestExecutor};
use super::registry::AccountRegistry;
use super::scheduler::RoundRobinScheduler;
use super::session::{QwenSession, QwenSessionPhase};

/// Owns account-local sessions and serializes registry mutations and login ownership.
pub struct QwenAccountPool {
    registry: Mutex<AccountRegistry>,
    mutation: Mutex<()>,
    sessions: Mutex<HashMap<AccountId, Arc<QwenSession>>>,
    active_login: Mutex<Option<AccountId>>,
    scheduler: RoundRobinScheduler,
    executor: Arc<QwenRequestExecutor>,
    runtime_health: Mutex<HashMap<AccountId, RuntimeHealth>>,
    shutting_down: AtomicBool,
    background_probes: Arc<Mutex<HashMap<u64, tokio::task::AbortHandle>>>,
    next_background_probe: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
enum RuntimeHealth {
    Healthy,
    CoolingDown(Instant),
    PendingVerification,
}

impl RuntimeHealth {
    fn is_pending(self) -> bool {
        matches!(self, Self::PendingVerification)
    }

    fn is_selectable(self, include_pending: bool) -> bool {
        matches!(self, Self::Healthy) || (include_pending && self.is_pending())
    }

    fn cooldown_remaining_seconds(self) -> Option<u64> {
        match self {
            Self::CoolingDown(until) => {
                Some(until.saturating_duration_since(Instant::now()).as_secs())
            }
            _ => None,
        }
    }
}

const COOLDOWN: Duration = Duration::from_secs(5 * 60);

/// Pool policy surrounding one account-bound protocol request.
struct LeaseExecution<'a> {
    config: &'a AppConfig,
    request: &'a BackendRequest,
    progress: Arc<TranslationProgressReporter>,
    stream_output: bool,
    save_history: bool,
    deadline: tokio::time::Instant,
    enforce_total_deadline: bool,
    commit_send: bool,
}

impl QwenAccountPool {
    pub fn open(qwen_root: &Path, http_client: reqwest::Client) -> Result<Self, QwenError> {
        let registry = AccountRegistry::open_or_recover(qwen_root)?;
        let sessions = registry
            .accounts()
            .iter()
            .map(|account| {
                (
                    account.account_id.clone(),
                    Arc::new(QwenSession::for_account(
                        account.account_id.clone(),
                        registry.account_dir(&account.account_id),
                        account.login_state,
                    )),
                )
            })
            .collect();
        let runtime_health = registry
            .accounts()
            .iter()
            .map(|account| {
                (
                    account.account_id.clone(),
                    if account.last_health == PersistedHealth::Unhealthy {
                        RuntimeHealth::PendingVerification
                    } else {
                        RuntimeHealth::Healthy
                    },
                )
            })
            .collect();
        Ok(Self {
            registry: Mutex::new(registry),
            mutation: Mutex::new(()),
            sessions: Mutex::new(sessions),
            active_login: Mutex::new(None),
            scheduler: RoundRobinScheduler::default(),
            executor: Arc::new(QwenRequestExecutor::new(http_client)),
            runtime_health: Mutex::new(runtime_health),
            shutting_down: AtomicBool::new(false),
            background_probes: Arc::new(Mutex::new(HashMap::new())),
            next_background_probe: AtomicU64::new(0),
        })
    }

    /// Reconciles every account-local session with its own credential path at startup.
    /// A stale Ready registry entry without a readable credential is downgraded instead
    /// of appearing as a selectable healthy account.
    pub fn restore_from_storage(&self) -> Result<(), QwenError> {
        let accounts = self
            .registry
            .lock()
            .expect("account registry lock")
            .accounts()
            .to_vec();
        let mut restored = Vec::with_capacity(accounts.len());
        for account in accounts {
            let session = self.session(&account.account_id)?;
            let account_dir = session
                .account_dir()
                .ok_or_else(QwenError::account_not_found)?;
            session.restore_from_storage(account_dir);
            let login_state = match session.status().phase {
                QwenSessionPhase::LoggedOut => PersistedLogin::LoggedOut,
                QwenSessionPhase::LoggingIn => continue,
                QwenSessionPhase::Ready => PersistedLogin::Ready,
                QwenSessionPhase::Expired => PersistedLogin::Expired,
            };
            let health = if login_state == PersistedLogin::LoggedOut {
                PersistedHealth::Unknown
            } else {
                account.last_health
            };
            restored.push((account.account_id, login_state, health));
        }
        let mut registry = self.registry.lock().expect("account registry lock");
        for (account_id, login_state, health) in restored {
            registry.set_login_state(&account_id, login_state, health)?;
        }
        Ok(())
    }

    pub fn snapshot(&self) -> QwenAccountPoolSnapshot {
        let registry = self.registry.lock().expect("account registry lock");
        let sessions = self.sessions.lock().expect("account sessions lock");
        let active_login = self.active_login.lock().expect("login lock").clone();
        let account_count = registry.accounts().len();
        QwenAccountPoolSnapshot {
            accounts: registry
                .accounts()
                .iter()
                .enumerate()
                .map(|(order, account)| {
                    snapshot_account(
                        order,
                        account_count,
                        account,
                        sessions.get(&account.account_id),
                        self.scheduler.is_busy(&account.account_id),
                        self.runtime_health(&account.account_id),
                    )
                })
                .collect(),
            maximum_accounts: MAXIMUM_ACCOUNTS,
            login_account_id: active_login,
            warning: registry.snapshot().warning,
        }
    }

    pub fn create_account(&self, display_name: &str) -> Result<AccountId, QwenError> {
        let _operation = self.mutation.lock().expect("account mutation lock");
        let registry = self.registry.lock().expect("account registry lock");
        if registry.accounts().len() >= MAXIMUM_ACCOUNTS {
            return Err(QwenError::pool_limit());
        }
        let account = PersistedAccount {
            account_id: AccountId::new(),
            display_name: super::account::DisplayName::parse(display_name)?.into_inner(),
            enabled: true,
            login_state: PersistedLogin::LoggedOut,
            last_health: PersistedHealth::Unknown,
        };
        let account_dir = registry.account_dir(&account.account_id);
        drop(registry);
        std::fs::create_dir_all(&account_dir).map_err(QwenError::storage_write)?;
        std::fs::create_dir_all(credential_store::account_profile_path(&account_dir))
            .map_err(QwenError::storage_write)?;
        let mut registry = self.registry.lock().expect("account registry lock");
        if let Err(error) = registry.insert_account(account.clone()) {
            let _ = std::fs::remove_dir_all(&account_dir);
            return Err(error);
        }
        drop(registry);
        self.sessions.lock().expect("account sessions lock").insert(
            account.account_id.clone(),
            Arc::new(QwenSession::for_account(
                account.account_id.clone(),
                account_dir,
                PersistedLogin::LoggedOut,
            )),
        );
        self.runtime_health
            .lock()
            .expect("runtime health lock")
            .insert(account.account_id.clone(), RuntimeHealth::Healthy);
        Ok(account.account_id)
    }

    pub fn rename_account(
        &self,
        id: &AccountId,
        display_name: &str,
    ) -> Result<QwenAccountPoolSnapshot, QwenError> {
        let _operation = self.mutation.lock().expect("account mutation lock");
        self.registry
            .lock()
            .expect("account registry lock")
            .rename_account(id, display_name)?;
        Ok(self.snapshot())
    }

    pub fn set_account_enabled(
        &self,
        id: &AccountId,
        enabled: bool,
    ) -> Result<QwenAccountPoolSnapshot, QwenError> {
        let _operation = self.mutation.lock().expect("account mutation lock");
        self.registry
            .lock()
            .expect("account registry lock")
            .set_enabled(id, enabled)?;
        Ok(self.snapshot())
    }

    pub fn move_account(
        &self,
        id: &AccountId,
        direction: AccountMoveDirection,
    ) -> Result<QwenAccountPoolSnapshot, QwenError> {
        let _operation = self.mutation.lock().expect("account mutation lock");
        self.registry
            .lock()
            .expect("account registry lock")
            .move_account(id, direction)?;
        Ok(self.snapshot())
    }

    /// Logs out an account without deleting its local slot or display order.
    /// Credential/profile paths are moved aside before the registry commits so a failed
    /// registry write can restore the original identity without affecting other accounts.
    pub fn logout_account(&self, id: &AccountId) -> Result<QwenAccountPoolSnapshot, QwenError> {
        let _operation = self.mutation.lock().expect("account mutation lock");
        self.ensure_not_logging_in(id)?;
        let lifecycle_lease = self.scheduler.try_acquire_fixed(id)?;
        let registry = self.registry.lock().expect("account registry lock");
        let account = registry
            .accounts()
            .iter()
            .find(|account| &account.account_id == id)
            .cloned()
            .ok_or_else(QwenError::account_not_found)?;
        let account_dir = registry.account_dir(id);
        let staging_root = cleanup_staging_root(registry.qwen_root(), id, "logout");
        drop(registry);
        reject_reparse_tree(&account_dir)?;
        let staged = stage_logout_files(&account_dir, &staging_root)?;

        let mut registry = self.registry.lock().expect("account registry lock");
        if let Err(error) =
            registry.set_login_state(id, PersistedLogin::LoggedOut, PersistedHealth::Unknown)
        {
            restore_staged_files(&staged);
            return Err(error);
        }
        drop(registry);

        if let Err(error) = remove_staging(&staging_root) {
            restore_staged_files(&staged);
            let _ = self
                .registry
                .lock()
                .expect("account registry lock")
                .set_login_state(id, account.login_state, account.last_health);
            return Err(error);
        }
        self.session(id)?.mark_logged_out();
        drop(lifecycle_lease);
        Ok(self.snapshot())
    }

    /// Deletes only the selected account. The account directory is first atomically moved to
    /// an operation staging path; failures retain that diagnostic data instead of erasing it.
    pub fn delete_account(&self, id: &AccountId) -> Result<QwenAccountPoolSnapshot, QwenError> {
        let _operation = self.mutation.lock().expect("account mutation lock");
        self.ensure_not_logging_in(id)?;
        let lifecycle_lease = self.scheduler.try_acquire_fixed(id)?;
        let registry = self.registry.lock().expect("account registry lock");
        let account_dir = registry.account_dir(id);
        let staging_dir = cleanup_staging_root(registry.qwen_root(), id, "delete");
        drop(registry);
        reject_reparse_tree(&account_dir)?;
        let mut registry = self.registry.lock().expect("account registry lock");
        if staging_dir.exists() {
            if account_dir.exists() {
                return Err(QwenError::storage_cleanup(
                    "existing account cleanup staging",
                ));
            }
            let account_exists = registry
                .accounts()
                .iter()
                .any(|account| &account.account_id == id);
            drop(registry);
            remove_staging(&staging_dir)?;
            if account_exists {
                self.registry
                    .lock()
                    .expect("account registry lock")
                    .remove_account(id)?;
            }
            self.sessions
                .lock()
                .expect("account sessions lock")
                .remove(id);
            drop(lifecycle_lease);
            return Ok(self.snapshot());
        }
        let (account_index, account) = registry
            .accounts()
            .iter()
            .enumerate()
            .find(|(_, account)| &account.account_id == id)
            .map(|(index, account)| (index, account.clone()))
            .ok_or_else(QwenError::account_not_found)?;
        if account_dir.exists() {
            reject_reparse_tree(&account_dir)?;
            drop(registry);
            std::fs::rename(&account_dir, &staging_dir).map_err(QwenError::storage_cleanup)?;
            registry = self.registry.lock().expect("account registry lock");
        }
        if let Err(error) = registry.remove_account(id) {
            if staging_dir.exists() {
                let _ = std::fs::rename(&staging_dir, &account_dir);
            }
            return Err(error);
        }
        drop(registry);

        if let Err(error) = remove_staging(&staging_dir) {
            if staging_dir.exists() {
                let _ = std::fs::rename(&staging_dir, &account_dir);
            }
            let mut registry = self.registry.lock().expect("account registry lock");
            let _ = registry.restore_account_at(account_index, account);
            return Err(error);
        }
        self.sessions
            .lock()
            .expect("account sessions lock")
            .remove(id);
        self.runtime_health
            .lock()
            .expect("runtime health lock")
            .remove(id);
        drop(lifecycle_lease);
        Ok(self.snapshot())
    }

    pub fn begin_login(&self, id: &AccountId) -> Result<Arc<QwenSession>, QwenError> {
        if self.scheduler.is_busy(id) {
            return Err(QwenError::account_busy());
        }
        let session = self.session(id)?;
        if session.account_id() != Some(id) {
            return Err(QwenError::account_not_found());
        }
        let mut active_login = self.active_login.lock().expect("login lock");
        if let Some(active) = active_login.as_ref() {
            if active != id {
                return Err(QwenError::login_occupied());
            }
        }
        if !session.try_begin_login() {
            return Err(QwenError::login_occupied());
        }
        *active_login = Some(id.clone());
        Ok(session)
    }

    pub fn cancel_login(&self, id: &AccountId) {
        if let Ok(session) = self.session(id) {
            session.cancel_login();
        }
        self.clear_active_login(id);
    }

    pub fn fail_login(&self, id: &AccountId, error: QwenError) {
        if let Ok(session) = self.session(id) {
            session.fail_login_with_code(error.safe_message(), error.code().as_str());
        }
        self.clear_active_login(id);
    }

    pub fn complete_login(&self, id: &AccountId, ticket: &str) -> Result<(), QwenError> {
        let session = self.session(id)?;
        if self.active_login.lock().expect("login lock").as_ref() != Some(id)
            || session.status().phase != QwenSessionPhase::LoggingIn
        {
            return Err(QwenError::login_occupied());
        }
        let ticket = credential_store::TicketSecret::new(ticket.to_string());
        if !session.is_fresh_login_ticket(&ticket) {
            return Err(QwenError::login_cookie());
        }
        let account_dir = session
            .account_dir()
            .ok_or_else(QwenError::account_not_found)?;
        let credential_path = credential_store::account_credentials_path(account_dir);
        let staged_path = credential_store::stage_ticket(&credential_path, ticket.as_str())
            .map_err(|_| QwenError::login_save())?;
        let prior = self
            .registry
            .lock()
            .expect("account registry lock")
            .accounts()
            .iter()
            .find(|account| &account.account_id == id)
            .cloned()
            .ok_or_else(QwenError::account_not_found)?;
        let update = self
            .registry
            .lock()
            .expect("account registry lock")
            .set_login_state(id, PersistedLogin::Ready, PersistedHealth::Healthy);
        if let Err(error) = update {
            let _ = credential_store::discard_staged_ticket(&staged_path);
            return Err(error);
        }
        if credential_store::commit_staged_ticket(&staged_path, &credential_path).is_err() {
            let _ = self
                .registry
                .lock()
                .expect("account registry lock")
                .set_login_state(id, prior.login_state, prior.last_health);
            let _ = credential_store::discard_staged_ticket(&staged_path);
            return Err(QwenError::login_save());
        }
        session.mark_login_complete();
        self.clear_active_login(id);
        Ok(())
    }

    pub fn session(&self, id: &AccountId) -> Result<Arc<QwenSession>, QwenError> {
        self.sessions
            .lock()
            .expect("account sessions lock")
            .get(id)
            .cloned()
            .ok_or_else(QwenError::account_not_found)
    }

    pub fn account_dir(&self, id: &AccountId) -> Result<PathBuf, QwenError> {
        let registry = self.registry.lock().expect("account registry lock");
        if registry
            .accounts()
            .iter()
            .any(|account| &account.account_id == id)
        {
            Ok(registry.account_dir(id))
        } else {
            Err(QwenError::account_not_found())
        }
    }

    pub fn first_session(&self) -> Option<Arc<QwenSession>> {
        let registry = self.registry.lock().expect("account registry lock");
        let id = registry
            .accounts()
            .iter()
            .find(|account| account.enabled)?
            .account_id
            .clone();
        drop(registry);
        self.session(&id).ok()
    }

    pub fn cancel_all_logins(&self) {
        let active = self.active_login.lock().expect("login lock").take();
        if let Some(id) = active {
            if let Ok(session) = self.session(&id) {
                session.cancel_watcher();
                session.cancel_login();
            }
        }
    }

    /// Stops pool-owned background work without waiting for upstream network I/O. Scheduler
    /// permits are released synchronously so application teardown cannot strand an account.
    pub fn shutdown(&self) {
        if self.shutting_down.swap(true, Ordering::AcqRel) {
            return;
        }
        self.cancel_all_logins();
        let tasks = std::mem::take(
            &mut *self
                .background_probes
                .lock()
                .expect("background probe lock"),
        );
        for (_, handle) in tasks {
            handle.abort();
        }
        self.scheduler.release_all();
    }

    /// Executes at most two formal one-shot attempts under one total request deadline.
    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        self.translate_one_shot(config, request, progress).await
    }

    pub async fn translate_stream(
        self: &Arc<Self>,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        self.execute_stream_once(config, request, progress).await
    }

    /// Global connection tests use the same scheduler but never save remote history.
    pub async fn test_global(
        &self,
        config: &AppConfig,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<String, BackendError> {
        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(config.timeout_seconds.clamp(5, 300));
        let eligible = self.healthy_account_order(false);
        if eligible.iter().all(Option::is_none) {
            return Err(BackendError::QwenPool(self.classify_pool_unavailable()));
        }
        let mut lease = self
            .scheduler
            .acquire(&eligible, deadline)
            .await
            .map_err(BackendError::QwenPool)?;
        let (result, sent) = self
            .execute_with_lease_observed(
                LeaseExecution {
                    config,
                    request: &request,
                    progress,
                    stream_output: config.stream_output,
                    save_history: false,
                    deadline,
                    enforce_total_deadline: true,
                    commit_send: true,
                },
                &mut lease,
            )
            .await;
        if sent {
            self.apply_test_result(lease.account_id(), &result);
        }
        let result = result?;
        Ok(connection_success_message("连接成功", &result))
    }

    /// Tests exactly one requested account. A busy account fails immediately and never queues.
    pub async fn test_account(
        &self,
        id: &AccountId,
        config: &AppConfig,
    ) -> Result<String, BackendError> {
        let available = self
            .fixed_test_is_eligible(id)
            .map_err(BackendError::QwenPool)?;
        let mut lease = self
            .scheduler
            .try_acquire_fixed_if_available(id, available)
            .map_err(BackendError::QwenPool)?;
        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(config.timeout_seconds.clamp(5, 300));
        let (result, sent) = self
            .execute_with_lease_observed(
                LeaseExecution {
                    config,
                    request: &request,
                    progress: Arc::new(TranslationProgressReporter::discard()),
                    stream_output: config.stream_output,
                    save_history: false,
                    deadline,
                    enforce_total_deadline: true,
                    commit_send: false,
                },
                &mut lease,
            )
            .await;
        if sent {
            self.apply_test_result(id, &result);
        }
        Ok(connection_success_message("连接成功", &result?))
    }

    async fn execute_stream_once(
        self: &Arc<Self>,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(config.timeout_seconds.clamp(5, 300));
        let eligible = self.healthy_account_order(true);
        if eligible.iter().all(Option::is_none) {
            return Err(BackendError::QwenPool(self.classify_pool_unavailable()));
        }
        let mut lease = self
            .scheduler
            .acquire(&eligible, deadline)
            .await
            .map_err(BackendError::QwenPool)?;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(BackendError::QwenPool(QwenError::no_healthy_account()));
        }
        let (result, sent) = self
            .execute_with_lease_observed(
                LeaseExecution {
                    config,
                    request: &request,
                    progress,
                    stream_output: true,
                    save_history: config.web_gateway.save_history,
                    deadline,
                    enforce_total_deadline: false,
                    commit_send: true,
                },
                &mut lease,
            )
            .await;
        if self.shutting_down.load(Ordering::Acquire) {
            return result;
        }
        if let Err(BackendError::Qwen(error)) = &result {
            if error.is_authentication_error() {
                self.mark_expired(lease.account_id())?;
            } else if sent && error.requires_probe() && !self.shutting_down.load(Ordering::Acquire)
            {
                self.spawn_stream_probe(config.clone(), lease);
                return result;
            }
        }
        result
    }

    /// A streaming failure has already been delivered to the caller. The retained fixed lease
    /// serializes exactly one discard probe; its health update only occurs after completion.
    fn spawn_stream_probe(
        self: &Arc<Self>,
        config: AppConfig,
        mut lease: super::scheduler::AccountLease,
    ) {
        let task_id = self.next_background_probe.fetch_add(1, Ordering::Relaxed);
        let (started_sender, started_receiver) = tokio::sync::oneshot::channel();
        let pool = Arc::clone(self);
        let tracker = Arc::clone(&self.background_probes);
        let handle = tokio::spawn(async move {
            // Register the abort handle before allowing the probe to start.
            if started_receiver.await.is_err() {
                return;
            }
            let account_id = lease.account_id().clone();
            let deadline = tokio::time::Instant::now()
                + Duration::from_secs(config.timeout_seconds.clamp(5, 300));
            let _ = pool
                .probe_lease(&config, &account_id, deadline, &mut lease)
                .await;
            tracker
                .lock()
                .expect("background probe lock")
                .remove(&task_id);
        });
        let abort_handle = handle.abort_handle();
        let should_start = {
            let mut probes = self
                .background_probes
                .lock()
                .expect("background probe lock");
            if self.shutting_down.load(Ordering::Acquire) {
                false
            } else {
                probes.insert(task_id, abort_handle.clone());
                true
            }
        };
        if should_start {
            let _ = started_sender.send(());
        } else {
            abort_handle.abort();
        }
    }

    async fn translate_one_shot(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_secs(config.timeout_seconds.clamp(5, 300));
        let mut formal_attempts = 0;
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(BackendError::Qwen(QwenError::timeout()));
            }
            let healthy = self.healthy_account_order(false);
            let eligible = if healthy.iter().any(Option::is_some) {
                healthy
            } else {
                self.healthy_account_order(true)
            };
            if eligible.iter().all(Option::is_none) {
                return Err(BackendError::QwenPool(self.classify_pool_unavailable()));
            }
            let mut lease = self
                .scheduler
                .acquire(&eligible, deadline)
                .await
                .map_err(BackendError::QwenPool)?;
            if self.runtime_health(lease.account_id()).is_pending() {
                let account_id = lease.account_id().clone();
                if let Err(error) = self
                    .probe_lease(config, &account_id, deadline, &mut lease)
                    .await
                {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(error);
                    }
                    continue;
                }
            }
            let result = self
                .execute_with_lease(
                    LeaseExecution {
                        config,
                        request: &request,
                        progress: Arc::clone(&progress),
                        stream_output: false,
                        save_history: config.web_gateway.save_history,
                        deadline,
                        enforce_total_deadline: true,
                        commit_send: true,
                    },
                    &mut lease,
                )
                .await;
            formal_attempts += 1;
            match result {
                Ok(result) => {
                    self.mark_healthy(lease.account_id())?;
                    return Ok(result);
                }
                Err(BackendError::Qwen(error)) => {
                    let retryable = formal_attempts == 1 && error.is_formal_retryable();
                    if error.is_authentication_error() {
                        self.mark_expired(lease.account_id())?;
                    } else if error.requires_probe() {
                        if tokio::time::Instant::now() >= deadline {
                            self.mark_pending(lease.account_id())?;
                        } else {
                            let account_id = lease.account_id().clone();
                            let _ = self
                                .probe_lease(config, &account_id, deadline, &mut lease)
                                .await;
                        }
                    }
                    if !retryable {
                        return Err(BackendError::Qwen(error));
                    }
                    let remaining = deadline
                        .checked_duration_since(tokio::time::Instant::now())
                        .ok_or_else(|| BackendError::Qwen(QwenError::timeout()))?;
                    tokio::time::sleep(Duration::from_millis(250).min(remaining)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn execute_with_lease(
        &self,
        options: LeaseExecution<'_>,
        lease: &mut super::scheduler::AccountLease,
    ) -> Result<BackendResult, BackendError> {
        self.execute_with_lease_observed(options, lease).await.0
    }

    async fn execute_with_lease_observed(
        &self,
        options: LeaseExecution<'_>,
        lease: &mut super::scheduler::AccountLease,
    ) -> (Result<BackendResult, BackendError>, bool) {
        let timeout = options
            .deadline
            .checked_duration_since(tokio::time::Instant::now())
            .ok_or_else(|| BackendError::Qwen(QwenError::timeout()));
        let result = (|| -> Result<_, BackendError> {
            let timeout = timeout?;
            let session = self
                .session(lease.account_id())
                .map_err(BackendError::QwenPool)?;
            let account_dir = session.account_dir().ok_or(BackendError::LoginRequired)?;
            let ticket = session
                .borrow_ticket(account_dir)?
                .ok_or(BackendError::LoginRequired)?;
            Ok((timeout, ticket))
        })();
        let Ok((timeout, ticket)) = result else {
            return (result.map(|_| unreachable!()), false);
        };
        let sent = Arc::new(AtomicBool::new(false));
        let sent_for_callback = Arc::clone(&sent);
        let execute = self.executor.execute_once(QwenExecutionOptions {
            config: options.config,
            request: options.request,
            ticket: &ticket,
            progress: options.progress,
            stream_output: options.stream_output,
            save_history: options.save_history,
            timeout,
            before_send: || {
                sent_for_callback.store(true, Ordering::Release);
                if options.commit_send {
                    lease.commit_send();
                }
            },
        });
        let result = if options.enforce_total_deadline {
            tokio::time::timeout_at(options.deadline, execute)
                .await
                .map_err(|_| BackendError::Qwen(QwenError::timeout()))
                .and_then(|result| result)
        } else {
            execute.await
        };
        (result, sent.load(Ordering::Acquire))
    }

    async fn probe_lease(
        &self,
        config: &AppConfig,
        id: &AccountId,
        deadline: tokio::time::Instant,
        lease: &mut super::scheduler::AccountLease,
    ) -> Result<(), BackendError> {
        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let result = self
            .execute_with_lease(
                LeaseExecution {
                    config,
                    request: &request,
                    progress: Arc::new(TranslationProgressReporter::discard()),
                    stream_output: false,
                    save_history: false,
                    deadline,
                    enforce_total_deadline: true,
                    commit_send: false,
                },
                lease,
            )
            .await;
        match result {
            Ok(_) => {
                self.mark_healthy(id)?;
                Ok(())
            }
            Err(BackendError::Qwen(error)) if error.is_authentication_error() => {
                self.mark_expired(id)?;
                Err(BackendError::Qwen(error))
            }
            Err(error) => {
                self.mark_cooling_down(id)?;
                Err(error)
            }
        }
    }

    fn healthy_account_order(&self, include_pending: bool) -> Vec<Option<AccountId>> {
        let registry = self.registry.lock().expect("account registry lock");
        let sessions = self.sessions.lock().expect("account sessions lock");
        registry
            .accounts()
            .iter()
            .map(|account| {
                (account.enabled
                    && account.login_state == PersistedLogin::Ready
                    && (account.last_health == PersistedHealth::Healthy
                        || (include_pending
                            && self.runtime_health(&account.account_id).is_pending()))
                    && self
                        .runtime_health(&account.account_id)
                        .is_selectable(include_pending)
                    && sessions
                        .get(&account.account_id)
                        .is_some_and(|session| session.status().phase == QwenSessionPhase::Ready))
                .then(|| account.account_id.clone())
            })
            .collect()
    }

    fn fixed_test_is_eligible(&self, id: &AccountId) -> Result<bool, QwenError> {
        let registry = self.registry.lock().expect("account registry lock");
        let account = registry
            .accounts()
            .iter()
            .find(|account| &account.account_id == id)
            .ok_or_else(QwenError::account_not_found)?;
        let session = self.session(id)?;
        let status = session.status();
        if self.scheduler.is_busy(id) {
            return Err(QwenError::account_busy());
        }
        if !account.enabled {
            return Err(QwenError::pool_all_disabled());
        }
        if account.login_state == PersistedLogin::LoggedOut
            || status.phase == QwenSessionPhase::LoggedOut
        {
            return Err(QwenError::pool_all_logged_out());
        }
        if account.login_state == PersistedLogin::Expired
            || status.phase == QwenSessionPhase::Expired
        {
            return Err(QwenError::pool_all_expired());
        }
        if !matches!(self.runtime_health(id), RuntimeHealth::Healthy) {
            return Err(QwenError::no_healthy_account());
        }
        if status.phase != QwenSessionPhase::Ready {
            return Err(QwenError::mixed_unavailable());
        }
        Ok(true)
    }

    fn classify_pool_unavailable(&self) -> QwenError {
        let registry = self.registry.lock().expect("account registry lock");
        let accounts = registry.accounts();
        if accounts.is_empty() {
            return QwenError::pool_empty();
        }
        if accounts.iter().all(|account| !account.enabled) {
            return QwenError::pool_all_disabled();
        }

        let enabled = accounts
            .iter()
            .filter(|account| account.enabled)
            .collect::<Vec<_>>();
        if enabled
            .iter()
            .all(|account| account.login_state == PersistedLogin::LoggedOut)
        {
            return QwenError::pool_all_logged_out();
        }
        if enabled
            .iter()
            .all(|account| account.login_state == PersistedLogin::Expired)
        {
            return QwenError::pool_all_expired();
        }

        let all_cooling_or_pending = enabled.iter().all(|account| {
            let runtime = self.runtime_health(&account.account_id);
            matches!(
                runtime,
                RuntimeHealth::CoolingDown(_) | RuntimeHealth::PendingVerification
            ) || account.last_health == PersistedHealth::Unhealthy
        });
        if all_cooling_or_pending {
            return QwenError::no_healthy_account();
        }
        QwenError::mixed_unavailable()
    }

    fn runtime_health(&self, id: &AccountId) -> RuntimeHealth {
        let mut runtime = self.runtime_health.lock().expect("runtime health lock");
        let current = *runtime.get(id).unwrap_or(&RuntimeHealth::Healthy);
        if let RuntimeHealth::CoolingDown(until) = current {
            if Instant::now() >= until {
                runtime.insert(id.clone(), RuntimeHealth::PendingVerification);
                return RuntimeHealth::PendingVerification;
            }
        }
        current
    }

    fn mark_healthy(&self, id: &AccountId) -> Result<(), BackendError> {
        self.registry
            .lock()
            .expect("account registry lock")
            .set_health(id, PersistedHealth::Healthy)
            .map_err(BackendError::QwenPool)?;
        self.runtime_health
            .lock()
            .expect("runtime health lock")
            .insert(id.clone(), RuntimeHealth::Healthy);
        Ok(())
    }

    fn mark_pending(&self, id: &AccountId) -> Result<(), BackendError> {
        self.registry
            .lock()
            .expect("account registry lock")
            .set_health(id, PersistedHealth::Unhealthy)
            .map_err(BackendError::QwenPool)?;
        self.runtime_health
            .lock()
            .expect("runtime health lock")
            .insert(id.clone(), RuntimeHealth::PendingVerification);
        Ok(())
    }

    fn mark_cooling_down(&self, id: &AccountId) -> Result<(), BackendError> {
        self.mark_pending(id)?;
        self.runtime_health
            .lock()
            .expect("runtime health lock")
            .insert(
                id.clone(),
                RuntimeHealth::CoolingDown(Instant::now() + COOLDOWN),
            );
        Ok(())
    }

    fn mark_expired(&self, id: &AccountId) -> Result<(), BackendError> {
        self.registry
            .lock()
            .expect("account registry lock")
            .set_login_state(id, PersistedLogin::Expired, PersistedHealth::Unhealthy)
            .map_err(BackendError::QwenPool)?;
        self.session(id)
            .map_err(BackendError::QwenPool)?
            .mark_expired();
        self.runtime_health
            .lock()
            .expect("runtime health lock")
            .insert(id.clone(), RuntimeHealth::PendingVerification);
        Ok(())
    }

    fn apply_test_result(&self, id: &AccountId, result: &Result<BackendResult, BackendError>) {
        match result {
            Ok(_) => {
                let _ = self.mark_healthy(id);
            }
            Err(BackendError::Qwen(error)) if error.is_authentication_error() => {
                let _ = self.mark_expired(id);
            }
            Err(_) => {
                let _ = self.mark_cooling_down(id);
            }
        }
    }

    fn ensure_not_logging_in(&self, id: &AccountId) -> Result<(), QwenError> {
        let logging_in = self.active_login.lock().expect("login lock").as_ref() == Some(id);
        if logging_in {
            Err(QwenError::account_busy())
        } else {
            Ok(())
        }
    }

    fn clear_active_login(&self, id: &AccountId) {
        let mut active_login = self.active_login.lock().expect("login lock");
        if active_login.as_ref() == Some(id) {
            *active_login = None;
        }
    }

    #[cfg(test)]
    fn background_probe_count(&self) -> usize {
        self.background_probes
            .lock()
            .expect("background probe lock")
            .len()
    }
}

fn snapshot_account(
    order: usize,
    account_count: usize,
    account: &PersistedAccount,
    session: Option<&Arc<QwenSession>>,
    busy: bool,
    runtime_health: RuntimeHealth,
) -> QwenAccountSnapshot {
    let session_status = session.map(|session| session.status());
    let phase = session_status.as_ref().map(|status| status.phase);
    let status = if !account.enabled {
        QwenAccountDisplayStatus::Disabled
    } else if phase == Some(QwenSessionPhase::LoggingIn) {
        QwenAccountDisplayStatus::LoggingIn
    } else if account.login_state == PersistedLogin::LoggedOut {
        QwenAccountDisplayStatus::LoggedOut
    } else if account.login_state == PersistedLogin::Expired {
        QwenAccountDisplayStatus::Expired
    } else if busy {
        QwenAccountDisplayStatus::Busy
    } else if matches!(runtime_health, RuntimeHealth::CoolingDown(_)) {
        QwenAccountDisplayStatus::CoolingDown
    } else if runtime_health.is_pending() || account.last_health == PersistedHealth::Unhealthy {
        QwenAccountDisplayStatus::PendingVerification
    } else {
        QwenAccountDisplayStatus::Available
    };
    QwenAccountSnapshot {
        account_id: account.account_id.clone(),
        display_name: account.display_name.clone(),
        enabled: account.enabled,
        order,
        status,
        cooldown_remaining_seconds: runtime_health.cooldown_remaining_seconds(),
        message: session_status
            .as_ref()
            .and_then(|status| status.message.clone()),
        message_code: session_status.and_then(|status| status.code),
        actions: QwenAccountActions {
            can_rename: true,
            can_toggle_enabled: true,
            can_move_up: order > 0,
            can_move_down: order + 1 < account_count,
            can_login: account.enabled
                && !busy
                && !matches!(status, QwenAccountDisplayStatus::LoggingIn),
            can_logout: !busy
                && !matches!(
                    status,
                    QwenAccountDisplayStatus::LoggingIn | QwenAccountDisplayStatus::LoggedOut
                ),
            can_test: account.enabled
                && !busy
                && account.login_state == PersistedLogin::Ready
                && !matches!(status, QwenAccountDisplayStatus::LoggingIn),
            can_delete: !busy && !matches!(status, QwenAccountDisplayStatus::LoggingIn),
        },
    }
}

fn cleanup_staging_root(qwen_root: &Path, id: &AccountId, operation: &str) -> PathBuf {
    qwen_root.join(format!(".{operation}-{}", id.as_str()))
}

fn stage_logout_files(
    account_dir: &Path,
    staging_root: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, QwenError> {
    let targets = ["credentials.bin", "profile"];
    let mut staged = Vec::new();
    for name in targets {
        let source = account_dir.join(name);
        if !source.exists() {
            continue;
        }
        std::fs::create_dir_all(staging_root).map_err(QwenError::storage_cleanup)?;
        let destination = staging_root.join(name);
        if let Err(error) = std::fs::rename(&source, &destination) {
            restore_staged_files(&staged);
            return Err(QwenError::storage_cleanup(error));
        }
        staged.push((source, destination));
    }
    Ok(staged)
}

fn restore_staged_files(staged: &[(PathBuf, PathBuf)]) {
    for (source, destination) in staged.iter().rev() {
        let _ = std::fs::rename(destination, source);
    }
}

fn remove_staging(path: &Path) -> Result<(), QwenError> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(QwenError::storage_cleanup)?;
    }
    Ok(())
}

fn reject_reparse_tree(path: &Path) -> Result<(), QwenError> {
    if !path.exists() {
        return Ok(());
    }
    if is_reparse_point(path)? {
        return Err(QwenError::storage_cleanup(
            "account data contains a reparse point",
        ));
    }
    if path.is_dir() {
        for entry in std::fs::read_dir(path).map_err(QwenError::storage_cleanup)? {
            let entry = entry.map_err(QwenError::storage_cleanup)?;
            reject_reparse_tree(&entry.path())?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn is_reparse_point(path: &Path) -> Result<bool, QwenError> {
    Ok(std::fs::symlink_metadata(path)
        .map_err(QwenError::storage_cleanup)?
        .file_type()
        .is_symlink())
}

#[cfg(windows)]
fn is_reparse_point(path: &Path) -> Result<bool, QwenError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, FILE_ATTRIBUTE_REPARSE_POINT, INVALID_FILE_ATTRIBUTES,
    };

    let metadata = std::fs::symlink_metadata(path).map_err(QwenError::storage_cleanup)?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attributes == INVALID_FILE_ATTRIBUTES {
        return Err(QwenError::storage_cleanup(std::io::Error::last_os_error()));
    }
    Ok(attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation_backend::web_gateway::qwen::test_support;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn pool(root: &Path) -> QwenAccountPool {
        QwenAccountPool::open(root, reqwest::Client::new()).unwrap()
    }

    #[test]
    fn create_initializes_two_isolated_account_directories_and_enforces_the_limit() {
        let root = test_support::TestDir::new("pool-create");
        let pool = pool(root.path());
        let first = pool.create_account("Personal").unwrap();
        let second = pool.create_account("Work").unwrap();

        assert_ne!(
            pool.account_dir(&first).unwrap(),
            pool.account_dir(&second).unwrap()
        );
        assert!(pool.account_dir(&first).unwrap().is_dir());
        assert!(
            credential_store::account_profile_path(&pool.account_dir(&first).unwrap()).is_dir()
        );
        for index in 2..MAXIMUM_ACCOUNTS {
            pool.create_account(&format!("Account {index}")).unwrap();
        }
        assert_eq!(
            pool.create_account("Eleven").unwrap_err().code().as_str(),
            "QW-POOL-002"
        );
    }

    #[test]
    fn only_one_account_can_own_the_login_flow_and_cancelling_preserves_other_accounts() {
        let root = test_support::TestDir::new("pool-login");
        let pool = pool(root.path());
        let first = pool.create_account("Personal").unwrap();
        let second = pool.create_account("Work").unwrap();

        pool.begin_login(&first).unwrap();
        assert_eq!(
            pool.begin_login(&second).unwrap_err().code().as_str(),
            "QW-LOGIN-001"
        );
        pool.cancel_login(&first);
        assert_eq!(
            pool.snapshot().accounts[1].status,
            QwenAccountDisplayStatus::LoggedOut
        );
    }

    #[test]
    fn stale_relogin_cookie_cannot_replace_the_existing_account_credential() {
        let root = test_support::TestDir::new("pool-relogin");
        let pool = pool(root.path());
        let id = pool.create_account("Personal").unwrap();
        let directory = pool.account_dir(&id).unwrap();
        credential_store::save_ticket_at(
            &credential_store::account_credentials_path(&directory),
            "old-ticket",
        )
        .unwrap();
        pool.registry
            .lock()
            .unwrap()
            .set_login_state(&id, PersistedLogin::Ready, PersistedHealth::Healthy)
            .unwrap();

        pool.begin_login(&id).unwrap();
        assert_eq!(
            pool.complete_login(&id, "old-ticket")
                .unwrap_err()
                .code()
                .as_str(),
            "QW-LOGIN-003"
        );
        assert_eq!(
            credential_store::load_ticket_at(&credential_store::account_credentials_path(
                &directory
            ))
            .unwrap()
            .unwrap()
            .as_str(),
            "old-ticket"
        );
        pool.cancel_login(&id);
        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::Available
        );
    }

    #[test]
    fn lifecycle_mutations_return_authoritative_ordered_snapshots_and_preserve_other_accounts() {
        let root = test_support::TestDir::new("pool-lifecycle");
        let pool = pool(root.path());
        let personal = pool.create_account("Personal").unwrap();
        let work = pool.create_account("Work").unwrap();

        let renamed = pool.rename_account(&personal, "Personal Qwen").unwrap();
        assert_eq!(renamed.accounts[0].display_name, "Personal Qwen");
        assert!(renamed.accounts[0].actions.can_toggle_enabled);

        let disabled = pool.set_account_enabled(&personal, false).unwrap();
        assert_eq!(
            disabled.accounts[0].status,
            QwenAccountDisplayStatus::Disabled
        );
        assert!(disabled.accounts[0].actions.can_move_down);

        let moved = pool.move_account(&work, AccountMoveDirection::Up).unwrap();
        assert_eq!(moved.accounts[0].account_id, work);
        assert_eq!(moved.accounts[1].account_id, personal);

        let personal_dir = pool.account_dir(&personal).unwrap();
        credential_store::save_ticket_at(
            &credential_store::account_credentials_path(&personal_dir),
            "personal-ticket",
        )
        .unwrap();
        std::fs::create_dir_all(credential_store::account_profile_path(&personal_dir)).unwrap();
        std::fs::write(
            credential_store::account_profile_path(&personal_dir).join("cookie"),
            "profile-data",
        )
        .unwrap();
        pool.registry
            .lock()
            .unwrap()
            .set_login_state(&personal, PersistedLogin::Ready, PersistedHealth::Healthy)
            .unwrap();

        let logged_out = pool.logout_account(&personal).unwrap();
        let account = logged_out
            .accounts
            .iter()
            .find(|account| account.account_id == personal)
            .unwrap();
        assert_eq!(account.display_name, "Personal Qwen");
        assert!(!account.enabled);
        assert_eq!(account.status, QwenAccountDisplayStatus::Disabled);
        assert!(!credential_store::account_credentials_path(&personal_dir).exists());
        assert!(!credential_store::account_profile_path(&personal_dir).exists());
        assert!(pool.account_dir(&work).unwrap().exists());

        let deleted = pool.delete_account(&work).unwrap();
        assert_eq!(deleted.accounts.len(), 1);
        assert_eq!(deleted.accounts[0].account_id, personal);
        assert!(!pool
            .account_dir(&work)
            .unwrap_err()
            .safe_message()
            .is_empty());
    }

    #[test]
    fn login_busy_action_matrix_allows_rename_and_disable_but_rejects_destructive_actions() {
        let root = test_support::TestDir::new("pool-lifecycle-busy");
        let pool = pool(root.path());
        let id = pool.create_account("Personal").unwrap();
        pool.begin_login(&id).unwrap();

        let snapshot = pool.snapshot();
        let actions = &snapshot.accounts[0].actions;
        assert!(actions.can_rename);
        assert!(actions.can_toggle_enabled);
        assert!(!actions.can_logout);
        assert!(!actions.can_delete);
        assert!(!actions.can_test);

        pool.rename_account(&id, "Renamed").unwrap();
        pool.set_account_enabled(&id, false).unwrap();
        for result in [pool.logout_account(&id), pool.delete_account(&id)] {
            assert_eq!(result.unwrap_err().code().as_str(), "QW-POOL-009");
        }
    }

    #[test]
    fn snapshot_exposes_authoritative_cooldown_then_pending_verification() {
        let root = test_support::TestDir::new("pool-cooldown-snapshot");
        let pool = pool(root.path());
        let id = pool.create_account("Personal").unwrap();
        pool.registry
            .lock()
            .unwrap()
            .set_login_state(&id, PersistedLogin::Ready, PersistedHealth::Healthy)
            .unwrap();

        pool.mark_cooling_down(&id).unwrap();
        let cooling = &pool.snapshot().accounts[0];
        assert_eq!(cooling.status, QwenAccountDisplayStatus::CoolingDown);
        assert!(cooling.cooldown_remaining_seconds.is_some());
        assert!(
            cooling.actions.can_test,
            "fixed test permits early recovery"
        );

        pool.runtime_health.lock().unwrap().insert(
            id.clone(),
            RuntimeHealth::CoolingDown(Instant::now() - Duration::from_secs(1)),
        );
        let pending = &pool.snapshot().accounts[0];
        assert_eq!(
            pending.status,
            QwenAccountDisplayStatus::PendingVerification
        );
        assert!(pending.cooldown_remaining_seconds.is_none());
    }

    #[test]
    fn fixed_account_operations_fail_immediately_when_the_account_is_leased() {
        let root = test_support::TestDir::new("pool-fixed-busy");
        let pool = pool(root.path());
        let id = pool.create_account("Personal").unwrap();
        let lease = pool.scheduler.try_acquire_fixed(&id).unwrap();

        let error = match pool.scheduler.try_acquire_fixed(&id) {
            Ok(_) => panic!("a fixed operation must not acquire a busy account"),
            Err(error) => error,
        };
        assert_eq!(error.code().as_str(), "QW-POOL-009");
        drop(lease);
    }

    #[tokio::test]
    async fn one_shot_429_probes_the_fixed_account_without_history_then_retries_on_the_next_account(
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        std::thread::spawn(move || {
            for response in [
                "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string(),
                sse_response("probe"),
                sse_response("translated"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 4096];
                    let read = stream.read(&mut buffer).unwrap();
                    request.extend_from_slice(&buffer[..read]);
                    let Some(header_end) =
                        request.windows(4).position(|window| window == b"\r\n\r\n")
                    else {
                        continue;
                    };
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let length = headers
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + length {
                        break;
                    }
                }
                observed
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(request).unwrap());
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let root = test_support::TestDir::new("pool-retry-probe");
        let mut pool = pool(root.path());
        pool.executor = Arc::new(QwenRequestExecutor::with_api_url(
            reqwest::Client::new(),
            format!("http://{address}/api/v2/chat"),
        ));
        let first = ready_account(&pool, "First");
        let second = ready_account(&pool, "Second");
        let mut config = crate::config::default_config();
        config.web_gateway.save_history = true;
        let result = pool
            .translate(
                &config,
                BackendRequest {
                    text: "formal text".to_string(),
                    target_language: "简体中文".to_string(),
                },
                Arc::new(TranslationProgressReporter::discard()),
            )
            .await
            .unwrap();

        assert_eq!(result.translated_text, "translated");
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].contains(&format!("tongyi_sso_ticket={}", first.as_str())));
        assert!(requests[1].contains(&format!("tongyi_sso_ticket={}", first.as_str())));
        assert!(requests[2].contains(&format!("tongyi_sso_ticket={}", second.as_str())));
        assert!(requests[0].contains("\"temporary\":false"));
        assert!(requests[1].contains("\"temporary\":true"));
        assert!(requests[1].contains("User: hi"));
        assert!(requests[2].contains("\"temporary\":false"));
    }

    #[tokio::test]
    async fn partial_stream_returns_without_retry_while_a_discard_probe_holds_the_account() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&requests);
        let (probe_started, probe_started_receiver) = tokio::sync::oneshot::channel();
        let (release_probe, release_probe_receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let responses = [partial_sse_response("partial"), sse_response("probe")];
            let mut probe_started = Some(probe_started);
            for (index, response) in responses.into_iter().enumerate() {
                let (mut stream, _) = listener.accept().unwrap();
                observed
                    .lock()
                    .unwrap()
                    .push(read_http_request(&mut stream));
                if index == 1 {
                    probe_started.take().unwrap().send(()).unwrap();
                    release_probe_receiver.recv().unwrap();
                }
                stream.write_all(response.as_bytes()).unwrap();
            }
        });

        let root = test_support::TestDir::new("pool-stream-background-probe");
        let mut pool = pool(root.path());
        pool.executor = Arc::new(QwenRequestExecutor::with_api_url(
            reqwest::Client::new(),
            format!("http://{address}/api/v2/chat"),
        ));
        let account = ready_account(&pool, "Personal");
        let pool = Arc::new(pool);
        let mut config = crate::config::default_config();
        config.web_gateway.save_history = true;

        let progress = Arc::new(RecordingProgress::default());
        let progress_sink: Arc<dyn crate::translation_backend::TranslationProgress> =
            progress.clone();
        let error = pool
            .translate_stream(
                &config,
                BackendRequest {
                    text: "formal text".to_string(),
                    target_language: "简体中文".to_string(),
                },
                Arc::new(TranslationProgressReporter::new(progress_sink)),
            )
            .await
            .expect_err("partial stream must return its original error");
        assert!(
            matches!(error, BackendError::Qwen(ref error) if error.code().as_str() == "QW-UPSTREAM-003")
        );

        tokio::time::timeout(Duration::from_secs(1), probe_started_receiver)
            .await
            .expect("one background probe should start")
            .expect("probe start sender should remain available");
        assert_eq!(pool.background_probe_count(), 1);
        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::Busy
        );
        assert_eq!(
            pool.test_account(&account, &config)
                .await
                .unwrap_err()
                .safe_message(),
            QwenError::account_busy().safe_message()
        );
        {
            let requests = requests.lock().unwrap();
            assert_eq!(
                requests.len(),
                2,
                "streaming failure must not retry formally"
            );
            assert!(requests[0].contains("User: formal text"));
            assert!(requests[0].contains("\"temporary\":false"));
            assert!(requests[1].contains("User: hi"));
            assert!(requests[1].contains("\"temporary\":true"));
        }
        assert_eq!(
            *progress.deltas.lock().unwrap(),
            vec!["partial".to_string()],
            "the discard probe must not emit a second content delta"
        );

        release_probe.send(()).unwrap();
        wait_until_available(&pool).await;
        assert_eq!(pool.background_probe_count(), 0);
        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::Available
        );
    }

    #[tokio::test]
    async fn cancelled_stream_does_not_spawn_a_probe_or_change_account_health() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut stream);
            stream
                .write_all(partial_sse_response("partial").as_bytes())
                .unwrap();
        });

        let root = test_support::TestDir::new("pool-stream-cancelled");
        let mut pool = pool(root.path());
        pool.executor = Arc::new(QwenRequestExecutor::with_api_url(
            reqwest::Client::new(),
            format!("http://{address}/api/v2/chat"),
        ));
        let account = ready_account(&pool, "Personal");
        let pool = Arc::new(pool);
        let error = pool
            .translate_stream(
                &crate::config::default_config(),
                BackendRequest {
                    text: "formal text".to_string(),
                    target_language: "简体中文".to_string(),
                },
                Arc::new(TranslationProgressReporter::new(Arc::new(
                    CancellingProgress,
                ))),
            )
            .await
            .expect_err("closed user stream must cancel the formal request");

        assert!(matches!(error, BackendError::Cancelled));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::Available
        );
        assert_eq!(pool.snapshot().accounts[0].account_id, account);
    }

    #[tokio::test]
    async fn aborting_an_in_flight_stream_releases_its_lease_without_a_probe_or_health_penalty() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (formal_started, formal_started_receiver) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut stream);
            formal_started.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(1));
        });

        let root = test_support::TestDir::new("pool-stream-aborted");
        let mut pool = pool(root.path());
        pool.executor = Arc::new(QwenRequestExecutor::with_api_url(
            reqwest::Client::new(),
            format!("http://{address}/api/v2/chat"),
        ));
        let account = ready_account(&pool, "Personal");
        let pool = Arc::new(pool);
        let task_pool = Arc::clone(&pool);
        let task = tokio::spawn(async move {
            task_pool
                .translate_stream(
                    &crate::config::default_config(),
                    BackendRequest {
                        text: "formal text".to_string(),
                        target_language: "简体中文".to_string(),
                    },
                    Arc::new(TranslationProgressReporter::discard()),
                )
                .await
        });

        tokio::time::timeout(Duration::from_secs(1), formal_started_receiver)
            .await
            .expect("formal stream should reach the controlled server")
            .expect("formal start sender should remain available");
        task.abort();
        assert!(task
            .await
            .expect_err("task must be cancelled")
            .is_cancelled());
        wait_until_available(&pool).await;

        assert_eq!(pool.background_probe_count(), 0);
        assert_eq!(pool.snapshot().accounts[0].account_id, account);
        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::Available
        );
    }

    #[tokio::test]
    async fn failed_background_probe_cools_down_the_original_streaming_account() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut formal, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut formal);
            formal
                .write_all(partial_sse_response("partial").as_bytes())
                .unwrap();
            let (mut probe, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut probe);
            probe
                .write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });

        let root = test_support::TestDir::new("pool-stream-probe-cooldown");
        let mut pool = pool(root.path());
        pool.executor = Arc::new(QwenRequestExecutor::with_api_url(
            reqwest::Client::new(),
            format!("http://{address}/api/v2/chat"),
        ));
        let _ = ready_account(&pool, "Personal");
        let pool = Arc::new(pool);

        let _ = pool
            .translate_stream(
                &crate::config::default_config(),
                BackendRequest {
                    text: "formal text".to_string(),
                    target_language: "简体中文".to_string(),
                },
                Arc::new(TranslationProgressReporter::discard()),
            )
            .await
            .expect_err("partial stream must fail before the probe result");
        wait_until_not_busy(&pool).await;

        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::CoolingDown
        );
    }

    #[tokio::test]
    async fn authentication_failure_from_background_probe_expires_the_original_account() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut formal, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut formal);
            formal
                .write_all(partial_sse_response("partial").as_bytes())
                .unwrap();
            let (mut probe, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut probe);
            probe
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });

        let root = test_support::TestDir::new("pool-stream-probe-expired");
        let mut pool = pool(root.path());
        pool.executor = Arc::new(QwenRequestExecutor::with_api_url(
            reqwest::Client::new(),
            format!("http://{address}/api/v2/chat"),
        ));
        let account = ready_account(&pool, "Personal");
        let pool = Arc::new(pool);

        let _ = pool
            .translate_stream(
                &crate::config::default_config(),
                BackendRequest {
                    text: "formal text".to_string(),
                    target_language: "简体中文".to_string(),
                },
                Arc::new(TranslationProgressReporter::discard()),
            )
            .await
            .expect_err("partial stream must fail before the probe result");
        wait_until_not_busy(&pool).await;

        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::Expired
        );
        assert_eq!(
            pool.registry.lock().unwrap().accounts()[0].login_state,
            PersistedLogin::Expired
        );
        assert_eq!(pool.snapshot().accounts[0].account_id, account);
    }

    #[tokio::test]
    async fn shutdown_aborts_a_background_probe_releases_the_lease_and_preserves_prior_health() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (probe_started, probe_started_receiver) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let (mut formal, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut formal);
            formal
                .write_all(partial_sse_response("partial").as_bytes())
                .unwrap();
            let (mut probe, _) = listener.accept().unwrap();
            let _ = read_http_request(&mut probe);
            probe_started.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(2));
        });

        let root = test_support::TestDir::new("pool-stream-shutdown");
        let mut pool = pool(root.path());
        pool.executor = Arc::new(QwenRequestExecutor::with_api_url(
            reqwest::Client::new(),
            format!("http://{address}/api/v2/chat"),
        ));
        let account = ready_account(&pool, "Personal");
        pool.runtime_health
            .lock()
            .unwrap()
            .insert(account, RuntimeHealth::PendingVerification);
        let pool = Arc::new(pool);

        let _ = pool
            .translate_stream(
                &crate::config::default_config(),
                BackendRequest {
                    text: "formal text".to_string(),
                    target_language: "简体中文".to_string(),
                },
                Arc::new(TranslationProgressReporter::discard()),
            )
            .await
            .expect_err("partial stream must fail before probe completes");
        tokio::time::timeout(Duration::from_secs(1), probe_started_receiver)
            .await
            .expect("background probe should be running")
            .expect("probe start sender should remain available");

        pool.shutdown();
        wait_until_not_busy(&pool).await;
        assert_eq!(pool.background_probe_count(), 0);
        assert_eq!(
            pool.snapshot().accounts[0].status,
            QwenAccountDisplayStatus::PendingVerification
        );
    }

    fn ready_account(pool: &QwenAccountPool, name: &str) -> AccountId {
        let id = pool.create_account(name).unwrap();
        let account_dir = pool.account_dir(&id).unwrap();
        pool.session(&id)
            .unwrap()
            .complete_login(&account_dir, id.as_str())
            .unwrap();
        pool.registry
            .lock()
            .unwrap()
            .set_login_state(&id, PersistedLogin::Ready, PersistedHealth::Healthy)
            .unwrap();
        id
    }

    fn sse_response(text: &str) -> String {
        let body = format!(
            "event: complete\ndata: {{\"error_code\":0,\"data\":{{\"messages\":[{{\"mime_type\":\"text/plain\",\"content\":\"{text}\"}}]}}}}\n\n"
        );
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    }

    fn partial_sse_response(text: &str) -> String {
        let body = format!(
            "data: {{\"error_code\":0,\"data\":{{\"messages\":[{{\"mime_type\":\"text/plain\",\"content\":\"{text}\"}}]}}}}\n\n"
        );
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..read]);
            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if request.len() >= header_end + 4 + length {
                return String::from_utf8(request).unwrap();
            }
        }
    }

    async fn wait_until_available(pool: &QwenAccountPool) {
        for _ in 0..50 {
            if pool.snapshot().accounts[0].status == QwenAccountDisplayStatus::Available {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("account did not become available");
    }

    async fn wait_until_not_busy(pool: &QwenAccountPool) {
        for _ in 0..50 {
            if pool.snapshot().accounts[0].status != QwenAccountDisplayStatus::Busy {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("shutdown did not release the probe lease");
    }

    struct CancellingProgress;

    impl crate::translation_backend::TranslationProgress for CancellingProgress {
        fn phase_changed(&self, _: crate::translation_backend::PhaseProgress) {}

        fn content_delta(&self, _: String) -> Result<(), BackendError> {
            Err(BackendError::Cancelled)
        }
    }

    #[derive(Default)]
    struct RecordingProgress {
        deltas: Mutex<Vec<String>>,
    }

    impl crate::translation_backend::TranslationProgress for RecordingProgress {
        fn phase_changed(&self, _: crate::translation_backend::PhaseProgress) {}

        fn content_delta(&self, delta: String) -> Result<(), BackendError> {
            self.deltas.lock().unwrap().push(delta);
            Ok(())
        }
    }
}
