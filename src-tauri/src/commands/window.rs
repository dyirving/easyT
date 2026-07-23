use crate::app_error::{AppError, AppResult};
use tauri::{Manager, PhysicalPosition, WebviewWindow};

const MAIN_WINDOW_LABEL: &str = "main";

/// 鼠标右下偏移量（物理像素）
const MOUSE_OFFSET: i32 = 16;

/// 获取主窗口
fn get_main_window(app: &tauri::AppHandle) -> AppResult<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| AppError::WindowError("主窗口未找到".to_string()))
}

/// 显示翻译窗口
/// 取消置顶并不影响：只是从隐藏状态切回显示并聚焦
#[tauri::command]
pub async fn show_translation_window(app: tauri::AppHandle) -> AppResult<()> {
    let win = get_main_window(&app)?;
    win.show()
        .map_err(|e| AppError::WindowError(e.to_string()))?;
    win.set_focus()
        .map_err(|e| AppError::WindowError(e.to_string()))?;
    Ok(())
}

/// 隐藏翻译窗口（不退出应用）
#[tauri::command]
pub async fn hide_translation_window(app: tauri::AppHandle) -> AppResult<()> {
    let win = get_main_window(&app)?;
    win.hide()
        .map_err(|e| AppError::WindowError(e.to_string()))?;
    Ok(())
}

/// 设置窗口固定状态
/// pinned 状态主要由前端 store 管理，Rust 端命令保留用于：
/// - 阶段8 判断是否需要重新定位窗口到鼠标附近
/// - 后续可能影响 always_on_top 等窗口属性
#[tauri::command]
pub async fn set_window_pinned(_app: tauri::AppHandle, _pinned: bool) -> AppResult<()> {
    // 当前阶段状态在前端 store 中，无需 Rust 端持久化
    Ok(())
}

/// 把主窗口重新定位到鼠标附近
/// 阶段8：
/// - pinned=true 时跳过重新定位（保持当前位置）
/// - 否则获取鼠标位置 + 所在显示器工作区
/// - 计算窗口位置：默认鼠标右下偏移 MOUSE_OFFSET；超出工作区时翻转或贴边
/// - 多显示器：以鼠标所在显示器的工作区为约束，自然支持跨屏
///
/// 注意：调用方应在窗口 show() 之前调用此命令，
/// 这样用户看到窗口时已经在正确位置，避免闪烁
#[tauri::command]
pub async fn position_window_near_mouse(app: tauri::AppHandle, pinned: bool) -> AppResult<()> {
    // 1. pinned=true 时跳过：保持当前窗口位置不动
    if pinned {
        log::debug!("窗口已固定，跳过重新定位");
        return Ok(());
    }

    let win = get_main_window(&app)?;

    // 2. 获取鼠标位置与所在显示器工作区
    //    这两个 Win32 调用都是内核快速路径（微秒级），无需 spawn_blocking
    let pos = crate::platform::current::get_mouse_position()?;
    let work = crate::platform::current::get_monitor_work_area(pos)?;
    let (mx, my) = pos;

    // 3. 当前窗口尺寸（物理像素），用于判断是否超出工作区
    let size = win
        .outer_size()
        .map_err(|e| AppError::WindowError(format!("获取窗口尺寸失败: {e}")))?;
    let w = size.width as i32;
    let h = size.height as i32;

    // 4. 计算窗口左上角坐标
    //    默认放鼠标右下偏移 MOUSE_OFFSET；若超出右/下边界则翻到鼠标左/上
    let mut x = mx + MOUSE_OFFSET;
    let mut y = my + MOUSE_OFFSET;

    if x + w > work.right {
        // 翻到鼠标左侧
        x = mx - MOUSE_OFFSET - w;
    }
    if y + h > work.bottom {
        // 翻到鼠标上方
        y = my - MOUSE_OFFSET - h;
    }

    // 5. 贴边检查：翻转后仍超出左/上边界时贴边
    if x < work.left {
        x = work.left;
    }
    if y < work.top {
        y = work.top;
    }

    // 6. 二次检查右/下边界：应对窗口比工作区还大的极端情况
    if x + w > work.right {
        x = work.right - w;
    }
    if y + h > work.bottom {
        y = work.bottom - h;
    }

    // 7. 应用新位置
    win.set_position(PhysicalPosition { x, y })
        .map_err(|e| AppError::WindowError(format!("设置窗口位置失败: {e}")))?;

    log::debug!(
        "窗口已重新定位到鼠标附近: pos=({x}, {y}), mouse=({mx}, {my}), work_area=({}-{},{}-{})",
        work.left,
        work.right,
        work.top,
        work.bottom
    );
    Ok(())
}
