//! Official API Adapter 实现
//!
//! 封装原 `llm::client::translate` 行为：
//! - 复用一个 HTTP Client（由 TranslationBackend 注入）
//! - 保留超时、401、429、5xx 和响应解析逻辑
//! - 保留不同 Official Provider 的 thinking 参数差异
//! - 输出 BackendResult，source.backend 为 OfficialApi

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, ModelProvider};
use crate::translation_backend::error::BackendError;
use crate::translation_backend::models::{
    BackendMode, BackendRequest, BackendResult, BackendSource,
};
use crate::translation_backend::prompt::build_system_prompt;
use crate::translation_backend::{TranslationPhase, TranslationProgressReporter};

use super::sse_decoder::{OpenAiDecodeOutcome, OpenAiSseDecoder};

/// OpenAI 兼容 Chat Completions 请求/响应结构
/// （从 llm/error.rs 移植，避免与旧模块耦合）

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// 思考模式开关配置
/// DeepSeek / GLM / Kimi 均采用相同格式：`{"thinking": {"type": "disabled"}}`
#[derive(Debug, Clone, Serialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String,
}

impl ThinkingConfig {
    pub fn disabled() -> Self {
        Self {
            thinking_type: "disabled".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatChoice {
    pub message: ChatChoiceMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatChoiceMessage {
    pub content: String,
}

/// Official API Adapter
pub struct OfficialApiAdapter {
    http_client: reqwest::Client,
}

impl OfficialApiAdapter {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self { http_client }
    }

    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        // 1. 基础校验
        if config.api_key.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("API Key 不能为空".to_string()));
        }
        if config.model.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("模型名称不能为空".to_string()));
        }

        // 2. 构建请求
        let body = build_request_body(config, &request, false);
        let url = format!("{}/chat/completions", normalize_base_url(&config.base_url));

        // 3. 发起请求（复用连接池，单次请求保留配置超时）
        let timeout = config.timeout_seconds.clamp(5, 300);
        // 日志只记录非敏感字段，绝不记录 api_key
        log::info!(
            "请求翻译: model={}, target_language={}, text_len={}",
            config.model,
            request.target_language,
            request.text.chars().count()
        );

        progress.phase(TranslationPhase::ConnectingBackend, None);
        let resp = self
            .http_client
            .post(&url)
            .bearer_auth(&config.api_key)
            .timeout(Duration::from_secs(timeout))
            .json(&body)
            .send()
            .await
            .map_err(map_request_error)?;

        // 4. 处理 HTTP 状态码
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(map_status_to_error(status, &body));
        }
        progress.phase(TranslationPhase::WaitingForContent, None);

        // 5. 解析响应
        let parsed: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|_| BackendError::InvalidResponse("响应格式不符合预期".to_string()))?;

        let translated_text = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| BackendError::InvalidResponse("响应缺少 choices".to_string()))?;

        let translated_text = translated_text.trim().to_string();
        if translated_text.is_empty() {
            return Err(BackendError::InvalidResponse("译文为空".to_string()));
        }

        progress.phase(TranslationPhase::ReceivingContent, None);

        build_backend_result(config, translated_text)
    }

    /// 使用标准 Chat Completions SSE 的流式翻译。
    pub async fn translate_stream(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        if config.api_key.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("API Key 不能为空".to_string()));
        }
        if config.model.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("模型名称不能为空".to_string()));
        }

        let body = build_request_body(config, &request, true);
        let url = format!("{}/chat/completions", normalize_base_url(&config.base_url));
        let timeout = Duration::from_secs(config.timeout_seconds.clamp(5, 300));

        log::info!(
            "请求流式翻译: model={}, target_language={}, text_len={}",
            config.model,
            request.target_language,
            request.text.chars().count()
        );

        progress.phase(TranslationPhase::ConnectingBackend, None);
        let response = tokio::time::timeout(
            timeout,
            self.http_client
                .post(&url)
                .bearer_auth(&config.api_key)
                .json(&body)
                .send(),
        )
        .await
        .map_err(|_| BackendError::Timeout)?
        .map_err(map_request_error)?;

        let status = response.status();
        if !status.is_success() {
            let body = tokio::time::timeout(timeout, response.text())
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            return Err(map_status_to_error(status, &body));
        }

        progress.phase(TranslationPhase::WaitingForContent, None);

        self.consume_sse_stream(response, config, progress).await
    }

    async fn consume_sse_stream(
        &self,
        response: reqwest::Response,
        config: &AppConfig,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        use futures_util::StreamExt;

        let timeout = Duration::from_secs(config.timeout_seconds.clamp(5, 300));
        let stream = response
            .bytes_stream()
            .map(|chunk| chunk.map_err(map_stream_error));
        consume_sse_chunks(stream, config, progress, timeout).await
    }

    /// 测试连接：发起极短翻译请求验证可用
    pub async fn test_connection(
        &self,
        config: &AppConfig,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<String, BackendError> {
        if config.api_key.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("API Key 不能为空".to_string()));
        }

        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let result = self.translate(config, request, progress).await?;
        Ok(crate::translation_backend::connection_success_message(
            "连接成功",
            &result,
        ))
    }

    pub async fn test_connection_stream(
        &self,
        config: &AppConfig,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<String, BackendError> {
        if config.api_key.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("API Key 不能为空".to_string()));
        }

        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let result = self.translate_stream(config, request, progress).await?;
        Ok(crate::translation_backend::connection_success_message(
            "流式连接成功",
            &result,
        ))
    }
}

async fn consume_sse_chunks<S, B>(
    stream: S,
    config: &AppConfig,
    progress: Arc<TranslationProgressReporter>,
    timeout: Duration,
) -> Result<BackendResult, BackendError>
where
    S: futures_util::Stream<Item = Result<B, BackendError>>,
    B: AsRef<[u8]>,
{
    use futures_util::StreamExt;

    futures_util::pin_mut!(stream);
    let mut deadline = tokio::time::Instant::now() + timeout;
    let mut decoder = OpenAiSseDecoder::new();
    let mut content = String::new();

    loop {
        let chunk = tokio::time::timeout_at(deadline, stream.next())
            .await
            .map_err(|_| BackendError::Timeout)?;
        let Some(chunk) = chunk else {
            return decoder.finish().and_then(|()| {
                Err(BackendError::Internal(
                    "Official API decoder 完成状态未产生 Completed 事件".to_string(),
                ))
            });
        };
        let chunk = chunk?;
        for outcome in decoder.feed(&chunk)? {
            match outcome {
                OpenAiDecodeOutcome::ContentDelta(delta) => {
                    if delta.is_empty() {
                        continue;
                    }
                    if content.is_empty() {
                        progress.phase(TranslationPhase::ReceivingContent, None);
                    }
                    progress.content_delta(delta.clone())?;
                    content.push_str(&delta);
                    deadline = tokio::time::Instant::now() + timeout;
                }
                OpenAiDecodeOutcome::Completed => {
                    return build_backend_result(config, content);
                }
            }
        }
    }
}

fn build_request_body(
    config: &AppConfig,
    request: &BackendRequest,
    stream: bool,
) -> ChatCompletionRequest {
    // 仅当用户关闭思考时注入关闭参数；开启时留空走供应商默认
    let (thinking, enable_thinking) = if config.enable_thinking {
        (None, None)
    } else {
        match config.provider {
            ModelProvider::Deepseek | ModelProvider::Glm | ModelProvider::Kimi => {
                (Some(ThinkingConfig::disabled()), None)
            }
            ModelProvider::Qwen => (None, Some(false)),
            // Agnes / DouBao / Custom：不支持或不需要关闭参数
            _ => (None, None),
        }
    };

    ChatCompletionRequest {
        model: config.model.clone(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: build_system_prompt(&request.target_language),
            },
            ChatMessage {
                role: "user".to_string(),
                content: request.text.clone(),
            },
        ],
        temperature: Some(0.2),
        stream: Some(stream),
        thinking,
        enable_thinking,
    }
}

fn build_backend_result(
    config: &AppConfig,
    translated_text: String,
) -> Result<BackendResult, BackendError> {
    let translated_text = translated_text.trim().to_string();
    if translated_text.is_empty() {
        return Err(BackendError::InvalidResponse("译文为空".to_string()));
    }

    Ok(BackendResult {
        translated_text,
        source: BackendSource {
            backend: BackendMode::OfficialApi,
            provider: config.provider.stable_id().to_string(),
            model: config.model.clone(),
        },
    })
}

/// 规范化 Base URL：去除末尾斜杠，避免拼出 //
fn normalize_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

/// 把 HTTP 状态码映射为 BackendError
fn map_status_to_error(status: reqwest::StatusCode, body: &str) -> BackendError {
    let code = status.as_u16();
    match code {
        401 | 403 => BackendError::Unauthorized,
        429 => BackendError::RateLimited,
        500..=599 => BackendError::Network(format!("服务器错误 ({})", code)),
        _ => {
            let detail = summarize_error_body(body);
            if detail.is_empty() {
                BackendError::Network(format!("HTTP {}", code))
            } else {
                // 不把上游 body 透传给前端
                log::warn!("Official API 非 2xx: code={code}, body_summary={}", detail);
                BackendError::Network(format!("HTTP {}", code))
            }
        }
    }
}

fn summarize_error_body(body: &str) -> String {
    body.chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect()
}

/// 把 reqwest::Error 映射为 BackendError
fn map_request_error(err: reqwest::Error) -> BackendError {
    if err.is_timeout() {
        BackendError::Timeout
    } else if let Some(status) = err.status() {
        match status.as_u16() {
            401 | 403 => BackendError::Unauthorized,
            429 => BackendError::RateLimited,
            500..=599 => BackendError::Network(format!("服务器错误 ({})", status)),
            _ => BackendError::Network(format!("HTTP {}", status)),
        }
    } else {
        // 网络层错误（DNS、连接拒绝、TLS 等）
        BackendError::Network("网络请求失败".to_string())
    }
}

fn map_stream_error(err: reqwest::Error) -> BackendError {
    if err.is_timeout() {
        BackendError::Timeout
    } else {
        BackendError::Network("流读取失败".to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures_util::stream;

    use super::*;
    use crate::translation_backend::{PhaseProgress, TranslationProgress};

    #[derive(Default)]
    struct RecordingProgress {
        deltas: Mutex<Vec<String>>,
        phases: Mutex<Vec<TranslationPhase>>,
        fail: bool,
    }

    impl TranslationProgress for RecordingProgress {
        fn phase_changed(&self, progress: PhaseProgress) {
            self.phases
                .lock()
                .expect("phases lock")
                .push(progress.phase);
        }

        fn content_delta(&self, delta: String) -> Result<(), BackendError> {
            if self.fail {
                return Err(BackendError::Cancelled);
            }
            self.deltas.lock().expect("deltas lock").push(delta);
            Ok(())
        }
    }

    fn reporter(progress: Arc<RecordingProgress>) -> Arc<TranslationProgressReporter> {
        let reporter = Arc::new(TranslationProgressReporter::new(progress));
        reporter.phase(TranslationPhase::PreparingRequest, None);
        reporter.phase(TranslationPhase::ConnectingBackend, None);
        reporter.phase(TranslationPhase::WaitingForContent, None);
        reporter
    }

    fn test_config() -> AppConfig {
        let mut config = crate::config::default_config();
        config.model = "test-model".to_string();
        config
    }

    fn delayed_chunks(
        chunks: Vec<(Duration, &'static str)>,
    ) -> impl futures_util::Stream<Item = Result<Vec<u8>, BackendError>> {
        stream::unfold(chunks.into_iter(), |mut chunks| async move {
            let (delay, chunk) = chunks.next()?;
            tokio::time::sleep(delay).await;
            Some((Ok(chunk.as_bytes().to_vec()), chunks))
        })
    }

    #[test]
    fn normalize_base_url_strips_trailing_slash() {
        assert_eq!(normalize_base_url("https://a.com/v1/"), "https://a.com/v1");
        assert_eq!(normalize_base_url("https://a.com/v1"), "https://a.com/v1");
    }

    #[test]
    fn status_401_maps_to_unauthorized() {
        let err = map_status_to_error(reqwest::StatusCode::UNAUTHORIZED, "");
        assert!(matches!(err, BackendError::Unauthorized));
    }

    #[test]
    fn status_429_maps_to_rate_limited() {
        let err = map_status_to_error(reqwest::StatusCode::TOO_MANY_REQUESTS, "");
        assert!(matches!(err, BackendError::RateLimited));
    }

    #[test]
    fn request_body_selects_stream_flag() {
        let config = crate::config::default_config();
        let request = BackendRequest {
            text: "hello".to_string(),
            target_language: "简体中文".to_string(),
        };

        let once = serde_json::to_value(build_request_body(&config, &request, false))
            .expect("serialize once body");
        let streaming = serde_json::to_value(build_request_body(&config, &request, true))
            .expect("serialize streaming body");

        assert_eq!(once["stream"], serde_json::Value::Bool(false));
        assert_eq!(streaming["stream"], serde_json::Value::Bool(true));
    }

    #[tokio::test]
    async fn content_deltas_refresh_idle_deadline() {
        let chunks = delayed_chunks(vec![
            (
                Duration::from_millis(20),
                "data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\n\n",
            ),
            (
                Duration::from_millis(20),
                "data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n\n",
            ),
            (Duration::from_millis(20), "data: [DONE]\n\n"),
        ]);
        let progress = Arc::new(RecordingProgress::default());

        let result = consume_sse_chunks(
            chunks,
            &test_config(),
            reporter(progress.clone()),
            Duration::from_millis(35),
        )
        .await
        .expect("each content delta should refresh the deadline");

        assert_eq!(result.translated_text, "ab");
        assert_eq!(*progress.deltas.lock().unwrap(), vec!["a", "b"]);
        assert_eq!(
            progress.phases.lock().unwrap().last(),
            Some(&TranslationPhase::ReceivingContent)
        );
    }

    #[tokio::test]
    async fn ignored_events_do_not_refresh_idle_deadline() {
        let chunks = delayed_chunks(vec![
            (Duration::from_millis(20), ": heartbeat\n\n"),
            (Duration::from_millis(20), ": heartbeat\n\n"),
        ]);

        let error = consume_sse_chunks(
            chunks,
            &test_config(),
            reporter(Arc::new(RecordingProgress::default())),
            Duration::from_millis(30),
        )
        .await
        .expect_err("heartbeats must not refresh the content deadline");

        assert!(matches!(error, BackendError::Timeout));
    }

    #[tokio::test]
    async fn eof_without_done_is_partial_after_content() {
        let chunks = stream::iter(vec![Ok::<_, BackendError>(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n".to_vec(),
        )]);

        let error = consume_sse_chunks(
            chunks,
            &test_config(),
            reporter(Arc::new(RecordingProgress::default())),
            Duration::from_secs(1),
        )
        .await
        .expect_err("EOF without done must fail");

        assert!(matches!(error, BackendError::PartialResponse(_)));
    }

    #[tokio::test]
    async fn closed_progress_sink_cancels_stream() {
        let chunks = stream::iter(vec![Ok::<_, BackendError>(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"stop\"}}]}\n\n".to_vec(),
        )]);
        let progress = RecordingProgress {
            deltas: Mutex::default(),
            phases: Mutex::default(),
            fail: true,
        };

        let error = consume_sse_chunks(
            chunks,
            &test_config(),
            reporter(Arc::new(progress)),
            Duration::from_secs(1),
        )
        .await
        .expect_err("closed sink must stop consumption");

        assert!(matches!(error, BackendError::Cancelled));
    }
}
