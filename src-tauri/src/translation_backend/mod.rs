//! 翻译后端深模块
//!
//! TranslationBackend 是翻译能力唯一的外部 seam：
//! - 根据 AppConfig.backend_mode 路由到 OfficialApiAdapter 或 WebGateway
//! - 在进入 Adapter 前执行共同输入校验
//! - 返回统一 BackendResult
//! - 将 Adapter 的错误统一为 BackendError
//!
//! 它不负责：
//! - latest-wins generation（继续由 TranslationRequestManager 唯一负责）
//! - 创建登录窗口
//! - Cookie 提取或凭证持久化
//! - Qwen Header、请求体、SSE 字段
//! - 前端状态更新

pub mod error;
pub mod models;
pub mod official_api;
pub mod prompt;
pub mod web_gateway;

pub use error::BackendError;
pub use models::{BackendMode, BackendRequest, BackendResult, TranslationProgress};

use std::sync::Arc;

use crate::config::AppConfig;

use self::official_api::OfficialApiAdapter;
use self::web_gateway::WebGateway;

/// 后端连接健康检查结果
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendHealth {
    pub ok: bool,
    pub message: String,
}

impl BackendHealth {
    fn translation_succeeded(prefix: &str, result: &BackendResult) -> Self {
        Self {
            ok: true,
            message: format!(
                "{prefix}，返回译文长度 {} 字符",
                result.translated_text.chars().count()
            ),
        }
    }
}

struct DiscardProgress;

impl TranslationProgress for DiscardProgress {
    fn emit(&self, _progress: models::BackendProgress) -> Result<(), BackendError> {
        Ok(())
    }
}

/// 翻译后端统一入口
pub struct TranslationBackend {
    official_api: OfficialApiAdapter,
    web_gateway: Arc<WebGateway>,
}

impl TranslationBackend {
    pub fn new(http_client: reqwest::Client) -> Self {
        let official_api = OfficialApiAdapter::new(http_client.clone());
        let web_gateway = Arc::new(WebGateway::new(http_client));
        Self {
            official_api,
            web_gateway,
        }
    }

    /// 共享的 WebGateway 引用（供登录管理命令转发使用）
    pub fn web_gateway(&self) -> Arc<WebGateway> {
        Arc::clone(&self.web_gateway)
    }

    /// 翻译入口
    ///
    /// 根据 `config.backend_mode` 路由：
    /// - OfficialApi：调用 OfficialApiAdapter，沿用现有 OpenAI 兼容协议
    /// - WebGateway：调用 WebGateway，按 provider 转发到具体 QwenWebAdapter
    ///
    /// 共同输入校验在此完成；Adapter 内部只校验与自己协议相关的字段。
    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
    ) -> Result<BackendResult, BackendError> {
        validate_translate_request(&request, config)?;

        match config.backend_mode {
            BackendMode::OfficialApi => self.official_api.translate(config, request).await,
            BackendMode::WebGateway => self.web_gateway.translate(config, request).await,
        }
    }

    /// 流式翻译入口：只向 progress 报告可见正文，完成后仍返回完整结果。
    pub async fn translate_stream(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: std::sync::Arc<dyn TranslationProgress>,
    ) -> Result<BackendResult, BackendError> {
        validate_translate_request(&request, config)?;

        match config.backend_mode {
            BackendMode::OfficialApi => {
                self.official_api
                    .translate_stream(config, request, progress)
                    .await
            }
            BackendMode::WebGateway => {
                self.web_gateway
                    .translate_stream(config, request, progress)
                    .await
            }
        }
    }

    /// 测试连接：必须通过当前 Adapter 进行真实轻量请求
    /// WebGateway 模式不得仅检查本地 ticket 存在后返回成功
    pub async fn test_connection(&self, config: &AppConfig) -> Result<BackendHealth, BackendError> {
        validate_test_connection(config)?;
        match config.backend_mode {
            BackendMode::OfficialApi if config.stream_output => {
                self.official_api
                    .test_connection_stream(config, Arc::new(DiscardProgress))
                    .await
            }
            BackendMode::OfficialApi => self.official_api.test_connection(config).await,
            BackendMode::WebGateway if config.stream_output => {
                self.web_gateway
                    .test_connection_stream(config, Arc::new(DiscardProgress))
                    .await
            }
            BackendMode::WebGateway => self.web_gateway.test_connection(config).await,
        }
    }
}

fn validate_translate_request(
    request: &BackendRequest,
    config: &AppConfig,
) -> Result<(), BackendError> {
    if request.text.trim().is_empty() {
        return Err(BackendError::ConfigInvalid("翻译文本不能为空".to_string()));
    }
    if request.text.chars().count() > config.max_text_length {
        return Err(BackendError::ConfigInvalid(format!(
            "文本长度超过最大限制 {}",
            config.max_text_length
        )));
    }
    if request.target_language.trim().is_empty() {
        return Err(BackendError::ConfigInvalid("目标语言不能为空".to_string()));
    }
    Ok(())
}

fn validate_test_connection(config: &AppConfig) -> Result<(), BackendError> {
    if config.timeout_seconds < 5 || config.timeout_seconds > 300 {
        return Err(BackendError::ConfigInvalid(
            "请求超时时间应在 5～300 秒之间".to_string(),
        ));
    }
    if config.max_text_length < 100 || config.max_text_length > 20000 {
        return Err(BackendError::ConfigInvalid(
            "最大翻译字符数应在 100～20000 之间".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_translate_rejects_empty_text() {
        let mut config = crate::config::default_config();
        config.max_text_length = 100;
        let request = BackendRequest {
            text: "   ".to_string(),
            target_language: "简体中文".to_string(),
        };
        let err = validate_translate_request(&request, &config).expect_err("should reject");
        assert!(matches!(err, BackendError::ConfigInvalid(_)));
    }

    #[test]
    fn validate_translate_rejects_oversize_text() {
        let mut config = crate::config::default_config();
        config.max_text_length = 3;
        let request = BackendRequest {
            text: "abcdef".to_string(),
            target_language: "简体中文".to_string(),
        };
        let err = validate_translate_request(&request, &config).expect_err("should reject");
        assert!(matches!(err, BackendError::ConfigInvalid(_)));
    }
}
