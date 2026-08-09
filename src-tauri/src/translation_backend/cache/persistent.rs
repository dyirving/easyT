//! L2 SQLite 持久化缓存 worker。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use tokio::sync::{mpsc, oneshot};

use super::entry::{
    backend_storage_label, parse_backend_storage_label, CacheEntry, PersistentCacheState,
};
use super::key::{CacheKey, CACHE_KEY_VERSION};
use crate::translation_backend::models::{BackendResult, BackendSource};
use crate::translation_backend::prompt::PROMPT_VERSION;

const CACHE_DIR_NAME: &str = "cache";
const DATABASE_FILE_NAME: &str = "translation_cache.sqlite3";
const COMMAND_QUEUE_CAPACITY: usize = 512;
const WORKER_STACK_BYTES: usize = 512 * 1024;
const LOOKUP_BUDGET: Duration = Duration::from_millis(50);
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(1);

pub struct PersistentStore {
    pub entry: CacheEntry,
    pub target_language: String,
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
        Self::start_inner(data_dir, Duration::ZERO, Duration::ZERO)
    }

    fn start_inner(data_dir: PathBuf, init_delay: Duration, lookup_delay: Duration) -> Self {
        let (sender, receiver) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        let state = Arc::new(AtomicPersistentCacheState::new(
            PersistentCacheState::Starting,
        ));
        let worker_state = Arc::clone(&state);
        let spawn = std::thread::Builder::new()
            .name("easyT-cache-db".to_string())
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || worker_main(data_dir, receiver, worker_state, init_delay, lookup_delay));
        if let Err(error) = spawn {
            log::warn!("L2 cache worker failed to start: kind=thread_spawn error={error}");
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
        Self::start_inner(data_dir, init_delay, lookup_delay)
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
            return PersistentLookup::Unavailable;
        }
        match tokio::time::timeout(LOOKUP_BUDGET, receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) | Err(_) => PersistentLookup::Unavailable,
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
) {
    if !init_delay.is_zero() {
        std::thread::sleep(init_delay);
    }
    let mut connection = match open_connection(&data_dir) {
        Ok(connection) => {
            state.store(PersistentCacheState::Ready);
            Some(connection)
        }
        Err(error) => {
            log::warn!(
                "cache_worker_state_changed: from=starting to=degraded reason=init error={error}"
            );
            state.store(PersistentCacheState::Degraded);
            None
        }
    };
    let current_epoch = 0u64;

    while let Some(command) = receiver.blocking_recv() {
        match command {
            CacheCommand::Lookup { key, epoch, reply } => {
                if !lookup_delay.is_zero() {
                    std::thread::sleep(lookup_delay);
                }
                let result = if epoch != current_epoch {
                    PersistentLookup::Unavailable
                } else if let Some(connection) = connection.as_ref() {
                    lookup_entry(connection, key).unwrap_or_else(|error| {
                        log::warn!("cache_lookup_failed: reason=sqlite error={error}");
                        PersistentLookup::Unavailable
                    })
                } else {
                    PersistentLookup::Unavailable
                };
                let _ = reply.send(result);
            }
            CacheCommand::Store { store, epoch } => {
                if epoch == current_epoch {
                    if let Some(connection) = connection.as_ref() {
                        if let Err(error) = upsert_entry(connection, &store) {
                            log::warn!(
                                "cache_store_failed: reason=sqlite logical_bytes={} error={error}",
                                store.entry.logical_size_bytes
                            );
                        }
                    }
                }
            }
            CacheCommand::Shutdown { reply } => {
                connection.take();
                state.store(PersistentCacheState::Stopped);
                let _ = reply.send(());
                return;
            }
        }
    }
    connection.take();
    state.store(PersistentCacheState::Stopped);
}

fn database_path(data_dir: &Path) -> PathBuf {
    data_dir.join(CACHE_DIR_NAME).join(DATABASE_FILE_NAME)
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
        .optional()?;
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
    let Some(backend) = parse_backend_storage_label(&backend) else {
        return Ok(PersistentLookup::Miss);
    };
    if text.is_empty() || hits < 0 || source_bytes < 0 || text_bytes < 0 || logical_bytes < 0 {
        return Ok(PersistentLookup::Miss);
    }
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

fn to_i64(value: u64) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
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
        database_path, PersistentCacheWorker, PersistentLookup, PersistentStore,
        COMMAND_QUEUE_CAPACITY,
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
        let failed = PersistentCacheWorker::start(invalid_root);
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
        assert_eq!(accepted, COMMAND_QUEUE_CAPACITY);
        assert!(started.elapsed() < Duration::from_millis(100));
        worker.shutdown().await;
    }
}
