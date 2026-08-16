use std::sync::{
    atomic::{AtomicBool, AtomicU8, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::translation_backend::models::{BackendMode, BackendSource};

pub const MAX_HISTORY_ENTRY_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_HISTORY_LIMIT: u8 = 20;
pub const MIN_HISTORY_LIMIT: u8 = 1;
pub const HISTORY_SAVE_BUDGET: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationHistorySummary {
    pub entry_id: String,
    pub original_summary: String,
    pub translated_summary: String,
    pub target_language: String,
    pub source_backend: BackendMode,
    pub source_provider: String,
    pub source_model: String,
    pub from_cache: bool,
    pub total_elapsed_ms: u64,
    pub completed_at_utc_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationHistoryEntry {
    #[serde(flatten)]
    pub summary: TranslationHistorySummary,
    pub original_text: String,
    pub translated_text: String,
}

#[derive(Debug, Clone)]
pub struct HistoryEntryDraft {
    pub entry_id: String,
    pub original_text: String,
    pub translated_text: String,
    pub original_summary: String,
    pub translated_summary: String,
    pub target_language: String,
    pub source: BackendSource,
    pub from_cache: bool,
    pub request_started_at: Instant,
}

impl HistoryEntryDraft {
    pub fn new(
        original_text: String,
        translated_text: String,
        target_language: String,
        source: BackendSource,
        from_cache: bool,
        request_started_at: Instant,
    ) -> Self {
        let original_summary = summarize(&original_text);
        let translated_summary = summarize(&translated_text);
        Self {
            entry_id: uuid::Uuid::new_v4().to_string(),
            original_text,
            translated_text,
            original_summary,
            translated_summary,
            target_language,
            source,
            from_cache,
            request_started_at,
        }
    }

    pub fn completed_at_utc_ms(&self) -> i64 {
        now_utc_ms()
    }

    pub fn total_elapsed_ms(&self) -> u64 {
        self.request_started_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    pub fn logical_size_bytes(&self) -> u64 {
        let backend = backend_storage_label(self.source.backend);
        [
            self.entry_id.len() as u64,
            self.original_text.len() as u64,
            self.translated_text.len() as u64,
            self.original_summary.len() as u64,
            self.translated_summary.len() as u64,
            self.target_language.len() as u64,
            backend.len() as u64,
            self.source.provider.len() as u64,
            self.source.model.len() as u64,
            8,
            1,
            8,
            8,
        ]
        .into_iter()
        .fold(0_u64, u64::saturating_add)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryWarningKind {
    StorageUnavailable,
    StorageRecovered,
    SaveFailed,
    SaveTimedOut,
    EntryTooLarge,
    LimitApplyFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryWarning {
    pub kind: HistoryWarningKind,
    pub message: String,
}

impl HistoryWarning {
    pub fn recovered() -> Self {
        Self {
            kind: HistoryWarningKind::StorageRecovered,
            message: "翻译历史存储异常，已创建新的历史记录库。".to_string(),
        }
    }

    pub fn save_failed() -> Self {
        Self {
            kind: HistoryWarningKind::SaveFailed,
            message: "译文已生成，但未能保存到翻译历史。".to_string(),
        }
    }

    pub fn timed_out() -> Self {
        Self {
            kind: HistoryWarningKind::SaveTimedOut,
            message: "译文已生成，但保存翻译历史超时。".to_string(),
        }
    }

    pub fn too_large() -> Self {
        Self {
            kind: HistoryWarningKind::EntryTooLarge,
            message: "译文已生成，但内容过大，未保存到翻译历史。".to_string(),
        }
    }

    pub fn limit_failed() -> Self {
        Self {
            kind: HistoryWarningKind::LimitApplyFailed,
            message: "设置已保存，但未能立即收缩翻译历史。".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum HistoryCommitOutcome {
    Saved {
        summary: TranslationHistorySummary,
        evicted_entry_ids: Vec<String>,
    },
    NotSaved {
        warning: HistoryWarning,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryInitState {
    Ready,
    Recovered,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySnapshot {
    pub state: HistoryInitState,
    pub limit: u8,
    pub summaries: Vec<TranslationHistorySummary>,
    pub warning: Option<HistoryWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearHistoryResult {
    pub cleared_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryLimitResult {
    pub summaries: Vec<TranslationHistorySummary>,
    pub evicted_entry_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "status"
)]
pub enum HistoryLimitUpdate {
    Applied {
        summaries: Vec<TranslationHistorySummary>,
        evicted_entry_ids: Vec<String>,
    },
    Warning {
        warning: HistoryWarning,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigResult {
    pub history_limit: u8,
    pub history_update: HistoryLimitUpdate,
}

#[derive(Clone)]
pub struct RequestEligibility {
    current: Arc<AtomicBool>,
}

impl RequestEligibility {
    pub fn new(current: Arc<AtomicBool>) -> Self {
        Self { current }
    }

    pub fn is_current(&self) -> bool {
        self.current.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct HistoryCommitEligibility {
    request: RequestEligibility,
    state: Arc<AtomicU8>,
    deadline: Instant,
}

const COMMIT_ACTIVE: u8 = 0;
const COMMIT_CANCELLED: u8 = 1;
const COMMIT_CLAIMED: u8 = 2;

impl HistoryCommitEligibility {
    pub fn new(request: RequestEligibility) -> Self {
        Self {
            request,
            state: Arc::new(AtomicU8::new(COMMIT_ACTIVE)),
            deadline: Instant::now() + HISTORY_SAVE_BUDGET,
        }
    }

    pub fn cancel(&self) -> bool {
        self.state
            .compare_exchange(
                COMMIT_ACTIVE,
                COMMIT_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    pub fn may_commit(&self) -> bool {
        self.request.is_current()
            && self.state.load(Ordering::Acquire) == COMMIT_ACTIVE
            && Instant::now() <= self.deadline
    }

    pub fn claim_commit(&self) -> bool {
        self.request.is_current()
            && Instant::now() <= self.deadline
            && self
                .state
                .compare_exchange(
                    COMMIT_ACTIVE,
                    COMMIT_CLAIMED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }

    pub fn commit_claimed(&self) -> bool {
        self.state.load(Ordering::Acquire) == COMMIT_CLAIMED
    }

    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

pub fn validate_limit(limit: u8) -> bool {
    (MIN_HISTORY_LIMIT..=MAX_HISTORY_LIMIT).contains(&limit)
}

pub fn summarize(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut result = String::new();
    for ch in normalized.chars() {
        let rendered = if ch == '\n' { ' ' } else { ch };
        result.push(rendered);
        if result.chars().count() == 160 {
            break;
        }
    }
    result
}

pub fn backend_storage_label(backend: BackendMode) -> &'static str {
    match backend {
        BackendMode::OfficialApi => "officialApi",
        BackendMode::WebGateway => "webGateway",
    }
}

pub fn parse_backend_storage_label(value: &str) -> Option<BackendMode> {
    match value {
        "officialApi" => Some(BackendMode::OfficialApi),
        "webGateway" => Some(BackendMode::WebGateway),
        _ => None,
    }
}

pub fn now_utc_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_normalizes_newlines_and_limits_unicode_scalars() {
        let text = format!("a\r\nb\rc\n\n{}", "界".repeat(200));
        let value = summarize(&text);
        assert!(value.starts_with("a b c  "));
        assert_eq!(value.chars().count(), 160);
    }

    #[test]
    fn limits_are_strict() {
        assert!(validate_limit(1));
        assert!(validate_limit(20));
        assert!(!validate_limit(0));
        assert!(!validate_limit(21));
    }

    #[test]
    fn commit_outcome_serializes_struct_variant_fields_for_frontend() {
        let outcome = HistoryCommitOutcome::Saved {
            summary: TranslationHistorySummary {
                entry_id: "entry-id".to_string(),
                original_summary: "source".to_string(),
                translated_summary: "target".to_string(),
                target_language: "简体中文".to_string(),
                source_backend: BackendMode::OfficialApi,
                source_provider: "agnes".to_string(),
                source_model: "agnes-2.0-flash".to_string(),
                from_cache: false,
                total_elapsed_ms: 10,
                completed_at_utc_ms: 20,
            },
            evicted_entry_ids: vec!["evicted-id".to_string()],
        };

        let value = serde_json::to_value(outcome).expect("serialize history outcome");
        assert_eq!(value["status"], "saved");
        assert_eq!(value["evictedEntryIds"][0], "evicted-id");
        assert!(value.get("evicted_entry_ids").is_none());
    }

    #[test]
    fn limit_update_serializes_struct_variant_fields_for_frontend() {
        let update = HistoryLimitUpdate::Applied {
            summaries: Vec::new(),
            evicted_entry_ids: vec!["evicted-id".to_string()],
        };

        let value = serde_json::to_value(update).expect("serialize limit update");
        assert_eq!(value["status"], "applied");
        assert_eq!(value["evictedEntryIds"][0], "evicted-id");
        assert!(value.get("evicted_entry_ids").is_none());
    }
}
