//! L2 SQLite 持久化缓存 worker。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::{mpsc, oneshot};

use super::entry::{
    backend_storage_label, now_ms, parse_backend_storage_label, CacheEntry, CacheOperationError,
    CacheStatsView, PersistentCacheState,
};
use super::key::{CacheKey, CACHE_KEY_VERSION};
use crate::translation_backend::models::{BackendResult, BackendSource};
use crate::translation_backend::prompt::PROMPT_VERSION;

const CACHE_DIR_NAME: &str = "cache";
const DATABASE_FILE_NAME: &str = "translation_cache.sqlite3";
const DATABASE_FAMILY_SUFFIXES: [&str; 3] = ["", "-wal", "-shm"];
const COMMAND_QUEUE_CAPACITY: usize = 512;
const WORKER_STACK_BYTES: usize = 512 * 1024;
const LOOKUP_BUDGET: Duration = Duration::from_millis(50);
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(1);
const TOUCH_BATCH_KEYS: usize = 256;
const TOUCH_FLUSH_INTERVAL: Duration = Duration::from_secs(30);
pub const MAX_L2_LOGICAL_BYTES: u64 = 256 * 1024 * 1024;
const MAX_L2_ENTRIES: u64 = 50_000;
const LOW_L2_LOGICAL_BYTES: u64 = MAX_L2_LOGICAL_BYTES * 9 / 10;
const LOW_L2_ENTRIES: u64 = 45_000;
const EVICTION_BATCH_SIZE: usize = 500;
const WAL_CHECKPOINT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
struct CapacityLimits {
    max_logical_bytes: u64,
    max_entries: u64,
    low_logical_bytes: u64,
    low_entries: u64,
    delete_batch: usize,
}

const PRODUCTION_CAPACITY: CapacityLimits = CapacityLimits {
    max_logical_bytes: MAX_L2_LOGICAL_BYTES,
    max_entries: MAX_L2_ENTRIES,
    low_logical_bytes: LOW_L2_LOGICAL_BYTES,
    low_entries: LOW_L2_ENTRIES,
    delete_batch: EVICTION_BATCH_SIZE,
};

pub struct PersistentStore {
    pub entry: CacheEntry,
    pub target_language: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TouchRecord {
    pub key: CacheKey,
    pub accessed_at_ms: i64,
    pub hit_delta: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct StatsDelta {
    l1_hits: u64,
    l2_hits: u64,
    misses: u64,
    bypasses: u64,
    refreshes: u64,
    oversized_bypasses: u64,
    lookup_failures: u64,
    store_failures: u64,
    touch_failures: u64,
}

impl StatsDelta {
    pub(super) fn l1_hit() -> Self {
        Self {
            l1_hits: 1,
            ..Self::default()
        }
    }

    pub(super) fn l2_hit() -> Self {
        Self {
            l2_hits: 1,
            ..Self::default()
        }
    }

    pub(super) fn miss() -> Self {
        Self {
            misses: 1,
            ..Self::default()
        }
    }

    pub(super) fn bypass() -> Self {
        Self {
            bypasses: 1,
            ..Self::default()
        }
    }

    pub(super) fn refresh() -> Self {
        Self {
            refreshes: 1,
            ..Self::default()
        }
    }

    pub(super) fn oversized_bypass() -> Self {
        Self {
            oversized_bypasses: 1,
            ..Self::default()
        }
    }

    fn lookup_failure() -> Self {
        Self {
            lookup_failures: 1,
            ..Self::default()
        }
    }

    fn touch_failure() -> Self {
        Self {
            touch_failures: 1,
            ..Self::default()
        }
    }

    fn merge(&mut self, other: Self) {
        self.l1_hits = self.l1_hits.saturating_add(other.l1_hits);
        self.l2_hits = self.l2_hits.saturating_add(other.l2_hits);
        self.misses = self.misses.saturating_add(other.misses);
        self.bypasses = self.bypasses.saturating_add(other.bypasses);
        self.refreshes = self.refreshes.saturating_add(other.refreshes);
        self.oversized_bypasses = self
            .oversized_bypasses
            .saturating_add(other.oversized_bypasses);
        self.lookup_failures = self.lookup_failures.saturating_add(other.lookup_failures);
        self.store_failures = self.store_failures.saturating_add(other.store_failures);
        self.touch_failures = self.touch_failures.saturating_add(other.touch_failures);
    }

    fn is_empty(self) -> bool {
        self.l1_hits == 0
            && self.l2_hits == 0
            && self.misses == 0
            && self.bypasses == 0
            && self.refreshes == 0
            && self.oversized_bypasses == 0
            && self.lookup_failures == 0
            && self.store_failures == 0
            && self.touch_failures == 0
    }
}

pub enum PersistentLookup {
    Hit(CacheEntry),
    Miss,
    Unavailable,
}

enum CacheCommand {
    Lookup {
        key: CacheKey,
        epoch: u64,
        reply: oneshot::Sender<PersistentLookup>,
    },
    Store {
        store: PersistentStore,
        epoch: u64,
    },
    Touch {
        touch: Option<TouchRecord>,
        stats: StatsDelta,
        epoch: u64,
    },
    Stats {
        reply: oneshot::Sender<Result<CacheStatsView, CacheOperationError>>,
    },
    Clear {
        epoch: u64,
        reply: oneshot::Sender<Result<CacheStatsView, CacheOperationError>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

/// sender 与原子状态可跨线程共享；SQLite Connection 只存在于命名 worker 闭包内。
pub struct PersistentCacheWorker {
    sender: mpsc::Sender<CacheCommand>,
    state: Arc<AtomicPersistentCacheState>,
}

struct AtomicPersistentCacheState(AtomicU8);

impl AtomicPersistentCacheState {
    fn new(state: PersistentCacheState) -> Self {
        Self(AtomicU8::new(state as u8))
    }

    fn load(&self) -> PersistentCacheState {
        match self.0.load(Ordering::Acquire) {
            value if value == PersistentCacheState::Starting as u8 => {
                PersistentCacheState::Starting
            }
            value if value == PersistentCacheState::Ready as u8 => PersistentCacheState::Ready,
            value if value == PersistentCacheState::Degraded as u8 => {
                PersistentCacheState::Degraded
            }
            value if value == PersistentCacheState::Stopped as u8 => PersistentCacheState::Stopped,
            _ => PersistentCacheState::Degraded,
        }
    }

    fn store(&self, state: PersistentCacheState) {
        self.0.store(state as u8, Ordering::Release);
    }
}

impl PersistentCacheWorker {
    pub fn start(data_dir: PathBuf) -> Self {
        Self::start_inner(
            data_dir,
            Duration::ZERO,
            Duration::ZERO,
            TOUCH_FLUSH_INTERVAL,
            PRODUCTION_CAPACITY,
        )
    }

    fn start_inner(
        data_dir: PathBuf,
        init_delay: Duration,
        lookup_delay: Duration,
        touch_flush_interval: Duration,
        capacity: CapacityLimits,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        let state = Arc::new(AtomicPersistentCacheState::new(
            PersistentCacheState::Starting,
        ));
        let worker_state = Arc::clone(&state);
        let spawn = std::thread::Builder::new()
            .name("easyT-cache-db".to_string())
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || {
                worker_main(
                    data_dir,
                    receiver,
                    worker_state,
                    init_delay,
                    lookup_delay,
                    touch_flush_interval,
                    capacity,
                )
            });
        if spawn.is_err() {
            log::warn!("cache_worker_state_changed: from=starting to=degraded reason=thread_spawn");
            state.store(PersistentCacheState::Degraded);
        }
        Self { sender, state }
    }

    #[cfg(test)]
    fn start_with_test_delays(
        data_dir: PathBuf,
        init_delay: Duration,
        lookup_delay: Duration,
    ) -> Self {
        Self::start_inner(
            data_dir,
            init_delay,
            lookup_delay,
            TOUCH_FLUSH_INTERVAL,
            PRODUCTION_CAPACITY,
        )
    }

    #[cfg(test)]
    fn start_with_test_touch_interval(data_dir: PathBuf, interval: Duration) -> Self {
        Self::start_inner(
            data_dir,
            Duration::ZERO,
            Duration::ZERO,
            interval,
            PRODUCTION_CAPACITY,
        )
    }

    #[cfg(test)]
    fn start_with_test_capacity(data_dir: PathBuf, capacity: CapacityLimits) -> Self {
        Self::start_inner(
            data_dir,
            Duration::ZERO,
            Duration::ZERO,
            TOUCH_FLUSH_INTERVAL,
            capacity,
        )
    }

    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        Self {
            sender,
            state: Arc::new(AtomicPersistentCacheState::new(
                PersistentCacheState::Degraded,
            )),
        }
    }

    pub fn state(&self) -> PersistentCacheState {
        self.state.load()
    }

    pub async fn lookup(&self, key: CacheKey, epoch: u64) -> PersistentLookup {
        if self.state() != PersistentCacheState::Ready {
            return PersistentLookup::Unavailable;
        }
        let (reply, receiver) = oneshot::channel();
        if self
            .sender
            .try_send(CacheCommand::Lookup { key, epoch, reply })
            .is_err()
        {
            let _ = self.try_record_stats(StatsDelta::lookup_failure(), epoch);
            return PersistentLookup::Unavailable;
        }
        match tokio::time::timeout(LOOKUP_BUDGET, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) | Err(_) => {
                let _ = self.try_record_stats(StatsDelta::lookup_failure(), epoch);
                PersistentLookup::Unavailable
            }
        }
    }

    pub fn try_store(&self, store: PersistentStore, epoch: u64) -> bool {
        if self.state() != PersistentCacheState::Ready {
            return false;
        }
        self.sender
            .try_send(CacheCommand::Store { store, epoch })
            .is_ok()
    }

    pub(super) fn try_touch(&self, touch: TouchRecord, stats: StatsDelta, epoch: u64) -> bool {
        self.try_activity(Some(touch), stats, epoch)
    }

    pub(super) fn try_record_stats(&self, stats: StatsDelta, epoch: u64) -> bool {
        self.try_activity(None, stats, epoch)
    }

    pub(super) fn try_activity(
        &self,
        touch: Option<TouchRecord>,
        stats: StatsDelta,
        epoch: u64,
    ) -> bool {
        if self.state() != PersistentCacheState::Ready {
            return false;
        }
        self.sender
            .try_send(CacheCommand::Touch {
                touch,
                stats,
                epoch,
            })
            .is_ok()
    }

    pub async fn stats(&self) -> Result<CacheStatsView, CacheOperationError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(CacheCommand::Stats { reply })
            .await
            .map_err(|_| CacheOperationError::Unavailable)?;
        receiver
            .await
            .map_err(|_| CacheOperationError::Unavailable)?
    }

    pub async fn clear(&self, epoch: u64) -> Result<CacheStatsView, CacheOperationError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(CacheCommand::Clear { epoch, reply })
            .await
            .map_err(|_| CacheOperationError::Unavailable)?;
        receiver
            .await
            .map_err(|_| CacheOperationError::Unavailable)?
    }

    pub async fn shutdown(&self) {
        if self.state() == PersistentCacheState::Stopped {
            return;
        }
        let shutdown = async {
            let (reply, receiver) = oneshot::channel();
            self.sender
                .send(CacheCommand::Shutdown { reply })
                .await
                .map_err(|_| ())?;
            receiver.await.map_err(|_| ())
        };
        if tokio::time::timeout(SHUTDOWN_BUDGET, shutdown)
            .await
            .is_err()
        {
            log::warn!("cache_shutdown_timeout");
        } else if self.sender.is_closed() {
            self.state.store(PersistentCacheState::Stopped);
        }
    }

    #[cfg(test)]
    pub(crate) async fn wait_until_ready(&self) {
        let wait = async {
            loop {
                match self.state() {
                    PersistentCacheState::Ready => return,
                    PersistentCacheState::Degraded | PersistentCacheState::Stopped => {
                        panic!("persistent cache did not become ready")
                    }
                    PersistentCacheState::Starting => tokio::task::yield_now().await,
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(2), wait)
            .await
            .expect("persistent cache initialization timed out");
    }
}

fn worker_main(
    data_dir: PathBuf,
    mut receiver: mpsc::Receiver<CacheCommand>,
    state: Arc<AtomicPersistentCacheState>,
    init_delay: Duration,
    lookup_delay: Duration,
    touch_flush_interval: Duration,
    capacity: CapacityLimits,
) {
    if !init_delay.is_zero() {
        std::thread::sleep(init_delay);
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => {
            log::warn!("cache_worker_state_changed: from=starting to=degraded reason=runtime");
            state.store(PersistentCacheState::Degraded);
            return;
        }
    };
    let mut connection = match open_connection_with_recovery(&data_dir) {
        Ok(connection) => {
            state.store(PersistentCacheState::Ready);
            Some(connection)
        }
        Err(error) => {
            log::warn!(
                "cache_worker_state_changed: from=starting to=degraded reason={}",
                sqlite_error_kind(&error).label()
            );
            state.store(PersistentCacheState::Degraded);
            None
        }
    };
    let mut current_epoch = 0u64;
    let mut pending_touches = BTreeMap::new();
    let mut pending_stats = StatsDelta::default();
    let mut pending_since: Option<std::time::Instant> = None;

    runtime.block_on(async {
        loop {
            let command = if let Some(since) = pending_since {
                let remaining = touch_flush_interval.saturating_sub(since.elapsed());
                match tokio::time::timeout(remaining, receiver.recv()).await {
                    Ok(command) => command,
                    Err(_) => {
                        if let Some(active_connection) = connection.as_mut() {
                            if let Some(kind) = flush_pending_or_record_touch_failure(
                                active_connection,
                                &mut pending_touches,
                                &mut pending_stats,
                            ) {
                                pending_since = Some(std::time::Instant::now());
                                if kind.recovers_database() {
                                    recover_connection(&data_dir, &mut connection, &state, kind);
                                }
                            } else {
                                pending_since = None;
                            }
                        }
                        continue;
                    }
                }
            } else {
                receiver.recv().await
            };
            let Some(command) = command else {
                break;
            };
            match command {
                CacheCommand::Lookup { key, epoch, reply } => {
                    if !lookup_delay.is_zero() {
                        std::thread::sleep(lookup_delay);
                    }
                    let result = if epoch != current_epoch {
                        PersistentLookup::Unavailable
                    } else if let Some(active_connection) = connection.as_ref() {
                        match lookup_entry(active_connection, key) {
                            Ok(result) => result,
                            Err(error) => {
                                log::warn!(
                                    "cache_lookup_failed: reason={}",
                                    sqlite_error_kind(&error).label()
                                );
                                pending_stats.merge(StatsDelta::lookup_failure());
                                let kind = sqlite_error_kind(&error);
                                if kind.recovers_database() {
                                    recover_connection(&data_dir, &mut connection, &state, kind);
                                }
                                PersistentLookup::Unavailable
                            }
                        }
                    } else {
                        PersistentLookup::Unavailable
                    };
                    let _ = reply.send(result);
                }
                CacheCommand::Store { store, epoch } => {
                    if epoch == current_epoch {
                        if let Some(active_connection) = connection.as_mut() {
                            match upsert_entry(active_connection, &store).and_then(|()| {
                                enforce_capacity(
                                    active_connection,
                                    &mut pending_touches,
                                    &mut pending_stats,
                                    capacity,
                                )?;
                                checkpoint_if_wal_large(
                                    active_connection,
                                    &database_path(&data_dir),
                                )
                            }) {
                                Ok(()) => {
                                    if pending_touches.is_empty() && pending_stats.is_empty() {
                                        pending_since = None;
                                    }
                                }
                                Err(error) => {
                                    log::warn!(
                                        "cache_store_failed: reason={} logical_bytes={}",
                                        sqlite_error_kind(&error).label(),
                                        store.entry.logical_size_bytes
                                    );
                                    let kind = sqlite_error_kind(&error);
                                    if kind.recovers_database() {
                                        recover_connection(
                                            &data_dir,
                                            &mut connection,
                                            &state,
                                            kind,
                                        );
                                    }
                                    pending_stats.merge(StatsDelta {
                                        store_failures: 1,
                                        ..StatsDelta::default()
                                    });
                                }
                            }
                        }
                    }
                }
                CacheCommand::Touch {
                    touch,
                    stats,
                    epoch,
                } => {
                    if epoch == current_epoch {
                        if pending_touches.is_empty() && pending_stats.is_empty() {
                            pending_since = Some(std::time::Instant::now());
                        }
                        if let Some(touch) = touch {
                            merge_touch(&mut pending_touches, touch);
                        }
                        pending_stats.merge(stats);
                        if pending_touches.len() >= TOUCH_BATCH_KEYS {
                            if let Some(active_connection) = connection.as_mut() {
                                if let Some(kind) = flush_pending_or_record_touch_failure(
                                    active_connection,
                                    &mut pending_touches,
                                    &mut pending_stats,
                                ) {
                                    pending_since = Some(std::time::Instant::now());
                                    if kind.recovers_database() {
                                        recover_connection(
                                            &data_dir,
                                            &mut connection,
                                            &state,
                                            kind,
                                        );
                                    }
                                } else {
                                    pending_since = None;
                                }
                            }
                        }
                    }
                }
                CacheCommand::Stats { reply } => {
                    let result = if let Some(active_connection) = connection.as_mut() {
                        match flush_pending(
                            active_connection,
                            &mut pending_touches,
                            &mut pending_stats,
                        )
                        .and_then(|()| {
                            read_stats_view(
                                active_connection,
                                state.load(),
                                &database_path(&data_dir),
                            )
                        }) {
                            Ok(stats) => Ok(stats),
                            Err(error) => {
                                log::warn!(
                                    "cache_stats_failed: reason={}",
                                    sqlite_error_kind(&error).label()
                                );
                                let kind = sqlite_error_kind(&error);
                                if kind.recovers_database() {
                                    recover_connection(&data_dir, &mut connection, &state, kind);
                                }
                                Err(CacheOperationError::Unavailable)
                            }
                        }
                    } else {
                        Ok(unavailable_stats_view(
                            state.load(),
                            &database_path(&data_dir),
                        ))
                    };
                    pending_since = None;
                    let _ = reply.send(result);
                }
                CacheCommand::Clear { epoch, reply } => {
                    if epoch < current_epoch {
                        let result = connection
                            .as_ref()
                            .map(|connection| {
                                read_stats_view(connection, state.load(), &database_path(&data_dir))
                            })
                            .unwrap_or_else(|| {
                                Ok(unavailable_stats_view(
                                    state.load(),
                                    &database_path(&data_dir),
                                ))
                            })
                            .map_err(|_| CacheOperationError::Unavailable);
                        let _ = reply.send(result);
                        continue;
                    }
                    current_epoch = epoch;
                    pending_touches.clear();
                    pending_stats = StatsDelta::default();
                    pending_since = None;
                    connection.take();
                    let result = rebuild_empty_database(&data_dir).and_then(|new_connection| {
                        let path = database_path(&data_dir);
                        new_connection.execute(
                            "UPDATE cache_stats SET last_cleared_at_ms = ?1 WHERE id = 1",
                            [now_ms()],
                        )?;
                        let stats =
                            read_stats_view(&new_connection, PersistentCacheState::Ready, &path)?;
                        connection = Some(new_connection);
                        state.store(PersistentCacheState::Ready);
                        Ok(stats)
                    });
                    if result.is_err() {
                        log::warn!("cache_clear_failed: reason=sqlite");
                        state.store(PersistentCacheState::Degraded);
                    }
                    let _ = reply.send(result.map_err(|_| CacheOperationError::Unavailable));
                }
                CacheCommand::Shutdown { reply } => {
                    if let Some(connection) = connection.as_mut() {
                        flush_pending_or_record_touch_failure(
                            connection,
                            &mut pending_touches,
                            &mut pending_stats,
                        );
                        if passive_checkpoint(connection).is_err() {
                            log::warn!("cache_shutdown_checkpoint_failed: reason=sqlite");
                        }
                    }
                    connection.take();
                    let _ = reply.send(());
                    break;
                }
            }
        }
        if let Some(connection) = connection.as_mut() {
            flush_pending_or_record_touch_failure(
                connection,
                &mut pending_touches,
                &mut pending_stats,
            );
        }
    });
    connection.take();
    state.store(PersistentCacheState::Stopped);
}

fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CACHE_DIR_NAME).join(DATABASE_FILE_NAME)
}

fn rebuild_empty_database(data_dir: &Path) -> rusqlite::Result<Connection> {
    remove_cache_database_family(data_dir)?;
    open_connection(data_dir)
}

fn open_connection_with_recovery(data_dir: &Path) -> rusqlite::Result<Connection> {
    match open_connection(data_dir) {
        Ok(connection) => Ok(connection),
        Err(error) if sqlite_error_kind(&error).recovers_on_open() => {
            let reason = sqlite_error_kind(&error).label();
            quarantine_database_family(data_dir)?;
            let connection = open_connection(data_dir)?;
            log::warn!("cache_rebuilt: reason={} quarantine_created=true", reason);
            Ok(connection)
        }
        Err(error) => Err(error),
    }
}

fn recover_connection(
    data_dir: &Path,
    connection: &mut Option<Connection>,
    state: &AtomicPersistentCacheState,
    reason: SqliteErrorKind,
) {
    connection.take();
    let rebuilt = if reason.recovers_database() {
        quarantine_database_family(data_dir).and_then(|()| open_connection(data_dir))
    } else {
        open_connection_with_recovery(data_dir)
    };
    match rebuilt {
        Ok(recovered) => {
            *connection = Some(recovered);
            state.store(PersistentCacheState::Ready);
            log::warn!(
                "cache_rebuilt: reason={} quarantine_created=true",
                reason.label()
            );
        }
        Err(error) => {
            log::warn!(
                "cache_worker_state_changed: from=ready to=degraded reason={}",
                sqlite_error_kind(&error).label()
            );
            state.store(PersistentCacheState::Degraded);
        }
    }
    log::warn!("cache_recovery_attempted: reason={}", reason.label());
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SqliteErrorKind {
    Busy,
    Locked,
    Io,
    Corrupt,
    NotADatabase,
    Permission,
    ReadOnly,
    DiskFull,
    CannotOpen,
    Schema,
    Other,
}

impl SqliteErrorKind {
    fn recovers_database(self) -> bool {
        matches!(self, Self::Corrupt | Self::NotADatabase | Self::Schema)
    }

    fn recovers_on_open(self) -> bool {
        self.recovers_database() || matches!(self, Self::Other)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::Locked => "locked",
            Self::Io => "io",
            Self::Corrupt => "corrupt",
            Self::NotADatabase => "not_a_database",
            Self::Permission => "permission",
            Self::ReadOnly => "read_only",
            Self::DiskFull => "disk_full",
            Self::CannotOpen => "cannot_open",
            Self::Schema => "schema",
            Self::Other => "sqlite",
        }
    }
}

fn sqlite_error_kind(error: &rusqlite::Error) -> SqliteErrorKind {
    match error {
        rusqlite::Error::SqliteFailure(sqlite_error, _) => match sqlite_error.code {
            rusqlite::ffi::ErrorCode::DatabaseBusy => SqliteErrorKind::Busy,
            rusqlite::ffi::ErrorCode::DatabaseLocked => SqliteErrorKind::Locked,
            rusqlite::ffi::ErrorCode::SystemIoFailure => SqliteErrorKind::Io,
            rusqlite::ffi::ErrorCode::DatabaseCorrupt => SqliteErrorKind::Corrupt,
            rusqlite::ffi::ErrorCode::NotADatabase => SqliteErrorKind::NotADatabase,
            rusqlite::ffi::ErrorCode::PermissionDenied => SqliteErrorKind::Permission,
            rusqlite::ffi::ErrorCode::ReadOnly => SqliteErrorKind::ReadOnly,
            rusqlite::ffi::ErrorCode::DiskFull => SqliteErrorKind::DiskFull,
            rusqlite::ffi::ErrorCode::CannotOpen => SqliteErrorKind::CannotOpen,
            _ => SqliteErrorKind::Other,
        },
        rusqlite::Error::InvalidQuery => SqliteErrorKind::Schema,
        _ => SqliteErrorKind::Other,
    }
}

fn remove_cache_database_family(data_dir: &Path) -> rusqlite::Result<()> {
    let cache_dir = validated_cache_dir(data_dir)?;

    for suffix in DATABASE_FAMILY_SUFFIXES {
        remove_validated_cache_file(&cache_dir, &format!("{DATABASE_FILE_NAME}{suffix}"))?;
    }
    remove_quarantine_database_families(&cache_dir)?;
    Ok(())
}

fn validated_cache_dir(data_dir: &Path) -> rusqlite::Result<PathBuf> {
    std::fs::create_dir_all(data_dir)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let data_dir = std::fs::canonicalize(data_dir)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let expected_cache_dir = data_dir.join(CACHE_DIR_NAME);
    std::fs::create_dir_all(&expected_cache_dir)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let cache_dir = std::fs::canonicalize(&expected_cache_dir)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    if cache_dir != expected_cache_dir {
        return Err(rusqlite::Error::InvalidPath(cache_dir));
    }
    Ok(cache_dir)
}

fn quarantine_database_family(data_dir: &Path) -> rusqlite::Result<()> {
    let cache_dir = validated_cache_dir(data_dir)?;
    remove_quarantine_database_families(&cache_dir)?;
    let quarantine = format!("translation_cache.corrupt-{}.sqlite3", now_ms());
    for suffix in DATABASE_FAMILY_SUFFIXES {
        let source = cache_dir.join(format!("{DATABASE_FILE_NAME}{suffix}"));
        let target = cache_dir.join(format!("{quarantine}{suffix}"));
        if source.parent() != Some(&cache_dir) || target.parent() != Some(&cache_dir) {
            return Err(rusqlite::Error::InvalidPath(source));
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&source) {
            if metadata.file_type().is_symlink() {
                return Err(rusqlite::Error::InvalidPath(source));
            }
        }
        match std::fs::rename(&source, &target) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(error))),
        }
    }
    Ok(())
}

fn remove_quarantine_database_families(cache_dir: &Path) -> rusqlite::Result<()> {
    for quarantine in quarantine_database_names(cache_dir)? {
        for suffix in DATABASE_FAMILY_SUFFIXES {
            remove_validated_cache_file(cache_dir, &format!("{quarantine}{suffix}"))?;
        }
    }
    Ok(())
}

fn quarantine_database_names(cache_dir: &Path) -> rusqlite::Result<Vec<String>> {
    let entries = std::fs::read_dir(cache_dir)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(_) = name
            .strip_prefix("translation_cache.corrupt-")
            .and_then(|value| value.strip_suffix(".sqlite3"))
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        names.push(name);
    }
    Ok(names)
}

fn remove_validated_cache_file(cache_dir: &Path, file_name: &str) -> rusqlite::Result<()> {
    let path = cache_dir.join(file_name);
    if path.parent() != Some(cache_dir) {
        return Err(rusqlite::Error::InvalidPath(path));
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            return Err(rusqlite::Error::InvalidPath(path));
        }
    }
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(rusqlite::Error::ToSqlConversionFailure(Box::new(error))),
    }
}

fn open_connection(data_dir: &Path) -> rusqlite::Result<Connection> {
    let path = database_path(data_dir);
    let is_new = !path.exists();
    std::fs::create_dir_all(path.parent().expect("cache database has parent"))
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let connection = Connection::open(path)?;
    if is_new {
        connection.execute_batch(
            "PRAGMA page_size = 4096;
             PRAGMA auto_vacuum = INCREMENTAL;",
        )?;
    }
    connection.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -2048;
         PRAGMA busy_timeout = 2000;
         PRAGMA wal_autocheckpoint = 1000;",
    )?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 0 && version != 1 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS cache_entries (
            cache_key BLOB PRIMARY KEY NOT NULL CHECK(length(cache_key) = 32),
            cache_key_version INTEGER NOT NULL,
            prompt_version INTEGER NOT NULL,
            target_language TEXT NOT NULL,
            translated_text TEXT NOT NULL,
            source_backend TEXT NOT NULL,
            source_provider TEXT NOT NULL,
            source_model TEXT NOT NULL,
            generated_at_ms INTEGER NOT NULL,
            last_accessed_at_ms INTEGER NOT NULL,
            hit_count INTEGER NOT NULL DEFAULT 0 CHECK(hit_count >= 0),
            source_text_bytes INTEGER NOT NULL CHECK(source_text_bytes >= 0),
            translated_text_bytes INTEGER NOT NULL CHECK(translated_text_bytes >= 0),
            logical_size_bytes INTEGER NOT NULL CHECK(logical_size_bytes >= 0)
         ) WITHOUT ROWID;
         CREATE INDEX IF NOT EXISTS idx_cache_entries_lru
           ON cache_entries(last_accessed_at_ms, generated_at_ms, cache_key);
         CREATE INDEX IF NOT EXISTS idx_cache_entries_prompt_version
           ON cache_entries(prompt_version);
         CREATE TABLE IF NOT EXISTS cache_stats (
            id INTEGER PRIMARY KEY CHECK(id = 1),
            l1_hits INTEGER NOT NULL DEFAULT 0,
            l2_hits INTEGER NOT NULL DEFAULT 0,
            misses INTEGER NOT NULL DEFAULT 0,
            bypasses INTEGER NOT NULL DEFAULT 0,
            refreshes INTEGER NOT NULL DEFAULT 0,
            oversized_bypasses INTEGER NOT NULL DEFAULT 0,
            lookup_failures INTEGER NOT NULL DEFAULT 0,
            store_failures INTEGER NOT NULL DEFAULT 0,
            touch_failures INTEGER NOT NULL DEFAULT 0,
            last_cleared_at_ms INTEGER
         );
         INSERT OR IGNORE INTO cache_stats(id) VALUES (1);
         PRAGMA user_version = 1;",
    )?;
    Ok(connection)
}

fn upsert_entry(connection: &Connection, store: &PersistentStore) -> rusqlite::Result<()> {
    let entry = &store.entry;
    connection.execute(
        "INSERT INTO cache_entries (
            cache_key, cache_key_version, prompt_version, target_language,
            translated_text, source_backend, source_provider, source_model,
            generated_at_ms, last_accessed_at_ms, hit_count, source_text_bytes,
            translated_text_bytes, logical_size_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
         ON CONFLICT(cache_key) DO UPDATE SET
            cache_key_version=excluded.cache_key_version,
            prompt_version=excluded.prompt_version,
            target_language=excluded.target_language,
            translated_text=excluded.translated_text,
            source_backend=excluded.source_backend,
            source_provider=excluded.source_provider,
            source_model=excluded.source_model,
            generated_at_ms=excluded.generated_at_ms,
            last_accessed_at_ms=excluded.last_accessed_at_ms,
            hit_count=excluded.hit_count,
            source_text_bytes=excluded.source_text_bytes,
            translated_text_bytes=excluded.translated_text_bytes,
            logical_size_bytes=excluded.logical_size_bytes",
        params![
            entry.key.as_bytes().as_slice(),
            i64::from(CACHE_KEY_VERSION),
            i64::from(PROMPT_VERSION),
            store.target_language,
            entry.result.translated_text,
            backend_storage_label(entry.result.source.backend),
            entry.result.source.provider,
            entry.result.source.model,
            entry.generated_at_ms,
            entry.last_accessed_at_ms,
            to_i64(entry.hit_count)?,
            to_i64(entry.source_text_bytes)?,
            to_i64(entry.translated_text_bytes)?,
            to_i64(entry.logical_size_bytes)?,
        ],
    )?;
    Ok(())
}

fn enforce_capacity(
    connection: &mut Connection,
    pending_touches: &mut BTreeMap<CacheKey, TouchRecord>,
    pending_stats: &mut StatsDelta,
    limits: CapacityLimits,
) -> rusqlite::Result<()> {
    let (mut entry_count, mut logical_bytes) = capacity_totals(connection)?;
    if entry_count <= limits.max_entries && logical_bytes <= limits.max_logical_bytes {
        return Ok(());
    }

    flush_pending(connection, pending_touches, pending_stats)?;
    let transaction = connection.transaction()?;
    while entry_count > limits.low_entries || logical_bytes > limits.low_logical_bytes {
        let victims: Vec<(Vec<u8>, u64)> = {
            let mut statement = transaction.prepare(
                "SELECT cache_key, logical_size_bytes
                 FROM cache_entries
                 ORDER BY last_accessed_at_ms ASC, generated_at_ms ASC, cache_key ASC
                 LIMIT ?1",
            )?;
            let rows = statement
                .query_map([limits.delete_batch.max(1) as i64], |row| {
                    let bytes = row.get::<_, i64>(1)?;
                    Ok((row.get::<_, Vec<u8>>(0)?, from_i64(bytes)?))
                })?
                .collect::<Result<_, _>>()?;
            rows
        };
        if victims.is_empty() {
            break;
        }
        {
            let mut delete =
                transaction.prepare("DELETE FROM cache_entries WHERE cache_key = ?1")?;
            for (key, size) in &victims {
                delete.execute([key])?;
                entry_count = entry_count.saturating_sub(1);
                logical_bytes = logical_bytes.saturating_sub(*size);
            }
        }
    }
    transaction.commit()?;
    passive_checkpoint(connection)?;
    Ok(())
}

fn capacity_totals(connection: &Connection) -> rusqlite::Result<(u64, u64)> {
    let (entry_count, logical_bytes): (i64, i64) = connection.query_row(
        "SELECT COUNT(*), COALESCE(SUM(logical_size_bytes), 0) FROM cache_entries",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok((from_i64(entry_count)?, from_i64(logical_bytes)?))
}

fn checkpoint_if_wal_large(connection: &Connection, database: &Path) -> rusqlite::Result<()> {
    if sidecar_path(database, "-wal")
        .metadata()
        .map(|metadata| metadata.len() > WAL_CHECKPOINT_BYTES)
        .unwrap_or(false)
    {
        passive_checkpoint(connection)?;
    }
    Ok(())
}

fn passive_checkpoint(connection: &Connection) -> rusqlite::Result<()> {
    connection.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()))
}

fn lookup_entry(connection: &Connection, key: CacheKey) -> rusqlite::Result<PersistentLookup> {
    let row = connection
        .query_row(
            "SELECT translated_text, source_backend, source_provider, source_model,
                    generated_at_ms, last_accessed_at_ms, hit_count, source_text_bytes,
                    translated_text_bytes, logical_size_bytes
             FROM cache_entries WHERE cache_key = ?1",
            [key.as_bytes().as_slice()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            },
        )
        .optional();
    let row = match row {
        Ok(row) => row,
        Err(error) if is_invalid_row_error(&error) => {
            delete_invalid_entry(connection, key)?;
            return Ok(PersistentLookup::Miss);
        }
        Err(error) => return Err(error),
    };
    let Some((
        text,
        backend,
        provider,
        model,
        generated,
        accessed,
        hits,
        source_bytes,
        text_bytes,
        logical_bytes,
    )) = row
    else {
        return Ok(PersistentLookup::Miss);
    };
    let valid = parse_backend_storage_label(&backend).filter(|_| {
        !text.is_empty()
            && !provider.is_empty()
            && !model.is_empty()
            && generated >= 0
            && accessed >= 0
            && hits >= 0
            && source_bytes >= 0
            && text_bytes >= 0
            && logical_bytes >= 0
    });
    let Some(backend) = valid else {
        delete_invalid_entry(connection, key)?;
        return Ok(PersistentLookup::Miss);
    };
    Ok(PersistentLookup::Hit(CacheEntry {
        key,
        result: Arc::new(BackendResult {
            translated_text: text,
            source: BackendSource {
                backend,
                provider,
                model,
            },
        }),
        generated_at_ms: generated,
        last_accessed_at_ms: accessed,
        hit_count: hits as u64,
        source_text_bytes: source_bytes as u64,
        translated_text_bytes: text_bytes as u64,
        logical_size_bytes: logical_bytes as u64,
        access_tick: 0,
    }))
}

fn is_invalid_row_error(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::FromSqlConversionFailure(..)
            | rusqlite::Error::IntegralValueOutOfRange(..)
            | rusqlite::Error::Utf8Error(..)
            | rusqlite::Error::InvalidColumnType(..)
    )
}

fn delete_invalid_entry(connection: &Connection, key: CacheKey) -> rusqlite::Result<()> {
    connection.execute(
        "DELETE FROM cache_entries WHERE cache_key = ?1",
        [key.as_bytes().as_slice()],
    )?;
    Ok(())
}

fn merge_touch(pending: &mut BTreeMap<CacheKey, TouchRecord>, touch: TouchRecord) {
    pending
        .entry(touch.key)
        .and_modify(|existing| {
            existing.accessed_at_ms = existing.accessed_at_ms.max(touch.accessed_at_ms);
            existing.hit_delta = existing.hit_delta.saturating_add(touch.hit_delta);
        })
        .or_insert(touch);
}

fn flush_pending(
    connection: &mut Connection,
    touches: &mut BTreeMap<CacheKey, TouchRecord>,
    stats: &mut StatsDelta,
) -> rusqlite::Result<()> {
    if touches.is_empty() && stats.is_empty() {
        return Ok(());
    }
    let transaction = connection.transaction()?;
    if !touches.is_empty() {
        let mut statement = transaction.prepare(
            "UPDATE cache_entries
             SET last_accessed_at_ms = MAX(last_accessed_at_ms, ?2),
                 hit_count = hit_count + ?3
             WHERE cache_key = ?1",
        )?;
        for touch in touches.values() {
            statement.execute(params![
                touch.key.as_bytes().as_slice(),
                touch.accessed_at_ms,
                to_i64(touch.hit_delta)?,
            ])?;
        }
    }
    if !stats.is_empty() {
        transaction.execute(
            "UPDATE cache_stats SET
                l1_hits = l1_hits + ?1,
                l2_hits = l2_hits + ?2,
                misses = misses + ?3,
                bypasses = bypasses + ?4,
                refreshes = refreshes + ?5,
                oversized_bypasses = oversized_bypasses + ?6,
                lookup_failures = lookup_failures + ?7,
                store_failures = store_failures + ?8,
                touch_failures = touch_failures + ?9
             WHERE id = 1",
            params![
                to_i64(stats.l1_hits)?,
                to_i64(stats.l2_hits)?,
                to_i64(stats.misses)?,
                to_i64(stats.bypasses)?,
                to_i64(stats.refreshes)?,
                to_i64(stats.oversized_bypasses)?,
                to_i64(stats.lookup_failures)?,
                to_i64(stats.store_failures)?,
                to_i64(stats.touch_failures)?,
            ],
        )?;
    }
    transaction.commit()?;
    touches.clear();
    *stats = StatsDelta::default();
    Ok(())
}

fn flush_pending_or_record_touch_failure(
    connection: &mut Connection,
    touches: &mut BTreeMap<CacheKey, TouchRecord>,
    stats: &mut StatsDelta,
) -> Option<SqliteErrorKind> {
    match flush_pending(connection, touches, stats) {
        Ok(()) => None,
        Err(error) => {
            log::warn!(
                "cache_touch_failed: reason={}",
                sqlite_error_kind(&error).label()
            );
            stats.merge(StatsDelta::touch_failure());
            Some(sqlite_error_kind(&error))
        }
    }
}

fn read_stats_view(
    connection: &Connection,
    state: PersistentCacheState,
    path: &Path,
) -> rusqlite::Result<CacheStatsView> {
    let entry_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM cache_entries", [], |row| row.get(0))?;
    let (l1_hits, l2_hits, misses): (i64, i64, i64) = connection.query_row(
        "SELECT l1_hits, l2_hits, misses FROM cache_stats WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let l1_hits = from_i64(l1_hits)?;
    let l2_hits = from_i64(l2_hits)?;
    let misses = from_i64(misses)?;
    let denominator = l1_hits.saturating_add(l2_hits).saturating_add(misses);
    Ok(CacheStatsView {
        state,
        entry_count: from_i64(entry_count)?,
        disk_bytes: database_family_bytes(path),
        max_disk_bytes: MAX_L2_LOGICAL_BYTES,
        hit_rate: (denominator != 0)
            .then(|| l1_hits.saturating_add(l2_hits) as f64 / denominator as f64),
        cache_path: path.to_string_lossy().into_owned(),
    })
}

fn unavailable_stats_view(state: PersistentCacheState, path: &Path) -> CacheStatsView {
    CacheStatsView {
        state,
        entry_count: 0,
        disk_bytes: database_family_bytes(path),
        max_disk_bytes: MAX_L2_LOGICAL_BYTES,
        hit_rate: None,
        cache_path: path.to_string_lossy().into_owned(),
    }
}

fn database_family_bytes(path: &Path) -> u64 {
    ["", "-wal", "-shm"]
        .into_iter()
        .filter_map(|suffix| {
            let member = if suffix.is_empty() {
                path.to_path_buf()
            } else {
                sidecar_path(path, suffix)
            };
            std::fs::metadata(member).ok()
        })
        .fold(0u64, |total, metadata| total.saturating_add(metadata.len()))
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut file_name = path.as_os_str().to_os_string();
    file_name.push(suffix);
    PathBuf::from(file_name)
}

fn to_i64(value: u64) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn from_i64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use rusqlite::Connection;

    use crate::translation_backend::cache::entry::{CacheEntry, PersistentCacheState};
    use crate::translation_backend::cache::key::CacheKey;
    use crate::translation_backend::cache::test_support::TestDir;
    use crate::translation_backend::models::{BackendMode, BackendResult, BackendSource};

    use super::{
        database_path, CapacityLimits, PersistentCacheWorker, PersistentLookup, PersistentStore,
        StatsDelta, TouchRecord, COMMAND_QUEUE_CAPACITY,
    };

    fn store(seed: u32, translated_text: &str) -> PersistentStore {
        let result = BackendResult {
            translated_text: translated_text.to_string(),
            source: BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "agnes".to_string(),
                model: "agnes-2.0-flash".to_string(),
            },
        };
        PersistentStore {
            target_language: "简体中文".to_string(),
            entry: CacheEntry {
                key: CacheKey::from_seed(seed),
                result: Arc::new(result),
                generated_at_ms: 1_700_000_000_000,
                last_accessed_at_ms: 1_700_000_000_000,
                hit_count: 0,
                source_text_bytes: 5,
                translated_text_bytes: translated_text.len() as u64,
                logical_size_bytes: 512,
                access_tick: 0,
            },
        }
    }

    fn sized_store(
        seed: u32,
        logical_size_bytes: u64,
        generated_at_ms: i64,
        last_accessed_at_ms: i64,
    ) -> PersistentStore {
        let mut item = store(seed, "译文");
        item.entry.logical_size_bytes = logical_size_bytes;
        item.entry.generated_at_ms = generated_at_ms;
        item.entry.last_accessed_at_ms = last_accessed_at_ms;
        item
    }

    #[tokio::test]
    async fn stored_translation_is_available_after_worker_restart() {
        let dir = TestDir::new("worker");
        let key = CacheKey::from_seed(7);

        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(worker.try_store(store(7, "持久译文"), 0));
        worker.shutdown().await;

        let reopened = PersistentCacheWorker::start(dir.0.clone());
        reopened.wait_until_ready().await;
        let lookup = reopened.lookup(key, 0).await;
        let PersistentLookup::Hit(entry) = lookup else {
            panic!("reopened worker should return the stored translation");
        };
        assert_eq!(entry.result.translated_text, "持久译文");
        reopened.shutdown().await;
    }

    #[tokio::test]
    async fn corrupt_database_is_quarantined_and_recreated_on_start() {
        let dir = TestDir::new("corrupt-start");
        let path = database_path(&dir.0);
        std::fs::create_dir_all(path.parent().expect("cache database has a parent"))
            .expect("cache directory should be created");
        std::fs::write(&path, b"not a sqlite database").expect("corrupt database should be seeded");
        std::fs::write(
            path.parent()
                .expect("cache database has a parent")
                .join("translation_cache.corrupt-1.sqlite3"),
            b"older quarantine",
        )
        .expect("older quarantine should be seeded");

        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert_eq!(
            worker
                .stats()
                .await
                .expect("recreated database should be usable")
                .entry_count,
            0
        );
        worker.shutdown().await;

        let quarantines: Vec<_> = std::fs::read_dir(dir.0.join("cache"))
            .expect("cache directory should be readable")
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| {
                name.starts_with("translation_cache.corrupt-") && name.ends_with(".sqlite3")
            })
            .collect();
        assert_eq!(
            quarantines.len(),
            1,
            "the corrupt database must be retained for recovery"
        );
    }

    #[tokio::test]
    async fn newer_schema_version_is_quarantined_and_recreated_on_start() {
        let dir = TestDir::new("newer-schema-start");
        let path = database_path(&dir.0);
        std::fs::create_dir_all(path.parent().expect("cache database has a parent"))
            .expect("cache directory should be created");
        let connection = Connection::open(&path).expect("database should be created");
        connection
            .execute_batch("PRAGMA user_version = 2;")
            .expect("newer schema version should be seeded");
        drop(connection);

        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert_eq!(
            worker
                .stats()
                .await
                .expect("recreated database should be usable")
                .entry_count,
            0
        );
        worker.shutdown().await;

        assert!(std::fs::read_dir(dir.0.join("cache"))
            .expect("cache directory should be readable")
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .any(
                |name| name.starts_with("translation_cache.corrupt-") && name.ends_with(".sqlite3")
            ));
    }

    #[tokio::test]
    async fn invalid_single_row_is_missed_and_removed_without_rebuilding_database() {
        let dir = TestDir::new("invalid-row");
        let key = CacheKey::from_seed(29);
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(worker.try_store(store(29, "有效译文"), 0));
        worker.shutdown().await;

        let connection = Connection::open(database_path(&dir.0)).expect("database should open");
        connection
            .execute(
                "UPDATE cache_entries SET source_backend = 'invalid-backend' WHERE cache_key = ?1",
                [key.as_bytes().as_slice()],
            )
            .expect("invalid row should be seeded");
        drop(connection);

        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(matches!(
            worker.lookup(key, 0).await,
            PersistentLookup::Miss
        ));
        let stats = worker
            .stats()
            .await
            .expect("worker should remain ready after invalid row");
        assert_eq!(stats.state, PersistentCacheState::Ready);
        assert_eq!(
            stats.entry_count, 0,
            "only the invalid row should be deleted"
        );
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn undecodable_single_row_is_missed_and_removed_without_rebuilding_database() {
        let dir = TestDir::new("undecodable-row");
        let key = CacheKey::from_seed(30);
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(worker.try_store(store(30, "有效译文"), 0));
        worker.shutdown().await;

        let connection = Connection::open(database_path(&dir.0)).expect("database should open");
        connection
            .execute(
                "UPDATE cache_entries SET source_backend = x'FF' WHERE cache_key = ?1",
                [key.as_bytes().as_slice()],
            )
            .expect("undecodable row should be seeded");
        drop(connection);

        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(matches!(
            worker.lookup(key, 0).await,
            PersistentLookup::Miss
        ));
        assert_eq!(
            worker
                .stats()
                .await
                .expect("worker should remain ready after undecodable row")
                .entry_count,
            0
        );
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn merged_touches_flush_on_shutdown_and_survive_restart() {
        let dir = TestDir::new("touch-shutdown");
        let key = CacheKey::from_seed(17);
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(worker.try_store(store(17, "持久译文"), 0));
        assert!(worker.try_touch(
            TouchRecord {
                key,
                accessed_at_ms: 1_800_000_000_000,
                hit_delta: 1,
            },
            StatsDelta::default(),
            0,
        ));
        assert!(worker.try_touch(
            TouchRecord {
                key,
                accessed_at_ms: 1_800_000_000_123,
                hit_delta: 2,
            },
            StatsDelta::default(),
            0,
        ));
        worker.shutdown().await;

        let reopened = PersistentCacheWorker::start(dir.0.clone());
        reopened.wait_until_ready().await;
        let PersistentLookup::Hit(entry) = reopened.lookup(key, 0).await else {
            panic!("touched entry should survive restart");
        };
        assert_eq!(entry.hit_count, 3);
        assert_eq!(entry.last_accessed_at_ms, 1_800_000_000_123);
        reopened.shutdown().await;
    }

    #[tokio::test]
    async fn touches_flush_after_256_distinct_keys() {
        let dir = TestDir::new("touch-count");
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        for seed in 0..256 {
            assert!(worker.try_store(store(seed, "译文"), 0));
        }
        for seed in 0..256 {
            assert!(worker.try_touch(
                TouchRecord {
                    key: CacheKey::from_seed(seed),
                    accessed_at_ms: 1_800_000_000_000 + i64::from(seed),
                    hit_delta: 1,
                },
                StatsDelta::default(),
                0,
            ));
        }

        let entry = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let PersistentLookup::Hit(entry) = worker.lookup(CacheKey::from_seed(0), 0).await
                {
                    break entry;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("worker should drain the queued stores and touches");
        assert_eq!(entry.hit_count, 1, "256 distinct keys must trigger flush");
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn touches_flush_after_the_configured_interval() {
        let dir = TestDir::new("touch-time");
        let key = CacheKey::from_seed(23);
        let worker = PersistentCacheWorker::start_with_test_touch_interval(
            dir.0.clone(),
            Duration::from_millis(30),
        );
        worker.wait_until_ready().await;
        assert!(worker.try_store(store(23, "译文"), 0));
        assert!(worker.try_touch(
            TouchRecord {
                key,
                accessed_at_ms: 1_800_000_000_000,
                hit_delta: 1,
            },
            StatsDelta::default(),
            0,
        ));
        tokio::time::sleep(Duration::from_millis(80)).await;

        let PersistentLookup::Hit(entry) = worker.lookup(key, 0).await else {
            panic!("stored entry should exist");
        };
        assert_eq!(entry.hit_count, 1, "elapsed interval must trigger flush");
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn eviction_reaches_both_low_watermarks_in_stable_lru_order() {
        let dir = TestDir::new("capacity");
        let limits = CapacityLimits {
            max_logical_bytes: 400,
            max_entries: 4,
            low_logical_bytes: 200,
            low_entries: 2,
            delete_batch: 1,
        };
        let worker = PersistentCacheWorker::start_with_test_capacity(dir.0.clone(), limits);
        worker.wait_until_ready().await;
        assert!(worker.try_store(sized_store(1, 100, 10, 10), 0));
        assert!(worker.try_store(sized_store(2, 100, 20, 10), 0));
        assert!(worker.try_store(sized_store(3, 100, 30, 20), 0));
        assert!(worker.try_store(sized_store(4, 100, 40, 30), 0));
        assert!(worker.try_store(sized_store(5, 100, 50, 40), 0));

        let stats = worker.stats().await.expect("stats should be available");
        assert_eq!(stats.entry_count, 2);
        for seed in 1..=3 {
            assert!(matches!(
                worker.lookup(CacheKey::from_seed(seed), 0).await,
                PersistentLookup::Miss
            ));
        }
        for seed in 4..=5 {
            assert!(matches!(
                worker.lookup(CacheKey::from_seed(seed), 0).await,
                PersistentLookup::Hit(_)
            ));
        }
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn pending_touch_is_flushed_before_eviction() {
        let dir = TestDir::new("capacity-touch");
        let limits = CapacityLimits {
            max_logical_bytes: 300,
            max_entries: 3,
            low_logical_bytes: 200,
            low_entries: 2,
            delete_batch: 1,
        };
        let worker = PersistentCacheWorker::start_with_test_capacity(dir.0.clone(), limits);
        worker.wait_until_ready().await;
        assert!(worker.try_store(sized_store(1, 100, 10, 10), 0));
        assert!(worker.try_store(sized_store(2, 100, 20, 20), 0));
        assert!(worker.try_store(sized_store(3, 100, 30, 30), 0));
        assert!(worker.try_touch(
            TouchRecord {
                key: CacheKey::from_seed(1),
                accessed_at_ms: 100,
                hit_delta: 1,
            },
            StatsDelta::default(),
            0,
        ));
        assert!(worker.try_store(sized_store(4, 100, 40, 40), 0));

        assert!(matches!(
            worker.lookup(CacheKey::from_seed(1), 0).await,
            PersistentLookup::Hit(_)
        ));
        assert!(matches!(
            worker.lookup(CacheKey::from_seed(4), 0).await,
            PersistentLookup::Hit(_)
        ));
        for seed in 2..=3 {
            assert!(matches!(
                worker.lookup(CacheKey::from_seed(seed), 0).await,
                PersistentLookup::Miss
            ));
        }
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn schema_v1_contains_only_approved_columns_and_no_source_text() {
        let dir = TestDir::new("schema");
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        worker.shutdown().await;

        let path = database_path(&dir.0);
        assert_eq!(path, dir.0.join("cache").join("translation_cache.sqlite3"));
        let connection = Connection::open(path).expect("schema database should open");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version should be readable");
        assert_eq!(version, 1);

        let mut statement = connection
            .prepare("PRAGMA table_info(cache_entries)")
            .expect("table info should prepare");
        let columns: BTreeSet<String> = statement
            .query_map([], |row| row.get(1))
            .expect("columns should query")
            .collect::<Result<_, _>>()
            .expect("columns should decode");
        let expected: BTreeSet<String> = [
            "cache_key",
            "cache_key_version",
            "prompt_version",
            "target_language",
            "translated_text",
            "source_backend",
            "source_provider",
            "source_model",
            "generated_at_ms",
            "last_accessed_at_ms",
            "hit_count",
            "source_text_bytes",
            "translated_text_bytes",
            "logical_size_bytes",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(columns, expected);
        assert!(!columns.contains("source_text"));

        let cache_entries_sql: String = connection
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type='table' AND name='cache_entries'",
                [],
                |row| row.get(0),
            )
            .expect("cache_entries schema should exist");
        assert!(cache_entries_sql.contains("WITHOUT ROWID"));
        let indexes: BTreeSet<String> = connection
            .prepare(
                "SELECT name FROM sqlite_schema WHERE type='index' AND tbl_name='cache_entries'",
            )
            .expect("index query should prepare")
            .query_map([], |row| row.get(0))
            .expect("indexes should query")
            .collect::<Result<_, _>>()
            .expect("indexes should decode");
        assert_eq!(
            indexes,
            [
                "idx_cache_entries_lru".to_string(),
                "idx_cache_entries_prompt_version".to_string(),
            ]
            .into_iter()
            .collect()
        );

        let mut stats_statement = connection
            .prepare("PRAGMA table_info(cache_stats)")
            .expect("stats table info should prepare");
        let stats_columns: BTreeSet<String> = stats_statement
            .query_map([], |row| row.get(1))
            .expect("stats columns should query")
            .collect::<Result<_, _>>()
            .expect("stats columns should decode");
        assert_eq!(
            stats_columns,
            [
                "id",
                "l1_hits",
                "l2_hits",
                "misses",
                "bypasses",
                "refreshes",
                "oversized_bypasses",
                "lookup_failures",
                "store_failures",
                "touch_failures",
                "last_cleared_at_ms",
            ]
            .into_iter()
            .map(str::to_string)
            .collect()
        );
    }

    #[tokio::test]
    async fn upsert_replaces_the_complete_result_for_the_same_key() {
        let dir = TestDir::new("upsert");
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(worker.try_store(store(9, "旧译文"), 0));
        assert!(worker.try_store(store(9, "新译文"), 0));
        worker.shutdown().await;

        let reopened = PersistentCacheWorker::start(dir.0.clone());
        reopened.wait_until_ready().await;
        let PersistentLookup::Hit(entry) = reopened.lookup(CacheKey::from_seed(9), 0).await else {
            panic!("upserted row should exist");
        };
        assert_eq!(entry.result.translated_text, "新译文");
        assert_eq!(entry.result.source.provider, "agnes");
        assert_eq!(entry.result.source.model, "agnes-2.0-flash");
        reopened.shutdown().await;
    }

    #[tokio::test]
    async fn starting_and_failed_initialization_are_non_blocking() {
        let dir = TestDir::new("states");
        let delayed = PersistentCacheWorker::start_with_test_delays(
            dir.0.clone(),
            Duration::from_millis(100),
            Duration::ZERO,
        );
        assert_eq!(delayed.state(), PersistentCacheState::Starting);
        let started = Instant::now();
        assert!(matches!(
            delayed.lookup(CacheKey::from_seed(1), 0).await,
            PersistentLookup::Unavailable
        ));
        assert!(started.elapsed() < Duration::from_millis(50));
        delayed.wait_until_ready().await;
        delayed.shutdown().await;

        let invalid_root = dir.0.join("not-a-directory");
        std::fs::create_dir_all(&dir.0).expect("temp root should be created");
        std::fs::write(&invalid_root, b"file").expect("invalid data root should be created");
        let failed = PersistentCacheWorker::start(invalid_root.clone());
        let wait = async {
            while failed.state() == PersistentCacheState::Starting {
                tokio::task::yield_now().await;
            }
        };
        tokio::time::timeout(Duration::from_secs(2), wait)
            .await
            .expect("failed initialization should settle");
        assert_eq!(failed.state(), PersistentCacheState::Degraded);
        assert!(matches!(
            failed.lookup(CacheKey::from_seed(1), 0).await,
            PersistentLookup::Unavailable
        ));
        std::fs::remove_file(&invalid_root)
            .expect("test-only permanent init failure should be repairable");
        let rebuilt = failed
            .clear(1)
            .await
            .expect("an explicit rebuild should recover a degraded cache");
        assert_eq!(rebuilt.state, PersistentCacheState::Ready);
        assert!(failed.try_store(store(8, "recovered"), 1));
        failed.shutdown().await;
    }

    #[tokio::test]
    async fn lookup_timeout_discards_late_worker_reply_within_budget() {
        let dir = TestDir::new("budget");
        let worker = PersistentCacheWorker::start_with_test_delays(
            dir.0.clone(),
            Duration::ZERO,
            Duration::from_millis(150),
        );
        worker.wait_until_ready().await;
        let started = Instant::now();
        let lookup = worker.lookup(CacheKey::from_seed(1), 0).await;
        assert!(matches!(lookup, PersistentLookup::Unavailable));
        assert!(started.elapsed() < Duration::from_millis(100));
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_flushes_and_closes_within_the_lifecycle_budget() {
        let dir = TestDir::new("shutdown-budget");
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;
        assert!(worker.try_store(store(31, "待退出写入"), 0));

        let started = Instant::now();
        worker.shutdown().await;
        assert!(started.elapsed() <= Duration::from_secs(1));
        assert_eq!(worker.state(), PersistentCacheState::Stopped);

        let reopened = PersistentCacheWorker::start(dir.0.clone());
        reopened.wait_until_ready().await;
        assert!(matches!(
            reopened.lookup(CacheKey::from_seed(31), 0).await,
            PersistentLookup::Hit(_)
        ));
        reopened.shutdown().await;
    }

    #[tokio::test]
    async fn full_bounded_queue_rejects_write_behind_without_waiting() {
        let dir = TestDir::new("queue");
        let worker = PersistentCacheWorker::start_with_test_delays(
            dir.0.clone(),
            Duration::ZERO,
            Duration::from_millis(250),
        );
        worker.wait_until_ready().await;

        let _ = worker.lookup(CacheKey::from_seed(1), 0).await;
        let started = Instant::now();
        let mut accepted = 0;
        for seed in 0..600 {
            if !worker.try_store(store(seed, "译文"), 0) {
                break;
            }
            accepted += 1;
        }
        assert_eq!(
            accepted + 1,
            COMMAND_QUEUE_CAPACITY,
            "the timed-out lookup records one failure in the bounded queue"
        );
        assert!(started.elapsed() < Duration::from_millis(100));
        worker.shutdown().await;
    }

    #[tokio::test]
    async fn stale_clear_cannot_move_the_worker_epoch_backwards() {
        let dir = TestDir::new("clear-epoch");
        let worker = PersistentCacheWorker::start(dir.0.clone());
        worker.wait_until_ready().await;

        worker.clear(2).await.expect("newer clear should succeed");
        let stale = worker
            .clear(1)
            .await
            .expect("stale clear should be harmless");
        assert_eq!(stale.entry_count, 0);

        assert!(worker.try_store(store(7, "new epoch"), 2));
        worker.stats().await.expect("store should drain");
        assert!(matches!(
            worker.lookup(CacheKey::from_seed(7), 2).await,
            PersistentLookup::Hit(_)
        ));
        worker.shutdown().await;
    }
}
