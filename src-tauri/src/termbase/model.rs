//! 术语表数据模型、校验与公开 DTO
//!
//! 术语表是用户维护的翻译约束集合，不是翻译缓存，也不是翻译记忆。

use serde::{Deserialize, Serialize};

use super::TermbaseError;

/// 单个术语表最多保存的条目数。
pub const MAX_TERMBASE_ENTRIES: usize = 200;

/// 源术语字符数上限（1 至 120 个 Unicode 字符）。
pub const MAX_SOURCE_TERM_CHARS: usize = 120;

/// 指定译法字符数上限（1 至 240 个 Unicode 字符）。
pub const MAX_TARGET_TERM_CHARS: usize = 240;

/// 术语表 JSON schema version。
pub const TERMBASE_SCHEMA_VERSION: u32 = 1;

/// Rust 端权威目标语言白名单（ASM-001 记录）。
///
/// 前端 `TARGET_LANGUAGES` 与这里保持一致；Rust 不接受任意字符串。
/// 两处列表是独立副本，修改任一列表时必须同步另一处（RISK-004）。
pub const TARGET_LANGUAGES: &[&str] = &["简体中文", "繁體中文", "English", "日本語"];

/// 术语条目：一条用户规则，绑定一个现有目标语言。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermEntry {
    /// 稳定 UUID。
    pub id: String,
    /// 英文源术语。
    pub source_term: String,
    /// 现有目标语言列表中的一项。
    pub target_language: String,
    /// 指定译法。
    pub target_term: String,
    /// 单条启用状态。
    pub enabled: bool,
    /// 是否大小写敏感，默认 false。
    pub case_sensitive: bool,
    pub created_at_utc_ms: i64,
    pub updated_at_utc_ms: i64,
}

/// 创建/更新条目时的用户输入；`enabled` 由单独 command 变更。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermEntryInput {
    pub source_term: String,
    pub target_language: String,
    pub target_term: String,
    pub case_sensitive: bool,
}

/// 持久化文档根对象。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermbaseDocument {
    pub schema_version: u32,
    pub enabled: bool,
    pub entries: Vec<TermEntry>,
}

/// 设置页可展示的一次性恢复提示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TermbaseWarningKind {
    StorageRecovered,
    StorageUnavailable,
}

impl TermbaseWarningKind {
    /// 与 serde camelCase 序列化一致的稳定字符串（前端 DTO 契约）。
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StorageRecovered => "storageRecovered",
            Self::StorageUnavailable => "storageUnavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermbaseWarning {
    pub kind: TermbaseWarningKind,
    pub message: String,
}

impl TermbaseWarning {
    /// 原文件已按时间戳隔离，并创建了关闭的空术语表。
    pub fn storage_recovered() -> Self {
        Self {
            kind: TermbaseWarningKind::StorageRecovered,
            message: "术语表文件损坏或格式不兼容，已隔离原文件并创建新的空术语表。".to_string(),
        }
    }

    /// 原文件无法隔离，术语表保持关闭的空状态。
    pub fn storage_unavailable() -> Self {
        Self {
            kind: TermbaseWarningKind::StorageUnavailable,
            message: "术语表无法读取，已临时使用空术语表，翻译不受影响。".to_string(),
        }
    }
}

/// 每次成功操作返回的完整权威快照。
/// 前端不得自行推导匹配、冲突或排序结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermbaseSnapshot {
    pub enabled: bool,
    pub entries: Vec<TermEntry>,
    pub maximum_entries: usize,
    pub warning: Option<TermbaseWarning>,
}

/// 校验文本字段：字符数上下限、不得只含空白、不得含控制字符。
fn validate_text_field(
    label: &str,
    value: &str,
    max_chars: usize,
) -> Result<(), TermbaseError> {
    let count = value.chars().count();
    if !(1..=max_chars).contains(&count) {
        return Err(TermbaseError::InvalidInput(format!(
            "{label}长度应为 1 至 {max_chars} 个字符"
        )));
    }
    if value.trim().is_empty() {
        return Err(TermbaseError::InvalidInput(format!(
            "{label}不能只包含空白"
        )));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(TermbaseError::InvalidInput(format!(
            "{label}不能包含控制字符"
        )));
    }
    Ok(())
}

/// 校验条目输入字段与目标语言白名单。
pub fn validate_entry_input(input: &TermEntryInput) -> Result<(), TermbaseError> {
    validate_text_field("源术语", &input.source_term, MAX_SOURCE_TERM_CHARS)?;
    validate_text_field("指定译法", &input.target_term, MAX_TARGET_TERM_CHARS)?;
    if !TARGET_LANGUAGES.contains(&input.target_language.as_str()) {
        return Err(TermbaseError::InvalidInput(format!(
            "不支持的目标语言: {}",
            input.target_language
        )));
    }
    Ok(())
}

/// 忽略大小写后的规范化源术语，用于不敏感条目的唯一性判定。
pub fn normalize_source_term(term: &str) -> String {
    term.to_lowercase()
}

/// 新条目与既有条目是否冲突。
///
/// - 大小写敏感条目：同一目标语言下源术语完全相同时冲突。
/// - 大小写不敏感条目：同一目标语言下忽略大小写规范化后相同时冲突。
/// - 敏感与不敏感条目可以共存（例如 `China` 与 `china`）。
pub fn conflicts_with(existing: &TermEntry, input: &TermEntryInput) -> bool {
    if existing.target_language != input.target_language {
        return false;
    }
    if input.case_sensitive {
        existing.case_sensitive && existing.source_term == input.source_term
    } else {
        !existing.case_sensitive
            && normalize_source_term(&existing.source_term)
                == normalize_source_term(&input.source_term)
    }
}

/// 校验持久化文档；任何不一致都视为损坏，交由 storage 恢复。
pub fn validate_document(doc: &TermbaseDocument) -> Result<(), TermbaseError> {
    if doc.schema_version != TERMBASE_SCHEMA_VERSION {
        return Err(TermbaseError::Storage(format!(
            "不支持的术语表 schema 版本: {}",
            doc.schema_version
        )));
    }
    if doc.entries.len() > MAX_TERMBASE_ENTRIES {
        return Err(TermbaseError::Storage(format!(
            "术语表条目超过上限 {}",
            MAX_TERMBASE_ENTRIES
        )));
    }
    for entry in &doc.entries {
        let input = TermEntryInput {
            source_term: entry.source_term.clone(),
            target_language: entry.target_language.clone(),
            target_term: entry.target_term.clone(),
            case_sensitive: entry.case_sensitive,
        };
        validate_entry_input(&input)?;
    }
    for (index, entry) in doc.entries.iter().enumerate() {
        for other in doc.entries.iter().skip(index + 1) {
            if conflicts_with(other, &entry_to_input(entry)) {
                return Err(TermbaseError::Storage(
                    "术语表包含重复或相互冲突的条目".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn entry_to_input(entry: &TermEntry) -> TermEntryInput {
    TermEntryInput {
        source_term: entry.source_term.clone(),
        target_language: entry.target_language.clone(),
        target_term: entry.target_term.clone(),
        case_sensitive: entry.case_sensitive,
    }
}

/// 当前 UTC 毫秒时间戳。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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

    fn entry(id: &str, source: &str, language: &str, target: &str, sensitive: bool) -> TermEntry {
        TermEntry {
            id: id.to_string(),
            source_term: source.to_string(),
            target_language: language.to_string(),
            target_term: target.to_string(),
            enabled: true,
            case_sensitive: sensitive,
            created_at_utc_ms: 1,
            updated_at_utc_ms: 1,
        }
    }

    #[test]
    fn validates_field_boundaries() {
        assert!(validate_entry_input(&input("a", "简体中文", "b")).is_ok());
        let long_source = "a".repeat(120);
        assert!(validate_entry_input(&input(&long_source, "简体中文", "b")).is_ok());
        let too_long_source = "a".repeat(121);
        assert!(validate_entry_input(&input(&too_long_source, "简体中文", "b")).is_err());
        let long_target = "译".repeat(240);
        assert!(validate_entry_input(&input("a", "简体中文", &long_target)).is_ok());
        let too_long_target = "译".repeat(241);
        assert!(validate_entry_input(&input("a", "简体中文", &too_long_target)).is_err());
        assert!(validate_entry_input(&input("", "简体中文", "b")).is_err());
        assert!(validate_entry_input(&input("a", "简体中文", "")).is_err());
    }

    #[test]
    fn rejects_whitespace_only_and_control_characters() {
        assert!(validate_entry_input(&input("   ", "简体中文", "b")).is_err());
        assert!(validate_entry_input(&input("a", "简体中文", "\t\n")).is_err());
        assert!(validate_entry_input(&input("a\u{0}", "简体中文", "b")).is_err());
        assert!(validate_entry_input(&input("a", "简体中文", "b\u{1}")).is_err());
        // 常规标点与空格合法
        assert!(validate_entry_input(&input("neural network", "简体中文", "神经网络")).is_ok());
    }

    #[test]
    fn rejects_unsupported_target_language() {
        assert!(validate_entry_input(&input("a", "法语", "b")).is_err());
        assert!(validate_entry_input(&input("a", "", "b")).is_err());
        for language in TARGET_LANGUAGES {
            assert!(validate_entry_input(&input("a", language, "b")).is_ok());
        }
    }

    #[test]
    fn sensitive_and_insensitive_can_coexist() {
        let insensitive = entry("1", "china", "简体中文", "瓷器", false);
        let sensitive_input = sensitive_input_case(input("China", "简体中文", "中国"), true);
        assert!(!conflicts_with(&insensitive, &sensitive_input));
        assert!(!conflicts_with(
            &entry("1", "China", "简体中文", "中国", true),
            &input("china", "简体中文", "瓷器")
        ));
    }

    #[test]
    fn duplicate_sensitive_exact_terms_conflict() {
        let existing = entry("1", "China", "简体中文", "中国", true);
        assert!(conflicts_with(
            &existing,
            &sensitive_input_case(input("China", "简体中文", "另一个"), true)
        ));
        // 大小写不同不冲突
        assert!(!conflicts_with(
            &existing,
            &sensitive_input_case(input("china", "简体中文", "小写"), true)
        ));
    }

    #[test]
    fn duplicate_insensitive_case_folded_terms_conflict() {
        let existing = entry("1", "China", "简体中文", "中国", false);
        assert!(conflicts_with(
            &existing,
            &input("CHINA", "简体中文", "另一个")
        ));
        // 不同目标语言不冲突
        assert!(!conflicts_with(
            &existing,
            &input("CHINA", "English", "另一个")
        ));
    }

    fn sensitive_input_case(mut input: TermEntryInput, sensitive: bool) -> TermEntryInput {
        input.case_sensitive = sensitive;
        input
    }

    #[test]
    fn document_validation_rejects_bad_schema_and_duplicates() {
        let doc = TermbaseDocument {
            schema_version: 99,
            enabled: false,
            entries: vec![],
        };
        assert!(validate_document(&doc).is_err());

        let doc = TermbaseDocument {
            schema_version: TERMBASE_SCHEMA_VERSION,
            enabled: false,
            entries: vec![
                entry("1", "china", "简体中文", "瓷器", false),
                entry("2", "CHINA", "简体中文", "重复", false),
            ],
        };
        assert!(validate_document(&doc).is_err());

        let doc = TermbaseDocument {
            schema_version: TERMBASE_SCHEMA_VERSION,
            enabled: false,
            entries: vec![
                entry("1", "china", "简体中文", "瓷器", false),
                entry("2", "China", "简体中文", "中国", true),
            ],
        };
        assert!(validate_document(&doc).is_ok());
    }

    #[test]
    fn snapshot_serializes_camel_case_for_frontend() {
        let snapshot = TermbaseSnapshot {
            enabled: true,
            entries: vec![entry("1", "function", "简体中文", "函数", false)],
            maximum_entries: MAX_TERMBASE_ENTRIES,
            warning: Some(TermbaseWarning::storage_recovered()),
        };
        let json = serde_json::to_value(&snapshot).expect("snapshot should serialize");
        assert_eq!(json["enabled"], true);
        assert_eq!(json["maximumEntries"], 200);
        assert_eq!(json["entries"][0]["sourceTerm"], "function");
        assert_eq!(json["entries"][0]["targetLanguage"], "简体中文");
        assert_eq!(json["entries"][0]["caseSensitive"], false);
        assert_eq!(json["entries"][0]["createdAtUtcMs"], 1);
        assert_eq!(json["warning"]["kind"], "storageRecovered");
    }
}
