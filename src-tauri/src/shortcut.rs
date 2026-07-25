use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent};

use crate::app_error::{AppError, AppResult};

/// 快捷键触发事件名（前端监听此事件触发翻译流程）
pub const SHORTCUT_EVENT_TRANSLATE: &str = "shortcut://translate";

#[derive(Default)]
struct ShortcutState {
    current_sc: Option<Shortcut>,
    stale_scs: Vec<Shortcut>,
}

/// 全局快捷键管理器。
/// 当前快捷键与待清理快捷键属于同一个不变量，必须由同一把锁保护。
pub struct ShortcutManager {
    state: Mutex<ShortcutState>,
}

impl ShortcutManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(ShortcutState::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ShortcutState> {
        self.state.lock().unwrap_or_else(|poisoned| {
            log::warn!("快捷键状态锁曾发生 panic，继续使用锁内状态");
            poisoned.into_inner()
        })
    }

    fn current_sc(&self) -> Option<Shortcut> {
        self.lock().current_sc
    }

    fn set_current(&self, sc: Option<Shortcut>) {
        let mut g = self.lock();
        g.current_sc = sc;
    }

    fn add_stale(&self, sc: Shortcut) {
        self.lock().stale_scs.push(sc);
    }

    fn tracked_shortcuts(&self) -> Vec<Shortcut> {
        let g = self.lock();
        g.current_sc
            .into_iter()
            .chain(g.stale_scs.iter().copied())
            .collect()
    }

    fn retain_cleanup_failures(&self, failed: Vec<Shortcut>) {
        let mut g = self.lock();
        g.current_sc = None;
        g.stale_scs = failed;
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
                    return Err(AppError::ShortcutRegistrationFailed(format!(
                        "快捷键格式错误：包含多个主键 '{s}'"
                    )));
                }
                code = Some(parse_code(p)?);
            }
        }
    }

    let code =
        code.ok_or_else(|| AppError::ShortcutRegistrationFailed("快捷键缺少主键".to_string()))?;

    Ok(Shortcut::new(Some(mods), code))
}

/// 解析单个按键名到 Code
fn parse_code(name: &str) -> AppResult<tauri_plugin_global_shortcut::Code> {
    use tauri_plugin_global_shortcut::Code;
    let lower = name.to_lowercase();
    let code = match lower.as_str() {
        // 字母
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        // 数字
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        // F 键
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
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

fn shortcut_handler(
    app: AppHandle,
) -> impl Fn(&AppHandle, &Shortcut, ShortcutEvent) + Send + Sync + 'static {
    move |_app: &AppHandle, _sc: &Shortcut, event: ShortcutEvent| {
        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
            log::info!("快捷键触发");
            let _ = app.emit(SHORTCUT_EVENT_TRANSLATE, ());
        }
    }
}

pub struct ShortcutReplacement {
    old_sc: Option<Shortcut>,
    new_sc: Shortcut,
    new_str: String,
}

/// 预注册新快捷键，但暂不修改当前状态，也不注销旧快捷键。
pub fn prepare_replacement(app: &AppHandle, shortcut_str: &str) -> AppResult<ShortcutReplacement> {
    let state = app.state::<ShortcutManager>();
    let old_sc = state.current_sc();
    let new_sc = parse_shortcut(shortcut_str)?;

    match app
        .global_shortcut()
        .on_shortcut(new_sc, shortcut_handler(app.clone()))
    {
        Ok(()) => Ok(ShortcutReplacement {
            old_sc,
            new_sc,
            new_str: shortcut_str.to_string(),
        }),
        Err(e) => Err(AppError::ShortcutRegistrationFailed(format!(
            "注册快捷键失败: {e}（可能已被其他应用占用）"
        ))),
    }
}

/// 提交快捷键替换：更新当前状态，并尽量注销旧快捷键。
/// 状态提交本身不会失败；旧快捷键注销失败时保留追踪，供退出时重试。
pub fn commit_replacement(app: &AppHandle, replacement: ShortcutReplacement) {
    let state = app.state::<ShortcutManager>();
    state.set_current(Some(replacement.new_sc));

    if let Some(old) = replacement.old_sc {
        if let Err(e) = app.global_shortcut().unregister(old) {
            log::warn!("注销旧快捷键失败，将在退出时继续清理: {e}");
            state.add_stale(old);
        }
    }

    log::info!("已注册快捷键: {}", replacement.new_str);
}

/// 放弃快捷键替换：注销预注册的新快捷键，保留旧快捷键状态。
pub fn rollback_replacement(app: &AppHandle, replacement: ShortcutReplacement) -> AppResult<()> {
    if let Err(e) = app.global_shortcut().unregister(replacement.new_sc) {
        log::warn!("回滚新快捷键失败: {e}");
        app.state::<ShortcutManager>().add_stale(replacement.new_sc);
        return Err(AppError::ShortcutRegistrationFailed(format!(
            "回滚新快捷键失败: {e}"
        )));
    }
    Ok(())
}

/// 注册快捷键：用于启动初始化等无外部事务场景。
pub fn register(app: &AppHandle, shortcut_str: &str) -> AppResult<()> {
    let replacement = prepare_replacement(app, shortcut_str)?;
    commit_replacement(app, replacement);
    Ok(())
}

/// 注销所有已注册的快捷键
pub fn unregister_all(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<ShortcutManager>();
    let mut failed = Vec::new();
    let mut errors = Vec::new();

    for sc in state.tracked_shortcuts() {
        if let Err(e) = app.global_shortcut().unregister(sc) {
            failed.push(sc);
            errors.push(e.to_string());
        }
    }
    state.retain_cleanup_failures(failed);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::ShortcutRegistrationFailed(format!(
            "注销快捷键失败: {}",
            errors.join("; ")
        )))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    fn shortcut(code: Code) -> Shortcut {
        Shortcut::new(Some(Modifiers::CONTROL), code)
    }

    #[test]
    fn cleanup_failures_remain_tracked_as_stale() {
        let manager = ShortcutManager::new();
        let current = shortcut(Code::KeyT);
        let stale = shortcut(Code::KeyY);
        manager.set_current(Some(current));
        manager.add_stale(stale);

        manager.retain_cleanup_failures(vec![current]);

        assert_eq!(manager.current_sc(), None);
        assert_eq!(manager.tracked_shortcuts(), vec![current]);
    }
}
