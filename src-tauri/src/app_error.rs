use serde::{Serialize, Serializer};

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

    #[error("请求失败: {0}")]
    ApiRequestFailed(String),

    #[error("响应格式无效: {0}")]
    ApiResponseInvalid(String),

    #[error("窗口操作失败: {0}")]
    WindowError(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

/// 序列化为前端可识别的结构 { kind, message }
/// 避免把 Rust panic / 调用栈 / API Key 暴露给前端
#[derive(Serialize)]
struct ErrorResponse {
    kind: &'static str,
    message: String,
}

impl AppError {
    fn kind_str(&self) -> &'static str {
        match self {
            AppError::NoSelectedText => "NoSelectedText",
            AppError::TextTooLong => "TextTooLong",
            AppError::ClipboardError(_) => "ClipboardError",
            AppError::ShortcutRegistrationFailed(_) => "ShortcutRegistrationFailed",
            AppError::ConfigInvalid(_) => "ConfigInvalid",
            AppError::ApiUnauthorized => "ApiUnauthorized",
            AppError::ApiRateLimited => "ApiRateLimited",
            AppError::ApiTimeout => "ApiTimeout",
            AppError::ApiRequestFailed(_) => "ApiRequestFailed",
            AppError::ApiResponseInvalid(_) => "ApiResponseInvalid",
            AppError::WindowError(_) => "WindowError",
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
        };
        resp.serialize(serializer)
    }
}

pub type AppResult<T> = Result<T, AppError>;
