//! Qwen Web Adapter
//!
//! Qwen 私有协议知识必须只存在于此模块内：
//! - 登录 URL
//! - Chat Base URL
//! - /api/v2/chat
//! - Origin/Referer
//! - 必要 Query 参数
//! - 模型映射
//! - 请求 JSON DTO
//! - 响应 JSON DTO 或受控 serde_json::Value 解析

pub mod adapter;
pub mod session;
pub mod sse_decoder;

pub use adapter::{
    QwenWebAdapter, LOGIN_WATCHER_INTERVAL, LOGIN_WATCHER_TIMEOUT, QWEN_LOGIN_URL,
    QWEN_LOGIN_WINDOW_LABEL, QWEN_TICKET_COOKIE_NAME,
};
pub use session::{QwenSession, QwenSessionStatus};
