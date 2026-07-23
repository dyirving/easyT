use crate::app_error::{AppError, AppResult};
use crate::commands::config::AppState;
use crate::llm::models::{TranslationRequest, TranslationResult};
use crate::llm::translate;
use tauri::State;

/// 翻译文本
/// config 从 AppState 读取，避免前端携带 api_key
/// 前端参数使用 camelCase（targetLanguage），Rust 端用 snake_case 接收
#[tauri::command]
pub async fn translate_text(
    state: State<'_, AppState>,
    text: String,
    target_language: String,
) -> AppResult<TranslationResult> {
    let config = state.snapshot()?;
    if text.trim().is_empty() {
        return Err(AppError::NoSelectedText);
    }
    if text.chars().count() > config.max_text_length {
        return Err(AppError::TextTooLong);
    }
    let request = TranslationRequest {
        text,
        target_language,
    };
    translate(&config, request).await
}

/// 测试 API 连接
/// 使用前端传入的草稿配置发起极短翻译请求（"hi" → 目标语言）验证可用
/// 不修改 AppState，避免未保存的草稿污染主流程
#[tauri::command]
pub async fn test_api_connection(config: crate::config::AppConfig) -> AppResult<String> {
    crate::commands::config::validate_config(&config)?;
    if config.api_key.trim().is_empty() {
        return Err(AppError::ApiUnauthorized);
    }
    let request = TranslationRequest {
        text: "hi".to_string(),
        target_language: config.target_language.clone(),
    };
    let result = translate(&config, request).await?;
    Ok(format!(
        "连接成功，返回译文长度 {} 字符",
        result.translated_text.chars().count()
    ))
}
