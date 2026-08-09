//! 缓存值、策略、来源状态与生命周期合同
//!
//! 02 工单把这些合同从 `translation_backend/models.rs` 与 `mod.rs`
//! 统一迁移到缓存深模块（SDD §6.2），保持对外的符号路径不变。

use std::sync::Arc;

use crate::translation_backend::models::BackendResult;

use super::key::CacheKey;

/// 缓存策略：唯一决策点位于 TranslationBackend（普通翻译 Use / 重新翻译 Refresh /
/// 保存网页历史、测试连接、诊断 Bypass）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    Use,
    Refresh,
    Bypass,
}

/// 结果来源状态：结果如何产生，前端只消费 `TranslationOutcome.is_from_cache()`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // 03：L2 lookup 构造 PersistentHit
pub enum CacheStatus {
    Miss,
    MemoryHit,
    PersistentHit,
    Refreshed,
    Bypassed,
}

/// L2 worker 生命周期；普通翻译在非 Ready 状态下直接按 miss 继续。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentCacheState {
    Starting,
    Ready,
    Degraded,
    Stopped,
}

/// 缓存查找结果：命中时携带完整译文（Arc 共享，避免复制）。
/// `status` 由 03 工单在 L2 查找链读取并对外报告。
#[derive(Debug)]
#[allow(dead_code)] // 03：L2 查找链读取 status
pub struct CacheLookupOutcome {
    pub status: CacheStatus,
    pub result: Option<Arc<BackendResult>>,
}

/// 缓存条目：字段由 Tauri 侧统一维护。
/// `access_tick` 单调递增，用于跨池全局淘汰的确定性排序；
/// `hit_count` 在覆盖（Refresh 或后续相同键写入）时归零。
/// 时间戳与来源/译文字节数由 03 工单 L2 store 写入并用于统计。
#[derive(Debug, Clone)]
#[allow(dead_code)] // 03：L2 store 写入与统计读取
pub struct CacheEntry {
    pub key: CacheKey,
    pub result: Arc<BackendResult>,
    pub generated_at_ms: i64,
    pub last_accessed_at_ms: i64,
    pub hit_count: u64,
    pub source_text_bytes: u64,
    pub translated_text_bytes: u64,
    pub logical_size_bytes: u64,
    pub access_tick: u64,
}

/// 翻译后端统一结果：完整译文 + 来源状态同行返回。
#[derive(Debug, Clone)]
pub struct TranslationOutcome {
    pub result: BackendResult,
    pub cache_status: CacheStatus,
}

impl TranslationOutcome {
    /// 前端 fromCache 布尔值的唯一来源。
    pub fn is_from_cache(&self) -> bool {
        matches!(
            self.cache_status,
            CacheStatus::MemoryHit | CacheStatus::PersistentHit
        )
    }
}

/// 当前 unix 毫秒时间戳（条目时间字段使用）。
pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => elapsed.as_millis() as i64,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation_backend::models::{BackendMode, BackendResult, BackendSource};

    fn sample_result() -> BackendResult {
        BackendResult {
            translated_text: "你好".to_string(),
            source: BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "agnes".to_string(),
                model: "agnes-2.0-flash".to_string(),
            },
        }
    }

    #[test]
    fn outcome_from_cache_marks_only_cache_hits() {
        let miss = TranslationOutcome {
            result: sample_result(),
            cache_status: CacheStatus::Miss,
        };
        let refreshed = TranslationOutcome {
            result: sample_result(),
            cache_status: CacheStatus::Refreshed,
        };
        let bypassed = TranslationOutcome {
            result: sample_result(),
            cache_status: CacheStatus::Bypassed,
        };
        let memory_hit = TranslationOutcome {
            result: sample_result(),
            cache_status: CacheStatus::MemoryHit,
        };
        let persistent_hit = TranslationOutcome {
            result: sample_result(),
            cache_status: CacheStatus::PersistentHit,
        };

        assert!(!miss.is_from_cache());
        assert!(!refreshed.is_from_cache());
        assert!(!bypassed.is_from_cache());
        assert!(memory_hit.is_from_cache());
        assert!(persistent_hit.is_from_cache());
    }
}
