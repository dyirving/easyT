//! WebGateway Adapter：使用网页登录态调用 Qwen 私有接口
//!
//! 第一版仅支持 Qwen；不要为假设中的未来供应商预先引入动态注册表。
//! 内部使用显式 `match WebProviderKind::Qwen` 路由。

pub mod credential_store;
pub mod qwen;

pub use qwen::QwenSession;

use std::sync::Arc;

use crate::config::AppConfig;
use crate::translation_backend::error::BackendError;
use crate::translation_backend::models::{
    BackendMode, BackendRequest, BackendResult, TranslationProgress, WebProviderKind,
};
use crate::translation_backend::BackendHealth;

use self::qwen::QwenWebAdapter;

/// WebGateway 入口
///
/// 职责：
/// - 根据 `WebProviderKind` 路由（第一版显式 match Qwen）
/// - 检查凭证状态
/// - 复用 HTTP Client
/// - 应用统一请求超时
/// - 执行有限重试
/// - 对日志进行敏感信息过滤
/// - 将 Qwen 错误转换为 BackendError
pub struct WebGateway {
    qwen: QwenWebAdapter,
}

impl WebGateway {
    pub fn new(http_client: reqwest::Client) -> Self {
        let qwen = QwenWebAdapter::new(http_client);
        Self { qwen }
    }

    /// 共享 QwenSession 引用（供登录管理命令转发）
    pub fn qwen_session(&self) -> Arc<QwenSession> {
        self.qwen.session()
    }

    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
    ) -> Result<BackendResult, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => {
                let mut result = self.qwen.translate(config, request).await?;
                // 统一标识为 WebGateway 来源
                result.source.backend = BackendMode::WebGateway;
                Ok(result)
            }
        }
    }

    pub async fn translate_stream(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<dyn TranslationProgress>,
    ) -> Result<BackendResult, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => {
                let mut result = self
                    .qwen
                    .translate_stream(config, request, progress)
                    .await?;
                result.source.backend = BackendMode::WebGateway;
                Ok(result)
            }
        }
    }

    pub async fn test_connection(&self, config: &AppConfig) -> Result<BackendHealth, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => self.qwen.test_connection(config).await,
        }
    }

    pub async fn test_connection_stream(
        &self,
        config: &AppConfig,
        progress: Arc<dyn TranslationProgress>,
    ) -> Result<BackendHealth, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => self.qwen.test_connection_stream(config, progress).await,
        }
    }
}
