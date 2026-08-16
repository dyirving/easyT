//! WebGateway Adapter：使用网页登录态调用 Qwen 私有接口
//!
//! 第一版仅支持 Qwen；不要为假设中的未来供应商预先引入动态注册表。
//! 内部使用显式 `match WebProviderKind::Qwen` 路由。

pub mod credential_store;
pub mod qwen;

pub use qwen::{QwenAccountPoolSnapshot, QwenSession};

use std::path::Path;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::translation_backend::error::BackendError;
use crate::translation_backend::models::{BackendRequest, BackendResult, WebProviderKind};
use crate::translation_backend::TranslationProgressReporter;

use self::qwen::{reconcile_legacy_migration, QwenAccountPool, QwenError};

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
    legacy_qwen_session: Arc<QwenSession>,
    qwen_account_pool: Arc<QwenAccountPool>,
}

impl WebGateway {
    pub fn open(http_client: reqwest::Client, app_data: &Path) -> Result<Self, QwenError> {
        let qwen_root = app_data.join("web_gateway").join("qwen");
        reconcile_legacy_migration(&qwen_root)?;
        let qwen_account_pool = Arc::new(QwenAccountPool::open(&qwen_root, http_client.clone())?);
        qwen_account_pool.restore_from_storage()?;
        // Legacy login commands still operate on the first enabled account until their callers
        // move to the account-specific commands. Translation itself always uses the pool.
        let legacy_qwen_session = qwen_account_pool
            .first_session()
            .unwrap_or_else(|| Arc::new(QwenSession::new()));
        Ok(Self {
            legacy_qwen_session,
            qwen_account_pool,
        })
    }

    /// 共享 QwenSession 引用（供登录管理命令转发）
    pub fn qwen_session(&self) -> Arc<QwenSession> {
        Arc::clone(&self.legacy_qwen_session)
    }

    pub fn qwen_account_pool(&self) -> QwenAccountPoolSnapshot {
        self.qwen_account_pool.snapshot()
    }

    pub fn qwen_accounts(&self) -> Arc<QwenAccountPool> {
        Arc::clone(&self.qwen_account_pool)
    }

    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => {
                self.qwen_account_pool
                    .translate(config, request, progress)
                    .await
            }
        }
    }

    pub async fn translate_stream(
        &self,
        config: &AppConfig,
        request: BackendRequest,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<BackendResult, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => {
                self.qwen_account_pool
                    .translate_stream(config, request, progress)
                    .await
            }
        }
    }

    pub async fn test_connection(
        &self,
        config: &AppConfig,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<String, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => self.qwen_account_pool.test_global(config, progress).await,
        }
    }

    pub async fn test_connection_stream(
        &self,
        config: &AppConfig,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<String, BackendError> {
        match config.web_gateway.provider {
            WebProviderKind::Qwen => self.qwen_account_pool.test_global(config, progress).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translation_backend::web_gateway::qwen::test_support;

    #[test]
    fn legacy_account_migrates_before_the_authoritative_snapshot_is_exposed() {
        let app_data = test_support::TestDir::new("web-gateway-startup");
        let qwen_root = app_data.path().join("web_gateway").join("qwen");
        std::fs::create_dir_all(&qwen_root).unwrap();
        std::fs::write(qwen_root.join("credentials.bin"), "fake-ticket").unwrap();

        let gateway = WebGateway::open(reqwest::Client::new(), app_data.path()).unwrap();
        let snapshot = gateway.qwen_account_pool();

        assert_eq!(snapshot.accounts.len(), 1);
        assert_eq!(snapshot.accounts[0].display_name, "默认账号");
        assert!(gateway
            .qwen_session()
            .account_dir()
            .unwrap()
            .join("credentials.bin")
            .exists());
        assert!(!qwen_root.join("credentials.bin").exists());
    }
}
