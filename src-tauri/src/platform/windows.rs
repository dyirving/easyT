use std::thread;
use std::time::Duration;

use enigo::{Button, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use windows_sys::Win32::Foundation::POINT;
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITOR_DEFAULTTONEAREST, MONITORINFO,
};

use crate::app_error::{AppError, AppResult};

/// 屏幕矩形（左上角 + 右下角，逻辑像素）
#[derive(Debug, Clone, Copy)]
pub struct ScreenRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// 模拟 Ctrl+C 复制选中文本
/// enigo 0.2 API：Keyboard trait 的 key(Key, Direction) -> InputResult<()>
/// Key::C 是 Windows-only 变体（仅在 windows.rs 内编译，受 cfg 保护）
pub fn simulate_copy() -> AppResult<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| AppError::Internal(format!("Enigo 初始化失败: {e}")))?;
    // 按下 Control + C，再依次释放
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| AppError::Internal(format!("按下 Control 失败: {e}")))?;
    enigo
        .key(Key::C, Direction::Press)
        .map_err(|e| AppError::Internal(format!("按下 C 失败: {e}")))?;
    // 短暂等待让系统处理按键
    thread::sleep(Duration::from_millis(20));
    enigo
        .key(Key::C, Direction::Release)
        .map_err(|e| AppError::Internal(format!("释放 C 失败: {e}")))?;
    enigo
        .key(Key::Control, Direction::Release)
        .map_err(|e| AppError::Internal(format!("释放 Control 失败: {e}")))?;
    Ok(())
}

/// 模拟鼠标左键单击（备用，暂未使用）
#[allow(dead_code)]
pub fn simulate_click() -> AppResult<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| AppError::Internal(format!("Enigo 初始化失败: {e}")))?;
    enigo
        .button(Button::Left, Direction::Press)
        .map_err(|e| AppError::Internal(format!("鼠标按下失败: {e}")))?;
    enigo
        .button(Button::Left, Direction::Release)
        .map_err(|e| AppError::Internal(format!("鼠标释放失败: {e}")))?;
    Ok(())
}

/// 读取剪贴板文本
pub fn read_clipboard_text(app: &AppHandle) -> AppResult<String> {
    app.clipboard()
        .read_text()
        .map_err(|e| AppError::ClipboardError(format!("读取剪贴板失败: {e}")))
}

/// 写入文本到剪贴板
pub fn write_clipboard_text(app: &AppHandle, text: &str) -> AppResult<()> {
    app.clipboard()
        .write_text(text)
        .map_err(|e| AppError::ClipboardError(format!("写入剪贴板失败: {e}")))
}

/// 捕获当前选中的文本
/// 流程：
/// 1. 保存当前剪贴板内容（用于恢复）
/// 2. 清空剪贴板
/// 3. 模拟 Ctrl+C 触发系统复制
/// 4. 等待系统把选中内容写入剪贴板
/// 5. 读取剪贴板文本
/// 6. 恢复原剪贴板内容
/// 7. 返回捕获到的文本
///
/// 注意：此函数会暂时占用剪贴板，调用方应确保调用期间用户不会主动复制其他内容
pub fn capture_selection(app: &AppHandle) -> AppResult<String> {
    // 1. 保存当前剪贴板内容
    let original = app.clipboard().read_text().ok();

    // 2. 清空剪贴板（写入空字符串），以便后面判断是否真的有新内容
    let _ = app.clipboard().write_text("");

    // 3. 模拟 Ctrl+C
    simulate_copy()?;

    // 4. 等待系统把选中内容写入剪贴板
    // 50ms 通常足够；某些慢应用可能需要更长，但太长会影响用户体验
    thread::sleep(Duration::from_millis(50));

    // 5. 读取剪贴板文本
    let text = read_clipboard_text(app)?;

    // 6. 恢复原剪贴板内容
    // 恢复失败不阻断流程，仅记录日志
    if let Some(orig) = original {
        if let Err(e) = app.clipboard().write_text(&orig) {
            log::warn!("恢复剪贴板内容失败: {e}");
        }
    }

    // 7. 返回结果
    Ok(text)
}

/// 获取当前鼠标光标的屏幕坐标
/// 使用 enigo 的 Mouse::location()，避免再引入 GetCursorPos
/// 返回 (x, y) 逻辑像素坐标（与 Win32 屏幕坐标系一致：主屏左上角为原点，向右为正 X，向下为正 Y）
pub fn get_mouse_position() -> AppResult<(i32, i32)> {
    let enigo = Enigo::new(&Settings::default())
        .map_err(|e| AppError::Internal(format!("Enigo 初始化失败: {e}")))?;
    enigo
        .location()
        .map_err(|e| AppError::Internal(format!("获取鼠标位置失败: {e}")))
}

/// 获取指定坐标所在显示器的工作区（rcWork）
/// 工作区已排除任务栏等保留区域，适合用于窗口定位
/// 若查询失败（极端情况：显示器被拔出），回退到主屏工作区
pub fn get_monitor_work_area(point: (i32, i32)) -> AppResult<ScreenRect> {
    let pt = POINT { x: point.0, y: point.1 };
    // MONITOR_DEFAULTTONEAREST：若点不在任何显示器上，返回最近的那块
    let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return Err(AppError::Internal(
            "MonitorFromPoint 返回 NULL，无法确定显示器".to_string(),
        ));
    }

    // MONITORINFO 初始化：cbSize 必须按字节填好，否则 GetMonitorInfoW 会失败
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

    let ok = unsafe { GetMonitorInfoW(monitor, &mut info) };
    // windows-sys 中 BOOL 是 i32；非 0 表示成功
    if ok == 0 {
        return Err(AppError::Internal(format!(
            "GetMonitorInfoW 失败"
        )));
    }

    let rc = info.rcWork;
    Ok(ScreenRect {
        left: rc.left,
        top: rc.top,
        right: rc.right,
        bottom: rc.bottom,
    })
}
