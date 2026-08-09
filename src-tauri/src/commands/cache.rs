//! 显式缓存详情命令；普通翻译不通过此模块报告缓存故障。

use std::sync::Arc;

use tauri::State;

use crate::app_error::{AppError, AppResult};
use crate::translation_backend::cache::{CacheStatsView, TranslationCache};

#[tauri::command]
pub async fn get_translation_cache_stats(
    cache: State<'_, Arc<TranslationCache>>,
) -> AppResult<CacheStatsView> {
    cache
        .stats()
        .await
        .map_err(|_| AppError::CacheOperationFailed("无法读取缓存详情".to_string()))
}

#[tauri::command]
pub async fn clear_translation_cache(
    cache: State<'_, Arc<TranslationCache>>,
) -> AppResult<CacheStatsView> {
    cache
        .clear()
        .await
        .map_err(|_| AppError::CacheOperationFailed("无法清除翻译缓存".to_string()))
}
