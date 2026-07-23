mod app_error;
mod commands;
mod config;
mod llm;
mod platform;
mod shortcut;

use commands::{
    clipboard::copy_translation,
    config::{get_config, save_config, AppState},
    selection::capture_selected_text,
    shortcut::{get_current_shortcut, register_shortcut, unregister_all_shortcuts},
    translate::{test_api_connection, translate_text},
    window::{
        hide_translation_window, position_window_near_mouse, set_window_pinned,
        show_translation_window,
    },
};
use config::load_config;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WindowEvent,
};

/// 托盘菜单事件
const TRAY_EVENT_SHOW: &str = "tray://show";
const TRAY_EVENT_SETTINGS: &str = "tray://settings";
const TRAY_EVENT_QUIT: &str = "tray://quit";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::SIZE)
                .build(),
        )
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // 启动时加载配置到 AppState
            let config = match load_config(app.handle()) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("加载配置失败，使用默认配置: {e}");
                    config::default_config()
                }
            };
            // 快捷键副本用于初始化全局快捷键
            let shortcut_str = config.shortcut.clone();
            app.manage(AppState::new(config));

            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
            }

            // 注册全局快捷键插件
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_clipboard_manager::init())?;

                app.handle()
                    .plugin(tauri_plugin_global_shortcut::Builder::new().build())?;
                // 初始化快捷键管理器并注册默认快捷键
                if let Err(e) = shortcut::init(app.handle(), &shortcut_str) {
                    log::warn!("快捷键初始化失败: {e}");
                }
            }

            // 创建系统托盘
            build_tray(app.handle())?;

            Ok(())
        })
        // 拦截窗口关闭：隐藏到托盘而非退出
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            capture_selected_text,
            translate_text,
            test_api_connection,
            show_translation_window,
            hide_translation_window,
            set_window_pinned,
            position_window_near_mouse,
            copy_translation,
            register_shortcut,
            unregister_all_shortcuts,
            get_current_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 构建系统托盘图标与菜单
fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let item_show = MenuItem::with_id(app, "show", "显示翻译窗口", true, None::<&str>)?;
    let item_settings = MenuItem::with_id(app, "settings", "打开设置", true, None::<&str>)?;
    let item_sep = PredefinedMenuItem::separator(app)?;
    let item_quit = MenuItem::with_id(app, "quit", "退出 easyT", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&item_show, &item_settings, &item_sep, &item_quit])?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("easyT - 划词翻译")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                let _ = app.emit(TRAY_EVENT_SHOW, ());
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "settings" => {
                let _ = app.emit(TRAY_EVENT_SETTINGS, ());
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                let _ = app.emit(TRAY_EVENT_QUIT, ());
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
