//! 翻译缓存统一入口：02 工单交付 L1 内存双池；L2 持久化 由 03 工单接入。
//!
//! 依赖方向：本模块不知道 Qwen / Official API / HTTP / SSE / 登录 / Tauri。
//! 翻译编排（Use/Refresh/Bypass）在 `translation_backend`，缓存错误不得改变
//! 普通翻译的成功/失败语义（FR-010）。

pub mod entry;
pub mod key;
pub mod memory;
pub mod persistent;

pub use entry::{CacheEntry, CacheLookupOutcome, CacheStatus};
pub use key::{is_definitely_oversized, prepare_cache_input, NormalizedCacheInput};

use std::path::Path;
use std::sync::Arc;

use crate::translation_backend::models::BackendResult;

use self::entry::now_ms;
use self::key::MAX_ENTRY_LOGICAL_BYTES;
use self::persistent::{PersistentCacheWorker, PersistentLookup, PersistentStore};

/// L1/L2 缓存门面。`start()` 永不失败：L1 立即可用，L2 异步初始化。
pub struct TranslationCache {
    memory: memory::MemoryCache,
    persistent: PersistentCacheWorker,
}

impl TranslationCache {
    /// 启动：L1 立即可用；L2 以异步 Starting/Ready/Degraded 状态挂接。
    pub fn start(data_dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            memory: memory::MemoryCache::new(),
            persistent: PersistentCacheWorker::start(data_dir.to_path_buf()),
        })
    }

    #[cfg(test)]
    pub(crate) fn memory_only_for_tests() -> Arc<Self> {
        Arc::new(Self {
            memory: memory::MemoryCache::new(),
            persistent: PersistentCacheWorker::disabled(),
        })
    }

    /// 当前 epoch：请求开始时快照，写入时交给 `store` 做 `insert_if_epoch` 校验。
    pub fn current_epoch(&self) -> u64 {
        self.memory.current_epoch()
    }

    /// 查找：L1 命中 → MemoryHit；否则在固定预算内查 L2，并将有效命中提升到 L1。
    /// 命中缓存即使流式输出也一次性返回，不伪造 delta。
    pub async fn lookup(&self, input: &NormalizedCacheInput) -> CacheLookupOutcome {
        match self.memory.lookup(&input.key) {
            Some(result) => CacheLookupOutcome {
                status: CacheStatus::MemoryHit,
                result: Some(result),
            },
            None => {
                let epoch = self.memory.current_epoch();
                match self.persistent.lookup(input.key, epoch).await {
                    PersistentLookup::Hit(entry)
                        if self.memory.insert_if_epoch(
                            entry.clone(),
                            input.is_short_text,
                            epoch,
                        ) =>
                    {
                        CacheLookupOutcome {
                            status: CacheStatus::PersistentHit,
                            result: Some(entry.result),
                        }
                    }
                    PersistentLookup::Hit(_)
                    | PersistentLookup::Miss
                    | PersistentLookup::Unavailable => CacheLookupOutcome {
                        status: CacheStatus::Miss,
                        result: None,
                    },
                }
            }
        }
    }

    /// 写入：只接受非空、逻辑大小 ≤1 MiB、epoch 有效的结果；
    /// 其余静默跳过（规则 §4），不改变翻译结果。
    pub fn store(&self, input: &NormalizedCacheInput, result: &BackendResult, epoch: u64) {
        if result.translated_text.trim().is_empty() {
            return;
        }
        let size = key::logical_size(input, result);
        if size > MAX_ENTRY_LOGICAL_BYTES {
            return;
        }
        let now = now_ms();
        let entry = CacheEntry {
            key: input.key,
            result: Arc::new(result.clone()),
            generated_at_ms: now,
            last_accessed_at_ms: now,
            hit_count: 0,
            source_text_bytes: input.normalized_source_bytes as u64,
            translated_text_bytes: result.translated_text.len() as u64,
            logical_size_bytes: size,
            access_tick: 0,
        };
        if self
            .memory
            .insert_if_epoch(entry.clone(), input.is_short_text, epoch)
        {
            let _ = self.persistent.try_store(
                PersistentStore {
                    entry,
                    target_language: input.target_language.clone(),
                },
                epoch,
            );
        }
    }

    /// 清空 L1 并推进 epoch，使在途写入失效（清除命令入口由 04 工单接入）。
    #[allow(dead_code)] // 04：clear_translation_cache 调用
    pub fn clear_l1(&self) {
        self.memory.clear_and_advance_epoch();
    }

    pub async fn shutdown(&self) {
        self.persistent.shutdown().await;
    }

    #[cfg(test)]
    pub(crate) async fn wait_until_persistent_ready(&self) {
        self.persistent.wait_until_ready().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(std::path::PathBuf);

    impl TestDir {
        fn new() -> Self {
            Self(
                std::env::temp_dir()
                    .join(format!("easyT-cache-facade-test-{}", uuid::Uuid::new_v4())),
            )
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn input(text: &str, target: &str) -> NormalizedCacheInput {
        prepare_cache_input(text, target)
    }

    fn result(text: &str) -> BackendResult {
        BackendResult {
            translated_text: text.to_string(),
            source: crate::translation_backend::models::BackendSource {
                backend: crate::translation_backend::models::BackendMode::OfficialApi,
                provider: "p".to_string(),
                model: "m".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn store_then_lookup_returns_memory_hit() {
        let cache = TranslationCache::memory_only_for_tests();
        let i = input("hello", "简体中文");
        let epoch = cache.current_epoch();
        cache.store(&i, &result("你好"), epoch);
        let outcome = cache.lookup(&i).await;
        assert_eq!(outcome.status, CacheStatus::MemoryHit);
        assert_eq!(outcome.result.unwrap().translated_text, "你好");
    }

    #[tokio::test]
    async fn lookup_miss_returns_none() {
        let cache = TranslationCache::memory_only_for_tests();
        let i = input("hello", "zh");
        let outcome = cache.lookup(&i).await;
        assert_eq!(outcome.status, CacheStatus::Miss);
        assert!(outcome.result.is_none());
    }

    #[tokio::test]
    async fn empty_result_is_not_stored() {
        let cache = TranslationCache::memory_only_for_tests();
        let i = input("hello", "zh");
        cache.store(&i, &result(""), cache.current_epoch());
        let outcome = cache.lookup(&i).await;
        assert_eq!(outcome.status, CacheStatus::Miss);
    }

    #[tokio::test]
    async fn oversized_entry_is_skipped_but_exact_limit_is_stored() {
        let cache = TranslationCache::memory_only_for_tests();
        let i = input("hello", "zh");
        let epoch = cache.current_epoch();

        // 恰好 1 MiB：以空译文为基准算填充量（ASCII 一字节一字符）
        let base_size = key::logical_size(&i, &result(""));
        let exact_len = MAX_ENTRY_LOGICAL_BYTES as usize - base_size as usize;
        let exact = "x".repeat(exact_len);
        assert_eq!(
            key::logical_size(&i, &result(&exact)),
            MAX_ENTRY_LOGICAL_BYTES
        );
        cache.store(&i, &result(&exact), epoch);
        assert_eq!(cache.lookup(&i).await.status, CacheStatus::MemoryHit);

        // 超过 1 MiB 不缓存
        let smaller = input("hello2", "zh");
        cache.store(&smaller, &result(&format!("{exact}x")), epoch);
        assert_eq!(cache.lookup(&smaller).await.status, CacheStatus::Miss);
    }

    #[tokio::test]
    async fn stale_epoch_store_is_rejected() {
        let cache = TranslationCache::memory_only_for_tests();
        let i = input("hello", "zh");
        let old_epoch = cache.current_epoch();
        cache.clear_l1();
        cache.store(&i, &result("你好"), old_epoch);
        let outcome = cache.lookup(&i).await;
        assert_eq!(outcome.status, CacheStatus::Miss);
    }

    #[tokio::test]
    async fn cross_target_language_miss() {
        let cache = TranslationCache::memory_only_for_tests();
        cache.store(
            &input("hello", "zh"),
            &result("你好"),
            cache.current_epoch(),
        );
        let outcome = cache.lookup(&input("hello", "en")).await;
        assert_eq!(outcome.status, CacheStatus::Miss);
    }

    #[tokio::test]
    async fn write_behind_survives_cache_restart_and_promotes_to_l1() {
        let dir = TestDir::new();
        let input = input("hello", "zh");
        let first = TranslationCache::start(&dir.0);
        first.wait_until_persistent_ready().await;
        first.store(&input, &result("持久译文"), first.current_epoch());
        first.shutdown().await;

        let reopened = TranslationCache::start(&dir.0);
        reopened.wait_until_persistent_ready().await;
        let persistent = reopened.lookup(&input).await;
        assert_eq!(persistent.status, CacheStatus::PersistentHit);
        assert_eq!(persistent.result.unwrap().translated_text, "持久译文");

        let promoted = reopened.lookup(&input).await;
        assert_eq!(promoted.status, CacheStatus::MemoryHit);
        reopened.shutdown().await;
    }

    #[tokio::test]
    async fn unavailable_persistent_cache_does_not_disable_l1() {
        let dir = TestDir::new();
        std::fs::create_dir_all(&dir.0).expect("temp root should be created");
        let invalid_data_dir = dir.0.join("file-not-directory");
        std::fs::write(&invalid_data_dir, b"file").expect("invalid data root should be created");

        let cache = TranslationCache::start(&invalid_data_dir);
        let input = input("hello", "zh");
        cache.store(&input, &result("仍可用"), cache.current_epoch());
        let outcome = cache.lookup(&input).await;
        assert_eq!(outcome.status, CacheStatus::MemoryHit);
        assert_eq!(outcome.result.unwrap().translated_text, "仍可用");
        cache.shutdown().await;
    }
}
