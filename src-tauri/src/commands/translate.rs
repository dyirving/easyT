use std::future::Future;
use std::sync::Mutex;

use crate::app_error::{AppError, AppResult};
use crate::commands::config::AppState;
use crate::llm::models::{TranslationRequest, TranslationResult};
use crate::llm::translate;
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

    async fn run_latest<F>(&self, future: F) -> AppResult<TranslationResult>
    where
        F: Future<Output = AppResult<TranslationResult>> + Send + 'static,
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
            Err(e) if e.is_cancelled() => Err(AppError::ApiRequestFailed(
                "翻译请求已被新请求取代".to_string(),
            )),
            Err(e) => Err(AppError::Internal(format!("翻译任务执行失败: {e}"))),
        }
    }
}

/// 翻译文本
/// config 从 AppState 读取，避免前端携带 api_key
/// 前端参数使用 camelCase（targetLanguage），Rust 端用 snake_case 接收
#[tauri::command]
pub async fn translate_text(
    state: State<'_, AppState>,
    request_manager: State<'_, TranslationRequestManager>,
    text: String,
    target_language: String,
) -> AppResult<TranslationResult> {
    let config = state.snapshot()?;
    if text.trim().is_empty() {
        return Err(AppError::NoSelectedText);
    }
    if text.chars().count() > config.max_text_length {
        return Err(AppError::TextTooLong);
    }
    let request = TranslationRequest {
        text,
        target_language,
    };
    request_manager
        .run_latest(async move { translate(&config, request).await })
        .await
}

/// 测试 API 连接
/// 使用前端传入的草稿配置发起极短翻译请求（"hi" → 目标语言）验证可用
/// 不修改 AppState，避免未保存的草稿污染主流程
#[tauri::command]
pub async fn test_api_connection(config: crate::config::AppConfig) -> AppResult<String> {
    crate::commands::config::validate_config(&config)?;
    if config.api_key.trim().is_empty() {
        return Err(AppError::ApiUnauthorized);
    }
    let request = TranslationRequest {
        text: "hi".to_string(),
        target_language: config.target_language.clone(),
    };
    let result = translate(&config, request).await?;
    Ok(format!(
        "连接成功，返回译文长度 {} 字符",
        result.translated_text.chars().count()
    ))
}

#[cfg(test)]
mod tests {
    use std::future::pending;
    use std::sync::Arc;

    use super::TranslationRequestManager;
    use crate::llm::models::TranslationResult;

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
                    .run_latest(async {
                        pending::<()>().await;
                        unreachable!("pending translation must be cancelled")
                    })
                    .await
            });
            tokio::task::yield_now().await;

            let second = manager
                .run_latest(async {
                    Ok(TranslationResult {
                        translated_text: "最新结果".to_string(),
                    })
                })
                .await
                .expect("latest translation should complete");

            assert_eq!(second.translated_text, "最新结果");
            let first_error = first
                .await
                .expect("first command task should join")
                .expect_err("first translation should be cancelled");
            assert!(first_error.to_string().contains("已被新请求取代"));
        });
    }
}
