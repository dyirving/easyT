use serde::{Deserialize, Serialize};

/// OpenAI-compatible Chat Completions 请求/响应结构

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// 思考模式开关配置
/// DeepSeek / GLM / Kimi 均采用相同格式：`{"thinking": {"type": "disabled"}}`
/// - type=disabled：关闭思考（翻译场景默认使用，省 token、降延迟）
/// - type=enabled：开启思考
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
    /// DeepSeek / GLM / Kimi 关闭思考用：`{"type":"disabled"}`
    /// 仅对支持该参数的供应商序列化，其余供应商留 None 不发送
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Qwen（阿里云百炼）关闭思考用：false
    /// 仅对 Qwen 序列化，其余供应商留 None 不发送
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

/// 把响应解析错误映射为 AppError（不暴露底层 serde 细节）
pub fn map_response_parse_error(_err: reqwest::Error) -> AppError {
    AppError::ApiResponseInvalid("响应格式不符合预期".to_string())
}

use crate::app_error::AppError;
