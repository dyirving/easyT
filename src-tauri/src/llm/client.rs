use std::time::Duration;

use crate::app_error::{AppError, AppResult};
use crate::config::AppConfig;
use crate::llm::error::{
    map_response_parse_error, ChatCompletionRequest, ChatCompletionResponse, ChatMessage,
};
use crate::llm::models::{TranslationRequest, TranslationResult};
use crate::llm::prompt::build_system_prompt;

/// 大模型翻译入口
/// 调用 OpenAI-compatible Chat Completions API
///
/// 安全约束：api_key 仅出现在 Authorization 头中，绝不写入日志或错误信息
pub async fn translate(
    config: &AppConfig,
    request: TranslationRequest,
) -> AppResult<TranslationResult> {
    // 1. 基础校验
    if config.api_key.trim().is_empty() {
        return Err(AppError::ApiUnauthorized);
    }
    if config.model.trim().is_empty() {
        return Err(AppError::ConfigInvalid("模型名称不能为空".to_string()));
    }

    // 2. 构建请求
    let body = build_request(config, &request);
    let url = format!("{}/chat/completions", normalize_base_url(&config.base_url));

    // 3. 构建 HTTP 客户端（带超时）
    let timeout = config.timeout_seconds.clamp(5, 300);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .build()
        .map_err(|e| AppError::Internal(format!("HTTP 客户端构建失败: {e}")))?;

    // 日志只记录非敏感字段，绝不记录 api_key
    log::info!(
        "请求翻译: model={}, target_language={}, text_len={}",
        config.model,
        request.target_language,
        request.text.chars().count()
    );

    // 4. 发起请求
    let resp = client
        .post(&url)
        .bearer_auth(&config.api_key)
        .json(&body)
        .send()
        .await
        .map_err(map_request_error)?;

    // 5. 处理 HTTP 状态码
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(map_status_to_error(status, &body));
    }

    // 6. 解析响应
    let parsed: ChatCompletionResponse = resp.json().await.map_err(map_response_parse_error)?;
    let translated_text = parsed
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| AppError::ApiResponseInvalid("响应缺少 choices".to_string()))?;

    let translated_text = translated_text.trim().to_string();
    if translated_text.is_empty() {
        return Err(AppError::ApiResponseInvalid("译文为空".to_string()));
    }
    Ok(TranslationResult { translated_text })
}

/// 构建 OpenAI-compatible 请求体
fn build_request(config: &AppConfig, request: &TranslationRequest) -> ChatCompletionRequest {
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
    }
}

/// 规范化 Base URL：去除末尾斜杠，避免拼出 //
fn normalize_base_url(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_string()
}

/// 把 HTTP 状态码映射为 AppError
/// 对于非 2xx 响应，尝试读取 body 文本作为错误上下文（不暴露完整堆栈）
fn map_status_to_error(status: reqwest::StatusCode, body: &str) -> AppError {
    let code = status.as_u16();
    match code {
        401 => AppError::ApiUnauthorized,
        429 => AppError::ApiRateLimited,
        500..=599 => AppError::ApiRequestFailed(format!("服务器错误 ({})", code)),
        _ => {
            let detail = summarize_error_body(body);
            if detail.is_empty() {
                AppError::ApiRequestFailed(format!("HTTP {}", code))
            } else {
                AppError::ApiRequestFailed(format!("HTTP {}: {}", code, detail))
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

/// 把 reqwest::Error 映射为 AppError
/// 区分超时、状态码错误、网络错误
pub fn map_request_error(err: reqwest::Error) -> AppError {
    if err.is_timeout() {
        AppError::ApiTimeout
    } else if let Some(status) = err.status() {
        match status.as_u16() {
            401 => AppError::ApiUnauthorized,
            429 => AppError::ApiRateLimited,
            500..=599 => AppError::ApiRequestFailed(format!("服务器错误 ({})", status)),
            _ => AppError::ApiRequestFailed(format!("HTTP {}", status)),
        }
    } else {
        // 网络层错误（DNS、连接拒绝、TLS 等）
        AppError::ApiRequestFailed("网络请求失败".to_string())
    }
}
