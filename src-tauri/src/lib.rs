mod app_error;
mod commands;
mod config;
mod llm;
mod platform;
mod shortcut;
mod translation_backend;
mod window_state;

use std::sync::Arc;

use commands::{
    clipboard::copy_translation,
    config::{get_config, save_config, AppState},
    selection::capture_selected_text,
    translate::{test_api_connection, test_connection, translate_text, TranslationRequestManager},
    web_gateway::{begin_web_login, get_web_login_status, logout_web_account},
    window::{
        hide_translation_window, position_window_near_mouse, set_window_pinned,
        show_translation_window,
    },
};
use config::{app_data_dir, load_config};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use translation_backend::TranslationBackend;

/// 托盘菜单事件
const TRAY_EVENT_SHOW: &str = "tray://show";
const TRAY_EVENT_SETTINGS: &str = "tray://settings";
const TRAY_EVENT_QUIT: &str = "tray://quit";

/// 窗口标签
const MAIN_WINDOW_LABEL: &str = "main";
const QWEN_LOGIN_WINDOW_LABEL: &str = "qwen-login";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app_data_dir()?;
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .clear_targets()
                        .target(tauri_plugin_log::Target::new(
                            tauri_plugin_log::TargetKind::Folder {
                                path: data_dir.join("logs"),
                                file_name: None,
                            },
                        ))
                        .build(),
                )?;
            }

            // 显式指定 WebView2 用户数据目录，避免其默认落入 LocalAppData。
            WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
                .title("easyT")
                .inner_size(520.0, 390.0)
                .min_inner_size(360.0, 200.0)
                .max_inner_size(900.0, 700.0)
                .resizable(true)
                .fullscreen(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .center()
                .data_directory(data_dir.join("webview"))
                .build()?;

            // 启动时加载配置到 AppState
            let config = match load_config() {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("加载配置失败，使用默认配置: {e}");
                    config::default_config()
                }
            };
            // 快捷键副本用于初始化全局快捷键
            let shortcut_str = config.shortcut.clone();
            app.manage(AppState::new(config));
            app.manage(TranslationRequestManager::new());

            // 初始化 TranslationBackend 并恢复 Qwen 登录态
            let http_client = reqwest::Client::new();
            let backend = TranslationBackend::new(http_client);
            // 启动时只检查 credentials.bin 是否存在且格式有效，不创建登录 WebView
            let qwen_session = backend.web_gateway().qwen_session();
            qwen_session.restore_from_storage(&data_dir);
            app.manage(Arc::new(backend));

            window_state::restore_main_window_size(app.handle());
            if let Some(win) = app.get_webview_window(MAIN_WINDOW_LABEL) {
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
        // 拦截窗口关闭：
        // - 主窗口：隐藏到托盘而非退出（保留现有行为）
        // - qwen-login：允许真正关闭，并通知 QwenSession 结束登录
        .on_window_event(|window, event| match event {
            WindowEvent::Resized(size) if window.label() == MAIN_WINDOW_LABEL => {
                window_state::schedule_main_window_size_save(*size);
            }
            WindowEvent::CloseRequested { api, .. } => {
                if window.label() == MAIN_WINDOW_LABEL {
                    api.prevent_close();
                    let _ = window.hide();
                } else if window.label() == QWEN_LOGIN_WINDOW_LABEL {
                    // 允许真正关闭：通知 QwenSession 取消登录 watcher
                    if let Some(backend) =
                        window.app_handle().try_state::<Arc<TranslationBackend>>()
                    {
                        let session = backend.web_gateway().qwen_session();
                        session.cancel_login();
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            capture_selected_text,
            translate_text,
            test_api_connection,
            test_connection,
            show_translation_window,
            hide_translation_window,
            set_window_pinned,
            position_window_near_mouse,
            copy_translation,
            begin_web_login,
            get_web_login_status,
            logout_web_account,
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
                if let Some(win) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "settings" => {
                let _ = app.emit(TRAY_EVENT_SETTINGS, ());
                if let Some(win) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                let _ = app.emit(TRAY_EVENT_QUIT, ());
                // 退出前清理 Qwen 登录窗口与 watcher
                if let Some(backend) = app.try_state::<Arc<TranslationBackend>>() {
                    let session = backend.web_gateway().qwen_session();
                    session.cancel_watcher();
                }
                if let Some(qwen_win) = app.get_webview_window(QWEN_LOGIN_WINDOW_LABEL) {
                    let _ = qwen_win.close();
                }
                if let Some(win) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                    if let Ok(size) = win.inner_size() {
                        if let Err(e) = window_state::save_main_window_size(size) {
                            log::warn!("退出前保存窗口尺寸失败: {e}");
                        }
                    }
                }
                if let Err(e) = shortcut::unregister_all(app) {
                    log::warn!("退出前注销快捷键失败: {e}");
                }
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
                if let Some(win) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
