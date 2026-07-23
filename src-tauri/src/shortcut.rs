use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent};

use crate::app_error::{AppError, AppResult};

/// 快捷键触发事件名（前端监听此事件触发翻译流程）
pub const SHORTCUT_EVENT_TRANSLATE: &str = "shortcut://translate";

/// 全局快捷键管理器：保存当前已注册的快捷键（原始字符串 + 解析后的 Shortcut）
/// 保存字符串便于前端展示与判断是否变更
pub struct ShortcutManager {
    current_str: Mutex<Option<String>>,
    current_sc: Mutex<Option<Shortcut>>,
}

impl ShortcutManager {
    pub fn new() -> Self {
        Self {
            current_str: Mutex::new(None),
            current_sc: Mutex::new(None),
        }
    }

    fn current_sc(&self) -> Option<Shortcut> {
        self.current_sc.lock().ok().and_then(|g| g.clone())
    }

    fn current_str(&self) -> Option<String> {
        self.current_str.lock().ok().and_then(|g| g.clone())
    }

    fn set_current(&self, sc: Option<Shortcut>, s: Option<String>) {
        if let Ok(mut g) = self.current_sc.lock() {
            *g = sc;
        }
        if let Ok(mut g) = self.current_str.lock() {
            *g = s;
        }
    }
}

/// 把字符串形式（如 "Ctrl+T"）解析为 Shortcut
/// 支持 Ctrl/Control/ControlOrCommand/Command/Alt/Shift + 字母/数字/F键
pub fn parse_shortcut(s: &str) -> AppResult<Shortcut> {
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    let s = s.trim();
    if s.is_empty() {
        return Err(AppError::ShortcutRegistrationFailed(
            "快捷键不能为空".to_string(),
        ));
    }

    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in s.split('+') {
        let p = part.trim();
        match p.to_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "meta" | "win" | "cmd" | "command" => mods |= Modifiers::SUPER,
            // "CommandOrControl"：跨平台兼容写法，Windows/Linux 上为 Control
            "commandorcontrol" | "cmdorctrl" => mods |= Modifiers::CONTROL,
            _ => {
                if code.is_some() {
                    return Err(AppError::ShortcutRegistrationFailed(
                        format!("快捷键格式错误：包含多个主键 '{s}'"),
                    ));
                }
                code = Some(parse_code(p)?);
            }
        }
    }

    let code = code.ok_or_else(|| {
        AppError::ShortcutRegistrationFailed("快捷键缺少主键".to_string())
    })?;

    Ok(Shortcut::new(Some(mods), code))
}

/// 解析单个按键名到 Code
fn parse_code(name: &str) -> AppResult<tauri_plugin_global_shortcut::Code> {
    use tauri_plugin_global_shortcut::Code;
    let lower = name.to_lowercase();
    let code = match lower.as_str() {
        // 字母
        "a" => Code::KeyA, "b" => Code::KeyB, "c" => Code::KeyC, "d" => Code::KeyD,
        "e" => Code::KeyE, "f" => Code::KeyF, "g" => Code::KeyG, "h" => Code::KeyH,
        "i" => Code::KeyI, "j" => Code::KeyJ, "k" => Code::KeyK, "l" => Code::KeyL,
        "m" => Code::KeyM, "n" => Code::KeyN, "o" => Code::KeyO, "p" => Code::KeyP,
        "q" => Code::KeyQ, "r" => Code::KeyR, "s" => Code::KeyS, "t" => Code::KeyT,
        "u" => Code::KeyU, "v" => Code::KeyV, "w" => Code::KeyW, "x" => Code::KeyX,
        "y" => Code::KeyY, "z" => Code::KeyZ,
        // 数字
        "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2,
        "3" => Code::Digit3, "4" => Code::Digit4, "5" => Code::Digit5,
        "6" => Code::Digit6, "7" => Code::Digit7, "8" => Code::Digit8,
        "9" => Code::Digit9,
        // F 键
        "f1" => Code::F1, "f2" => Code::F2, "f3" => Code::F3, "f4" => Code::F4,
        "f5" => Code::F5, "f6" => Code::F6, "f7" => Code::F7, "f8" => Code::F8,
        "f9" => Code::F9, "f10" => Code::F10, "f11" => Code::F11, "f12" => Code::F12,
        // 空格、回车、ESC
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "esc" | "escape" => Code::Escape,
        "tab" => Code::Tab,
        _ => {
            return Err(AppError::ShortcutRegistrationFailed(format!(
                "无法识别的按键: {name}"
            )))
        }
    };
    Ok(code)
}

/// 注册快捷键：先注销旧的，再注册新的
/// 失败时不影响旧快捷键（保持原状态）
pub fn register(app: &AppHandle, shortcut_str: &str) -> AppResult<()> {
    let new_sc = parse_shortcut(shortcut_str)?;
    let state = app.state::<ShortcutManager>();

    // 1. 注销旧的（如果存在）
    if let Some(old) = state.current_sc() {
        if let Err(e) = app.global_shortcut().unregister(old) {
            log::warn!("注销旧快捷键失败: {e}");
        }
    }

    // 2. 注册新的
    let app_clone = app.clone();
    let handler = move |_app: &AppHandle, _sc: &Shortcut, event: ShortcutEvent| {
        // 只处理按下事件（避免按下+释放都触发）
        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
            log::info!("快捷键触发");
            let _ = app_clone.emit(SHORTCUT_EVENT_TRANSLATE, ());
        }
    };

    match app.global_shortcut().on_shortcut(new_sc, handler) {
        Ok(()) => {
            state.set_current(Some(new_sc), Some(shortcut_str.to_string()));
            log::info!("已注册快捷键: {shortcut_str}");
            Ok(())
        }
        Err(e) => {
            // 注册失败：尝试恢复旧的（如果之前有）
            if let Some(old) = state.current_sc() {
                let _ = app.global_shortcut().on_shortcut(old, |_, _, _| {});
            }
            Err(AppError::ShortcutRegistrationFailed(format!(
                "注册快捷键失败: {e}（可能已被其他应用占用）"
            )))
        }
    }
}

/// 注销所有已注册的快捷键
pub fn unregister_all(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<ShortcutManager>();
    if let Some(old) = state.current_sc() {
        let _ = app.global_shortcut().unregister(old);
        state.set_current(None, None);
    }
    Ok(())
}

/// 初始化快捷键插件并注册默认快捷键
/// 在 setup 中调用
pub fn init(app: &AppHandle, default_shortcut: &str) -> AppResult<()> {
    // 注册状态容器
    app.manage(ShortcutManager::new());

    // 注册默认快捷键（失败仅 warn，不阻塞启动）
    if let Err(e) = register(app, default_shortcut) {
        log::warn!("初始化快捷键失败: {e}");
    }
    Ok(())
}

/// 查询当前已注册的快捷键字符串
/// 用于前端显示与设置页初始化
pub fn current_shortcut_str(app: &AppHandle) -> Option<String> {
    let state = app.state::<ShortcutManager>();
    state.current_str()
}
