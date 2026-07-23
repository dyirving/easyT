use serde::{Deserialize, Serialize};

/// OpenAI-compatible Chat Completions 请求/响应结构

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
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
