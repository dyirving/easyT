//! 翻译后端统一错误类型
//!
//! 设计原则：
//! - 内部错误可携带可诊断上下文，但绝不含 Cookie、ticket、Header 或用户正文
//! - ProtocolMismatch 表示私有协议结构已变化，不应归类为普通网络错误
//! - PartialResponse 不能作为成功返回
//! - Cancelled 不应把 QwenSession 标记为 Expired

use serde::Serialize;

use crate::translation_backend::web_gateway::qwen::QwenError;

/// 已识别的上游上下文过长错误（FR-010）：固定安全文案，绝不透出上游正文。
/// 前端 `src/services/tauriCommands.ts` 的 `TERMBASE_CONTEXT_LENGTH_MESSAGE` 必须保持同步。
pub(crate) const TERMBASE_CONTEXT_LENGTH_MESSAGE: &str = "内容过长，超出上游上下文限制";

/// 非空有效术语集下通用失败追加的非断言建议（FR-010）：错误分类不变，仅追加建议。
/// 前端 `src/services/tauriCommands.ts` 的 `TERMBASE_CONTEXT_SUGGESTION` 必须保持同步。
pub(crate) const TERMBASE_CONTEXT_SUGGESTION: &str =
    "如果开启了术语表，可尝试精简术语或临时关闭术语表后重试";

/// 上游错误正文的上下文过长模式识别（FR-010）。
///
/// 只做模式判定，绝不把正文写入错误或日志；识别成功后由调用方映射为
/// [`TERMBASE_CONTEXT_LENGTH_MESSAGE`]。模式（大小写不敏感）：
/// - 英文：`context length`、`too long`、`token limit`、`input length`
/// - 中文：`超出上下文`、`上下文长度`、`太长`
pub(crate) fn is_context_length_pattern(message: &str) -> bool {
    let lowered = message.to_lowercase();
    [
        "context length",
        "too long",
        "token limit",
        "input length",
        "超出上下文",
        "上下文长度",
        "太长",
    ]
    .iter()
    .any(|pattern| lowered.contains(pattern))
}

/// 翻译后端统一错误
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    /// WebGateway 模式下本地无可用凭证，需要用户先登录
    #[error("未登录，请先在设置中登录 Qwen")]
    LoginRequired,

    /// 凭证曾存在但已过期，需要重新登录
    #[error("登录状态已过期，请重新登录")]
    SessionExpired,

    /// 上游返回 401/403，已将 QwenSession 标记为 Expired
    #[error("上游未授权")]
    Unauthorized,

    /// 上游返回 429，已达到速率限制
    #[error("请求过于频繁")]
    RateLimited,

    /// 请求超时
    #[error("请求超时")]
    Timeout,

    /// 用户 abort（新请求取代旧请求）；不应改变登录状态
    #[error("翻译请求已被新请求取代")]
    Cancelled,

    /// 网络层错误（DNS、连接拒绝、TLS 等）
    #[error("网络错误: {0}")]
    Network(String),

    /// Qwen 私有协议结构已变化（字段缺失或形状变化）
    #[error("上游协议结构已变化: {0}")]
    ProtocolMismatch(String),

    /// 流式响应中断且已收到部分正文，不能作为成功返回
    #[error("部分响应: {0}")]
    PartialResponse(String),

    /// 响应无法解析或缺少必要字段
    #[error("响应无效: {0}")]
    InvalidResponse(String),

    /// 当前后端不支持标准流式输出或未遵循流式协议
    #[error("当前后端不支持流式输出: {0}")]
    StreamingUnsupported(String),

    /// 配置无效
    #[error("配置无效: {0}")]
    ConfigInvalid(String),

    /// 当前平台不支持此操作
    #[error("当前平台不支持此操作")]
    #[cfg_attr(windows, allow(dead_code))]
    UnsupportedPlatform,

    /// 凭证文件已损坏，需要用户显式注销后重建
    #[error("凭证文件已损坏")]
    CredentialCorrupted,

    /// Qwen account-pool selection failed before a concrete upstream request.
    #[error("{0}")]
    QwenPool(QwenError),

    /// A Qwen request reached the protocol boundary and has a stable public Qwen error code.
    #[error("{0}")]
    Qwen(QwenError),

    /// 内部错误（不应暴露底层堆栈给前端）
    #[error("内部错误: {0}")]
    Internal(String),
}

/// 用于序列化给前端的错误标识
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BackendErrorKind {
    LoginRequired,
    SessionExpired,
    Unauthorized,
    RateLimited,
    Timeout,
    Cancelled,
    Network,
    ProtocolMismatch,
    PartialResponse,
    InvalidResponse,
    StreamingUnsupported,
    ConfigInvalid,
    UnsupportedPlatform,
    CredentialCorrupted,
    QwenPool,
    Qwen,
    Internal,
}

impl BackendError {
    pub fn kind(&self) -> BackendErrorKind {
        match self {
            BackendError::LoginRequired => BackendErrorKind::LoginRequired,
            BackendError::SessionExpired => BackendErrorKind::SessionExpired,
            BackendError::Unauthorized => BackendErrorKind::Unauthorized,
            BackendError::RateLimited => BackendErrorKind::RateLimited,
            BackendError::Timeout => BackendErrorKind::Timeout,
            BackendError::Cancelled => BackendErrorKind::Cancelled,
            BackendError::Network(_) => BackendErrorKind::Network,
            BackendError::ProtocolMismatch(_) => BackendErrorKind::ProtocolMismatch,
            BackendError::PartialResponse(_) => BackendErrorKind::PartialResponse,
            BackendError::InvalidResponse(_) => BackendErrorKind::InvalidResponse,
            BackendError::StreamingUnsupported(_) => BackendErrorKind::StreamingUnsupported,
            BackendError::ConfigInvalid(_) => BackendErrorKind::ConfigInvalid,
            BackendError::UnsupportedPlatform => BackendErrorKind::UnsupportedPlatform,
            BackendError::CredentialCorrupted => BackendErrorKind::CredentialCorrupted,
            BackendError::QwenPool(_) => BackendErrorKind::QwenPool,
            BackendError::Qwen(_) => BackendErrorKind::Qwen,
            BackendError::Internal(_) => BackendErrorKind::Internal,
        }
    }

    /// 返回不携带敏感上下文的用户可读消息
    ///
    /// FR-010：当通用失败的消息携带非空术语集建议（由 translate 编排追加）时，
    /// 该消息只由固定安全文案拼接而成，可安全透传给 IPC。
    pub fn safe_message(&self) -> String {
        match self {
            BackendError::Network(message) if message.contains(TERMBASE_CONTEXT_SUGGESTION) => {
                message.clone()
            }
            BackendError::ProtocolMismatch(message)
                if message.contains(TERMBASE_CONTEXT_SUGGESTION) =>
            {
                message.clone()
            }
            BackendError::PartialResponse(message)
                if message.contains(TERMBASE_CONTEXT_SUGGESTION) =>
            {
                message.clone()
            }
            BackendError::InvalidResponse(message)
                if message == TERMBASE_CONTEXT_LENGTH_MESSAGE =>
            {
                message.clone()
            }
            BackendError::InvalidResponse(message)
                if message.contains(TERMBASE_CONTEXT_SUGGESTION) =>
            {
                message.clone()
            }
            BackendError::StreamingUnsupported(message)
                if message.contains(TERMBASE_CONTEXT_SUGGESTION) =>
            {
                message.clone()
            }
            BackendError::ConfigInvalid(message)
                if message.contains(TERMBASE_CONTEXT_SUGGESTION) =>
            {
                message.clone()
            }
            BackendError::Internal(message) if message.contains(TERMBASE_CONTEXT_SUGGESTION) => {
                message.clone()
            }
            BackendError::Network(_) => "网络请求失败".to_string(),
            BackendError::ProtocolMismatch(_) => "上游协议结构已变化".to_string(),
            BackendError::PartialResponse(_) => "上游响应不完整".to_string(),
            BackendError::InvalidResponse(_) => "响应格式无效".to_string(),
            BackendError::StreamingUnsupported(_) => "当前后端不支持流式输出".to_string(),
            BackendError::Internal(_) => "内部错误".to_string(),
            BackendError::QwenPool(error) => error.safe_message().to_string(),
            BackendError::Qwen(error) => error.safe_message().to_string(),
            _ => self.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_message_does_not_leak_network_detail() {
        let err = BackendError::Network("connection refused on https://example.com/path".into());
        let msg = err.safe_message();
        assert!(!msg.contains("example.com"));
        assert!(!msg.contains("connection refused"));
    }

    #[test]
    fn kind_maps_each_variant() {
        assert_eq!(
            BackendError::LoginRequired.kind(),
            BackendErrorKind::LoginRequired
        );
        assert_eq!(
            BackendError::ProtocolMismatch("x".into()).kind(),
            BackendErrorKind::ProtocolMismatch
        );
        assert_eq!(
            BackendError::PartialResponse("x".into()).kind(),
            BackendErrorKind::PartialResponse
        );
    }

    // ===== FR-010 上下文过长模式识别（T-012）=====

    #[test]
    fn context_length_patterns_are_recognized_without_case_sensitivity() {
        let recognized = [
            "This model's maximum context length is 8192 tokens",
            "the request is too long",
            "input length exceeds the limit",
            "Request exceeded the token limit",
            "请求内容超出上下文长度限制",
            "输入太长，请精简后重试",
            "超出上下文限制",
        ];
        for message in recognized {
            assert!(
                is_context_length_pattern(message),
                "应当识别: {message}"
            );
        }
    }

    #[test]
    fn unrelated_upstream_bodies_are_not_recognized() {
        let ignored = [
            "invalid api key",
            "rate limit exceeded, retry later",
            "model not found",
            "服务器繁忙",
            "参数错误: model 字段缺失",
        ];
        for message in ignored {
            assert!(
                !is_context_length_pattern(message),
                "不应误识别: {message}"
            );
        }
    }

    #[test]
    fn suggestion_carrying_messages_survive_safe_message() {
        let err = BackendError::Network(format!("网络请求失败。{TERMBASE_CONTEXT_SUGGESTION}"));
        let message = err.safe_message();
        assert!(message.contains(TERMBASE_CONTEXT_SUGGESTION));
        assert!(!message.contains("http"));
    }

    #[test]
    fn plain_messages_keep_fixed_safe_text() {
        let err = BackendError::Network(
            "connection refused on https://secret.example.com/path".to_string(),
        );
        assert_eq!(err.safe_message(), "网络请求失败");
        assert!(!err.safe_message().contains("secret.example.com"));
    }
}
