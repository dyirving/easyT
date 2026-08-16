use std::future::Future;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Instant;

use crate::app_error::{AppError, AppResult};
use crate::commands::config::AppState;
use crate::translation_backend::{
    BackendMode, BackendRequest, PhaseProgress, ProgressBackendSource, TranslationBackend,
    TranslationOptions, TranslationPhase, TranslationProgress, TranslationProgressReporter,
};
use crate::translation_history::{
    HistoryCommitEligibility, HistoryCommitOutcome, HistoryEntryDraft, RequestEligibility,
    TranslationHistory,
};
use serde::Serialize;
use tauri::{ipc::Channel, State};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranslationResult {
    translated_text: String,
    from_cache: bool,
    total_elapsed_ms: u64,
    history: HistoryCommitOutcome,
}

struct ActiveTranslation {
    generation: u64,
    abort_handle: tokio::task::AbortHandle,
    current: Arc<AtomicBool>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TranslationProgressEvent {
    PhaseChanged {
        #[serde(rename = "requestId")]
        request_id: String,
        sequence: u64,
        phase: TranslationPhase,
        #[serde(rename = "totalElapsedMs")]
        total_elapsed_ms: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        backend: Option<ProgressBackendSource>,
    },
    ContentDelta {
        #[serde(rename = "requestId")]
        request_id: String,
        delta: String,
    },
}

struct ChannelProgress {
    request_id: String,
    channel: Channel<TranslationProgressEvent>,
}

impl TranslationProgress for ChannelProgress {
    fn phase_changed(&self, progress: PhaseProgress) {
        if self
            .channel
            .send(TranslationProgressEvent::PhaseChanged {
                request_id: self.request_id.clone(),
                sequence: progress.sequence,
                phase: progress.phase,
                total_elapsed_ms: progress.total_elapsed_ms,
                backend: progress.backend,
            })
            .is_err()
        {
            log::warn!(
                "translation_progress_phase_send_failed: phase={:?} sequence={}",
                progress.phase,
                progress.sequence
            );
        }
    }

    fn content_delta(&self, delta: String) -> Result<(), crate::translation_backend::BackendError> {
        self.channel
            .send(TranslationProgressEvent::ContentDelta {
                request_id: self.request_id.clone(),
                delta,
            })
            .map_err(|_| crate::translation_backend::BackendError::Cancelled)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationCommandError {
    kind: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_elapsed_ms: Option<u64>,
}

impl TranslationCommandError {
    fn from_app(error: AppError, total_elapsed_ms: Option<u64>) -> Self {
        Self {
            kind: error.kind_str(),
            message: error.to_string(),
            code: match &error {
                AppError::QwenStorage { code, .. } | AppError::Qwen { code, .. } => Some(*code),
                _ => None,
            },
            total_elapsed_ms,
        }
    }
}

impl TranslationRequestManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(TranslationRequestState::default()),
        }
    }

    async fn run_latest<F, Fut, T>(&self, factory: F) -> AppResult<T>
    where
        F: FnOnce(RequestEligibility) -> Fut,
        Fut: Future<Output = AppResult<T>> + Send + 'static,
        T: Send + 'static,
    {
        let current = Arc::new(AtomicBool::new(true));
        let eligibility = RequestEligibility::new(Arc::clone(&current));
        let future = factory(eligibility);
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
                current,
            }) {
                previous.current.store(false, Ordering::Release);
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
                if let Some(active) = state.active.take() {
                    active.current.store(false, Ordering::Release);
                }
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
/// forceRefresh 为 true 时表示"重新翻译"（绕过缓存读取并在成功后覆盖共享缓存）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn translate_text(
    state: State<'_, AppState>,
    request_manager: State<'_, TranslationRequestManager>,
    backend: State<'_, Arc<TranslationBackend>>,
    history: State<'_, Arc<TranslationHistory>>,
    request_id: String,
    text: String,
    target_language: String,
    force_refresh: bool,
    replace_entry_id: Option<String>,
    on_event: Channel<TranslationProgressEvent>,
) -> Result<TranslationResult, TranslationCommandError> {
    if request_id.trim().is_empty() {
        return Err(TranslationCommandError::from_app(
            AppError::ConfigInvalid("请求 ID 不能为空".to_string()),
            None,
        ));
    }
    if replace_entry_id.is_some() && !force_refresh {
        return Err(TranslationCommandError::from_app(
            AppError::ConfigInvalid("历史替换只能用于重新翻译".to_string()),
            None,
        ));
    }
    let config = state
        .snapshot()
        .map_err(|error| TranslationCommandError::from_app(error, None))?;
    validate_translate_request(&config, &text)
        .map_err(|error| TranslationCommandError::from_app(error, None))?;

    let request = BackendRequest {
        text: text.clone(),
        target_language: target_language.clone(),
    };
    let options = TranslationOptions { force_refresh };
    let sink: Arc<dyn TranslationProgress> = Arc::new(ChannelProgress {
        request_id,
        channel: on_event,
    });
    let progress = Arc::new(TranslationProgressReporter::new(sink));
    let timing = Arc::clone(&progress);

    // 提取 Arc<TranslationBackend>，避免 State 的非静态生命周期逃逸到 run_latest 中。
    let backend = backend.inner().clone();
    let history = history.inner().clone();
    let request_started_at = Instant::now();
    let timing_for_task = Arc::clone(&timing);
    let result = request_manager
        .run_latest(move |request_eligibility| async move {
            let outcome = backend
                .translate(&config, request, options, progress)
                .await
                .map_err(AppError::from)?;
            if !request_eligibility.is_current() {
                return Err(AppError::BackendCancelled);
            }
            timing_for_task.phase(TranslationPhase::SavingHistory, None);
            let from_cache = outcome.is_from_cache();
            let draft = HistoryEntryDraft::new(
                text,
                outcome.result.translated_text.clone(),
                target_language,
                outcome.result.source.clone(),
                from_cache,
                request_started_at,
            );
            let history_outcome = history
                .commit_entry(
                    draft,
                    replace_entry_id,
                    config.translation_history_limit,
                    HistoryCommitEligibility::new(request_eligibility),
                )
                .await;
            Ok(translate_outcome_to_result(
                outcome,
                timing_for_task.elapsed_ms(),
                history_outcome,
            ))
        })
        .await
        .map_err(|error| TranslationCommandError::from_app(error, Some(timing.elapsed_ms())))?;

    Ok(result)
}

/// 把统一 outcome 映射为前端命令结果；只有 L1/L2 命中时 fromCache 为 true。
fn translate_outcome_to_result(
    outcome: crate::translation_backend::TranslationOutcome,
    total_elapsed_ms: u64,
    history: HistoryCommitOutcome,
) -> TranslationResult {
    let from_cache = outcome.is_from_cache();
    TranslationResult {
        translated_text: outcome.result.translated_text,
        from_cache,
        total_elapsed_ms,
        history,
    }
}

fn validate_translate_request(config: &crate::config::AppConfig, text: &str) -> AppResult<()> {
    if text.trim().is_empty() {
        return Err(AppError::NoSelectedText);
    }
    if text.chars().count() > config.max_text_length {
        return Err(AppError::TextTooLong);
    }
    Ok(())
}

/// 使用未保存的设置草稿测试连接。
#[tauri::command]
pub async fn test_api_connection(
    state: State<'_, AppState>,
    backend: State<'_, Arc<TranslationBackend>>,
    config: crate::config::AppConfig,
) -> AppResult<String> {
    let _ = state.snapshot()?;
    validate_test_config(&config)?;
    backend.test_connection(&config).await.map_err(Into::into)
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

    use super::TranslationProgressEvent;
    use super::TranslationRequestManager;
    use super::{ChannelProgress, TranslationProgress};
    use crate::app_error::AppError;
    use crate::translation_backend::BackendError;

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
                    .run_latest::<_, _, ()>(|_| async {
                        pending::<()>().await;
                        unreachable!("pending translation must be cancelled")
                    })
                    .await
            });
            tokio::task::yield_now().await;

            let second = manager
                .run_latest(|_| async { Ok::<_, AppError>("最新结果".to_string()) })
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

    #[test]
    fn stream_event_serializes_with_frontend_contract() {
        let event = TranslationProgressEvent::ContentDelta {
            request_id: "req_test".to_string(),
            delta: "你好".to_string(),
        };
        let json = serde_json::to_value(event).expect("event should serialize");

        assert_eq!(json["type"], "contentDelta");
        assert_eq!(json["requestId"], "req_test");
        assert_eq!(json["delta"], "你好");
    }

    #[test]
    fn closed_channel_maps_to_cancelled_without_panicking() {
        let channel = tauri::ipc::Channel::new(|_| {
            Err(tauri::Error::Io(std::io::Error::other(
                "channel consumer closed",
            )))
        });
        let progress = ChannelProgress {
            request_id: "req_test".to_string(),
            channel,
        };

        let error = progress
            .content_delta("delta".to_string())
            .expect_err("closed channel must cancel the request");

        assert!(matches!(error, BackendError::Cancelled));
    }

    #[test]
    fn outcome_maps_to_frontend_result_with_from_cache() {
        use crate::translation_backend::models::{BackendResult, BackendSource};
        use crate::translation_backend::{BackendMode, CacheStatus, TranslationOutcome};

        let outcome = TranslationOutcome {
            result: BackendResult {
                translated_text: "你好".to_string(),
                source: BackendSource {
                    backend: BackendMode::OfficialApi,
                    provider: "agnes".to_string(),
                    model: "agnes-2.0-flash".to_string(),
                },
            },
            cache_status: CacheStatus::Miss,
        };

        let result = super::translate_outcome_to_result(
            outcome,
            37,
            crate::translation_history::HistoryCommitOutcome::NotSaved {
                warning: crate::translation_history::HistoryWarning::save_failed(),
            },
        );
        let json = serde_json::to_value(&result).expect("result should serialize");

        assert_eq!(json["translatedText"], "你好");
        assert_eq!(json["fromCache"], false);
        assert_eq!(json["totalElapsedMs"], 37);
    }

    #[test]
    fn phase_event_serializes_with_frontend_contract() {
        let event = TranslationProgressEvent::PhaseChanged {
            request_id: "req_test".to_string(),
            sequence: 2,
            phase: crate::translation_backend::TranslationPhase::ConnectingBackend,
            total_elapsed_ms: 1250,
            backend: Some(crate::translation_backend::ProgressBackendSource {
                mode: crate::translation_backend::BackendMode::OfficialApi,
                provider: "deepseek".to_string(),
            }),
        };
        let json = serde_json::to_value(event).expect("event should serialize");

        assert_eq!(json["type"], "phaseChanged");
        assert_eq!(json["requestId"], "req_test");
        assert_eq!(json["sequence"], 2);
        assert_eq!(json["phase"], "connectingBackend");
        assert_eq!(json["totalElapsedMs"], 1250);
        assert_eq!(json["backend"]["mode"], "officialApi");
    }
}
