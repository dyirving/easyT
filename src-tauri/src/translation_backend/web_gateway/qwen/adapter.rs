//! Qwen Web Adapter 实现
//!
//! 职责：
//! - 检查凭证状态（通过 QwenSession）
//! - 复用 HTTP Client
//! - 应用统一请求超时
//! - 执行有限重试
//! - 对日志进行敏感信息过滤
//! - 将 Qwen 错误转换为 BackendError
//!
//! 协议约定（独立重新实现，不复制 ProxyAgent GPL 代码）：
//! - 登录入口：https://www.qianwen.com/
//! - 翻译上游：https://chat2.qianwen.com/api/v2/chat
//! - 凭证 Cookie 名：tongyi_sso_ticket
//! - 请求 method：POST，SSE 流式响应
//! - 请求/响应 DTO 仅在 qwen 模块内可见

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{
    HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, ORIGIN, REFERER, USER_AGENT,
};
use serde::Serialize;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::translation_backend::error::BackendError;
use crate::translation_backend::models::{
    BackendProgress, BackendRequest, BackendResult, BackendSource, TranslationProgress,
};
use crate::translation_backend::prompt::build_system_prompt;
use crate::translation_backend::BackendHealth;

use super::session::{ensure_qwen_ready, QwenSession};
use super::sse_decoder::{DecodeOutcome, QwenSseDecoder};
use crate::translation_backend::web_gateway::credential_store::TicketSecret;

/// Qwen 登录入口（仅作 settings 页面跳转用）
pub const QWEN_LOGIN_URL: &str = "https://www.qianwen.com/";

/// 翻译上游 Base URL
const QWEN_API_BASE: &str = "https://chat2.qianwen.com";

/// Cookie 名
pub const QWEN_TICKET_COOKIE_NAME: &str = "tongyi_sso_ticket";

/// 登录 watcher 总等待上限
pub const LOGIN_WATCHER_TIMEOUT: Duration = Duration::from_secs(300);

/// 登录 watcher 轮询间隔
pub const LOGIN_WATCHER_INTERVAL: Duration = Duration::from_millis(750);

/// 登录窗口标签
pub const QWEN_LOGIN_WINDOW_LABEL: &str = "qwen-login";

/// QwenWebAdapter
pub struct QwenWebAdapter {
    http_client: reqwest::Client,
    session: Arc<QwenSession>,
}

impl QwenWebAdapter {
    pub fn new(http_client: reqwest::Client) -> Self {
        Self {
            http_client,
            session: Arc::new(QwenSession::new()),
        }
    }

    pub fn session(&self) -> Arc<QwenSession> {
        Arc::clone(&self.session)
    }

    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
    ) -> Result<BackendResult, BackendError> {
        ensure_qwen_ready(&self.session, config)?;

        let app_data = crate::config::app_data_dir()
            .map_err(|e| BackendError::Internal(format!("无法定位应用数据目录: {e}")))?;

        // 取出短期使用的 ticket
        let ticket = self
            .session
            .borrow_ticket(&app_data)?
            .ok_or(BackendError::LoginRequired)?;

        let result = self.translate_with_ticket(config, request, &ticket).await;
        // ticket 在此 drop 并显式清理内存副本
        drop(ticket);

        match result {
            Ok(r) => Ok(r),
            Err(BackendError::Unauthorized) => {
                self.session.mark_expired();
                Err(BackendError::SessionExpired)
            }
            Err(e) => Err(e),
        }
    }

    pub async fn translate_stream(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<dyn TranslationProgress>,
    ) -> Result<BackendResult, BackendError> {
        ensure_qwen_ready(&self.session, config)?;

        let app_data = crate::config::app_data_dir()
            .map_err(|e| BackendError::Internal(format!("无法定位应用数据目录: {e}")))?;
        let ticket = self
            .session
            .borrow_ticket(&app_data)?
            .ok_or(BackendError::LoginRequired)?;

        let result = self
            .translate_stream_with_ticket(config, request, &ticket, progress)
            .await;
        drop(ticket);

        match result {
            Ok(result) => Ok(result),
            Err(BackendError::Unauthorized) => {
                self.session.mark_expired();
                Err(BackendError::SessionExpired)
            }
            Err(error) => Err(error),
        }
    }

    async fn translate_with_ticket(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        ticket: &TicketSecret,
    ) -> Result<BackendResult, BackendError> {
        let timeout = config.timeout_seconds.clamp(5, 300);
        let prepared = prepare_qwen_request(config, &request, ticket)?;

        // 日志只记录非敏感字段
        log::info!(
            "Qwen WebGateway 翻译: model={}, target_language={}, text_len={}",
            prepared.model,
            request.target_language,
            request.text.chars().count()
        );

        let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
        for attempt in 0..=1 {
            let remaining = deadline
                .checked_duration_since(tokio::time::Instant::now())
                .ok_or(BackendError::Timeout)?;
            let resp = self
                .http_client
                .post(&prepared.url)
                .headers(prepared.headers.clone())
                .query(&prepared.params)
                .timeout(remaining)
                .json(&prepared.body)
                .send()
                .await
                .map_err(map_request_error)?;

            let status = resp.status();
            if status.is_success() {
                return self
                    .consume_sse_stream(resp, &prepared.model, None, None)
                    .await;
            }

            let response_body = resp.text().await.unwrap_or_default();
            log::warn!(
                "Qwen 上游非 2xx: code={}, body_len={}, attempt={}",
                status.as_u16(),
                response_body.len(),
                attempt + 1
            );
            if attempt == 0 && is_retryable_status(status) {
                let remaining = deadline
                    .checked_duration_since(tokio::time::Instant::now())
                    .ok_or(BackendError::Timeout)?;
                let backoff = Duration::from_millis(250).min(remaining);
                tokio::time::sleep(backoff).await;
                continue;
            }
            return Err(map_status_to_error(status));
        }
        Err(BackendError::Internal("Qwen 重试状态异常".to_string()))
    }

    async fn translate_stream_with_ticket(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        ticket: &TicketSecret,
        progress: Arc<dyn TranslationProgress>,
    ) -> Result<BackendResult, BackendError> {
        let timeout = Duration::from_secs(config.timeout_seconds.clamp(5, 300));
        let prepared = prepare_qwen_request(config, &request, ticket)?;

        log::info!(
            "Qwen WebGateway 流式翻译: model={}, target_language={}, text_len={}",
            prepared.model,
            request.target_language,
            request.text.chars().count()
        );

        let response = tokio::time::timeout(
            timeout,
            self.http_client
                .post(&prepared.url)
                .headers(prepared.headers)
                .query(&prepared.params)
                .json(&prepared.body)
                .send(),
        )
        .await
        .map_err(|_| BackendError::Timeout)?
        .map_err(map_request_error)?;

        let status = response.status();
        if !status.is_success() {
            let response_body = tokio::time::timeout(timeout, response.text())
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            log::warn!(
                "Qwen 流式上游非 2xx: code={}, body_len={}",
                status.as_u16(),
                response_body.len()
            );
            return Err(map_status_to_error(status));
        }

        self.consume_sse_stream(response, &prepared.model, Some(progress), Some(timeout))
            .await
    }

    async fn consume_sse_stream(
        &self,
        resp: reqwest::Response,
        model: &str,
        progress: Option<Arc<dyn TranslationProgress>>,
        idle_timeout: Option<Duration>,
    ) -> Result<BackendResult, BackendError> {
        use futures_util::StreamExt;

        let stream = resp
            .bytes_stream()
            .map(|chunk| chunk.map_err(map_stream_error));
        consume_qwen_sse_chunks(stream, model, progress, idle_timeout).await
    }

    pub async fn test_connection(&self, config: &AppConfig) -> Result<BackendHealth, BackendError> {
        ensure_qwen_ready(&self.session, config)?;

        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let result = self.translate(config, request).await?;
        Ok(BackendHealth::translation_succeeded("连接成功", &result))
    }

    pub async fn test_connection_stream(
        &self,
        config: &AppConfig,
        progress: Arc<dyn TranslationProgress>,
    ) -> Result<BackendHealth, BackendError> {
        ensure_qwen_ready(&self.session, config)?;
        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let result = self.translate_stream(config, request, progress).await?;
        Ok(BackendHealth::translation_succeeded(
            "流式连接成功",
            &result,
        ))
    }
}

async fn consume_qwen_sse_chunks<S, B>(
    stream: S,
    model: &str,
    progress: Option<Arc<dyn TranslationProgress>>,
    idle_timeout: Option<Duration>,
) -> Result<BackendResult, BackendError>
where
    S: futures_util::Stream<Item = Result<B, BackendError>>,
    B: AsRef<[u8]>,
{
    use futures_util::StreamExt;

    futures_util::pin_mut!(stream);
    let mut decoder = QwenSseDecoder::new();
    let mut content_acc = String::new();
    let mut deadline = idle_timeout.map(|timeout| tokio::time::Instant::now() + timeout);

    loop {
        let chunk_result = if let Some(deadline) = deadline {
            tokio::time::timeout_at(deadline, stream.next())
                .await
                .map_err(|_| BackendError::Timeout)?
        } else {
            stream.next().await
        };
        let Some(chunk_result) = chunk_result else {
            break;
        };
        let chunk = chunk_result?;
        let outcomes = decoder.feed(&chunk)?;
        for outcome in outcomes {
            match outcome {
                DecodeOutcome::Delta(delta) => {
                    if let Some(content) = delta.content_delta {
                        content_acc.push_str(&content);
                        if let Some(progress) = progress.as_ref() {
                            progress.emit(BackendProgress::ContentDelta(content))?;
                        }
                        if let Some(timeout) = idle_timeout {
                            deadline = Some(tokio::time::Instant::now() + timeout);
                        }
                    }
                }
                DecodeOutcome::Completed => {
                    let translated_text = content_acc.trim().to_string();
                    if translated_text.is_empty() {
                        return Err(BackendError::InvalidResponse("Qwen 返回空译文".to_string()));
                    }
                    return Ok(BackendResult {
                        translated_text,
                        source: BackendSource {
                            backend: crate::translation_backend::models::BackendMode::WebGateway,
                            provider: "qwen".to_string(),
                            model: model.to_string(),
                        },
                    });
                }
                DecodeOutcome::UpstreamError { code, message } => {
                    log::warn!(
                        "Qwen 上游业务错误: code={}, message_len={}",
                        code,
                        message.len()
                    );
                    return Err(BackendError::InvalidResponse("Qwen 上游错误".to_string()));
                }
            }
        }
    }

    // 流结束但未收到 Completed
    decoder.finish().and_then(|()| {
        Err(BackendError::Internal(
            "Qwen decoder 完成状态未产生 Completed 事件".to_string(),
        ))
    })
}

struct PreparedQwenRequest {
    model: String,
    url: String,
    headers: HeaderMap,
    params: Vec<(&'static str, String)>,
    body: QwenRequestBody,
}

fn prepare_qwen_request(
    config: &AppConfig,
    request: &BackendRequest,
    ticket: &TicketSecret,
) -> Result<PreparedQwenRequest, BackendError> {
    let model = config.web_gateway.model.clone();
    let session_id = Uuid::new_v4().simple().to_string();
    let req_id = Uuid::new_v4().simple().to_string();
    let device_id = Uuid::new_v4().simple().to_string();

    Ok(PreparedQwenRequest {
        body: build_qwen_request_body(
            &model,
            &request.text,
            &request.target_language,
            &session_id,
            &req_id,
            config.web_gateway.save_history,
        ),
        model,
        url: format!("{}/api/v2/chat", QWEN_API_BASE),
        headers: build_qwen_headers(ticket)?,
        params: build_qwen_query_params(&device_id),
    })
}

// ===== Qwen 私有协议字段 =====

#[derive(Debug, Serialize)]
struct QwenRequestBody {
    deep_search: &'static str,
    req_id: String,
    model: String,
    scene: &'static str,
    session_id: String,
    sub_scene: &'static str,
    temporary: bool,
    messages: Vec<QwenMessageDto>,
    from: &'static str,
    parent_req_id: &'static str,
    enable_search: bool,
    biz_data: &'static str,
    scene_param: &'static str,
    chat_client: &'static str,
    client_tm: String,
    protocol_version: &'static str,
    biz_id: &'static str,
}

#[derive(Debug, Serialize)]
struct QwenMessageDto {
    content: String,
    mime_type: &'static str,
    meta_data: QwenMessageMetaDto,
}

#[derive(Debug, Serialize)]
struct QwenMessageMetaDto {
    ori_query: String,
}

fn build_qwen_request_body(
    model: &str,
    text: &str,
    target_language: &str,
    session_id: &str,
    req_id: &str,
    save_history: bool,
) -> QwenRequestBody {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string();

    let prompt = build_system_prompt(target_language);
    let content = format!("{}\n\nUser: {}", prompt, text);

    QwenRequestBody {
        deep_search: "0",
        req_id: req_id.to_string(),
        model: model.to_string(),
        scene: "chat",
        session_id: session_id.to_string(),
        sub_scene: "chat",
        temporary: !save_history,
        messages: vec![QwenMessageDto {
            content,
            mime_type: "text/plain",
            meta_data: QwenMessageMetaDto {
                ori_query: text.to_string(),
            },
        }],
        from: "default",
        parent_req_id: "0",
        enable_search: false,
        biz_data: "{\"entryPoint\":\"tongyigw\"}",
        scene_param: "first_turn",
        chat_client: "h5",
        client_tm: timestamp,
        protocol_version: "v2",
        biz_id: "ai_qwen",
    }
}

fn build_qwen_headers(ticket: &TicketSecret) -> Result<HeaderMap, BackendError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/json, text/event-stream, text/plain, */*"),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/145.0.0.0 Safari/537.36"
    ));
    headers.insert(ORIGIN, HeaderValue::from_static("https://www.qianwen.com"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://www.qianwen.com/"),
    );
    headers.insert("X-Platform", HeaderValue::from_static("pc_tongyi"));

    // Cookie：只发送必要 ticket，不复制浏览器全部 Cookie
    let cookie_value = format!("{}={}", QWEN_TICKET_COOKIE_NAME, ticket.as_str());
    headers.insert(
        COOKIE,
        HeaderValue::from_str(&cookie_value)
            .map_err(|_| BackendError::Internal("Cookie 值包含非法字符".to_string()))?,
    );

    Ok(headers)
}

fn build_qwen_query_params(device_id: &str) -> Vec<(&'static str, String)> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string();
    let nonce = Uuid::new_v4().simple().to_string();

    vec![
        ("biz_id", "ai_qwen".to_string()),
        ("chat_client", "h5".to_string()),
        ("device", "pc".to_string()),
        ("fr", "pc".to_string()),
        ("pr", "qwen".to_string()),
        ("ut", device_id.to_string()),
        ("nonce", nonce),
        ("timestamp", timestamp),
        ("la", "zh_CN".to_string()),
        ("tz", "Asia/Shanghai".to_string()),
        ("wv", "1".to_string()),
        ("ve", "1".to_string()),
    ]
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

// ===== 错误映射 =====

fn map_request_error(err: reqwest::Error) -> BackendError {
    if err.is_timeout() {
        BackendError::Timeout
    } else if let Some(status) = err.status() {
        map_status_to_error(status)
    } else {
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

fn map_status_to_error(status: reqwest::StatusCode) -> BackendError {
    match status.as_u16() {
        401 | 403 => BackendError::Unauthorized,
        429 => BackendError::RateLimited,
        500..=599 => BackendError::Network(format!("服务器错误 ({})", status.as_u16())),
        _ => BackendError::Network(format!("HTTP {}", status.as_u16())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures_util::stream;

    use super::*;

    #[derive(Default)]
    struct RecordingProgress(Mutex<Vec<String>>);

    impl TranslationProgress for RecordingProgress {
        fn emit(&self, progress: BackendProgress) -> Result<(), BackendError> {
            let BackendProgress::ContentDelta(delta) = progress;
            self.0.lock().expect("deltas lock").push(delta);
            Ok(())
        }
    }

    fn content_event(content: &str, complete: bool) -> Vec<u8> {
        let event = if complete { "event: complete\n" } else { "" };
        format!(
            "{event}data: {{\"error_code\":0,\"data\":{{\"messages\":[{{\"mime_type\":\"text/plain\",\"content\":{}}}]}}}}\n\n",
            serde_json::to_string(content).expect("content JSON")
        )
        .into_bytes()
    }

    fn reasoning_event(reasoning: &str) -> Vec<u8> {
        format!(
            "data: {{\"error_code\":0,\"data\":{{\"messages\":[{{\"meta_data\":{{\"multi_load\":[{{\"type\":\"deep_think\",\"content\":{{\"think_content\":{}}}}}]}}}}]}}}}\n\n",
            serde_json::to_string(reasoning).expect("reasoning JSON")
        )
        .into_bytes()
    }

    #[test]
    fn retry_is_limited_to_rate_limit_and_server_errors() {
        assert!(is_retryable_status(reqwest::StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(
            reqwest::StatusCode::SERVICE_UNAVAILABLE
        ));
        assert!(!is_retryable_status(reqwest::StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(reqwest::StatusCode::BAD_REQUEST));
    }

    #[test]
    fn per_request_device_id_is_sent_as_ut() {
        let params = build_qwen_query_params("device-test");
        assert!(params
            .iter()
            .any(|(key, value)| *key == "ut" && value == "device-test"));
    }

    #[test]
    fn history_setting_controls_temporary_request_flag() {
        let temporary = build_qwen_request_body(
            "Qwen3.7-Max",
            "hello",
            "简体中文",
            "session",
            "request",
            false,
        );
        assert!(temporary.temporary);

        let persisted = build_qwen_request_body(
            "Qwen3.7-Max",
            "hello",
            "简体中文",
            "session",
            "request",
            true,
        );
        assert!(!persisted.temporary);
    }

    #[tokio::test]
    async fn completed_stream_reports_only_content() {
        let progress = Arc::new(RecordingProgress::default());
        let chunks = stream::iter(vec![
            Ok::<_, BackendError>(reasoning_event("thinking")),
            Ok(content_event("译", false)),
            Ok(content_event("译文", true)),
        ]);

        let result = consume_qwen_sse_chunks(
            chunks,
            "test-model",
            Some(progress.clone()),
            Some(Duration::from_secs(1)),
        )
        .await
        .expect("completed stream");

        assert_eq!(result.translated_text, "译文");
        assert_eq!(*progress.0.lock().unwrap(), vec!["译", "文"]);
    }

    #[tokio::test]
    async fn eof_without_complete_is_partial() {
        let chunks = stream::iter(vec![Ok::<_, BackendError>(content_event("partial", false))]);

        let error = consume_qwen_sse_chunks(chunks, "test-model", None, None)
            .await
            .expect_err("EOF without completion must fail");

        assert!(matches!(error, BackendError::PartialResponse(_)));
    }

    #[tokio::test]
    async fn reasoning_does_not_refresh_idle_deadline() {
        let chunks = stream::unfold(0, |step| async move {
            match step {
                0 => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Some((Ok::<_, BackendError>(reasoning_event("one")), 1))
                }
                1 => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Some((Ok(reasoning_event("one two")), 2))
                }
                _ => None,
            }
        });

        let error = consume_qwen_sse_chunks(
            chunks,
            "test-model",
            Some(Arc::new(RecordingProgress::default())),
            Some(Duration::from_millis(30)),
        )
        .await
        .expect_err("reasoning must not refresh the content deadline");

        assert!(matches!(error, BackendError::Timeout));
    }
}
