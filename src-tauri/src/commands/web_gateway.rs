//! WebGateway 登录管理 Commands
//!
//! - begin_web_login: 启动非阻塞登录流程
//! - get_web_login_status: 查询当前登录状态
//! - logout_web_account: 退出登录（显式 destructive 操作）
//!
//! 安全要求：
//! - Commands 只能被 `main` 窗口调用
//! - `qwen-login` 不在 capability 的 windows 列表中
//! - provider 只接受枚举，不接受任意 URL
//! - 前端不能传 Cookie、ticket、Base URL 或 Header
//! - logout 关闭登录窗口、取消 watcher、清除凭证和 Qwen profile

use std::sync::Arc;

use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::app_error::{AppError, AppResult};
use crate::config::app_data_dir;
use crate::translation_backend::models::WebProviderKind;
use crate::translation_backend::web_gateway::qwen::{QwenSession, QwenSessionStatus};
use crate::translation_backend::TranslationBackend;

/// 开始 Qwen 网页登录流程
///
/// 非阻塞：立即返回当前状态，不等待用户完成登录。
/// 后台 watcher 会读取 `tongyi_sso_ticket` Cookie，找到后明文保存并切到 Ready。
#[tauri::command]
pub async fn begin_web_login(
    app: AppHandle,
    backend: State<'_, Arc<TranslationBackend>>,
    provider: WebProviderKind,
) -> AppResult<QwenSessionStatus> {
    match provider {
        WebProviderKind::Qwen => {
            let web_gateway = backend.web_gateway();
            let session = web_gateway.qwen_session();

            if !session.try_begin_login() {
                // 已经在登录中：直接返回当前状态
                return Ok(session.status());
            }

            // 在锁外创建登录窗口
            let app_for_window = app.clone();
            let session_for_watcher: Arc<QwenSession> = session.clone();

            if let Err(error) = spawn_qwen_login_window(app_for_window.clone()) {
                session.fail_login("无法打开 Qwen 登录窗口");
                return Err(error);
            }
            spawn_qwen_login_watcher(app_for_window, session_for_watcher);

            Ok(session.status())
        }
    }
}

/// 查询当前登录状态
#[tauri::command]
pub async fn get_web_login_status(
    backend: State<'_, Arc<TranslationBackend>>,
    provider: WebProviderKind,
) -> AppResult<QwenSessionStatus> {
    match provider {
        WebProviderKind::Qwen => {
            let session = backend.web_gateway().qwen_session();
            Ok(session.status())
        }
    }
}

/// 退出登录：关闭登录窗口、取消 watcher、清除凭证与 profile
#[tauri::command]
pub async fn logout_web_account(
    app: AppHandle,
    backend: State<'_, Arc<TranslationBackend>>,
    provider: WebProviderKind,
) -> AppResult<QwenSessionStatus> {
    match provider {
        WebProviderKind::Qwen => {
            let session = backend.web_gateway().qwen_session();
            let app_data = app_data_dir()?;
            session.logout(&app_data).map_err(AppError::from)?;

            if let Some(win) = app.get_webview_window("qwen-login") {
                if let Err(error) = win.close() {
                    log::warn!("关闭 Qwen 登录窗口失败: {error}");
                }
            }

            // WebView2 关闭后 profile 句柄可能短暂未释放，有限重试清理。
            let profile_result = tokio::task::spawn_blocking(move || {
                let mut last_error = None;
                for attempt in 0..5 {
                    match crate::translation_backend::web_gateway::credential_store::delete_qwen_profile(
                        &app_data,
                    ) {
                        Ok(()) => return Ok(()),
                        Err(error) => last_error = Some(error),
                    }
                    if attempt < 4 {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                Err(last_error.expect("profile cleanup attempted"))
            })
            .await
            .map_err(|e| AppError::Internal(format!("Qwen profile 清理任务失败: {e}")))?;
            profile_result
                .map_err(|e| AppError::Internal(format!("删除 Qwen profile 失败: {e}")))?;

            Ok(session.status())
        }
    }
}

/// 创建 Qwen 登录窗口
fn spawn_qwen_login_window(app: AppHandle) -> AppResult<()> {
    use crate::translation_backend::web_gateway::qwen::QWEN_LOGIN_URL;
    use crate::translation_backend::web_gateway::qwen::QWEN_LOGIN_WINDOW_LABEL;

    // 已存在则只聚焦
    if let Some(win) = app.get_webview_window(QWEN_LOGIN_WINDOW_LABEL) {
        win.show()
            .map_err(|e| AppError::WindowError(format!("显示 Qwen 登录窗口失败: {e}")))?;
        win.set_focus()
            .map_err(|e| AppError::WindowError(format!("聚焦 Qwen 登录窗口失败: {e}")))?;
        return Ok(());
    }

    let app_data = app_data_dir()?;
    let profile_dir =
        crate::translation_backend::web_gateway::credential_store::qwen_profile_path(&app_data);
    if let Some(parent) = profile_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(format!("创建 Qwen profile 父目录失败: {e}")))?;
    }

    let window = WebviewWindowBuilder::new(
        &app,
        QWEN_LOGIN_WINDOW_LABEL,
        WebviewUrl::External(
            QWEN_LOGIN_URL
                .parse()
                .map_err(|e| AppError::Internal(format!("Qwen 登录 URL 解析失败: {e}")))?,
        ),
    )
    .title("登录 Qwen")
    .inner_size(1000.0, 720.0)
    .min_inner_size(800.0, 600.0)
    .resizable(true)
    .always_on_top(false)
    .decorations(true)
    .skip_taskbar(false)
    .visible(false)
    .center()
    .data_directory(profile_dir)
    .on_navigation(|url| {
        if is_allowed_qwen_login_host(url.host_str()) {
            true
        } else {
            // 不记录 query/fragment，避免 OAuth 参数进入日志。
            log::warn!(
                "Qwen 登录窗口拒绝跳转: host={:?}, path={}",
                url.host_str(),
                url.path()
            );
            false
        }
    })
    .build()
    .map_err(|e| AppError::WindowError(format!("创建 Qwen 登录窗口失败: {e}")))?;

    window
        .show()
        .map_err(|e| AppError::WindowError(format!("显示 Qwen 登录窗口失败: {e}")))?;
    window
        .set_focus()
        .map_err(|e| AppError::WindowError(format!("聚焦 Qwen 登录窗口失败: {e}")))?;
    Ok(())
}

fn is_allowed_qwen_login_host(host: Option<&str>) -> bool {
    matches!(
        host,
        Some("qianwen.com")
            | Some("www.qianwen.com")
            | Some("account.qianwen.com")
            | Some("login.taobao.com")
            | Some("passport.taobao.com")
            | Some("oauth.taobao.com")
    )
}

/// 启动后台 watcher：轮询 Cookie，找到 ticket 后明文保存并切到 Ready
fn spawn_qwen_login_watcher(app: AppHandle, session: Arc<QwenSession>) {
    use crate::translation_backend::web_gateway::qwen::{
        LOGIN_WATCHER_INTERVAL, LOGIN_WATCHER_TIMEOUT, QWEN_LOGIN_WINDOW_LABEL,
        QWEN_TICKET_COOKIE_NAME,
    };

    tokio::spawn(async move {
        let deadline = std::time::Instant::now() + LOGIN_WATCHER_TIMEOUT;
        let mut found_ticket: Option<String> = None;

        while session.watcher_should_run() && std::time::Instant::now() < deadline {
            let win = app.get_webview_window(QWEN_LOGIN_WINDOW_LABEL);
            if let Some(win) = win {
                let cookie_result = tokio::task::spawn_blocking(move || {
                    win.cookies().ok().and_then(|cookies| {
                        cookies.into_iter().find_map(|cookie| {
                            if cookie.name() == QWEN_TICKET_COOKIE_NAME
                                && !cookie.value().is_empty()
                            {
                                Some(cookie.value().to_string())
                            } else {
                                None
                            }
                        })
                    })
                })
                .await;
                match cookie_result {
                    Ok(ticket) => found_ticket = ticket,
                    Err(error) => {
                        log::warn!("Qwen Cookie 读取任务失败: {error}");
                        session.fail_login("读取 Qwen 登录状态失败");
                        return;
                    }
                }
            }

            if let Some(mut ticket) = found_ticket {
                use zeroize::Zeroize;

                let app_data = match app_data_dir() {
                    Ok(p) => p,
                    Err(e) => {
                        log::warn!("无法定位应用数据目录: {e}");
                        ticket.zeroize();
                        session.fail_login("无法定位凭证存储目录");
                        return;
                    }
                };
                let save_result = session.complete_login(&app_data, &ticket);
                ticket.zeroize();
                match save_result {
                    Ok(()) => {
                        log::info!("Qwen 登录成功");
                        if let Some(win) = app.get_webview_window(QWEN_LOGIN_WINDOW_LABEL) {
                            let _ = win.close();
                        }
                    }
                    Err(e) => {
                        log::warn!("保存 Qwen 凭证失败: {:?}", e);
                        session.fail_login("保存 Qwen 登录凭证失败");
                    }
                }
                return;
            }

            tokio::time::sleep(LOGIN_WATCHER_INTERVAL).await;
        }

        if session.watcher_should_run() {
            log::info!("Qwen 登录 watcher 超时");
            session.fail_login("Qwen 登录超时");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_login_entry_is_allowed_but_obsolete_host_is_not() {
        assert_eq!(
            crate::translation_backend::web_gateway::qwen::QWEN_LOGIN_URL,
            "https://www.qianwen.com/"
        );
        assert!(is_allowed_qwen_login_host(Some("www.qianwen.com")));
        assert!(!is_allowed_qwen_login_host(Some("chat2.qianwen.com")));
        assert!(!is_allowed_qwen_login_host(Some("evil.example")));
    }
}
