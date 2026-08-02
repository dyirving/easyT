//! Official API SSE decoder.
//!
//! This module only decodes OpenAI-compatible Chat Completions events. It does
//! not perform HTTP, logging, configuration access, or frontend updates.

use serde::Deserialize;

use crate::translation_backend::error::BackendError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenAiDecodeOutcome {
    ContentDelta(String),
    Completed,
}

#[derive(Default)]
pub struct OpenAiSseDecoder {
    buffer: Vec<u8>,
    completed: bool,
    observed_valid_event: bool,
    observed_content: bool,
}

impl OpenAiSseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(
        &mut self,
        chunk: impl AsRef<[u8]>,
    ) -> Result<Vec<OpenAiDecodeOutcome>, BackendError> {
        if self.completed {
            return Ok(Vec::new());
        }

        self.buffer.extend_from_slice(chunk.as_ref());
        let mut outcomes = Vec::new();
        while let Some((block_len, rest_offset)) = find_event_boundary(&self.buffer) {
            let block = self.buffer[..block_len].to_vec();
            self.buffer.drain(..rest_offset);
            let block = std::str::from_utf8(&block).map_err(|_| {
                BackendError::InvalidResponse("Official API SSE 事件包含无效 UTF-8".to_string())
            })?;
            let Some(event) = parse_sse_block(block) else {
                continue;
            };

            let event_outcomes = self.process_event(&event)?;
            for outcome in event_outcomes {
                let completed = matches!(outcome, OpenAiDecodeOutcome::Completed);
                outcomes.push(outcome);
                if completed {
                    self.completed = true;
                    break;
                }
            }
            if self.completed {
                break;
            }
        }
        Ok(outcomes)
    }

    pub fn finish(&self) -> Result<(), BackendError> {
        if self.completed {
            return Ok(());
        }
        if self.observed_content {
            return Err(BackendError::PartialResponse(
                "Official API 流结束但未收到 [DONE]".to_string(),
            ));
        }
        if self.observed_valid_event {
            return Err(BackendError::StreamingUnsupported(
                "Official API 流结束但没有正文或 [DONE]".to_string(),
            ));
        }
        Err(BackendError::StreamingUnsupported(
            "Official API 未返回有效 SSE 事件".to_string(),
        ))
    }

    fn process_event(
        &mut self,
        event: &SseEvent,
    ) -> Result<Vec<OpenAiDecodeOutcome>, BackendError> {
        let data = event.data.trim();
        if data.is_empty() {
            return Ok(Vec::new());
        }
        if data == "[DONE]" {
            return Ok(vec![OpenAiDecodeOutcome::Completed]);
        }

        let parsed: OpenAiChunk = serde_json::from_str(data).map_err(|_| {
            BackendError::InvalidResponse(
                "Official API SSE data 不是有效 Chat Completions JSON".to_string(),
            )
        })?;
        self.observed_valid_event = true;

        if let Some(error) = parsed.error {
            return Err(map_upstream_error(&error));
        }

        let Some(choice) = parsed.choices.into_iter().next() else {
            return Ok(Vec::new());
        };
        let Some(content) = choice.delta.content else {
            return Ok(Vec::new());
        };
        if content.is_empty() {
            return Ok(Vec::new());
        }

        self.observed_content = true;
        Ok(vec![OpenAiDecodeOutcome::ContentDelta(content)])
    }
}

fn map_upstream_error(error: &serde_json::Value) -> BackendError {
    let classification = ["code", "type"]
        .into_iter()
        .filter_map(|field| error.get(field))
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    if classification.contains("rate_limit")
        || classification.contains("too_many_requests")
        || classification.contains("quota")
    {
        BackendError::RateLimited
    } else if classification.contains("auth")
        || classification.contains("unauthorized")
        || classification.contains("invalid_api_key")
    {
        BackendError::Unauthorized
    } else {
        BackendError::InvalidResponse("Official API 返回流式错误事件".to_string())
    }
}

#[derive(Debug, Default)]
struct SseEvent {
    data: String,
}

fn parse_sse_block(block: &str) -> Option<SseEvent> {
    let mut data_lines = Vec::new();
    let mut has_data = false;

    for raw_line in block.split('\n') {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            has_data = true;
        }
    }

    if !has_data {
        return None;
    }
    Some(SseEvent {
        data: data_lines.join("\n"),
    })
}

fn find_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer.windows(2).position(|window| window == b"\n\n");
    let crlf = buffer.windows(4).position(|window| window == b"\r\n\r\n");

    match (lf, crlf) {
        (Some(lf), Some(crlf)) if crlf < lf => Some((crlf, crlf + 4)),
        (Some(lf), _) => Some((lf, lf + 2)),
        (None, Some(crlf)) => Some((crlf, crlf + 4)),
        (None, None) => None,
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiChunk {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    delta: OpenAiDelta,
}

#[derive(Debug, Default, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_event(json: &str) -> String {
        format!("data: {json}\n\n")
    }

    #[test]
    fn emits_content_delta() {
        let json = r#"{"choices":[{"delta":{"content":"你好"}}]}"#;
        let mut decoder = OpenAiSseDecoder::new();

        let outcomes = decoder.feed(data_event(json)).expect("decode");

        assert_eq!(
            outcomes,
            vec![OpenAiDecodeOutcome::ContentDelta("你好".to_string())]
        );
    }

    #[test]
    fn supports_network_and_utf8_splits() {
        let full = data_event(r#"{"choices":[{"delta":{"content":"中文"}}]}"#);
        let split = full.find('中').expect("Chinese text should exist") + 1;
        let mut decoder = OpenAiSseDecoder::new();

        assert!(decoder.feed(&full.as_bytes()[..split]).unwrap().is_empty());
        let outcomes = decoder.feed(&full.as_bytes()[split..]).expect("decode");

        assert_eq!(
            outcomes,
            vec![OpenAiDecodeOutcome::ContentDelta("中文".to_string())]
        );
    }

    #[test]
    fn ignores_role_only_and_empty_choices_events() {
        let chunk = format!(
            "{}{}",
            data_event(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#),
            data_event(r#"{"choices":[]}"#)
        );
        let mut decoder = OpenAiSseDecoder::new();

        assert!(decoder.feed(chunk).unwrap().is_empty());
    }

    #[test]
    fn done_is_required_for_success() {
        let json = r#"{"choices":[{"delta":{"content":"partial"}}]}"#;
        let mut decoder = OpenAiSseDecoder::new();
        decoder.feed(data_event(json)).expect("decode");

        assert!(matches!(
            decoder.finish(),
            Err(BackendError::PartialResponse(_))
        ));
        let outcomes = decoder.feed("data: [DONE]\n\n").expect("done");
        assert_eq!(outcomes, vec![OpenAiDecodeOutcome::Completed]);
        decoder.finish().expect("completed stream");
    }

    #[test]
    fn malformed_json_is_invalid_response() {
        let mut decoder = OpenAiSseDecoder::new();

        let error = decoder.feed("data: {not-json}\n\n").expect_err("must fail");

        assert!(matches!(error, BackendError::InvalidResponse(_)));
    }

    #[test]
    fn upstream_error_event_is_invalid_response() {
        let mut decoder = OpenAiSseDecoder::new();

        let error = decoder
            .feed(data_event(r#"{"error":{"message":"upstream failed"}}"#))
            .expect_err("must fail");

        assert!(matches!(error, BackendError::InvalidResponse(_)));
        assert!(!error.safe_message().contains("upstream failed"));
    }

    #[test]
    fn upstream_error_event_preserves_safe_error_category() {
        let mut decoder = OpenAiSseDecoder::new();

        let error = decoder
            .feed(data_event(
                r#"{"error":{"type":"rate_limit_error","message":"secret detail"}}"#,
            ))
            .expect_err("must fail");

        assert!(matches!(error, BackendError::RateLimited));
        assert!(!error.safe_message().contains("secret detail"));
    }

    #[test]
    fn crlf_and_comments_are_supported() {
        let json = r#"{"choices":[{"delta":{"content":"ok"}}]}"#;
        let mut decoder = OpenAiSseDecoder::new();

        let outcomes = decoder
            .feed(format!(": heartbeat\r\ndata: {json}\r\n\r\n"))
            .expect("decode");

        assert_eq!(
            outcomes,
            vec![OpenAiDecodeOutcome::ContentDelta("ok".to_string())]
        );
    }
}
