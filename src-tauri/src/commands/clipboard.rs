use tauri::AppHandle;

use crate::app_error::AppResult;
use crate::platform::current::write_clipboard_text;

/// 复制译文到剪贴板
/// 阶段7：调用 platform::current::write_clipboard_text 写入文本
/// 写入是阻塞型操作（剪贴板内核调用），但耗时极短，无需 spawn_blocking
#[tauri::command]
pub async fn copy_translation(app: AppHandle, text: String) -> AppResult<()> {
    write_clipboard_text(&app, &text)
}
