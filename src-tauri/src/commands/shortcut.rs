use crate::app_error::AppResult;
use crate::shortcut;

/// 注册（或切换）全局快捷键
/// 在设置页保存配置时由前端调用，传入新的快捷键字符串
/// 失败时不会覆盖旧的（由 shortcut::register 内部保证）
#[tauri::command]
pub async fn register_shortcut(app: tauri::AppHandle, shortcut_str: String) -> AppResult<()> {
    shortcut::register(&app, &shortcut_str)
}

/// 注销所有快捷键（暂未在 UI 暴露，保留给后续可能需要的"禁用划词"开关）
#[tauri::command]
pub async fn unregister_all_shortcuts(app: tauri::AppHandle) -> AppResult<()> {
    shortcut::unregister_all(&app)
}

/// 查询当前已注册的快捷键字符串
#[tauri::command]
pub async fn get_current_shortcut(app: tauri::AppHandle) -> AppResult<Option<String>> {
    Ok(shortcut::current_shortcut_str(&app))
}
