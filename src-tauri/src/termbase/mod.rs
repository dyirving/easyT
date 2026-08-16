//! 术语表门面：内存状态、序列化提交与匹配解析
//!
//! 不变式（SDD §5.1）：术语表与翻译缓存、翻译历史、AppConfig 完全独立；
//! 每次成功操作返回完整权威快照，前端不得自行推导匹配、冲突或排序结果。

use std::path::Path;
use std::sync::Mutex;

use self::matcher::resolve as resolve_matches;
use self::model::{
    conflicts_with, now_ms, validate_entry_input, TermbaseDocument, MAX_TERMBASE_ENTRIES,
    TERMBASE_SCHEMA_VERSION,
};
use self::storage::TermbaseStorage;

pub(crate) mod matcher;
pub(crate) mod model;
pub(crate) mod storage;

pub use self::matcher::EffectiveTermbase;
pub use self::model::{TermEntry, TermEntryInput, TermbaseSnapshot, TermbaseWarning};

/// 术语表错误；所有消息均可安全展示（§13.5 映射见 app_error.rs）。
#[derive(Debug)]
pub enum TermbaseError {
    /// 输入校验失败，用户可修正。
    InvalidInput(String),
    /// 与现有条目冲突，用户可修正。
    Duplicate(String),
    /// 条目不存在，可能已被删除或隔离。
    NotFound(String),
    /// 超过最大条目数。
    MaxEntries(String),
    /// 存储失败，属内部错误，不暴露细节。
    Storage(String),
}

impl std::fmt::Display for TermbaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidInput(m)
            | Self::Duplicate(m)
            | Self::NotFound(m)
            | Self::MaxEntries(m)
            | Self::Storage(m) => m,
        };
        write!(f, "{message}")
    }
}

impl std::error::Error for TermbaseError {}

/// 术语表门面：短锁内存状态 + 串行化文件提交。
///
/// - `state`：每次读取短暂持有。
/// - `mutation`：磁盘 I/O 串行化；一次只允许一个写者。
/// - 写路径：校验 -> 草稿 -> 原子保存 -> 发布到内存（失败绝不污染内存）。
pub struct Termbase {
    state: Mutex<TermbaseState>,
    mutation: Mutex<()>,
    storage: TermbaseStorage,
}

#[derive(Clone)]
struct TermbaseState {
    enabled: bool,
    entries: Vec<TermEntry>,
    pending_warning: Option<TermbaseWarning>,
}

impl Termbase {
    /// 启动时打开。存储损坏时自动隔离并创建关闭的空术语表；
    /// 返回一次性警告（None 表示存储完好），警告由 `snapshot` 单次交付。
    pub fn open(data_dir: &Path) -> Result<(Self, Option<TermbaseWarning>), TermbaseError> {
        let storage = TermbaseStorage::new(data_dir);
        let (doc, warning) = storage.load()?;
        Ok((
            Self {
                state: Mutex::new(TermbaseState {
                    enabled: doc.enabled,
                    entries: doc.entries,
                    pending_warning: warning.clone(),
                }),
                mutation: Mutex::new(()),
                storage,
            },
            warning,
        ))
    }

    /// 完整权威快照；消耗一次性警告（仅第一次快照携带）。
    pub fn snapshot(&self) -> TermbaseSnapshot {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        snapshot_from_state(&mut state)
    }

    /// 解析本次原文的有效术语集（无副作用，不消耗警告）。
    pub fn resolve(&self, source_text: &str, target_language: &str) -> EffectiveTermbase {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        resolve_matches(source_text, target_language, state.enabled, &state.entries)
    }

    pub fn create(&self, raw: TermEntryInput) -> Result<TermbaseSnapshot, TermbaseError> {
        let input = normalize_input(raw);
        self.mutate(|state| {
            validate_entry_input(&input)?;
            if state.entries.len() >= MAX_TERMBASE_ENTRIES {
                return Err(TermbaseError::MaxEntries(format!(
                    "术语表条目数已达上限（{}）",
                    MAX_TERMBASE_ENTRIES
                )));
            }
            if state.entries.iter().any(|e| conflicts_with(e, &input)) {
                return Err(TermbaseError::Duplicate(format!(
                    "术语“{}”与现有条目冲突",
                    input.source_term
                )));
            }
            let now = now_ms();
            state.entries.push(TermEntry {
                id: uuid::Uuid::new_v4().to_string(),
                source_term: input.source_term.clone(),
                target_language: input.target_language.clone(),
                target_term: input.target_term.clone(),
                enabled: true,
                case_sensitive: input.case_sensitive,
                created_at_utc_ms: now,
                updated_at_utc_ms: now,
            });
            Ok(())
        })
    }

    pub fn update(&self, id: &str, raw: TermEntryInput) -> Result<TermbaseSnapshot, TermbaseError> {
        let input = normalize_input(raw);
        self.mutate(|state| {
            validate_entry_input(&input)?;
            let position = state
                .entries
                .iter()
                .position(|e| e.id == id)
                .ok_or_else(|| TermbaseError::NotFound("术语条目不存在，可能已被删除".to_string()))?;
            if state
                .entries
                .iter()
                .enumerate()
                .any(|(index, e)| index != position && conflicts_with(e, &input))
            {
                return Err(TermbaseError::Duplicate(format!(
                    "术语“{}”与现有条目冲突",
                    input.source_term
                )));
            }
            let entry = &mut state.entries[position];
            entry.source_term = input.source_term.clone();
            entry.target_language = input.target_language.clone();
            entry.target_term = input.target_term.clone();
            entry.case_sensitive = input.case_sensitive;
            entry.updated_at_utc_ms = now_ms();
            Ok(())
        })
    }

    pub fn delete(&self, id: &str) -> Result<TermbaseSnapshot, TermbaseError> {
        self.mutate(|state| {
            let before = state.entries.len();
            state.entries.retain(|e| e.id != id);
            if state.entries.len() == before {
                return Err(TermbaseError::NotFound("术语条目不存在，可能已被删除".to_string()));
            }
            Ok(())
        })
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<TermbaseSnapshot, TermbaseError> {
        self.mutate(|state| {
            state.enabled = enabled;
            Ok(())
        })
    }

    pub fn set_entry_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<TermbaseSnapshot, TermbaseError> {
        self.mutate(|state| {
            let entry = state
                .entries
                .iter_mut()
                .find(|e| e.id == id)
                .ok_or_else(|| TermbaseError::NotFound("术语条目不存在，可能已被删除".to_string()))?;
            entry.enabled = enabled;
            entry.updated_at_utc_ms = now_ms();
            Ok(())
        })
    }

    /// 校验 -> 草稿 -> 原子保存 -> 发布；任何失败都不改变内存与磁盘。
    ///
    /// 锁纪律（SDD §6.1）：短 `state` 锁不得跨越文件系统 I/O——草稿在短锁内
    /// 构建，`storage.save` 只持有 `mutation` 锁，保存成功后再重新获取 `state`
    /// 锁发布。翻译解析（resolve）在同一 `state` 锁上运行，因此写路径不可阻塞它。
    fn mutate(
        &self,
        change: impl FnOnce(&mut TermbaseState) -> Result<(), TermbaseError>,
    ) -> Result<TermbaseSnapshot, TermbaseError> {
        let _mutation = self.mutation.lock().unwrap_or_else(|p| p.into_inner());
        let draft = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let mut draft = state.clone();
            change(&mut draft)?;
            draft
        };
        let doc = TermbaseDocument {
            schema_version: TERMBASE_SCHEMA_VERSION,
            enabled: draft.enabled,
            entries: draft.entries.clone(),
        };
        self.storage.save(&doc)?;
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        *state = draft;
        Ok(snapshot_from_state(&mut state))
    }
}

/// 从短锁持有的状态构建权威快照并消耗一次性警告。
fn snapshot_from_state(state: &mut TermbaseState) -> TermbaseSnapshot {
    TermbaseSnapshot {
        enabled: state.enabled,
        entries: state.entries.clone(),
        maximum_entries: MAX_TERMBASE_ENTRIES,
        warning: state.pending_warning.take(),
    }
}

/// 写入前规范化：首尾空白裁剪（校验与冲突判定基于裁剪后的值）。
fn normalize_input(mut input: TermEntryInput) -> TermEntryInput {
    input.source_term = input.source_term.trim().to_string();
    input.target_term = input.target_term.trim().to_string();
    input
}

/// 测试支撑：构造非空有效术语集（全 crate 测试复用，避免每个模块重复建临时目录）。
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// 返回带一条启用条目的临时 Termbase 与目录；调用方负责清理目录。
    pub(crate) fn termbase_with_entry() -> (Termbase, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "easyT-termbase-support-{}",
            uuid::Uuid::new_v4()
        ));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        termbase
            .create(TermEntryInput {
                source_term: "function".to_string(),
                target_language: "简体中文".to_string(),
                target_term: "函数".to_string(),
                case_sensitive: false,
            })
            .expect("create");
        termbase.set_enabled(true).expect("enable");
        (termbase, dir)
    }

    /// 解析 "function" 得到的非空有效术语集（临时目录已清理）。
    pub(crate) fn non_empty_effective() -> EffectiveTermbase {
        let (termbase, dir) = termbase_with_entry();
        let effective = termbase.resolve("function", "简体中文");
        assert!(!effective.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        effective
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(source: &str, language: &str, target: &str) -> TermEntryInput {
        TermEntryInput {
            source_term: source.to_string(),
            target_language: language.to_string(),
            target_term: target.to_string(),
            case_sensitive: false,
        }
    }

    #[test]
    fn create_normalizes_whitespace_and_assigns_metadata() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        let snapshot = termbase
            .create(input("  function  ", "简体中文", "  函数  "))
            .expect("create");
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].source_term, "function");
        assert_eq!(snapshot.entries[0].target_term, "函数");
        assert!(!snapshot.entries[0].id.is_empty());
        assert!(snapshot.entries[0].enabled);
        assert!(snapshot.entries[0].created_at_utc_ms > 0);
        assert_eq!(
            snapshot.entries[0].created_at_utc_ms,
            snapshot.entries[0].updated_at_utc_ms
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trimmed_conflict_duplicates_are_rejected() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        termbase
            .create(input("function", "简体中文", "函数"))
            .expect("create");
        let duplicate = termbase.create(input(" function ", "简体中文", "功能"));
        assert!(matches!(duplicate, Err(TermbaseError::Duplicate(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_replaces_fields_and_bumps_timestamp() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        let created = termbase
            .create(input("function", "简体中文", "函数"))
            .expect("create");
        let id = created.entries[0].id.clone();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let updated = termbase
            .update(&id, input("function", "English", "function (verb)"))
            .expect("update");
        assert_eq!(updated.entries[0].source_term, "function");
        assert_eq!(updated.entries[0].target_language, "English");
        assert_eq!(updated.entries[0].target_term, "function (verb)");
        assert!(
            updated.entries[0].updated_at_utc_ms > created.entries[0].created_at_utc_ms
        );
        assert_eq!(updated.entries.len(), 1, "update 不新增条目");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_rejects_conflicting_with_another_entry() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        let created = termbase
            .create(input("function", "简体中文", "函数"))
            .expect("create");
        termbase
            .create(input("class", "简体中文", "类"))
            .expect("create");
        let conflict = termbase.update(&created.entries[0].id, input("class", "简体中文", "类"));
        assert!(matches!(conflict, Err(TermbaseError::Duplicate(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_and_not_found_errors() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        let created = termbase
            .create(input("function", "简体中文", "函数"))
            .expect("create");
        assert!(matches!(
            termbase.delete("missing-id"),
            Err(TermbaseError::NotFound(_))
        ));
        let deleted = termbase.delete(&created.entries[0].id).expect("delete");
        assert!(deleted.entries.is_empty());
        assert!(matches!(
            termbase.set_entry_enabled(&created.entries[0].id, true),
            Err(TermbaseError::NotFound(_))
        ));
        assert!(matches!(
            termbase.update(&created.entries[0].id, input("x", "简体中文", "y")),
            Err(TermbaseError::NotFound(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn entry_toggle_does_not_touch_other_entries() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        let created = termbase
            .create(input("function", "简体中文", "函数"))
            .expect("create");
        termbase
            .create(input("class", "简体中文", "类"))
            .expect("create");
        let snapshot = termbase
            .set_entry_enabled(&created.entries[0].id, false)
            .expect("toggle");
        assert!(!snapshot.entries[0].enabled);
        assert!(snapshot.entries[1].enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enabled_switch_gates_resolve() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        termbase
            .create(input("function", "简体中文", "函数"))
            .expect("create");
        assert!(
            termbase.resolve("function", "简体中文").is_empty(),
            "总开关默认关闭"
        );
        termbase.set_enabled(true).expect("enable");
        assert!(!termbase.resolve("function", "简体中文").is_empty());
        termbase.set_enabled(false).expect("disable");
        assert!(termbase.resolve("function", "简体中文").is_empty());
        termbase.set_enabled(true).expect("enable");
        assert!(!termbase.resolve("function", "简体中文").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_input_never_reaches_storage() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        assert!(matches!(
            termbase.create(input("\u{0}", "简体中文", "坏")),
            Err(TermbaseError::InvalidInput(_))
        ));
        assert!(matches!(
            termbase.create(input("a", "法语", "b")),
            Err(TermbaseError::InvalidInput(_))
        ));
        assert!(termbase.snapshot().entries.is_empty());
        assert!(!dir.join("termbase").join("termbase.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_mutation_leaves_snapshot_and_storage_unchanged() {
        let dir = std::env::temp_dir().join(format!("easyT-termbase-facade-{}", uuid::Uuid::new_v4()));
        let (termbase, _) = Termbase::open(&dir).expect("open");
        termbase
            .create(input("function", "简体中文", "函数"))
            .expect("create");
        termbase.set_enabled(true).expect("enable");
        let before = termbase.snapshot();

        assert!(matches!(
            termbase.create(input("function", "简体中文", "功能")),
            Err(TermbaseError::Duplicate(_))
        ));
        assert!(matches!(
            termbase.create(input("a", "法语", "b")),
            Err(TermbaseError::InvalidInput(_))
        ));
        assert!(matches!(
            termbase.delete("missing-id"),
            Err(TermbaseError::NotFound(_))
        ));
        assert!(matches!(
            termbase.update("missing-id", input("x", "简体中文", "y")),
            Err(TermbaseError::NotFound(_))
        ));

        let after = termbase.snapshot();
        assert_eq!(after.enabled, before.enabled);
        assert_eq!(after.entries.len(), before.entries.len());
        assert_eq!(after.entries[0].target_term, "函数");

        drop(termbase);
        let (reopened, _) = Termbase::open(&dir).expect("reopen");
        let reloaded = reopened.snapshot();
        assert_eq!(reloaded.enabled, true);
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.entries[0].target_term, "函数");
        let _ = std::fs::remove_dir_all(&dir);
    }
}