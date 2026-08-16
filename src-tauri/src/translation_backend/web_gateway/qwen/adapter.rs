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
use crate::translation_backend::error::{
    is_context_length_pattern, BackendError, TERMBASE_CONTEXT_LENGTH_MESSAGE,
};
use crate::translation_backend::models::{BackendRequest, BackendResult, BackendSource};
use crate::translation_backend::{TranslationPhase, TranslationProgressReporter};

use super::sse_decoder::{DecodeOutcome, QwenSseDecoder};
use super::QwenError;
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

pub(crate) async fn consume_qwen_sse_chunks<S, B>(
    stream: S,
    model: &str,
    progress: Arc<TranslationProgressReporter>,
    emit_content: bool,
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
                        if content.is_empty() {
                            continue;
                        }
                        if content_acc.is_empty() {
                            progress.phase(TranslationPhase::ReceivingContent, None);
                        }
                        content_acc.push_str(&content);
                        if emit_content {
                            progress.content_delta(content)?;
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
                    if is_qwen_context_length_error(&code, &message) {
                        log::warn!(
                            "termbase_prompt_context_error: recognized Qwen context-length code={code}"
                        );
                        return Err(BackendError::InvalidResponse(
                            TERMBASE_CONTEXT_LENGTH_MESSAGE.to_string(),
                        ));
                    }
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

pub(crate) struct PreparedQwenRequest {
    pub(crate) model: String,
    pub(crate) url: String,
    pub(crate) headers: HeaderMap,
    pub(crate) params: Vec<(&'static str, String)>,
    pub(crate) body: QwenRequestBody,
}

pub(crate) fn prepare_qwen_request(
    config: &AppConfig,
    request: &BackendRequest,
    ticket: &TicketSecret,
    save_history: bool,
) -> Result<PreparedQwenRequest, BackendError> {
    let model = config.web_gateway.model.clone();
    let session_id = Uuid::new_v4().simple().to_string();
    let req_id = Uuid::new_v4().simple().to_string();
    let device_id = Uuid::new_v4().simple().to_string();

    Ok(PreparedQwenRequest {
        body: build_qwen_request_body(
            &model,
            &request.text,
            &request.prompt,
            &session_id,
            &req_id,
            save_history,
        ),
        model,
        url: format!("{}/api/v2/chat", QWEN_API_BASE),
        headers: build_qwen_headers(ticket)?,
        params: build_qwen_query_params(&device_id),
    })
}

// ===== Qwen 私有协议字段 =====

#[derive(Debug, Serialize)]
pub(crate) struct QwenRequestBody {
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
    prompt: &str,
    session_id: &str,
    req_id: &str,
    save_history: bool,
) -> QwenRequestBody {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string();

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

// ===== 错误映射 =====

pub(crate) fn map_request_error(err: reqwest::Error) -> BackendError {
    if err.is_timeout() {
        BackendError::Qwen(QwenError::timeout())
    } else if let Some(status) = err.status() {
        map_status_to_error(status, "")
    } else {
        BackendError::Qwen(QwenError::network())
    }
}

pub(crate) fn map_stream_error(err: reqwest::Error) -> BackendError {
    if err.is_timeout() {
        BackendError::Qwen(QwenError::timeout())
    } else {
        BackendError::Qwen(QwenError::network())
    }
}

pub(crate) fn map_status_to_error(status: reqwest::StatusCode, body: &str) -> BackendError {
    match status.as_u16() {
        401 => BackendError::Qwen(QwenError::auth_401()),
        403 => BackendError::Qwen(QwenError::auth_403()),
        429 => BackendError::Qwen(QwenError::upstream_rate_limited()),
        500..=599 => BackendError::Qwen(QwenError::upstream_server_error(status.as_u16())),
        400 if is_context_length_pattern(body) => {
            log::warn!("termbase_prompt_context_error: recognized Qwen HTTP context-length");
            BackendError::InvalidResponse(TERMBASE_CONTEXT_LENGTH_MESSAGE.to_string())
        }
        _ => BackendError::Qwen(QwenError::upstream_other()),
    }
}

/// FR-010：Qwen 上游可识别的上下文过长错误。
///
/// 业务错误码 4010（参数错误，含输入过长）或正文匹配
/// [`is_context_length_pattern`]。只做判定，绝不把正文写入错误或日志；
/// 匹配后由调用方映射为固定的 [`TERMBASE_CONTEXT_LENGTH_MESSAGE`]。
fn is_qwen_context_length_error(code: &str, message: &str) -> bool {
    code == "4010" || is_context_length_pattern(message)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use futures_util::stream;

    use super::*;
    use crate::translation_backend::prompt::build_system_prompt;
    use crate::translation_backend::{PhaseProgress, TranslationProgress};

    #[derive(Default)]
    struct RecordingProgress {
        deltas: Mutex<Vec<String>>,
        phases: Mutex<Vec<TranslationPhase>>,
    }

    impl TranslationProgress for RecordingProgress {
        fn phase_changed(&self, progress: PhaseProgress) {
            self.phases
                .lock()
                .expect("phases lock")
                .push(progress.phase);
        }

        fn content_delta(&self, delta: String) -> Result<(), BackendError> {
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
            "系统提示词",
            "session",
            "request",
            false,
        );
        assert!(temporary.temporary);

        let persisted = build_qwen_request_body(
            "Qwen3.7-Max",
            "hello",
            "系统提示词",
            "session",
            "request",
            true,
        );
        assert!(!persisted.temporary);
    }

    #[test]
    fn non_empty_term_block_reaches_qwen_body_verbatim() {
        // FR-005/T-007：build_system_prompt 的非空术语块原样进入 Qwen 请求体 prompt。
        let effective = crate::termbase::test_support::non_empty_effective();
        let prompt = build_system_prompt("简体中文", &effective);
        let body = build_qwen_request_body(
            "Qwen3.7-Max",
            "a function call",
            &prompt,
            "session",
            "request",
            false,
        );

        assert_eq!(
            body.messages[0].content,
            format!("{prompt}\n\nUser: a function call"),
            "Qwen 请求体必须携带同一个非空术语块"
        );
        assert!(body.messages[0].content.contains("function"));
        assert!(body.messages[0].content.contains("函数"));
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
            reporter(progress.clone()),
            true,
            Some(Duration::from_secs(1)),
        )
        .await
        .expect("completed stream");

        assert_eq!(result.translated_text, "译文");
        assert_eq!(*progress.deltas.lock().unwrap(), vec!["译", "文"]);
        assert_eq!(
            progress.phases.lock().unwrap().last(),
            Some(&TranslationPhase::ReceivingContent)
        );
    }

    #[tokio::test]
    async fn eof_without_complete_is_partial() {
        let chunks = stream::iter(vec![Ok::<_, BackendError>(content_event("partial", false))]);

        let error = consume_qwen_sse_chunks(
            chunks,
            "test-model",
            Arc::new(TranslationProgressReporter::discard()),
            false,
            None,
        )
        .await
        .expect_err("EOF without completion must fail");

        assert!(matches!(error, BackendError::PartialResponse(_)));
    }

    #[test]
    fn context_length_error_patterns_are_documented() {
        // FR-010/T-012：识别模式与上游正文的对应关系（大小写不敏感，且不逐字回显）。
        assert!(is_qwen_context_length_error(
            "4010",
            "The input text is too long"
        ));
        assert!(is_qwen_context_length_error(
            "4001",
            "The context length of the request exceeds the limit"
        ));
        assert!(is_qwen_context_length_error(
            "1001",
            "请求内容超出上下文长度限制"
        ));
        assert!(!is_qwen_context_length_error("1001", "rate limited"));
        assert!(!is_qwen_context_length_error("4001", "model not found"));
    }

    #[test]
    fn http_context_length_error_gets_dedicated_message_without_body() {
        let error = map_status_to_error(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"message":"The input text is too long, max 3000 tokens"}"#,
        );

        assert!(matches!(
            error,
            BackendError::InvalidResponse(ref message) if message == TERMBASE_CONTEXT_LENGTH_MESSAGE
        ));
    }

    #[tokio::test]
    async fn recognized_context_length_gets_dedicated_message_without_body() {
        // FR-010/T-012：可识别模式映射为固定专属文案，正文绝不进入错误。
        let body = format!(
            "data: {}\n\n",
            r#"{"error_code":4010,"error_msg":"The input text is too long, max 3000 tokens"}"#
        );
        let chunks = stream::iter(vec![Ok::<_, BackendError>(body.into_bytes())]);

        let error = consume_qwen_sse_chunks(
            chunks,
            "test-model",
            Arc::new(TranslationProgressReporter::discard()),
            true,
            Some(Duration::from_secs(1)),
        )
        .await
        .expect_err("context-length error must fail");

        let BackendError::InvalidResponse(message) = error else {
            panic!("expected InvalidResponse, got {error:?}");
        };
        assert_eq!(message, TERMBASE_CONTEXT_LENGTH_MESSAGE);
        assert!(!message.contains("3000"));
        assert!(!message.contains("tokens"));
    }

    #[tokio::test]
    async fn unrecognized_upstream_error_stays_generic_without_body() {
        let body = format!(
            "data: {}\n\n",
            r#"{"error_code":1001,"error_msg":"rate limited, retry in 30s"}"#
        );
        let chunks = stream::iter(vec![Ok::<_, BackendError>(body.into_bytes())]);

        let error = consume_qwen_sse_chunks(
            chunks,
            "test-model",
            Arc::new(TranslationProgressReporter::discard()),
            true,
            Some(Duration::from_secs(1)),
        )
        .await
        .expect_err("upstream error must fail");

        let BackendError::InvalidResponse(message) = error else {
            panic!("expected InvalidResponse, got {error:?}");
        };
        assert_eq!(message, "Qwen 上游错误");
        assert!(!message.contains("30s"));
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
            reporter(Arc::new(RecordingProgress::default())),
            true,
            Some(Duration::from_millis(30)),
        )
        .await
        .expect_err("reasoning must not refresh the content deadline");

        assert!(matches!(error, BackendError::Timeout));
    }
}
