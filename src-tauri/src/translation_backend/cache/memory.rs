//! L1 内存双池 LRU（《翻译缓存规则》§8；FR-006/009）
//!
//! 全部状态受同一 Mutex 保护：锁内不 await、不 I/O、不复制大字符串。
//! 容量：总 10 MiB / 1,024 条；短池 768 条；长池 256 条 / 7 MiB。
//! 跨池全局淘汰选 access_tick 最旧（平手按 key 字典序；生产中 tick 单调不会平手）。
//! 锁中毒时接管内部状态并继续运行（缓存故障不得升级为翻译错误）。

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard};

use lru::LruCache;

use super::entry::{now_ms, CacheEntry};
use super::key::CacheKey;
use crate::translation_backend::models::BackendResult;

/// L1 总容量与条目数固定值（MiB 均为 1024*1024）。
const TOTAL_MAX_BYTES: u64 = 10 * 1024 * 1024;
const SHORT_MAX_ENTRIES: usize = 768;
const LONG_MAX_ENTRIES: usize = 256;
const LONG_MAX_BYTES: u64 = 7 * 1024 * 1024;

pub struct MemoryCache {
    state: Mutex<MemoryCacheState>,
}

struct MemoryCacheState {
    short_pool: LruCache<CacheKey, Arc<CacheEntry>>,
    long_pool: LruCache<CacheKey, Arc<CacheEntry>>,
    epoch: u64,
    next_access_tick: u64,
    total_logical_bytes: u64,
    long_logical_bytes: u64,
}

impl Default for MemoryCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryCache {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MemoryCacheState {
                short_pool: LruCache::new(lru_capacity(SHORT_MAX_ENTRIES)),
                long_pool: LruCache::new(lru_capacity(LONG_MAX_ENTRIES)),
                epoch: 0,
                next_access_tick: 0,
                total_logical_bytes: 0,
                long_logical_bytes: 0,
            }),
        }
    }

    /// 命中：移到 MRU 并更新访问计数/时间，返回共享译文（不复制大字符串）。
    pub fn lookup(&self, key: &CacheKey) -> Option<Arc<BackendResult>> {
        let mut state = lock_state(&self.state);
        let (cached, is_short) = if state.short_pool.contains(key) {
            (state.short_pool.pop(key), true)
        } else if state.long_pool.contains(key) {
            (state.long_pool.pop(key), false)
        } else {
            return None;
        };
        // Arc<CacheEntry> 不可变：克隆条目本体（result 仍是 Arc 共享）再维护访问信息。
        let mut entry = (*cached?).clone();
        let result = entry.result.clone();
        touch(&mut entry, &mut state);
        if is_short {
            state.short_pool.push(*key, Arc::new(entry));
        } else {
            state.long_pool.push(*key, Arc::new(entry));
        }
        Some(result)
    }

    /// epoch 条件才写入；清除后迟到的结果在此被拒绝（FR-009）。
    pub fn insert_if_epoch(
        &self,
        entry: CacheEntry,
        is_short_text: bool,
        expected_epoch: u64,
    ) -> bool {
        let mut state = lock_state(&self.state);
        if state.epoch != expected_epoch {
            return false;
        }
        insert_locked(&mut state, entry, is_short_text);
        true
    }

    /// 清空两层并推进 epoch，返回新 epoch；在途请求被拒绝回填。
    /// 由 `TranslationCache::clear_l1` 调用（清除命令入口在 04 工单接入）。
    #[allow(dead_code)]
    pub fn clear_and_advance_epoch(&self) -> u64 {
        let mut state = lock_state(&self.state);
        state.short_pool.clear();
        state.long_pool.clear();
        state.total_logical_bytes = 0;
        state.long_logical_bytes = 0;
        state.epoch = state.epoch.wrapping_add(1);
        state.epoch
    }

    pub fn current_epoch(&self) -> u64 {
        lock_state(&self.state).epoch
    }
}

fn lru_capacity(max: usize) -> NonZeroUsize {
    NonZeroUsize::new(max).expect("L1 分池容量必须非零")
}

/// 锁中毒不携带敏感内容，直接接管内部状态继续运行。
fn lock_state(state: &Mutex<MemoryCacheState>) -> MutexGuard<'_, MemoryCacheState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::warn!("L1 内存缓存状态锁中毒，已接管状态继续运行");
            let mut guard = poisoned.into_inner();
            if !state_is_valid(&guard) {
                log::warn!("L1 内存缓存状态不变量损坏，已清空缓存并推进 epoch");
                reset_invalid_state(&mut guard);
            }
            state.clear_poison();
            guard
        }
    }
}

fn state_is_valid(state: &MemoryCacheState) -> bool {
    if state.short_pool.len() > SHORT_MAX_ENTRIES
        || state.long_pool.len() > LONG_MAX_ENTRIES
        || state.short_pool.len() + state.long_pool.len() > SHORT_MAX_ENTRIES + LONG_MAX_ENTRIES
    {
        return false;
    }

    if state
        .short_pool
        .iter()
        .any(|(key, _)| state.long_pool.contains(key))
    {
        return false;
    }

    let Some(short_bytes) = state.short_pool.iter().try_fold(0u64, |sum, (_, entry)| {
        sum.checked_add(entry.logical_size_bytes)
    }) else {
        return false;
    };
    let Some(long_bytes) = state.long_pool.iter().try_fold(0u64, |sum, (_, entry)| {
        sum.checked_add(entry.logical_size_bytes)
    }) else {
        return false;
    };
    let Some(total_bytes) = short_bytes.checked_add(long_bytes) else {
        return false;
    };

    state.long_logical_bytes == long_bytes
        && state.total_logical_bytes == total_bytes
        && long_bytes <= LONG_MAX_BYTES
        && total_bytes <= TOTAL_MAX_BYTES
}

fn reset_invalid_state(state: &mut MemoryCacheState) {
    state.short_pool.clear();
    state.long_pool.clear();
    state.total_logical_bytes = 0;
    state.long_logical_bytes = 0;
    state.next_access_tick = 0;
    state.epoch = state.epoch.saturating_add(1);
}

/// 命中维护：访问计数 +1、刷新访问时间、前进确定性 access_tick。
fn touch(entry: &mut CacheEntry, state: &mut MemoryCacheState) {
    entry.hit_count = entry.hit_count.saturating_add(1);
    entry.last_accessed_at_ms = now_ms();
    entry.access_tick = take_next_access_tick(state);
}

/// 锁定写入：先满足分池限制再满足全局限制（规则 §8）。
fn insert_locked(state: &mut MemoryCacheState, mut entry: CacheEntry, is_short_text: bool) {
    let key = entry.key;
    // 覆盖不留下旧计量：移除既有条目（两个池都不会持有同一键）。
    if let Some(old) = state.short_pool.pop(&key) {
        deduct(state, old.logical_size_bytes, false);
    }
    if let Some(old) = state.long_pool.pop(&key) {
        deduct(state, old.logical_size_bytes, true);
    }

    let size = entry.logical_size_bytes;

    // 新条目取得唯一单调 tick（确定性淘汰排序）；覆盖后 hit_count 归零（规则 §4.3）。
    entry.access_tick = take_next_access_tick(state);

    // 分池条数上限（768 / 256）由 LruCache 自身按 LRU 淘汰。
    let evicted = if is_short_text {
        state.short_pool.push(key, Arc::new(entry))
    } else {
        state.long_pool.push(key, Arc::new(entry))
    };
    if let Some((_evicted_key, evicted)) = evicted {
        deduct(state, evicted.logical_size_bytes, !is_short_text);
    }

    state.total_logical_bytes = state.total_logical_bytes.saturating_add(size);
    if !is_short_text {
        state.long_logical_bytes = state.long_logical_bytes.saturating_add(size);
    }

    // 插入后先满足分池限制，再满足全局限制（规则 §8）。
    if !is_short_text {
        while state.long_logical_bytes > LONG_MAX_BYTES {
            let Some((_victim_key, victim)) = state.long_pool.pop_lru() else {
                break;
            };
            deduct(state, victim.logical_size_bytes, true);
        }
    }

    // 全局 10 MiB：跨池淘汰 access_tick 最旧项。
    while state.total_logical_bytes > TOTAL_MAX_BYTES {
        let Some((victim_key, victim_is_short)) = oldest_across_pools(state) else {
            break;
        };
        evict(state, &victim_key, victim_is_short);
    }
}

fn take_next_access_tick(state: &mut MemoryCacheState) -> u64 {
    if state.next_access_tick == u64::MAX {
        rebase_access_ticks(state);
    }
    let tick = state.next_access_tick;
    state.next_access_tick += 1;
    tick
}

/// 极端 tick 溢出前按既有全局年龄顺序压缩编号；不改变两个 LRU 的内部顺序。
fn rebase_access_ticks(state: &mut MemoryCacheState) {
    let mut by_age: Vec<(u64, CacheKey)> = state
        .short_pool
        .iter()
        .chain(state.long_pool.iter())
        .map(|(key, entry)| (entry.access_tick, *key))
        .collect();
    by_age.sort_unstable();
    let ranks: BTreeMap<CacheKey, u64> = by_age
        .into_iter()
        .enumerate()
        .map(|(rank, (_, key))| (key, rank as u64))
        .collect();

    for (key, entry) in state.short_pool.iter_mut() {
        Arc::make_mut(entry).access_tick = ranks[key];
    }
    for (key, entry) in state.long_pool.iter_mut() {
        Arc::make_mut(entry).access_tick = ranks[key];
    }
    state.next_access_tick = ranks.len() as u64;
}

fn deduct(state: &mut MemoryCacheState, bytes: u64, from_long: bool) {
    state.total_logical_bytes = state.total_logical_bytes.saturating_sub(bytes);
    if from_long {
        state.long_logical_bytes = state.long_logical_bytes.saturating_sub(bytes);
    }
}

fn evict(state: &mut MemoryCacheState, key: &CacheKey, is_short: bool) {
    let removed = if is_short {
        state.short_pool.pop(key)
    } else {
        state.long_pool.pop(key)
    };
    if let Some(entry) = removed {
        deduct(state, entry.logical_size_bytes, !is_short);
    }
}

/// 跨两个池选 access_tick 最旧（平手按 key 字典序升序），返回 (victim_key, is_short)。
fn oldest_across_pools(state: &MemoryCacheState) -> Option<(CacheKey, bool)> {
    let short = state
        .short_pool
        .peek_lru()
        .map(|(key, entry)| ((entry.access_tick, *key), true));
    let long = state
        .long_pool
        .peek_lru()
        .map(|(key, entry)| ((entry.access_tick, *key), false));

    match (short, long) {
        (Some((short_rank, _)), Some((long_rank, _))) if short_rank <= long_rank => {
            Some((short_rank.1, true))
        }
        (Some(_), Some((long_rank, _))) => Some((long_rank.1, false)),
        (Some((short_rank, _)), None) => Some((short_rank.1, true)),
        (None, Some((long_rank, _))) => Some((long_rank.1, false)),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation_backend::models::{BackendMode, BackendSource};

    fn key(seed: u32) -> CacheKey {
        CacheKey::from_seed(seed)
    }

    fn result(text: &str) -> BackendResult {
        BackendResult {
            translated_text: text.to_string(),
            source: BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "p".to_string(),
                model: "m".to_string(),
            },
        }
    }

    /// 构造条目；tick/hit_count 等动态字段由 MemoryCache 维护。
    fn entry(seed: u32, size: u64) -> CacheEntry {
        CacheEntry {
            key: key(seed),
            result: Arc::new(result("译文")),
            generated_at_ms: 0,
            last_accessed_at_ms: 0,
            hit_count: 0,
            source_text_bytes: 0,
            translated_text_bytes: 0,
            logical_size_bytes: size,
            access_tick: 0,
        }
    }

    fn insert_short(cache: &MemoryCache, e: CacheEntry) {
        assert!(cache.insert_if_epoch(e, true, cache.current_epoch()));
    }

    fn insert_long(cache: &MemoryCache, e: CacheEntry) {
        assert!(cache.insert_if_epoch(e, false, cache.current_epoch()));
    }

    #[test]
    fn short_pool_evicts_lru_when_over_768() {
        let cache = MemoryCache::new();
        for seed in 0..768 {
            insert_short(&cache, entry(seed, 1));
        }
        insert_short(&cache, entry(768, 1));
        assert_eq!(cache.state.lock().unwrap().short_pool.len(), 768);
        // 最早的 0 被 LRU 淘汰，最新插入的 768 存活
        assert!(cache.lookup(&key(0)).is_none());
        assert!(cache.lookup(&key(768)).is_some());
        assert!(cache.lookup(&key(767)).is_some());
    }

    #[test]
    fn long_pool_respects_byte_and_entry_caps() {
        let cache = MemoryCache::new();
        // 300 条 × 30 KiB 超长池 7 MiB 上限
        for seed in 0..300 {
            insert_long(&cache, entry(seed, 30 * 1024));
        }
        let state = cache.state.lock().unwrap();
        assert!(state.long_logical_bytes <= LONG_MAX_BYTES);
        assert!(state.long_pool.len() <= LONG_MAX_ENTRIES);
        assert!(state.total_logical_bytes <= TOTAL_MAX_BYTES);
    }

    #[test]
    fn global_byte_cap_evicts_oldest_across_pools() {
        let cache = MemoryCache::new();
        // 长池 A=3MiB(先插) B=3MiB（合计 6MiB ≤7MiB）；随后短池 5×1MiB → 超 10MiB
        insert_long(&cache, entry(1, 3 * 1024 * 1024));
        insert_long(&cache, entry(2, 3 * 1024 * 1024));
        for seed in 10..15 {
            insert_short(&cache, entry(seed, 1024 * 1024));
        }
        // 最旧的 A（tick=0）被跨池淘汰
        assert!(cache.lookup(&key(1)).is_none());
        assert!(cache.lookup(&key(2)).is_some());
        for seed in 10..15 {
            assert!(cache.lookup(&key(seed)).is_some());
        }
        assert!(cache.state.lock().unwrap().total_logical_bytes <= TOTAL_MAX_BYTES);
    }

    #[test]
    fn lookup_touches_recency_so_latest_entry_survives() {
        let cache = MemoryCache::new();
        insert_short(&cache, entry(2, 8 * 1024 * 1024 - 1024));
        insert_short(&cache, entry(1, 1024));
        // 访问 1 号让其变最新；插入 3 号（2 MiB+1024）触发全局淘汰 → 应淘汰最旧的 2 号
        assert!(cache.lookup(&key(1)).is_some());
        insert_short(&cache, entry(3, 2 * 1024 * 1024 + 1024));
        assert!(cache.lookup(&key(2)).is_none());
        assert!(cache.lookup(&key(1)).is_some());
        assert!(cache.lookup(&key(3)).is_some());
    }

    #[test]
    fn long_pool_pressure_is_resolved_before_global_eviction() {
        let cache = MemoryCache::new();
        insert_short(&cache, entry(1, 4 * 1024 * 1024));
        insert_long(&cache, entry(2, 6 * 1024 * 1024));

        // 新长文本让总量和长池都超限；规范要求先清长池，再看全局容量。
        insert_long(&cache, entry(3, 2 * 1024 * 1024));

        assert!(cache.lookup(&key(1)).is_some(), "短池不应先被全局淘汰");
        assert!(cache.lookup(&key(2)).is_none(), "长池 LRU 应先被淘汰");
        assert!(cache.lookup(&key(3)).is_some());
        let state = cache.state.lock().unwrap();
        assert!(state.long_logical_bytes <= LONG_MAX_BYTES);
        assert!(state.total_logical_bytes <= TOTAL_MAX_BYTES);
    }

    #[test]
    fn lookups_increase_hit_count() {
        let cache = MemoryCache::new();
        insert_short(&cache, entry(1, 1));
        assert!(cache.lookup(&key(1)).is_some());
        assert!(cache.lookup(&key(1)).is_some());
        let state = cache.state.lock().unwrap();
        assert_eq!(
            state.short_pool.peek(&key(1)).expect("present").hit_count,
            2
        );
    }

    #[test]
    fn same_key_cannot_live_in_both_pools() {
        let cache = MemoryCache::new();
        insert_short(&cache, entry(1, 100));
        assert!(cache.lookup(&key(1)).is_some());
        // 同一键以长文本身份重写 → 从短池迁到长池
        insert_long(&cache, entry(1, 200));
        let state = cache.state.lock().unwrap();
        assert!(state.long_pool.contains(&key(1)));
        assert!(!state.short_pool.contains(&key(1)));
        assert_eq!(state.short_pool.len() + state.long_pool.len(), 1);
        assert_eq!(state.long_logical_bytes, 200);
        assert_eq!(state.total_logical_bytes, 200);
    }

    #[test]
    fn clear_advances_epoch_and_rejects_stale_insert() {
        let cache = MemoryCache::new();
        let old_epoch = cache.current_epoch();
        insert_short(&cache, entry(1, 1));
        let new_epoch = cache.clear_and_advance_epoch();
        assert_eq!(new_epoch, old_epoch + 1);
        assert!(cache.lookup(&key(1)).is_none());
        assert!(!cache.insert_if_epoch(entry(1, 1), true, old_epoch));
        assert!(cache.insert_if_epoch(entry(1, 1), true, new_epoch));
        assert!(cache.lookup(&key(1)).is_some());
    }

    #[test]
    fn recovers_after_poisoned_lock() {
        let cache = MemoryCache::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = cache.state.lock().unwrap();
            panic!("intentional poison");
        }));
        insert_short(&cache, entry(9, 1));
        assert!(cache.lookup(&key(9)).is_some());
    }

    #[test]
    fn poisoned_invalid_state_is_cleared_and_advances_epoch() {
        let cache = MemoryCache::new();
        let old_epoch = cache.current_epoch();
        insert_short(&cache, entry(1, 100));

        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut state = cache.state.lock().unwrap();
            state.total_logical_bytes = 999;
            panic!("poison after corrupting metrics");
        }));

        assert_eq!(cache.current_epoch(), old_epoch + 1);
        assert!(cache.lookup(&key(1)).is_none());
        assert!(!cache.insert_if_epoch(entry(2, 1), true, old_epoch));
    }

    #[test]
    fn access_tick_overflow_rebases_without_evicting_the_newest_entry() {
        let cache = MemoryCache::new();
        insert_short(&cache, entry(1, 8 * 1024 * 1024));
        insert_short(&cache, entry(2, 1024));
        {
            let mut state = cache.state.lock().unwrap();
            state.next_access_tick = u64::MAX;
        }

        assert!(cache.lookup(&key(1)).is_some(), "访问 1 号使其成为最新项");
        insert_short(&cache, entry(3, 2 * 1024 * 1024));

        assert!(
            cache.lookup(&key(1)).is_some(),
            "最新访问项不应因 tick 回绕被淘汰"
        );
        assert!(cache.lookup(&key(2)).is_none(), "较旧项应先淘汰");
        assert!(cache.lookup(&key(3)).is_some());
    }

    #[test]
    fn overwrite_refreshes_value_and_metrics() {
        let cache = MemoryCache::new();
        let mut old = entry(1, 100);
        old.result = Arc::new(result("旧译文"));
        insert_short(&cache, old);
        assert_eq!(cache.lookup(&key(1)).unwrap().translated_text, "旧译文");
        let mut fresh = entry(1, 500);
        fresh.result = Arc::new(result("新译文"));
        insert_short(&cache, fresh);
        let state = cache.state.lock().unwrap();
        assert_eq!(state.total_logical_bytes, 500, "覆盖后计量为最新值");
        assert_eq!(
            state.short_pool.peek(&key(1)).expect("present").hit_count,
            0
        );
        drop(state);
        assert_eq!(cache.lookup(&key(1)).unwrap().translated_text, "新译文");
    }
}
