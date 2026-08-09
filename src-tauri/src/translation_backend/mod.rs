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
pub use models::{
    BackendMode, BackendRequest, BackendResult, TranslationOptions, TranslationOutcome,
    TranslationProgress,
};

use std::sync::Arc;

use crate::config::AppConfig;

use self::models::CacheStatus;
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
    /// options.force_refresh 决定缓存策略（Use/Refresh），策略结果随 outcome 返回。
    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        options: TranslationOptions,
    ) -> Result<TranslationOutcome, BackendError> {
        validate_translate_request(&request, config)?;
        let policy = resolve_cache_policy(config, options);

        let result = match config.backend_mode {
            BackendMode::OfficialApi => self.official_api.translate(config, request).await,
            BackendMode::WebGateway => self.web_gateway.translate(config, request).await,
        };
        Ok(outcome_for(result?, policy))
    }

    /// 流式翻译入口：只向 progress 报告可见正文，完成后仍返回完整结果。
    pub async fn translate_stream(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        options: TranslationOptions,
        progress: std::sync::Arc<dyn TranslationProgress>,
    ) -> Result<TranslationOutcome, BackendError> {
        validate_translate_request(&request, config)?;
        let policy = resolve_cache_policy(config, options);

        let result = match config.backend_mode {
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
        };
        Ok(outcome_for(result?, policy))
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

/// 缓存策略：唯一决策点。缓存模块接入后，Use 分支在此执行查找/回填。
/// - WebGateway 且保存网页历史：Bypass（测试连接/诊断/saveHistory 同样绕过）
/// - 用户显式重新翻译：Refresh
/// - 其余：Use
fn resolve_cache_policy(config: &AppConfig, options: TranslationOptions) -> CachePolicy {
    if config.backend_mode == BackendMode::WebGateway && config.web_gateway.save_history {
        CachePolicy::Bypass
    } else if options.force_refresh {
        CachePolicy::Refresh
    } else {
        CachePolicy::Use
    }
}

/// 未接入实际缓存：Use 视为 miss，Refresh/Bypass 报告对应来源状态，fromCache 均为 false。
fn outcome_for(result: BackendResult, policy: CachePolicy) -> TranslationOutcome {
    let cache_status = match policy {
        CachePolicy::Use => CacheStatus::Miss,
        CachePolicy::Refresh => CacheStatus::Refreshed,
        CachePolicy::Bypass => CacheStatus::Bypassed,
    };
    TranslationOutcome {
        result,
        cache_status,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CachePolicy {
    Use,
    Refresh,
    Bypass,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation_backend::models::BackendSource;

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

    #[test]
    fn plain_request_is_use_policy() {
        let config = crate::config::default_config();
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: false
                }
            ),
            CachePolicy::Use
        );
    }

    #[test]
    fn explicit_refresh_is_refresh_policy() {
        let config = crate::config::default_config();
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: true
                }
            ),
            CachePolicy::Refresh
        );
    }

    #[test]
    fn web_gateway_save_history_bypasses_even_when_refreshing() {
        let mut config = crate::config::default_config();
        config.backend_mode = BackendMode::WebGateway;
        config.web_gateway.save_history = true;
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: true
                }
            ),
            CachePolicy::Bypass
        );
    }

    #[test]
    fn web_gateway_without_save_history_uses_policy() {
        let mut config = crate::config::default_config();
        config.backend_mode = BackendMode::WebGateway;
        config.web_gateway.save_history = false;
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: false
                }
            ),
            CachePolicy::Use
        );
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: true
                }
            ),
            CachePolicy::Refresh
        );
    }

    #[test]
    fn outcome_reports_non_cache_status_without_cache() {
        let result = BackendResult {
            translated_text: "你好".to_string(),
            source: BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "agnes".to_string(),
                model: "agnes-2.0-flash".to_string(),
            },
        };
        assert_eq!(
            outcome_for(result.clone(), CachePolicy::Use).cache_status,
            CacheStatus::Miss
        );
        assert_eq!(
            outcome_for(result.clone(), CachePolicy::Refresh).cache_status,
            CacheStatus::Refreshed
        );
        assert_eq!(
            outcome_for(result, CachePolicy::Bypass).cache_status,
            CacheStatus::Bypassed
        );
    }
}
