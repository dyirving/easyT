//! 术语表持久化：原子 JSON 提交与损坏隔离
//!
//! 术语表独立于 AppConfig 和翻译历史，位于 `easyT_Data/termbase/termbase.json`。
//! 提交采用“写临时文件 -> flush -> 原子替换”。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::model::{validate_document, TermbaseDocument, TermbaseWarning, TERMBASE_SCHEMA_VERSION};
use super::TermbaseError;

/// 隔离后的原文件名模式：`termbase.json.corrupt-YYYYMMDD-HHMMSS`。
const QUARANTINE_PREFIX: &str = "termbase.json.corrupt-";

pub(crate) struct TermbaseStorage {
    path: PathBuf,
}

impl TermbaseStorage {
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("termbase").join("termbase.json"),
        }
    }

    /// 启动时加载。
    ///
    /// - 文件缺失：关闭的空术语表，无警告。
    /// - 文件损坏、schema 不兼容或校验失败：按时间戳隔离原文件（失败则保留原文件），
    ///   返回关闭的空术语表与一次性警告；翻译必须继续。
    pub(crate) fn load(&self) -> Result<(TermbaseDocument, Option<TermbaseWarning>), TermbaseError> {
        if !self.path.exists() {
            return Ok((TermbaseDocument::empty(), None));
        }
        let content = match fs::read_to_string(&self.path) {
            Ok(content) => content,
            Err(error) => {
                log::warn!(
                    "termbase_storage_unavailable: read_failed={}",
                    error.kind()
                );
                return Ok((
                    TermbaseDocument::empty(),
                    Some(self.isolate_or_warn(TermbaseWarning::storage_recovered())),
                ));
            }
        };
        match serde_json::from_str::<TermbaseDocument>(&content) {
            Ok(doc) if validate_document(&doc).is_ok() => Ok((doc, None)),
            _ => {
                log::warn!("termbase_storage_recovered: invalid_document");
                Ok((
                    TermbaseDocument::empty(),
                    Some(self.isolate_or_warn(TermbaseWarning::storage_recovered())),
                ))
            }
        }
    }

    /// 隔离原文件；隔离失败时绝不得删除或覆盖原文件。
    fn isolate_or_warn(&self, warning: TermbaseWarning) -> TermbaseWarning {
        if let Some(quarantine) = self.quarantine_path() {
            match fs::rename(&self.path, &quarantine) {
                Ok(()) => return warning,
                Err(error) => {
                    log::warn!(
                        "termbase_storage_unavailable: quarantine_failed={}",
                        error.kind()
                    );
                }
            }
        }
        TermbaseWarning::storage_unavailable()
    }

    fn quarantine_path(&self) -> Option<PathBuf> {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_secs() as i64;
        Some(self.path.with_file_name(format!(
            "{}{}",
            QUARANTINE_PREFIX,
            format_utc_timestamp(seconds)
        )))
    }

    /// 原子提交：写临时文件 -> sync_all -> 原子替换。
    pub(crate) fn save(&self, doc: &TermbaseDocument) -> Result<(), TermbaseError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| TermbaseError::Storage("术语表目录不可用".to_string()))?;
        fs::create_dir_all(parent)
            .map_err(|e| TermbaseError::Storage(format!("创建术语表目录失败: {}", e)))?;
        let json = serde_json::to_string_pretty(doc)
            .map_err(|e| TermbaseError::Storage(format!("序列化术语表失败: {}", e)))?;
        let tmp = parent.join("termbase.json.tmp");
        {
            let mut file = fs::File::create(&tmp)
                .map_err(|e| TermbaseError::Storage(format!("创建术语表临时文件失败: {}", e)))?;
            file.write_all(json.as_bytes())
                .map_err(|e| TermbaseError::Storage(format!("写入术语表失败: {}", e)))?;
            file.sync_all()
                .map_err(|e| TermbaseError::Storage(format!("刷新术语表失败: {}", e)))?;
        }
        fs::rename(&tmp, &self.path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            TermbaseError::Storage(format!("提交术语表失败: {}", e))
        })
    }
}

impl TermbaseDocument {
    fn empty() -> Self {
        Self {
            schema_version: TERMBASE_SCHEMA_VERSION,
            enabled: false,
            entries: Vec::new(),
        }
    }
}

/// 把 Unix 秒格式化为 UTC `YYYYMMDD-HHMMSS`（civil-from-days 算法，无额外依赖）。
fn format_utc_timestamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::termbase::model::{TermEntry, TermbaseWarningKind};
    use crate::termbase::Termbase;

    fn test_dir(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "easyT-termbase-{prefix}-test-{}",
            uuid::Uuid::new_v4()
        ))
    }

    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new(prefix: &str) -> Self {
            Self(test_dir(prefix))
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn utc_timestamp_format_is_zero_padded() {
        assert_eq!(format_utc_timestamp(0), "19700101-000000");
        // 2026-08-17 00:00:00 UTC = 1786924800
        assert_eq!(format_utc_timestamp(1_786_924_800), "20260817-000000");
        // 2026-08-17 13:09:11 UTC
        assert_eq!(
            format_utc_timestamp(1_786_924_800 + 13 * 3600 + 9 * 60 + 11),
            "20260817-130911"
        );
    }

    #[test]
    fn missing_file_is_disabled_empty_table_without_warning() {
        let dir = TestDir::new("missing");
        let storage = TermbaseStorage::new(&dir.0);
        let (doc, warning) = storage.load().expect("load should succeed");
        assert!(!doc.enabled);
        assert!(doc.entries.is_empty());
        assert!(warning.is_none());
    }

    #[test]
    fn saved_document_survives_reload() {
        let dir = TestDir::new("roundtrip");
        let storage = TermbaseStorage::new(&dir.0);
        let mut doc = TermbaseDocument {
            schema_version: TERMBASE_SCHEMA_VERSION,
            enabled: true,
            entries: vec![TermEntry {
                id: "entry-1".to_string(),
                source_term: "function".to_string(),
                target_language: "简体中文".to_string(),
                target_term: "函数".to_string(),
                enabled: true,
                case_sensitive: false,
                created_at_utc_ms: 42,
                updated_at_utc_ms: 43,
            }],
        };
        storage.save(&doc).expect("save should succeed");

        let (reloaded, warning) = storage.load().expect("load should succeed");
        assert_eq!(reloaded, doc);
        assert!(warning.is_none());

        doc.enabled = false;
        storage.save(&doc).expect("second save should succeed");
        let (reloaded, _) = storage.load().expect("load should succeed");
        assert!(!reloaded.enabled);
    }

    #[test]
    fn corrupt_document_is_quarantined_and_replaced_with_empty_table() {
        let dir = TestDir::new("corrupt");
        let storage = TermbaseStorage::new(&dir.0);
        std::fs::create_dir_all(dir.0.join("termbase")).expect("dir should be created");
        std::fs::write(&storage.path, "{ not json !!").expect("fixture should be written");

        let (doc, warning) = storage.load().expect("load must not fail on corruption");
        assert!(!doc.enabled);
        assert!(doc.entries.is_empty());
        let warning = warning.expect("corruption must warn");
        assert_eq!(warning.kind, TermbaseWarningKind::StorageRecovered);
        assert!(!storage.path.exists(), "original file must be isolated");
        let isolated = std::fs::read_dir(dir.0.join("termbase"))
            .expect("termbase dir should exist")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(isolated.len(), 1);
        assert!(isolated[0].starts_with(QUARANTINE_PREFIX));
    }

    #[test]
    fn unsupported_schema_is_recovered() {
        let dir = TestDir::new("schema");
        let storage = TermbaseStorage::new(&dir.0);
        std::fs::create_dir_all(dir.0.join("termbase")).expect("dir should be created");
        let doc = TermbaseDocument {
            schema_version: 2,
            enabled: false,
            entries: vec![],
        };
        std::fs::write(
            &storage.path,
            serde_json::to_string(&doc).expect("doc should serialize"),
        )
        .expect("fixture should be written");

        let (recovered, warning) = storage.load().expect("load should recover");
        assert!(recovered.entries.is_empty());
        assert_eq!(
            warning.expect("unsupported schema must warn").kind,
            TermbaseWarningKind::StorageRecovered
        );
    }

    #[test]
    fn invalid_entries_are_recovered() {
        let dir = TestDir::new("invalid-entries");
        let storage = TermbaseStorage::new(&dir.0);
        std::fs::create_dir_all(dir.0.join("termbase")).expect("dir should be created");
        let doc = TermbaseDocument {
            schema_version: TERMBASE_SCHEMA_VERSION,
            enabled: false,
            entries: vec![TermEntry {
                id: "bad".to_string(),
                source_term: "x\u{0}".to_string(),
                target_language: "简体中文".to_string(),
                target_term: "y".to_string(),
                enabled: true,
                case_sensitive: false,
                created_at_utc_ms: 1,
                updated_at_utc_ms: 1,
            }],
        };
        std::fs::write(
            &storage.path,
            serde_json::to_string(&doc).expect("doc should serialize"),
        )
        .expect("fixture should be written");

        let (recovered, warning) = storage.load().expect("load should recover");
        assert!(recovered.entries.is_empty());
        assert!(warning.is_some());
    }

    #[test]
    fn unreadable_storage_is_isolated_without_data_loss() {
        let dir = TestDir::new("unreadable");
        let root = dir.0.join("termbase");
        std::fs::create_dir_all(&root).expect("dir should be created");
        // 用一个目录占位 path，使 read_to_string 失败；隔离时目录整体被按时间戳改名，
        // 原数据不丢失，且不会覆盖为新术语表文件。
        std::fs::create_dir(root.join("termbase.json")).expect("fixture dir should be created");

        let storage = TermbaseStorage::new(&dir.0);
        let (doc, warning) = storage.load().expect("load must not fail");
        assert!(doc.entries.is_empty());
        let warning = warning.expect("unreadable must warn");
        assert_eq!(warning.kind, TermbaseWarningKind::StorageRecovered);
        let names = std::fs::read_dir(&root)
            .expect("root should be listable")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 1, "original path is preserved under quarantine");
        assert!(names[0].starts_with(QUARANTINE_PREFIX));
    }

    #[test]
    fn recovery_warning_is_consumed_once_by_first_snapshot() {
        let dir = TestDir::new("one-shot-warning");
        std::fs::create_dir_all(dir.0.join("termbase")).expect("dir should be created");
        std::fs::write(dir.0.join("termbase").join("termbase.json"), "{ broken !!")
            .expect("fixture should be written");

        let (termbase, _) = Termbase::open(&dir.0).expect("open must succeed");
        let first = termbase.snapshot();
        assert!(first.warning.is_some(), "first snapshot carries the warning");
        let second = termbase.snapshot();
        assert!(
            second.warning.is_none(),
            "warning is shown only once"
        );
    }

    #[test]
    fn failed_mutation_leaves_disk_and_memory_unchanged() {
        let dir = TestDir::new("failed-mutation");
        let (termbase, _) = Termbase::open(&dir.0).expect("open must succeed");
        let snapshot = termbase
            .create(crate::termbase::TermEntryInput {
                source_term: "keep".to_string(),
                target_language: "简体中文".to_string(),
                target_term: "保留".to_string(),
                case_sensitive: false,
            })
            .expect("create should succeed");
        assert_eq!(snapshot.entries.len(), 1);

        let failed = termbase.create(crate::termbase::TermEntryInput {
            source_term: "\u{1}".to_string(),
            target_language: "简体中文".to_string(),
            target_term: "坏".to_string(),
            case_sensitive: false,
        });
        assert!(matches!(failed, Err(TermbaseError::InvalidInput(_))));

        let after = termbase.snapshot();
        assert_eq!(after.entries.len(), 1);
        assert_eq!(after.entries[0].source_term, "keep");
        let disk = Termbase::open(&dir.0).expect("reopen").0.snapshot();
        assert_eq!(disk.entries.len(), 1, "disk must match memory");
    }

    #[test]
    fn max_entries_reached_on_second_open_after_restart() {
        let dir = TestDir::new("restart");
        let (termbase, _) = Termbase::open(&dir.0).expect("open");
        for index in 0..crate::termbase::model::MAX_TERMBASE_ENTRIES {
            termbase
                .create(crate::termbase::TermEntryInput {
                    source_term: format!("term-{index}"),
                    target_language: "简体中文".to_string(),
                    target_term: format!("译-{index}"),
                    case_sensitive: false,
                })
                .expect("create should succeed");
        }
        let rejected = termbase.create(crate::termbase::TermEntryInput {
            source_term: "overflow".to_string(),
            target_language: "简体中文".to_string(),
            target_term: "超限".to_string(),
            case_sensitive: false,
        });
        assert!(matches!(rejected, Err(TermbaseError::MaxEntries(_))));

        let (reopened, _) = Termbase::open(&dir.0).expect("reopen");
        let snapshot = reopened.snapshot();
        assert_eq!(snapshot.entries.len(), crate::termbase::model::MAX_TERMBASE_ENTRIES);
    }

    #[test]
    fn concurrent_mutations_are_serialized_and_persist_all() {
        let dir = Arc::new(TestDir::new("concurrent"));
        let (termbase, _) = Termbase::open(&dir.0).expect("open");
        let termbase = Arc::new(termbase);
        let mut handles = Vec::new();
        for index in 0..12 {
            let termbase = Arc::clone(&termbase);
            handles.push(std::thread::spawn(move || {
                termbase
                    .create(crate::termbase::TermEntryInput {
                        source_term: format!("word-{index}"),
                        target_language: "简体中文".to_string(),
                        target_term: format!("词-{index}"),
                        case_sensitive: false,
                    })
                    .expect("concurrent create should succeed");
            }));
        }
        for handle in handles {
            handle.join().expect("thread should join");
        }
        let snapshot = termbase.snapshot();
        assert_eq!(snapshot.entries.len(), 12);
    }
}
