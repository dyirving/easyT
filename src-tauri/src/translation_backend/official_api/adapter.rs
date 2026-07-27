//! Official API Adapter 实现
//!
//! 封装原 `llm::client::translate` 行为：
//! - 复用一个 HTTP Client（由 TranslationBackend 注入）
//! - 保留超时、401、429、5xx 和响应解析逻辑
//! - 保留不同 Official Provider 的 thinking 参数差异
//! - 输出 BackendResult，source.backend 为 OfficialApi

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, ModelProvider};
use crate::translation_backend::error::BackendError;
use crate::translation_backend::models::{
    BackendMode, BackendRequest, BackendResult, BackendSource,
};
use crate::translation_backend::prompt::build_system_prompt;

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
    ) -> Result<BackendResult, BackendError> {
        // 1. 基础校验
        if config.api_key.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("API Key 不能为空".to_string()));
        }
        if config.model.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("模型名称不能为空".to_string()));
        }

        // 2. 构建请求
        let body = build_request_body(config, &request);
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

        Ok(BackendResult {
            translated_text,
            source: BackendSource {
                backend: BackendMode::OfficialApi,
                provider: provider_str(&config.provider),
                model: config.model.clone(),
            },
        })
    }

    /// 测试连接：发起极短翻译请求验证可用
    pub async fn test_connection(
        &self,
        config: &AppConfig,
    ) -> Result<crate::translation_backend::BackendHealth, BackendError> {
        if config.api_key.trim().is_empty() {
            return Err(BackendError::ConfigInvalid("API Key 不能为空".to_string()));
        }

        let request = BackendRequest {
            text: "hi".to_string(),
            target_language: config.target_language.clone(),
        };
        let result = self.translate(config, request).await?;
        Ok(crate::translation_backend::BackendHealth {
            ok: true,
            message: format!(
                "连接成功，返回译文长度 {} 字符",
                result.translated_text.chars().count()
            ),
        })
    }
}

fn provider_str(provider: &ModelProvider) -> String {
    match provider {
        ModelProvider::Agnes => "agnes".to_string(),
        ModelProvider::Deepseek => "deepseek".to_string(),
        ModelProvider::Qwen => "qwen".to_string(),
        ModelProvider::Glm => "glm".to_string(),
        ModelProvider::Kimi => "kimi".to_string(),
        ModelProvider::Doubao => "doubao".to_string(),
        ModelProvider::Custom => "custom".to_string(),
    }
}

fn build_request_body(config: &AppConfig, request: &BackendRequest) -> ChatCompletionRequest {
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
        stream: Some(false),
        thinking,
        enable_thinking,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
