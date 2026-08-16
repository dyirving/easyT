use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QwenErrorCode {
    PoolEmpty,
    PoolAllDisabled,
    PoolAllLoggedOut,
    PoolAllExpired,
    PoolUnavailable,
    PoolInvalidName,
    PoolLimit,
    PoolNotFound,
    PoolBusy,
    PoolInvalidOrder,
    PoolBusyTimeout,
    LoginOccupied,
    LoginWindow,
    LoginCookie,
    LoginTimeout,
    LoginSave,
    StorageCorruptedRecovered,
    StorageRecoveryFailed,
    StorageRead,
    StorageWrite,
    StorageCleanup,
    StorageMigration,
    Auth401,
    Auth403,
    Network,
    Timeout,
    Upstream429,
    Upstream5xx,
    ProtocolMismatch,
    InvalidResponse,
    PartialResponse,
    StreamingUnsupported,
    UpstreamOther,
}

impl QwenErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PoolEmpty => "QW-POOL-001",
            Self::PoolAllDisabled => "QW-POOL-003",
            Self::PoolAllLoggedOut => "QW-POOL-004",
            Self::PoolAllExpired => "QW-POOL-005",
            Self::PoolUnavailable => "QW-POOL-006",
            Self::PoolInvalidName => "QW-POOL-011",
            Self::PoolLimit => "QW-POOL-002",
            Self::PoolNotFound => "QW-POOL-008",
            Self::PoolBusy => "QW-POOL-009",
            Self::PoolInvalidOrder => "QW-POOL-012",
            Self::PoolBusyTimeout => "QW-POOL-007",
            Self::LoginOccupied => "QW-LOGIN-001",
            Self::LoginWindow => "QW-LOGIN-002",
            Self::LoginCookie => "QW-LOGIN-003",
            Self::LoginTimeout => "QW-LOGIN-004",
            Self::LoginSave => "QW-LOGIN-005",
            Self::StorageCorruptedRecovered => "QW-STORAGE-002",
            Self::StorageRecoveryFailed => "QW-STORAGE-003",
            Self::StorageRead => "QW-STORAGE-004",
            Self::StorageWrite => "QW-STORAGE-005",
            Self::StorageCleanup => "QW-STORAGE-007",
            Self::StorageMigration => "QW-STORAGE-008",
            Self::Auth401 => "QW-AUTH-401",
            Self::Auth403 => "QW-AUTH-403",
            Self::Network => "QW-NET-001",
            Self::Timeout => "QW-NET-408",
            Self::Upstream429 => "QW-UPSTREAM-429",
            Self::Upstream5xx => "QW-UPSTREAM-5XX",
            Self::ProtocolMismatch => "QW-UPSTREAM-001",
            Self::InvalidResponse => "QW-UPSTREAM-002",
            Self::PartialResponse => "QW-UPSTREAM-003",
            Self::StreamingUnsupported => "QW-UPSTREAM-004",
            Self::UpstreamOther => "QW-UPSTREAM-005",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QwenError {
    code: QwenErrorCode,
    safe_message: &'static str,
    recoverable_registry_corruption: bool,
}

impl QwenError {
    pub fn invalid_account_id() -> Self {
        Self::invalid_display_name()
    }

    pub fn invalid_display_name() -> Self {
        Self {
            code: QwenErrorCode::PoolInvalidName,
            safe_message: "Qwen 账号名称无效",
            recoverable_registry_corruption: false,
        }
    }

    pub fn pool_limit() -> Self {
        Self {
            code: QwenErrorCode::PoolLimit,
            safe_message: "Qwen 账号池已满",
            recoverable_registry_corruption: false,
        }
    }

    pub fn pool_empty() -> Self {
        Self::new(QwenErrorCode::PoolEmpty, "Qwen 账号池为空")
    }

    pub fn pool_all_disabled() -> Self {
        Self::new(QwenErrorCode::PoolAllDisabled, "所有 Qwen 账号已停用")
    }

    pub fn pool_all_logged_out() -> Self {
        Self::new(QwenErrorCode::PoolAllLoggedOut, "所有 Qwen 账号均未登录")
    }

    pub fn pool_all_expired() -> Self {
        Self::new(QwenErrorCode::PoolAllExpired, "所有 Qwen 账号登录均已过期")
    }

    pub fn account_not_found() -> Self {
        Self {
            code: QwenErrorCode::PoolNotFound,
            safe_message: "指定的 Qwen 账号不存在",
            recoverable_registry_corruption: false,
        }
    }

    pub fn pool_busy_timeout() -> Self {
        Self {
            code: QwenErrorCode::PoolBusyTimeout,
            safe_message: "所有健康 Qwen 账号正在使用中，请稍后重试",
            recoverable_registry_corruption: false,
        }
    }

    pub fn account_busy() -> Self {
        Self {
            code: QwenErrorCode::PoolBusy,
            safe_message: "指定的 Qwen 账号正在使用中",
            recoverable_registry_corruption: false,
        }
    }

    pub fn invalid_account_order() -> Self {
        Self {
            code: QwenErrorCode::PoolInvalidOrder,
            safe_message: "Qwen 账号顺序操作无效",
            recoverable_registry_corruption: false,
        }
    }

    pub fn no_healthy_account() -> Self {
        Self {
            code: QwenErrorCode::PoolUnavailable,
            safe_message: "没有可用的健康 Qwen 账号",
            recoverable_registry_corruption: false,
        }
    }

    pub fn mixed_unavailable() -> Self {
        Self::new(
            QwenErrorCode::PoolUnavailable,
            "当前 Qwen 账号池没有可用账号",
        )
    }

    pub fn auth_401() -> Self {
        Self::new(QwenErrorCode::Auth401, "Qwen 登录状态已过期")
    }

    pub fn auth_403() -> Self {
        Self::new(QwenErrorCode::Auth403, "Qwen 拒绝了当前登录状态")
    }

    pub fn upstream_rate_limited() -> Self {
        Self::new(QwenErrorCode::Upstream429, "Qwen 请求过于频繁")
    }

    pub fn upstream_server_error(status: u16) -> Self {
        let message = if status == 503 {
            "Qwen 服务暂时不可用（HTTP 503）"
        } else {
            "Qwen 服务暂时不可用"
        };
        Self::new(QwenErrorCode::Upstream5xx, message)
    }

    pub fn network() -> Self {
        Self::new(QwenErrorCode::Network, "Qwen 网络请求失败")
    }

    pub fn timeout() -> Self {
        Self::new(QwenErrorCode::Timeout, "Qwen 请求超时")
    }

    pub fn protocol_mismatch() -> Self {
        Self::new(QwenErrorCode::ProtocolMismatch, "Qwen 上游协议结构已变化")
    }

    pub fn invalid_response() -> Self {
        Self::new(QwenErrorCode::InvalidResponse, "Qwen 响应格式无效")
    }

    pub fn partial_response() -> Self {
        Self::new(QwenErrorCode::PartialResponse, "Qwen 上游响应不完整")
    }

    pub fn streaming_unsupported() -> Self {
        Self::new(
            QwenErrorCode::StreamingUnsupported,
            "当前 Qwen 响应不支持流式输出",
        )
    }

    pub fn upstream_other() -> Self {
        Self::new(QwenErrorCode::UpstreamOther, "Qwen 返回了无效状态")
    }

    pub fn is_authentication_error(&self) -> bool {
        matches!(self.code, QwenErrorCode::Auth401 | QwenErrorCode::Auth403)
    }

    pub fn is_formal_retryable(&self) -> bool {
        matches!(
            self.code,
            QwenErrorCode::Upstream429 | QwenErrorCode::Upstream5xx
        )
    }

    pub fn requires_probe(&self) -> bool {
        matches!(
            self.code,
            QwenErrorCode::Network
                | QwenErrorCode::Timeout
                | QwenErrorCode::Upstream429
                | QwenErrorCode::Upstream5xx
                | QwenErrorCode::ProtocolMismatch
                | QwenErrorCode::InvalidResponse
                | QwenErrorCode::PartialResponse
                | QwenErrorCode::StreamingUnsupported
                | QwenErrorCode::UpstreamOther
        )
    }

    pub fn from_backend_error(
        error: &crate::translation_backend::error::BackendError,
    ) -> Option<Self> {
        use crate::translation_backend::error::BackendError;

        match error {
            BackendError::Timeout => Some(Self::timeout()),
            BackendError::Network(_) => Some(Self::network()),
            BackendError::ProtocolMismatch(_) => Some(Self::protocol_mismatch()),
            BackendError::PartialResponse(_) => Some(Self::partial_response()),
            BackendError::InvalidResponse(_) => Some(Self::invalid_response()),
            BackendError::StreamingUnsupported(_) => Some(Self::streaming_unsupported()),
            BackendError::Unauthorized => Some(Self::auth_401()),
            BackendError::Qwen(error) => Some(error.clone()),
            _ => None,
        }
    }

    fn new(code: QwenErrorCode, safe_message: &'static str) -> Self {
        Self {
            code,
            safe_message,
            recoverable_registry_corruption: false,
        }
    }

    pub fn login_occupied() -> Self {
        Self {
            code: QwenErrorCode::LoginOccupied,
            safe_message: "另一个 Qwen 账号正在登录",
            recoverable_registry_corruption: false,
        }
    }

    pub fn login_window() -> Self {
        Self {
            code: QwenErrorCode::LoginWindow,
            safe_message: "无法打开 Qwen 登录窗口",
            recoverable_registry_corruption: false,
        }
    }

    pub fn login_cookie() -> Self {
        Self {
            code: QwenErrorCode::LoginCookie,
            safe_message: "无法读取 Qwen 登录状态",
            recoverable_registry_corruption: false,
        }
    }

    pub fn login_timeout() -> Self {
        Self {
            code: QwenErrorCode::LoginTimeout,
            safe_message: "Qwen 登录超时",
            recoverable_registry_corruption: false,
        }
    }

    pub fn login_save() -> Self {
        Self {
            code: QwenErrorCode::LoginSave,
            safe_message: "保存 Qwen 登录凭证失败",
            recoverable_registry_corruption: false,
        }
    }

    pub fn storage_corrupted_recovered() -> Self {
        Self {
            code: QwenErrorCode::StorageCorruptedRecovered,
            safe_message: "Qwen 账号注册表已恢复，请检查恢复账号后再启用",
            recoverable_registry_corruption: false,
        }
    }

    pub fn storage_recovery_failed() -> Self {
        Self {
            code: QwenErrorCode::StorageRecoveryFailed,
            safe_message: "Qwen 账号注册表恢复失败",
            recoverable_registry_corruption: false,
        }
    }

    pub(crate) fn storage_read(_diagnostic: impl std::fmt::Display) -> Self {
        Self {
            code: QwenErrorCode::StorageRead,
            safe_message: "无法读取 Qwen 账号存储",
            recoverable_registry_corruption: true,
        }
    }

    pub fn storage_write(_diagnostic: impl std::fmt::Display) -> Self {
        Self {
            code: QwenErrorCode::StorageWrite,
            safe_message: "无法写入 Qwen 账号存储",
            recoverable_registry_corruption: false,
        }
    }

    pub fn storage_migration(_diagnostic: impl std::fmt::Display) -> Self {
        Self {
            code: QwenErrorCode::StorageMigration,
            safe_message: "无法迁移旧版 Qwen 账号存储",
            recoverable_registry_corruption: false,
        }
    }

    pub fn storage_cleanup(_diagnostic: impl std::fmt::Display) -> Self {
        Self {
            code: QwenErrorCode::StorageCleanup,
            safe_message: "无法清理 Qwen 账号数据，请关闭相关窗口后重试",
            recoverable_registry_corruption: false,
        }
    }

    pub fn code(&self) -> QwenErrorCode {
        self.code
    }

    pub fn safe_message(&self) -> &'static str {
        self.safe_message
    }

    pub fn is_recoverable_registry_corruption(&self) -> bool {
        self.recoverable_registry_corruption
    }

    pub fn storage_incompatible() -> Self {
        Self {
            code: QwenErrorCode::StorageRead,
            safe_message: "无法读取 Qwen 账号存储",
            recoverable_registry_corruption: false,
        }
    }
}

impl std::fmt::Display for QwenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.safe_message)
    }
}

impl std::error::Error for QwenError {}

impl Serialize for QwenError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct PublicError<'a> {
            code: &'a str,
            message: &'a str,
        }

        PublicError {
            code: self.code.as_str(),
            message: self.safe_message,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_error_is_structured_and_redacted() {
        let error = QwenError::storage_read("https://secret.example/ticket=not-for-ipc");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("QW-STORAGE-004"));
        assert!(!json.contains("secret.example"));
        assert!(!json.contains("ticket"));
    }
}
