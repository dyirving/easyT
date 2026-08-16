use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

use super::models::{
    backend_storage_label, parse_backend_storage_label, validate_limit, ClearHistoryResult,
    HistoryCommitEligibility, HistoryCommitOutcome, HistoryEntryDraft, HistoryInitState,
    HistoryLimitResult, HistorySnapshot, HistoryWarning, TranslationHistoryEntry,
    TranslationHistorySummary, MAX_HISTORY_ENTRY_BYTES,
};
use super::HistoryError;

pub const HISTORY_DIR_NAME: &str = "translation_history";
pub const HISTORY_DATABASE_FILE: &str = "history.sqlite";
const SCHEMA_VERSION: i64 = 1;

pub struct HistoryDatabase {
    connection: Connection,
    database_path: PathBuf,
    limit: u8,
    init_state: HistoryInitState,
    init_warning: Option<HistoryWarning>,
}

impl HistoryDatabase {
    pub fn open(data_dir: &Path, limit: u8) -> Result<Self, HistoryError> {
        if !validate_limit(limit) {
            return Err(HistoryError::InvalidLimit);
        }
        let history_dir = data_dir.join(HISTORY_DIR_NAME);
        fs::create_dir_all(&history_dir).map_err(|_| HistoryError::Unavailable)?;
        let database_path = history_dir.join(HISTORY_DATABASE_FILE);
        let existed = database_path.exists();

        let opened = if existed {
            validate_database_header(&database_path).and_then(|_| open_and_validate(&database_path))
        } else {
            open_and_validate(&database_path)
        };
        match opened {
            Ok(connection) => {
                let mut db = Self {
                    connection,
                    database_path,
                    limit,
                    init_state: HistoryInitState::Ready,
                    init_warning: None,
                };
                db.apply_limit(limit)?;
                Ok(db)
            }
            Err(error) if existed => {
                log::warn!(
                    "history_operation_failed: operation=initialize kind=corrupt_or_incompatible path={}",
                    database_path.display()
                );
                quarantine_database_family(&database_path)?;
                let connection = open_and_validate(&database_path).map_err(|_| error)?;
                Ok(Self {
                    connection,
                    database_path,
                    limit,
                    init_state: HistoryInitState::Recovered,
                    init_warning: Some(HistoryWarning::recovered()),
                })
            }
            Err(error) => Err(error),
        }
    }

    pub fn snapshot(&mut self) -> Result<HistorySnapshot, HistoryError> {
        Ok(HistorySnapshot {
            state: self.init_state,
            limit: self.limit,
            summaries: self.list_summaries()?,
            warning: self.init_warning.take(),
        })
    }

    pub fn list_summaries(&self) -> Result<Vec<TranslationHistorySummary>, HistoryError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT entry_id, original_summary, translated_summary, target_language, \
                 source_backend, source_provider, source_model, from_cache, total_elapsed_ms, \
                 completed_at_utc_ms FROM translation_history \
                 ORDER BY completed_at_utc_ms DESC, sequence_id DESC LIMIT ?1",
            )
            .map_err(|_| HistoryError::Unavailable)?;
        let rows = statement
            .query_map([i64::from(self.limit)], summary_from_row)
            .map_err(|_| HistoryError::Unavailable)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| HistoryError::CorruptEntry)
    }

    pub fn get_entry(&self, entry_id: &str) -> Result<TranslationHistoryEntry, HistoryError> {
        validate_entry_id(entry_id)?;
        let result = self
            .connection
            .query_row(
                "SELECT entry_id, original_summary, translated_summary, target_language, \
                 source_backend, source_provider, source_model, from_cache, total_elapsed_ms, \
                 completed_at_utc_ms, original_text, translated_text, logical_size_bytes \
                 FROM translation_history WHERE entry_id = ?1",
                [entry_id],
                |row| {
                    let summary = summary_from_row(row)?;
                    let original_text: String = row.get(10)?;
                    let translated_text: String = row.get(11)?;
                    let stored_size: i64 = row.get(12)?;
                    Ok((summary, original_text, translated_text, stored_size))
                },
            )
            .optional()
            .map_err(|_| HistoryError::Unavailable)?
            .ok_or(HistoryError::NotFound)?;

        let (summary, original_text, translated_text, stored_size) = result;
        let actual_size = logical_size_for_stored(&summary, &original_text, &translated_text);
        if stored_size < 0
            || stored_size as u64 > MAX_HISTORY_ENTRY_BYTES
            || actual_size > MAX_HISTORY_ENTRY_BYTES
            || stored_size as u64 != actual_size
        {
            return Err(HistoryError::EntryTooLarge);
        }
        Ok(TranslationHistoryEntry {
            summary,
            original_text,
            translated_text,
        })
    }

    pub fn commit_entry(
        &mut self,
        draft: HistoryEntryDraft,
        limit: u8,
        eligibility: &HistoryCommitEligibility,
    ) -> Result<HistoryCommitOutcome, HistoryError> {
        if !validate_limit(limit) {
            return Err(HistoryError::InvalidLimit);
        }
        if !eligibility.may_commit() {
            return Err(HistoryError::Cancelled);
        }
        let completed_at = draft.completed_at_utc_ms();
        let total_elapsed = draft.total_elapsed_ms();
        let logical_size = draft.logical_size_bytes();
        if logical_size > MAX_HISTORY_ENTRY_BYTES {
            return Ok(HistoryCommitOutcome::NotSaved {
                warning: HistoryWarning::too_large(),
            });
        }

        let tx = self
            .connection
            .transaction()
            .map_err(|_| HistoryError::Unavailable)?;
        insert_draft(&tx, &draft, total_elapsed, completed_at, logical_size)?;
        let evicted_entry_ids = evict_over_limit(&tx, limit)?;
        if !eligibility.claim_commit() {
            return Err(HistoryError::Cancelled);
        }
        tx.commit().map_err(|_| HistoryError::Unavailable)?;
        // 配置保存成功但即时裁剪失败时，后续每次写入都携带配置中的新上限，
        // 因而会在这里继续尝试收敛，并在成功后更新工作线程状态。
        self.limit = limit;

        Ok(HistoryCommitOutcome::Saved {
            summary: TranslationHistorySummary {
                entry_id: draft.entry_id,
                original_summary: draft.original_summary,
                translated_summary: draft.translated_summary,
                target_language: draft.target_language,
                source_backend: draft.source.backend,
                source_provider: draft.source.provider,
                source_model: draft.source.model,
                from_cache: draft.from_cache,
                total_elapsed_ms: total_elapsed,
                completed_at_utc_ms: completed_at,
            },
            evicted_entry_ids,
        })
    }

    pub fn clear_all(&mut self) -> Result<ClearHistoryResult, HistoryError> {
        let tx = self
            .connection
            .transaction()
            .map_err(|_| HistoryError::Unavailable)?;
        let count = tx
            .execute("DELETE FROM translation_history", [])
            .map_err(|_| HistoryError::Unavailable)? as u64;
        tx.commit().map_err(|_| HistoryError::Unavailable)?;
        if let Err(error) = self
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM;")
        {
            log::warn!(
                "history_maintenance_failed: operation=clear kind=sqlite path={} error_code={:?}",
                self.database_path.display(),
                error.sqlite_error_code()
            );
        }
        Ok(ClearHistoryResult {
            cleared_count: count,
        })
    }

    pub fn apply_limit(&mut self, limit: u8) -> Result<HistoryLimitResult, HistoryError> {
        if !validate_limit(limit) {
            return Err(HistoryError::InvalidLimit);
        }
        let tx = self
            .connection
            .transaction()
            .map_err(|_| HistoryError::Unavailable)?;
        let evicted_entry_ids = evict_over_limit(&tx, limit)?;
        tx.commit().map_err(|_| HistoryError::Unavailable)?;
        self.limit = limit;
        Ok(HistoryLimitResult {
            summaries: self.list_summaries()?,
            evicted_entry_ids,
        })
    }
}

fn validate_database_header(path: &Path) -> Result<(), HistoryError> {
    let mut file = fs::File::open(path).map_err(|_| HistoryError::Unavailable)?;
    let mut header = [0_u8; 16];
    file.read_exact(&mut header)
        .map_err(|_| HistoryError::CorruptDatabase)?;
    if &header != b"SQLite format 3\0" {
        return Err(HistoryError::CorruptDatabase);
    }
    Ok(())
}

fn open_and_validate(path: &Path) -> Result<Connection, HistoryError> {
    let connection = Connection::open(path).map_err(|_| HistoryError::Unavailable)?;
    connection
        .busy_timeout(Duration::from_millis(500))
        .map_err(|_| HistoryError::Unavailable)?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|_| HistoryError::Unavailable)?;
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|_| HistoryError::CorruptDatabase)?;
    if quick_check != "ok" {
        return Err(HistoryError::CorruptDatabase);
    }
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| HistoryError::CorruptDatabase)?;
    match version {
        0 => create_schema(&connection)?,
        SCHEMA_VERSION => validate_schema(&connection)?,
        _ => return Err(HistoryError::UnsupportedSchema),
    }
    Ok(connection)
}

fn create_schema(connection: &Connection) -> Result<(), HistoryError> {
    connection
        .execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE translation_history (
                sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                entry_id TEXT NOT NULL UNIQUE,
                original_text TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                original_summary TEXT NOT NULL,
                translated_summary TEXT NOT NULL,
                target_language TEXT NOT NULL,
                source_backend TEXT NOT NULL,
                source_provider TEXT NOT NULL,
                source_model TEXT NOT NULL,
                from_cache INTEGER NOT NULL CHECK (from_cache IN (0, 1)),
                total_elapsed_ms INTEGER NOT NULL CHECK (total_elapsed_ms >= 0),
                completed_at_utc_ms INTEGER NOT NULL,
                logical_size_bytes INTEGER NOT NULL CHECK (logical_size_bytes >= 0)
             );
             CREATE INDEX history_completed_order
             ON translation_history(completed_at_utc_ms DESC, sequence_id DESC);
             PRAGMA user_version=1;
             COMMIT;",
        )
        .map_err(|_| HistoryError::Unavailable)
}

fn validate_schema(connection: &Connection) -> Result<(), HistoryError> {
    let required = [
        "sequence_id",
        "entry_id",
        "original_text",
        "translated_text",
        "original_summary",
        "translated_summary",
        "target_language",
        "source_backend",
        "source_provider",
        "source_model",
        "from_cache",
        "total_elapsed_ms",
        "completed_at_utc_ms",
        "logical_size_bytes",
    ];
    let mut statement = connection
        .prepare("PRAGMA table_info(translation_history)")
        .map_err(|_| HistoryError::UnsupportedSchema)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|_| HistoryError::UnsupportedSchema)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| HistoryError::UnsupportedSchema)?;
    if columns != required {
        return Err(HistoryError::UnsupportedSchema);
    }
    let invalid_sources: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM translation_history WHERE source_backend NOT IN ('officialApi','webGateway')",
            [],
            |row| row.get(0),
        )
        .map_err(|_| HistoryError::UnsupportedSchema)?;
    if invalid_sources > 0 {
        return Err(HistoryError::UnsupportedSchema);
    }
    Ok(())
}

fn insert_draft(
    tx: &Transaction<'_>,
    draft: &HistoryEntryDraft,
    total_elapsed_ms: u64,
    completed_at_utc_ms: i64,
    logical_size_bytes: u64,
) -> Result<(), HistoryError> {
    tx.execute(
        "INSERT INTO translation_history (
            entry_id, original_text, translated_text, original_summary, translated_summary,
            target_language, source_backend, source_provider, source_model, from_cache,
            total_elapsed_ms, completed_at_utc_ms, logical_size_bytes
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            draft.entry_id,
            draft.original_text,
            draft.translated_text,
            draft.original_summary,
            draft.translated_summary,
            draft.target_language,
            backend_storage_label(draft.source.backend),
            draft.source.provider,
            draft.source.model,
            draft.from_cache,
            total_elapsed_ms.min(i64::MAX as u64) as i64,
            completed_at_utc_ms,
            logical_size_bytes.min(i64::MAX as u64) as i64,
        ],
    )
    .map_err(|_| HistoryError::Unavailable)?;
    Ok(())
}

fn evict_over_limit(tx: &Transaction<'_>, limit: u8) -> Result<Vec<String>, HistoryError> {
    let mut statement = tx
        .prepare(
            "SELECT entry_id FROM translation_history
             ORDER BY completed_at_utc_ms DESC, sequence_id DESC LIMIT -1 OFFSET ?1",
        )
        .map_err(|_| HistoryError::Unavailable)?;
    let ids = statement
        .query_map([i64::from(limit)], |row| row.get::<_, String>(0))
        .map_err(|_| HistoryError::Unavailable)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| HistoryError::Unavailable)?;
    drop(statement);
    for id in &ids {
        tx.execute("DELETE FROM translation_history WHERE entry_id = ?1", [id])
            .map_err(|_| HistoryError::Unavailable)?;
    }
    Ok(ids)
}

fn summary_from_row(row: &Row<'_>) -> rusqlite::Result<TranslationHistorySummary> {
    let source_backend: String = row.get(4)?;
    let source_backend = parse_backend_storage_label(&source_backend).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            "unknown history backend".into(),
        )
    })?;
    let elapsed: i64 = row.get(8)?;
    if elapsed < 0 {
        return Err(rusqlite::Error::IntegralValueOutOfRange(8, elapsed));
    }
    Ok(TranslationHistorySummary {
        entry_id: row.get(0)?,
        original_summary: row.get(1)?,
        translated_summary: row.get(2)?,
        target_language: row.get(3)?,
        source_backend,
        source_provider: row.get(5)?,
        source_model: row.get(6)?,
        from_cache: row.get(7)?,
        total_elapsed_ms: elapsed as u64,
        completed_at_utc_ms: row.get(9)?,
    })
}

fn logical_size_for_stored(
    summary: &TranslationHistorySummary,
    original_text: &str,
    translated_text: &str,
) -> u64 {
    [
        summary.entry_id.len() as u64,
        original_text.len() as u64,
        translated_text.len() as u64,
        summary.original_summary.len() as u64,
        summary.translated_summary.len() as u64,
        summary.target_language.len() as u64,
        backend_storage_label(summary.source_backend).len() as u64,
        summary.source_provider.len() as u64,
        summary.source_model.len() as u64,
        8,
        1,
        8,
        8,
    ]
    .into_iter()
    .fold(0_u64, u64::saturating_add)
}

fn validate_entry_id(entry_id: &str) -> Result<(), HistoryError> {
    uuid::Uuid::parse_str(entry_id)
        .map(|_| ())
        .map_err(|_| HistoryError::InvalidEntryId)
}

fn quarantine_database_family(database_path: &Path) -> Result<(), HistoryError> {
    let parent = database_path.parent().ok_or(HistoryError::Unavailable)?;
    let expected_parent = parent.join(HISTORY_DATABASE_FILE);
    if expected_parent != database_path {
        return Err(HistoryError::Unavailable);
    }
    let timestamp = chrono_like_timestamp();
    // 主库最后 rename：它是 family 隔离完成的提交标志。任一 sidecar
    // rename 失败时主库仍留在原位，后续初始化不会误建新库并混用旧 family。
    for suffix in ["-wal", "-shm", ""] {
        let source = PathBuf::from(format!("{}{}", database_path.display(), suffix));
        if !source.exists() {
            continue;
        }
        let target = PathBuf::from(format!(
            "{}{}.corrupt-{}",
            database_path.display(),
            suffix,
            timestamp
        ));
        fs::rename(&source, &target).map_err(|_| HistoryError::Unavailable)?;
    }
    Ok(())
}

fn chrono_like_timestamp() -> String {
    // 不新增时间依赖；按 UTC 生成固定、可排序的隔离后缀。
    let seconds = super::models::now_utc_ms().max(0) / 1_000;
    let days = seconds / 86_400;
    let seconds_in_day = seconds % 86_400;
    let (year, month, day) = civil_date_from_unix_days(days);
    let hour = seconds_in_day / 3_600;
    let minute = (seconds_in_day % 3_600) / 60;
    let second = seconds_in_day % 60;
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

fn civil_date_from_unix_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic::AtomicBool, Arc};
    use std::time::Instant;

    use crate::translation_backend::models::{BackendMode, BackendSource};

    use super::*;
    use crate::translation_history::models::{HistoryCommitEligibility, RequestEligibility};

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("easyt-history-{name}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("temp dir");
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn draft(text: &str) -> HistoryEntryDraft {
        HistoryEntryDraft::new(
            text.to_string(),
            format!("译文-{text}"),
            "简体中文".to_string(),
            BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "agnes".to_string(),
                model: "agnes-2.0-flash".to_string(),
            },
            false,
            Instant::now(),
        )
    }

    fn eligible() -> HistoryCommitEligibility {
        HistoryCommitEligibility::new(RequestEligibility::new(Arc::new(AtomicBool::new(true))))
    }

    #[test]
    fn creates_reopens_and_evicts_in_stable_order() {
        let dir = TempDir::new("create");
        let mut db = HistoryDatabase::open(&dir.0, 2).expect("open");
        for value in ["a", "b", "c"] {
            db.commit_entry(draft(value), 2, &eligible())
                .expect("commit");
        }
        let summaries = db.list_summaries().expect("list");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].original_summary, "c");
        drop(db);
        let reopened = HistoryDatabase::open(&dir.0, 2).expect("reopen");
        assert_eq!(reopened.list_summaries().expect("list").len(), 2);
    }

    #[test]
    fn cancelled_commit_preserves_existing_records() {
        let dir = TempDir::new("cancel");
        let mut db = HistoryDatabase::open(&dir.0, 5).expect("open");
        let first = db
            .commit_entry(draft("old"), 5, &eligible())
            .expect("first");
        let id = match first {
            HistoryCommitOutcome::Saved { summary, .. } => summary.entry_id,
            _ => panic!("saved"),
        };
        let eligibility = eligible();
        eligibility.cancel();
        assert!(matches!(
            db.commit_entry(draft("new"), 5, &eligibility),
            Err(HistoryError::Cancelled)
        ));
        assert!(db.get_entry(&id).is_ok());
    }

    #[test]
    fn clear_commits_all_records() {
        let dir = TempDir::new("clear");
        let mut db = HistoryDatabase::open(&dir.0, 5).expect("open");
        db.commit_entry(draft("a"), 5, &eligible())
            .expect("commit");
        assert_eq!(db.clear_all().expect("clear").cleared_count, 1);
        assert!(db.list_summaries().expect("list").is_empty());
    }

    #[test]
    fn exact_logical_limit_is_saved_and_one_byte_over_is_not() {
        let dir = TempDir::new("logical-size");
        let mut db = HistoryDatabase::open(&dir.0, 5).expect("open");
        let mut exact = draft("");
        exact.original_text.clear();
        exact.translated_text.clear();
        exact.original_summary.clear();
        exact.translated_summary.clear();
        let fixed = exact.logical_size_bytes();
        exact.original_text = "a".repeat((MAX_HISTORY_ENTRY_BYTES - fixed) as usize);
        assert_eq!(exact.logical_size_bytes(), MAX_HISTORY_ENTRY_BYTES);
        assert!(matches!(
            db.commit_entry(exact, 5, &eligible())
                .expect("exact commit"),
            HistoryCommitOutcome::Saved { .. }
        ));

        let mut oversized = draft("");
        oversized.original_text.clear();
        oversized.translated_text.clear();
        oversized.original_summary.clear();
        oversized.translated_summary.clear();
        let fixed = oversized.logical_size_bytes();
        oversized.original_text = "a".repeat((MAX_HISTORY_ENTRY_BYTES - fixed + 1) as usize);
        assert!(matches!(
            db.commit_entry(oversized, 5, &eligible())
                .expect("oversize outcome"),
            HistoryCommitOutcome::NotSaved { .. }
        ));
        assert_eq!(db.list_summaries().expect("list").len(), 1);
    }

    #[test]
    fn corrupt_database_family_is_quarantined_and_recreated() {
        let dir = TempDir::new("corrupt");
        let history_dir = dir.0.join(HISTORY_DIR_NAME);
        fs::create_dir_all(&history_dir).expect("history dir");
        let database_path = history_dir.join(HISTORY_DATABASE_FILE);
        fs::write(&database_path, b"not a sqlite database").expect("main");
        fs::write(history_dir.join("history.sqlite-wal"), b"wal").expect("wal");
        fs::write(history_dir.join("history.sqlite-shm"), b"shm").expect("shm");

        let mut db = HistoryDatabase::open(&dir.0, 5).expect("recover");
        let snapshot = db.snapshot().expect("snapshot");
        assert_eq!(snapshot.state, HistoryInitState::Recovered);
        assert!(snapshot.warning.is_some());
        assert!(database_path.exists());
        for prefix in [
            "history.sqlite.corrupt-",
            "history.sqlite-wal.corrupt-",
            "history.sqlite-shm.corrupt-",
        ] {
            assert!(fs::read_dir(&history_dir)
                .expect("entries")
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().starts_with(prefix)));
        }
        db.commit_entry(draft("after recovery"), 5, &eligible())
            .expect("new database is writable");
    }

    #[test]
    fn commit_applies_the_latest_limit_even_after_prior_apply_failure() {
        let dir = TempDir::new("limit-retry");
        let mut db = HistoryDatabase::open(&dir.0, 5).expect("open");
        for value in ["a", "b", "c"] {
            db.commit_entry(draft(value), 5, &eligible())
                .expect("seed");
        }
        db.commit_entry(draft("d"), 1, &eligible())
            .expect("commit with latest limit");
        assert_eq!(db.list_summaries().expect("list").len(), 1);
        assert_eq!(db.limit, 1);
    }

    #[test]
    fn timestamp_suffix_has_the_documented_shape() {
        let value = chrono_like_timestamp();
        assert_eq!(value.len(), 15);
        assert_eq!(value.as_bytes()[8], b'-');
        assert!(value
            .chars()
            .enumerate()
            .all(|(index, ch)| index == 8 || ch.is_ascii_digit()));
    }
}
