use tauri::AppHandle;

use crate::app_error::{AppError, AppResult};
use crate::platform::current::capture_selection;

/// 捕获选中文本
/// 流程：保存原剪贴板 → 清空 → 模拟 Ctrl+C → 等待 50ms → 读取 → 恢复原剪贴板
/// 阻塞型操作放到 spawn_blocking 中执行，避免阻塞 async 运行时
#[tauri::command]
pub async fn capture_selected_text(app: AppHandle) -> AppResult<String> {
    let app_clone = app.clone();
    let text = tokio::task::spawn_blocking(move || capture_selection(&app_clone))
        .await
        .map_err(|e| AppError::Internal(format!("捕获选中文本任务失败: {e}")))??;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(AppError::NoSelectedText);
    }
    Ok(trimmed.to_string())
}
