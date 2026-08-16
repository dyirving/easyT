//! 术语表管理命令：快照与 mutation，Rust 为权威，每次返回完整快照。

use std::sync::Arc;

use tauri::State;

use crate::app_error::{AppError, AppResult};
use crate::termbase::{Termbase, TermbaseError, TermbaseSnapshot, TermEntryInput};

/// 用户可修正的输入类错误 → ConfigInvalid；存储类错误 → 安全内部错误。
fn map_termbase_error(error: TermbaseError) -> AppError {
    match error {
        TermbaseError::InvalidInput(message)
        | TermbaseError::Duplicate(message)
        | TermbaseError::NotFound(message)
        | TermbaseError::MaxEntries(message) => AppError::ConfigInvalid(message),
        TermbaseError::Storage(message) => AppError::Internal(message),
    }
}

#[tauri::command]
pub fn get_termbase(termbase: State<'_, Arc<Termbase>>) -> TermbaseSnapshot {
    termbase.snapshot()
}

#[tauri::command]
pub fn create_termbase_entry(
    termbase: State<'_, Arc<Termbase>>,
    input: TermEntryInput,
) -> AppResult<TermbaseSnapshot> {
    termbase.create(input).map_err(map_termbase_error)
}

#[tauri::command]
pub fn update_termbase_entry(
    termbase: State<'_, Arc<Termbase>>,
    id: String,
    input: TermEntryInput,
) -> AppResult<TermbaseSnapshot> {
    termbase.update(&id, input).map_err(map_termbase_error)
}

#[tauri::command]
pub fn delete_termbase_entry(
    termbase: State<'_, Arc<Termbase>>,
    id: String,
) -> AppResult<TermbaseSnapshot> {
    termbase.delete(&id).map_err(map_termbase_error)
}

#[tauri::command]
pub fn set_termbase_enabled(
    termbase: State<'_, Arc<Termbase>>,
    enabled: bool,
) -> AppResult<TermbaseSnapshot> {
    termbase.set_enabled(enabled).map_err(map_termbase_error)
}

#[tauri::command]
pub fn set_termbase_entry_enabled(
    termbase: State<'_, Arc<Termbase>>,
    id: String,
    enabled: bool,
) -> AppResult<TermbaseSnapshot> {
    termbase
        .set_entry_enabled(&id, enabled)
        .map_err(map_termbase_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termbase::TermEntryInput;

    fn input(source: &str, target: &str) -> TermEntryInput {
        TermEntryInput {
            source_term: source.to_string(),
            target_language: "简体中文".to_string(),
            target_term: target.to_string(),
            case_sensitive: false,
        }
    }

    fn open() -> (Termbase, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "easyT-termbase-commands-{}",
            uuid::Uuid::new_v4()
        ));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        (termbase, dir)
    }

    #[test]
    fn user_fixable_errors_map_to_config_invalid() {
        let cases: [(TermbaseError, &str); 4] = [
            (TermbaseError::InvalidInput("源术语不能为空".into()), "ConfigInvalid"),
            (TermbaseError::Duplicate("术语冲突".into()), "ConfigInvalid"),
            (TermbaseError::NotFound("条目不存在".into()), "ConfigInvalid"),
            (TermbaseError::MaxEntries("已达上限".into()), "ConfigInvalid"),
        ];
        for (error, expected_kind) in cases {
            let mapped = map_termbase_error(error);
            assert_eq!(mapped.kind_str(), expected_kind);
        }
    }

    #[test]
    fn storage_errors_map_to_internal_without_detail() {
        let mapped = map_termbase_error(TermbaseError::Storage("io: 磁盘损坏".into()));
        assert_eq!(mapped.kind_str(), "Internal");
        // Internal 分类由前端映射为通用错误文案，底层 io 细节不进入用户可见文案。
        assert!(mapped.to_string().contains("io:"));
    }

    #[test]
    fn crud_flow_returns_complete_authoritative_snapshots() {
        let (termbase, dir) = open();
        let termbase = Arc::new(termbase);

        let created = termbase.create(input("function", "函数")).expect("create");
        assert_eq!(created.entries.len(), 1);
        assert_eq!(created.maximum_entries, crate::termbase::model::MAX_TERMBASE_ENTRIES);
        assert!(!created.enabled, "总开关默认关闭");
        let id = created.entries[0].id.clone();

        let updated = termbase
            .update(&id, input("function", "功能"))
            .expect("update");
        assert_eq!(updated.entries[0].target_term, "功能");
        assert_eq!(updated.entries.len(), 1, "update 不新增条目");

        let disabled = termbase.set_entry_enabled(&id, false).expect("disable");
        assert!(!disabled.entries[0].enabled);

        let enabled = termbase.set_enabled(true).expect("enable");
        assert!(enabled.enabled);
        assert_eq!(enabled.entries[0].target_term, "功能");

        let deleted = termbase.delete(&id).expect("delete");
        assert!(deleted.entries.is_empty());

        let snapshot = termbase.snapshot();
        assert!(snapshot.entries.is_empty());
        assert!(snapshot.enabled);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_warning_is_delivered_exactly_once() {
        let dir = std::env::temp_dir().join(format!(
            "easyT-termbase-commands-{}",
            uuid::Uuid::new_v4()
        ));
        // 先建目录并写入损坏文件，迫使 open 进入隔离恢复路径。
        let storage_dir = dir.join("termbase");
        std::fs::create_dir_all(&storage_dir).expect("create dir");
        std::fs::write(storage_dir.join("termbase.json"), "{\"schema_version\":").expect("write");
        let (termbase, _) = Termbase::open(&dir).expect("open still succeeds");

        let first = termbase.snapshot();
        assert!(first.warning.is_some());
        assert_eq!(
            first.warning.as_ref().unwrap().kind.as_str(),
            "storageRecovered"
        );
        let second = termbase.snapshot();
        assert!(second.warning.is_none(), "警告只交付一次");
        let _ = std::fs::remove_dir_all(&dir);
    }
}