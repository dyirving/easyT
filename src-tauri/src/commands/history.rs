use std::sync::Arc;

use tauri::State;

use crate::app_error::{AppError, AppResult};
use crate::translation_history::{
    ClearHistoryResult, HistorySnapshot, TranslationHistory, TranslationHistoryEntry,
};

#[tauri::command]
pub async fn initialize_translation_history(
    history: State<'_, Arc<TranslationHistory>>,
) -> AppResult<HistorySnapshot> {
    history.initialize().await.map_err(map_history_error)
}

#[tauri::command]
pub async fn get_translation_history_entry(
    history: State<'_, Arc<TranslationHistory>>,
    entry_id: String,
) -> AppResult<TranslationHistoryEntry> {
    history.get_entry(entry_id).await.map_err(map_history_error)
}

#[tauri::command]
pub async fn clear_translation_history(
    history: State<'_, Arc<TranslationHistory>>,
) -> AppResult<ClearHistoryResult> {
    history.clear_all().await.map_err(map_history_error)
}

fn map_history_error(_error: crate::translation_history::HistoryError) -> AppError {
    AppError::HistoryOperationFailed("无法完成翻译历史操作".to_string())
}
