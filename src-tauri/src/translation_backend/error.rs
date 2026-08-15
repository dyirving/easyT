//! 翻译后端统一错误类型
//!
//! 设计原则：
//! - 内部错误可携带可诊断上下文，但绝不含 Cookie、ticket、Header 或用户正文
//! - ProtocolMismatch 表示私有协议结构已变化，不应归类为普通网络错误
//! - PartialResponse 不能作为成功返回
//! - Cancelled 不应把 QwenSession 标记为 Expired

use serde::Serialize;

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
            BackendError::Internal(_) => BackendErrorKind::Internal,
        }
    }

    /// 返回不携带敏感上下文的用户可读消息
    pub fn safe_message(&self) -> String {
        match self {
            BackendError::Network(_) => "网络请求失败".to_string(),
            BackendError::ProtocolMismatch(_) => "上游协议结构已变化".to_string(),
            BackendError::PartialResponse(_) => "上游响应不完整".to_string(),
            BackendError::InvalidResponse(_) => "响应格式无效".to_string(),
            BackendError::StreamingUnsupported(_) => "当前后端不支持流式输出".to_string(),
            BackendError::Internal(_) => "内部错误".to_string(),
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
}
