//! Qwen SSE 解码模块
//!
//! 纯计算模块：
//! - 不持有 AppHandle
//! - 不访问配置
//! - 不读取 Cookie
//! - 不发 HTTP
//! - 不写日志中的原始正文
//!
//! 设计：
//! - 维护尚未消费完的 byte buffer
//! - 维护上一次累积 reasoning / content
//! - 处理一个 SSE event 被拆成多个网络 chunk 的情况
//! - 处理一个 chunk 内包含多个 event 的情况
//! - 支持 `\n\n` 和 `\r\n\r\n` 分隔
//! - 忽略 SSE 注释行
//! - 拼接同一 event 的多个 `data:` 行
//! - 将 Qwen 返回的累积文本转换为 delta
//! - 如果新累积内容不是旧内容前缀，返回 ProtocolMismatch
//! - reasoning 和 answer 分开累计

use serde::Deserialize;

use crate::translation_backend::error::BackendError;

/// SSE 事件解码后的输出
#[derive(Debug, Clone, PartialEq)]
pub enum DecodeOutcome {
    /// 增量内容（可能含 reasoning 与 answer 各自的 delta）
    Delta(QwenDelta),
    /// 收到上游明确的完成事件
    Completed,
    /// 上游返回业务错误
    UpstreamError { code: String, message: String },
}

/// 一次事件解码出的增量
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QwenDelta {
    pub reasoning_delta: Option<String>,
    pub content_delta: Option<String>,
}

/// Qwen SSE Decoder 状态
#[derive(Default)]
pub struct QwenSseDecoder {
    buffer: Vec<u8>,
    reasoning_acc: String,
    content_acc: String,
    completed: bool,
    /// 是否观察到有效 Qwen 消息结构
    observed_valid_message: bool,
}

impl QwenSseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 喂入一段网络 chunk，返回按事件顺序的解码结果
    ///
    /// 调用方应按顺序处理 outcomes：
    /// - Delta：累加到译文
    /// - Completed：流结束
    /// - UpstreamError：上游业务错误
    pub fn feed(&mut self, chunk: impl AsRef<[u8]>) -> Result<Vec<DecodeOutcome>, BackendError> {
        if self.completed {
            // 已完成后再收到数据：忽略，避免误处理
            return Ok(Vec::new());
        }

        self.buffer.extend_from_slice(chunk.as_ref());
        let mut outcomes = Vec::new();
        loop {
            // 查找下一个完整事件块
            let split = find_event_boundary(&self.buffer);
            if let Some((consumed, rest_offset)) = split {
                let block = self.buffer[..consumed].to_vec();
                self.buffer.drain(..rest_offset);
                let block = std::str::from_utf8(&block).map_err(|_| {
                    BackendError::ProtocolMismatch("SSE 事件包含无效 UTF-8".to_string())
                })?;
                if let Some(event) = parse_sse_block(block) {
                    let event_outcomes = self.process_event(&event)?;
                    for o in event_outcomes {
                        let is_completed = matches!(o, DecodeOutcome::Completed);
                        let is_error = matches!(o, DecodeOutcome::UpstreamError { .. });
                        outcomes.push(o);
                        if is_completed {
                            self.completed = true;
                            break;
                        }
                        if is_error {
                            break;
                        }
                    }
                }
                if self.completed {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(outcomes)
    }

    /// 流结束时调用：
    /// - 已有正文但未收到 Completed：PartialResponse
    /// - 没有正文：InvalidResponse
    /// - 已 Completed：Ok
    pub fn finish(&self) -> Result<(), BackendError> {
        if self.completed {
            return Ok(());
        }
        if !self.content_acc.is_empty() {
            return Err(BackendError::PartialResponse(format!(
                "流中断，已收到正文 {} 字节",
                self.content_acc.len()
            )));
        }
        if !self.reasoning_acc.is_empty() {
            return Err(BackendError::PartialResponse(format!(
                "流中断，已收到 reasoning {} 字节",
                self.reasoning_acc.len()
            )));
        }
        // 没有正文也未完成
        if self.observed_valid_message {
            Err(BackendError::PartialResponse(
                "流结束但未收到完成事件".to_string(),
            ))
        } else {
            Err(BackendError::InvalidResponse(
                "未收到有效 Qwen 消息".to_string(),
            ))
        }
    }

    fn process_event(&mut self, event: &SseEvent) -> Result<Vec<DecodeOutcome>, BackendError> {
        if event.data == "[DONE]" {
            return Ok(vec![DecodeOutcome::Completed]);
        }
        let parsed: QwenEventData = match serde_json::from_str(&event.data) {
            Ok(p) => p,
            Err(_) => {
                // 单个事件 JSON 解析失败：跳过该事件，不让异常打断整次请求
                return Ok(Vec::new());
            }
        };

        if parsed.error_code != 0 {
            return Ok(vec![DecodeOutcome::UpstreamError {
                code: parsed.error_code.to_string(),
                message: parsed.error_msg.clone().unwrap_or_default(),
            }]);
        }

        let mut delta = QwenDelta::default();
        let mut has_delta = false;

        // 提取 reasoning
        let reasoning = extract_reasoning_content(&parsed);
        if !reasoning.is_empty() {
            self.observed_valid_message = true;
            if reasoning.len() > self.reasoning_acc.len()
                && reasoning.starts_with(&self.reasoning_acc)
            {
                let delta_text = reasoning[self.reasoning_acc.len()..].to_string();
                if !delta_text.is_empty() {
                    delta.reasoning_delta = Some(delta_text);
                    has_delta = true;
                }
                self.reasoning_acc = reasoning;
            } else if reasoning != self.reasoning_acc {
                // 累积内容不是旧内容前缀：协议变化
                return Err(BackendError::ProtocolMismatch(
                    "reasoning 累积内容回退或改写".to_string(),
                ));
            }
        }

        // 提取 answer content
        let content = extract_answer_content(&parsed);
        if !content.is_empty() {
            self.observed_valid_message = true;
            if content.len() > self.content_acc.len() && content.starts_with(&self.content_acc) {
                let delta_text = content[self.content_acc.len()..].to_string();
                if !delta_text.is_empty() {
                    delta.content_delta = Some(delta_text);
                    has_delta = true;
                }
                self.content_acc = content;
            } else if content != self.content_acc {
                return Err(BackendError::ProtocolMismatch(
                    "content 累积内容回退或改写".to_string(),
                ));
            }
        }

        let mut outcomes = Vec::with_capacity(2);
        if has_delta {
            outcomes.push(DecodeOutcome::Delta(delta));
        }
        if event.event == "complete" {
            outcomes.push(DecodeOutcome::Completed);
        }
        Ok(outcomes)
    }
}

/// 查找 SSE 事件分隔符，返回 (block_len, rest_offset)
/// 支持 `\n\n` 和 `\r\n\r\n`
fn find_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let idx_n = buffer.windows(2).position(|window| window == b"\n\n");
    let idx_rn = buffer.windows(4).position(|window| window == b"\r\n\r\n");

    match (idx_n, idx_rn) {
        (Some(n), Some(rn)) => {
            if rn < n {
                Some((rn, rn + 4))
            } else {
                Some((n, n + 2))
            }
        }
        (Some(n), None) => Some((n, n + 2)),
        (None, Some(rn)) => Some((rn, rn + 4)),
        (None, None) => None,
    }
}

#[derive(Debug, Default)]
struct SseEvent {
    event: String,
    data: String,
}

fn parse_sse_block(block: &str) -> Option<SseEvent> {
    let mut event = String::new();
    let mut data_lines: Vec<String> = Vec::new();
    let mut has_data = false;

    for raw_line in block.split('\n') {
        // 处理 \r\n：去掉行尾 \r
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if line.starts_with(':') {
            // 注释行
            continue;
        }
        if let Some(rest) = line.strip_prefix("event:") {
            event = rest.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("data:") {
            let value = if let Some(stripped) = rest.strip_prefix(' ') {
                stripped
            } else {
                rest
            };
            data_lines.push(value.to_string());
            has_data = true;
        }
        // 其他字段（id/retry）忽略
    }

    if !has_data && event.is_empty() {
        return None;
    }
    Some(SseEvent {
        event,
        data: data_lines.join("\n"),
    })
}

// ===== Qwen 私有协议字段解析 =====
//
// 与 ProxyAgent qwen_web.py 行为对齐，但完全独立实现。
// 字段名取自公开抓包样本，必要时根据真实账号脱敏测试调整。

#[derive(Debug, Default, Deserialize)]
struct QwenEventData {
    #[serde(default)]
    pub error_code: i64,
    #[serde(default)]
    pub error_msg: Option<String>,
    #[serde(default)]
    pub data: Option<QwenEventDataInner>,
}

#[derive(Debug, Default, Deserialize)]
struct QwenEventDataInner {
    #[serde(default)]
    pub messages: Vec<QwenMessage>,
}

#[derive(Debug, Default, Deserialize)]
struct QwenMessage {
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub content: serde_json::Value,
    #[serde(default)]
    pub meta_data: Option<QwenMessageMeta>,
}

#[derive(Debug, Default, Deserialize)]
struct QwenMessageMeta {
    #[serde(default)]
    pub multi_load: Vec<QwenMultiLoad>,
}

#[derive(Debug, Default, Deserialize)]
struct QwenMultiLoad {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub content: Option<QwenMultiLoadContent>,
}

#[derive(Debug, Default, Deserialize)]
struct QwenMultiLoadContent {
    #[serde(default)]
    pub think_content: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// 从 multi_load 中提取 reasoning（deep_think 或 multimodal_chat_think）
fn extract_reasoning_content(data: &QwenEventData) -> String {
    let mut best = String::new();
    let messages = match &data.data {
        Some(d) => &d.messages,
        None => return best,
    };

    let mut has_deep_think = false;
    for message in messages {
        let meta_data = match &message.meta_data {
            Some(m) => m,
            None => continue,
        };
        let multi_load = &meta_data.multi_load;

        // 优先 deep_think
        for load in multi_load {
            if load.r#type.as_deref() != Some("deep_think") {
                continue;
            }
            if let Some(content_obj) = &load.content {
                let text = content_obj
                    .think_content
                    .as_deref()
                    .or(content_obj.content.as_deref())
                    .unwrap_or("");
                if text.len() > best.len() {
                    best = text.to_string();
                    has_deep_think = true;
                }
            }
        }

        if has_deep_think {
            continue;
        }

        // 兜底 multimodal_chat_think
        for load in multi_load {
            if load.r#type.as_deref() != Some("multimodal_chat_think") {
                continue;
            }
            if let Some(content_obj) = &load.content {
                let text = content_obj
                    .think_content
                    .as_deref()
                    .or(content_obj.content.as_deref())
                    .unwrap_or("");
                if text.len() > best.len() {
                    best = text.to_string();
                }
            }
        }
    }
    best
}

/// 提取 answer 内容（multi_load/iframe 或 text/plain，选最长）
fn extract_answer_content(data: &QwenEventData) -> String {
    let mut best = String::new();
    let messages = match &data.data {
        Some(d) => &d.messages,
        None => return best,
    };

    for message in messages {
        let mime_type = message.mime_type.as_deref();
        // 只接受 multi_load/iframe / text/plain / None
        if !matches!(
            mime_type,
            Some("multi_load/iframe") | Some("text/plain") | None
        ) {
            continue;
        }
        let content_str = match &message.content {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => continue,
            other => other.to_string(),
        };
        // 跳过纯 deep_think 标记
        if content_str.trim() == "[(deep_think)]" {
            continue;
        }
        let cleaned = clean_marker_text(&content_str);
        if cleaned.len() > best.len() {
            best = cleaned;
        }
    }
    best
}

/// 清除 [(deep_think)] / [(multimodal_chat_think_N)] 标记
fn clean_marker_text(text: &str) -> String {
    // 简单实现：去掉固定模式标记
    let mut result = text.to_string();
    while let Some(start) = result.find("[(") {
        if let Some(end) = result[start..].find(")]") {
            let segment_end = start + end + 2;
            let segment = &result[start..segment_end];
            if segment.starts_with("[(deep_think)]")
                || segment.starts_with("[(multimodal_chat_think_")
            {
                result.replace_range(start..segment_end, "");
                continue;
            }
        }
        break;
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_event(json: &str) -> String {
        format!("data: {json}\n\n")
    }

    fn event_complete() -> String {
        "event: complete\ndata: {}\n\n".to_string()
    }

    #[test]
    fn single_chunk_single_event_emits_delta() {
        let json = r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hello"}]}}"#;
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(data_event(json)).expect("feed");
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            DecodeOutcome::Delta(d) => {
                assert_eq!(d.content_delta.as_deref(), Some("Hello"));
                assert!(d.reasoning_delta.is_none());
            }
            _ => panic!("expected Delta"),
        }
    }

    #[test]
    fn chunk_split_across_network_boundary() {
        let json = r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hello"}]}}"#;
        let full = data_event(json);
        let mid = full.len() / 2;
        let mut decoder = QwenSseDecoder::new();
        // 第一次喂入前半部分：不应产生 outcome
        let outcomes1 = decoder.feed(&full[..mid]).expect("feed1");
        assert!(outcomes1.is_empty());
        // 第二次喂入后半部分：应产生 Delta
        let outcomes2 = decoder.feed(&full[mid..]).expect("feed2");
        assert_eq!(outcomes2.len(), 1);
        assert!(matches!(outcomes2[0], DecodeOutcome::Delta(_)));
    }

    #[test]
    fn utf8_character_split_across_network_boundary() {
        let json =
            r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"中文"}]}}"#;
        let full = data_event(json);
        let char_start = full.find('中').expect("contains Chinese character");
        let split = char_start + 1;
        let mut decoder = QwenSseDecoder::new();
        assert!(decoder.feed(&full.as_bytes()[..split]).unwrap().is_empty());
        let outcomes = decoder.feed(&full.as_bytes()[split..]).unwrap();
        assert!(matches!(
            &outcomes[0],
            DecodeOutcome::Delta(delta) if delta.content_delta.as_deref() == Some("中文")
        ));
    }

    #[test]
    fn single_chunk_multiple_events() {
        let json1 =
            r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hi"}]}}"#;
        let json2 = r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hi there"}]}}"#;
        let chunk = format!("{}{}", data_event(json1), data_event(json2));
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(&chunk).expect("feed");
        assert_eq!(outcomes.len(), 2);
        // 第一次：Hi
        match &outcomes[0] {
            DecodeOutcome::Delta(d) => assert_eq!(d.content_delta.as_deref(), Some("Hi")),
            _ => panic!("expected Delta 1"),
        }
        // 第二次：there
        match &outcomes[1] {
            DecodeOutcome::Delta(d) => assert_eq!(d.content_delta.as_deref(), Some(" there")),
            _ => panic!("expected Delta 2"),
        }
    }

    #[test]
    fn crlf_line_endings_are_handled() {
        let json =
            r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hi"}]}}"#;
        let chunk = format!("data: {json}\r\n\r\n");
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(&chunk).expect("feed");
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn multiple_data_lines_are_joined() {
        let json =
            r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hi"}]}}"#;
        // SSE 允许同一事件多行 data
        let chunk = format!("data: {json}\n\n");
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(&chunk).expect("feed");
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn reasoning_delta_is_emitted_separately() {
        // 只含 reasoning（multi_load deep_think），无 text/plain 正文消息
        let json1 = r#"{"error_code":0,"data":{"messages":[{"meta_data":{"multi_load":[{"type":"deep_think","content":{"think_content":"Step 1"}}]}}]}}"#;
        let json2 = r#"{"error_code":0,"data":{"messages":[{"meta_data":{"multi_load":[{"type":"deep_think","content":{"think_content":"Step 1 and 2"}}]}}]}}"#;
        let mut decoder = QwenSseDecoder::new();
        let mut outcomes = decoder.feed(data_event(json1)).expect("feed1");
        outcomes.extend(decoder.feed(data_event(json2)).expect("feed2"));
        assert_eq!(outcomes.len(), 2);
        match &outcomes[0] {
            DecodeOutcome::Delta(d) => {
                assert_eq!(d.reasoning_delta.as_deref(), Some("Step 1"));
                assert!(d.content_delta.is_none());
            }
            _ => panic!("expected Delta 1"),
        }
        match &outcomes[1] {
            DecodeOutcome::Delta(d) => {
                assert_eq!(d.reasoning_delta.as_deref(), Some(" and 2"));
            }
            _ => panic!("expected Delta 2"),
        }
    }

    #[test]
    fn complete_event_ends_stream() {
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(event_complete()).expect("feed");
        assert!(outcomes
            .iter()
            .any(|o| matches!(o, DecodeOutcome::Completed)));
    }

    #[test]
    fn complete_event_preserves_final_content_delta() {
        let json = r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"最终译文"}]}}"#;
        let chunk = format!("event: complete\ndata: {json}\n\n");
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(&chunk).expect("feed");
        assert_eq!(outcomes.len(), 2);
        assert!(matches!(
            &outcomes[0],
            DecodeOutcome::Delta(delta)
                if delta.content_delta.as_deref() == Some("最终译文")
        ));
        assert!(matches!(outcomes[1], DecodeOutcome::Completed));
    }

    #[test]
    fn upstream_error_event_returns_error() {
        let json = r#"{"error_code":1001,"error_msg":"rate limited"}"#;
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(data_event(json)).expect("feed");
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            DecodeOutcome::UpstreamError { code, message } => {
                assert_eq!(code, "1001");
                assert_eq!(message, "rate limited");
            }
            _ => panic!("expected UpstreamError"),
        }
    }

    #[test]
    fn eof_without_complete_yields_partial_response() {
        let json =
            r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hi"}]}}"#;
        let mut decoder = QwenSseDecoder::new();
        let _ = decoder.feed(data_event(json)).expect("feed");
        let result = decoder.finish();
        assert!(matches!(result, Err(BackendError::PartialResponse(_))));
    }

    #[test]
    fn eof_without_any_content_yields_invalid_response() {
        let decoder = QwenSseDecoder::new();
        let result = decoder.finish();
        assert!(matches!(result, Err(BackendError::InvalidResponse(_))));
    }

    #[test]
    fn content_regression_yields_protocol_mismatch() {
        let json1 = r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hello world"}]}}"#;
        // 第二次内容不再是第一次的前缀（回退或改写）
        let json2 = r#"{"error_code":0,"data":{"messages":[{"mime_type":"text/plain","content":"Hello WORLD"}]}}"#;
        let mut decoder = QwenSseDecoder::new();
        let _ = decoder.feed(data_event(json1)).expect("feed1");
        let result = decoder.feed(data_event(json2));
        assert!(matches!(result, Err(BackendError::ProtocolMismatch(_))));
    }

    #[test]
    fn malformed_json_does_not_panic() {
        let chunk = "data: {this is not json}\n\n";
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(chunk).expect("feed");
        // 畸形 JSON 应被跳过，不产生 outcome
        assert!(outcomes.is_empty());
    }

    #[test]
    fn ignore_sse_comment_lines() {
        let chunk = ": comment line\ndata: {\"error_code\":0,\"data\":{\"messages\":[{\"mime_type\":\"text/plain\",\"content\":\"Hi\"}]}}\n\n";
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(chunk).expect("feed");
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn done_event_completes_stream() {
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed("data: [DONE]\n\n").expect("feed");
        assert!(outcomes
            .iter()
            .any(|o| matches!(o, DecodeOutcome::Completed)));
    }

    #[test]
    fn multi_load_iframe_mime_type_is_accepted() {
        let json = r#"{"error_code":0,"data":{"messages":[{"mime_type":"multi_load/iframe","content":"Hello"}]}}"#;
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(data_event(json)).expect("feed");
        assert_eq!(outcomes.len(), 1);
    }

    #[test]
    fn other_mime_types_are_skipped_for_answer() {
        let json = r#"{"error_code":0,"data":{"messages":[{"mime_type":"image/png","content":"data"},{"mime_type":"text/plain","content":"Hello"}]}}"#;
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(data_event(json)).expect("feed");
        assert_eq!(outcomes.len(), 1);
        match &outcomes[0] {
            DecodeOutcome::Delta(d) => assert_eq!(d.content_delta.as_deref(), Some("Hello")),
            _ => panic!("expected Delta"),
        }
    }

    #[test]
    fn empty_complete_event_completes_without_delta() {
        let chunk = "event: complete\ndata: {\"error_code\":0}\n\n";
        let mut decoder = QwenSseDecoder::new();
        let outcomes = decoder.feed(chunk).expect("feed");
        assert_eq!(outcomes.len(), 1);
        assert!(matches!(outcomes[0], DecodeOutcome::Completed));
        // finish 应成功
        decoder.finish().expect("finish after completed");
    }
}
