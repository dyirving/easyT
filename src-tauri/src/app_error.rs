//! 应用统一错误类型
//! 对应前端 types/index.ts 中的 ERROR_KIND 常量

use serde::{Serialize, Serializer};

use crate::translation_backend::error::{BackendError, BackendErrorKind};
use crate::translation_backend::web_gateway::qwen::QwenError;

/// 应用统一错误类型
/// 对应前端 types/index.ts 中的 ERROR_KIND 常量
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("未检测到选中文本")]
    NoSelectedText,

    #[error("文本长度超过限制")]
    TextTooLong,

    #[error("剪贴板操作失败: {0}")]
    ClipboardError(String),

    #[error("快捷键注册失败: {0}")]
    ShortcutRegistrationFailed(String),

    #[error("配置无效: {0}")]
    ConfigInvalid(String),

    #[error("API Key 无效或未授权 (401)")]
    ApiUnauthorized,

    #[error("请求过于频繁 (429)")]
    ApiRateLimited,

    #[error("请求超时")]
    ApiTimeout,

    #[error("窗口操作失败: {0}")]
    WindowError(String),

    #[error("缓存操作失败: {0}")]
    CacheOperationFailed(String),

    #[error("翻译历史操作失败: {0}")]
    HistoryOperationFailed(String),

    // ===== Backend 错误（来自 TranslationBackend）=====
    #[error("请先在设置中登录 Qwen")]
    LoginRequired,

    #[error("Qwen 登录状态已过期，请重新登录")]
    SessionExpired,

    #[error("翻译请求已被新请求取代")]
    BackendCancelled,

    #[error("后端网络错误: {0}")]
    BackendNetwork(String),

    #[error("上游协议已变化: {0}")]
    BackendProtocolMismatch(String),

    #[error("上游响应不完整: {0}")]
    BackendPartialResponse(String),

    #[error("响应无效: {0}")]
    BackendInvalidResponse(String),

    #[error("当前后端不支持流式输出")]
    BackendStreamingUnsupported,

    #[error("{message}")]
    QwenStorage {
        code: &'static str,
        message: &'static str,
    },

    #[error("{message}")]
    Qwen {
        code: &'static str,
        message: &'static str,
    },

    #[error("内部错误: {0}")]
    Internal(String),
}

/// 序列化为前端可识别的结构 { kind, message }
/// 避免把 Rust panic / 调用栈 / API Key 暴露给前端
#[derive(Serialize)]
struct ErrorResponse {
    kind: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

impl AppError {
    pub(crate) fn kind_str(&self) -> &'static str {
        match self {
            AppError::NoSelectedText => "NoSelectedText",
            AppError::TextTooLong => "TextTooLong",
            AppError::ClipboardError(_) => "ClipboardError",
            AppError::ShortcutRegistrationFailed(_) => "ShortcutRegistrationFailed",
            AppError::ConfigInvalid(_) => "ConfigInvalid",
            AppError::ApiUnauthorized => "ApiUnauthorized",
            AppError::ApiRateLimited => "ApiRateLimited",
            AppError::ApiTimeout => "ApiTimeout",
            AppError::WindowError(_) => "WindowError",
            AppError::CacheOperationFailed(_) => "CacheOperationFailed",
            AppError::HistoryOperationFailed(_) => "HistoryOperationFailed",
            AppError::LoginRequired => "LoginRequired",
            AppError::SessionExpired => "SessionExpired",
            AppError::BackendCancelled => "BackendCancelled",
            AppError::BackendNetwork(_) => "BackendNetwork",
            AppError::BackendProtocolMismatch(_) => "BackendProtocolMismatch",
            AppError::BackendPartialResponse(_) => "BackendPartialResponse",
            AppError::BackendInvalidResponse(_) => "BackendInvalidResponse",
            AppError::BackendStreamingUnsupported => "BackendStreamingUnsupported",
            AppError::QwenStorage { .. } => "Internal",
            AppError::Qwen { .. } => "BackendNetwork",
            AppError::Internal(_) => "Internal",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let resp = ErrorResponse {
            kind: self.kind_str(),
            // 不暴露底层堆栈与敏感信息
            message: self.to_string(),
            code: match self {
                AppError::QwenStorage { code, .. } => Some(*code),
                AppError::Qwen { code, .. } => Some(*code),
                _ => None,
            },
        };
        resp.serialize(serializer)
    }
}

impl From<QwenError> for AppError {
    fn from(error: QwenError) -> Self {
        AppError::QwenStorage {
            code: error.code().as_str(),
            message: error.safe_message(),
        }
    }
}

/// 把 BackendError 映射为 AppError
///
/// 设计原则：
/// - BackendError 携带的内部上下文可能含 URL 等敏感信息，不应原样透传
/// - LoginRequired / SessionExpired / Cancelled 是用户可操作的关键状态
/// - 不允许各 Adapter 自行拼前端错误字符串
impl From<BackendError> for AppError {
    fn from(err: BackendError) -> Self {
        match err.kind() {
            BackendErrorKind::LoginRequired => AppError::LoginRequired,
            BackendErrorKind::SessionExpired => AppError::SessionExpired,
            BackendErrorKind::Unauthorized => AppError::SessionExpired,
            BackendErrorKind::RateLimited => AppError::ApiRateLimited,
            BackendErrorKind::Timeout => AppError::ApiTimeout,
            BackendErrorKind::Cancelled => AppError::BackendCancelled,
            BackendErrorKind::Network => AppError::BackendNetwork(err.safe_message()),
            BackendErrorKind::ProtocolMismatch => {
                AppError::BackendProtocolMismatch(err.safe_message())
            }
            BackendErrorKind::PartialResponse => {
                AppError::BackendPartialResponse(err.safe_message())
            }
            BackendErrorKind::InvalidResponse => {
                AppError::BackendInvalidResponse(err.safe_message())
            }
            BackendErrorKind::StreamingUnsupported => AppError::BackendStreamingUnsupported,
            BackendErrorKind::ConfigInvalid => AppError::ConfigInvalid(err.safe_message()),
            BackendErrorKind::UnsupportedPlatform => {
                AppError::Internal("当前平台不支持此操作".to_string())
            }
            BackendErrorKind::CredentialCorrupted => AppError::SessionExpired,
            BackendErrorKind::QwenPool => match err {
                BackendError::QwenPool(error) => AppError::QwenStorage {
                    code: error.code().as_str(),
                    message: error.safe_message(),
                },
                _ => AppError::Internal("Qwen 账号池错误映射异常".to_string()),
            },
            BackendErrorKind::Qwen => match err {
                BackendError::Qwen(error) => AppError::Qwen {
                    code: error.code().as_str(),
                    message: error.safe_message(),
                },
                _ => AppError::Internal("Qwen 上游错误映射异常".to_string()),
            },
            BackendErrorKind::Internal => AppError::Internal(err.safe_message()),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_login_required_maps_to_login_required() {
        let app_err: AppError = BackendError::LoginRequired.into();
        assert!(matches!(app_err, AppError::LoginRequired));
    }

    #[test]
    fn backend_unauthorized_maps_to_session_expired() {
        let app_err: AppError = BackendError::Unauthorized.into();
        assert!(matches!(app_err, AppError::SessionExpired));
    }

    #[test]
    fn backend_network_does_not_leak_detail() {
        let err = BackendError::Network("https://secret.example.com/path".to_string());
        let app_err: AppError = err.into();
        let msg = app_err.to_string();
        assert!(!msg.contains("secret.example.com"));
    }

    #[test]
    fn backend_cancelled_maps_to_backend_cancelled() {
        let app_err: AppError = BackendError::Cancelled.into();
        assert!(matches!(app_err, AppError::BackendCancelled));
    }

    #[test]
    fn streaming_unsupported_maps_to_frontend_error_kind() {
        let app_err: AppError = BackendError::StreamingUnsupported("unsupported".into()).into();
        let json = serde_json::to_value(app_err).expect("error should serialize");

        assert_eq!(json["kind"], "BackendStreamingUnsupported");
    }

    #[test]
    fn qwen_storage_error_serializes_a_redacted_code() {
        let app_error: AppError = QwenError::storage_read("fake-ticket").into();
        let json = serde_json::to_value(app_error).expect("error should serialize");

        assert_eq!(json["code"], "QW-STORAGE-004");
        assert!(!json["message"].as_str().unwrap().contains("fake-ticket"));
    }

    #[test]
    fn qwen_upstream_errors_keep_their_stable_code_at_the_ipc_boundary() {
        let app_error: AppError = BackendError::Qwen(QwenError::upstream_rate_limited()).into();
        let json = serde_json::to_value(app_error).expect("error should serialize");

        assert_eq!(json["code"], "QW-UPSTREAM-429");
        assert_eq!(json["message"], "Qwen 请求过于频繁");
    }
}
