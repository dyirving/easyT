use std::future::Future;
use std::sync::{Arc, Mutex};

use crate::app_error::{AppError, AppResult};
use crate::commands::config::AppState;
use crate::translation_backend::{BackendMode, BackendRequest, TranslationBackend};
use tauri::State;

struct ActiveTranslation {
    generation: u64,
    abort_handle: tokio::task::AbortHandle,
}

#[derive(Default)]
struct TranslationRequestState {
    next_generation: u64,
    active: Option<ActiveTranslation>,
}

/// 只保留最新翻译任务。新任务安装时立即取消旧任务，限制并发和内存占用。
pub struct TranslationRequestManager {
    state: Mutex<TranslationRequestState>,
}

impl TranslationRequestManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(TranslationRequestState::default()),
        }
    }

    async fn run_latest<F, T>(&self, future: F) -> AppResult<T>
    where
        F: Future<Output = AppResult<T>> + Send + 'static,
        T: Send + 'static,
    {
        let task = tokio::spawn(future);
        let generation = {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(e) => {
                    task.abort();
                    return Err(AppError::Internal(format!("翻译任务锁获取失败: {e}")));
                }
            };
            state.next_generation = state.next_generation.wrapping_add(1);
            let generation = state.next_generation;
            if let Some(previous) = state.active.replace(ActiveTranslation {
                generation,
                abort_handle: task.abort_handle(),
            }) {
                previous.abort_handle.abort();
            }
            generation
        };

        let result = task.await;
        if let Ok(mut state) = self.state.lock() {
            if state
                .active
                .as_ref()
                .is_some_and(|active| active.generation == generation)
            {
                state.active = None;
            }
        }

        match result {
            Ok(result) => result,
            Err(e) if e.is_cancelled() => Err(AppError::BackendCancelled),
            Err(e) => Err(AppError::Internal(format!("翻译任务执行失败: {e}"))),
        }
    }
}

/// 翻译文本
///
/// 后端入口统一通过 TranslationBackend 路由：
/// - OfficialApi：调用 OpenAI 兼容协议
/// - WebGateway：使用 Qwen 网页登录态
///
/// latest-wins 仍由 TranslationRequestManager 唯一负责。
/// WebGateway 不会自动创建登录窗口或回退到付费 API。
#[tauri::command]
pub async fn translate_text(
    state: State<'_, AppState>,
    request_manager: State<'_, TranslationRequestManager>,
    backend: State<'_, Arc<TranslationBackend>>,
    text: String,
    target_language: String,
) -> AppResult<crate::llm::models::TranslationResult> {
    let config = state.snapshot()?;
    if text.trim().is_empty() {
        return Err(AppError::NoSelectedText);
    }
    if text.chars().count() > config.max_text_length {
        return Err(AppError::TextTooLong);
    }

    let request = BackendRequest {
        text,
        target_language,
    };

    // 提取 Arc<TranslationBackend>，避免 State 的非静态生命周期逃逸到 run_latest 中。
    let backend = backend.inner().clone();
    let result = request_manager
        .run_latest(async move {
            backend
                .translate(&config, request)
                .await
                .map(|r| crate::llm::models::TranslationResult {
                    translated_text: r.translated_text,
                })
                .map_err(AppError::from)
        })
        .await?;

    Ok(result)
}

/// 测试连接
///
/// 通过当前选中的 Adapter 进行真实轻量请求。
/// WebGateway 模式不得仅检查本地 ticket 存在后返回成功。
#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    backend: State<'_, Arc<TranslationBackend>>,
) -> AppResult<String> {
    let config = state.snapshot()?;
    validate_test_config(&config)?;
    let health = backend.test_connection(&config).await?;
    Ok(health.message)
}

/// 兼容 wrapper：保留旧 command 名 `test_api_connection`
#[tauri::command]
pub async fn test_api_connection(
    state: State<'_, AppState>,
    backend: State<'_, Arc<TranslationBackend>>,
    config: crate::config::AppConfig,
) -> AppResult<String> {
    // 优先使用传入的草稿配置（不修改 AppState），便于在设置页测试未保存的配置
    let _ = state.snapshot()?; // 仅校验 AppState 可用
    validate_test_config(&config)?;
    let health = backend.test_connection(&config).await?;
    Ok(health.message)
}

fn validate_test_config(config: &crate::config::AppConfig) -> AppResult<()> {
    match config.backend_mode {
        BackendMode::OfficialApi => {
            crate::commands::config::validate_config(config)?;
            if config.api_key.trim().is_empty() {
                return Err(AppError::ApiUnauthorized);
            }
        }
        BackendMode::WebGateway => {
            // WebGateway 模式不要求 API Key
            if config.timeout_seconds < 5 || config.timeout_seconds > 300 {
                return Err(AppError::ConfigInvalid(
                    "请求超时时间应在 5～300 秒之间".to_string(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::Arc;

    use super::TranslationRequestManager;
    use crate::app_error::AppError;

    #[test]
    fn a_new_translation_cancels_the_previous_task() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime should build");

        runtime.block_on(async {
            let manager = Arc::new(TranslationRequestManager::new());
            let first_manager = Arc::clone(&manager);
            let first = tokio::spawn(async move {
                first_manager
                    .run_latest::<_, ()>(async {
                        pending::<()>().await;
                        unreachable!("pending translation must be cancelled")
                    })
                    .await
            });
            tokio::task::yield_now().await;

            let second = manager
                .run_latest(async { Ok::<_, AppError>("最新结果".to_string()) })
                .await
                .expect("latest translation should complete");

            assert_eq!(second, "最新结果");
            let first_error = first
                .await
                .expect("first command task should join")
                .expect_err("first translation should be cancelled");
            // BackendCancelled 是新错误类型，消息为"翻译请求已被新请求取代"
            assert!(first_error.to_string().contains("已被新请求取代"));
        });
    }
}
