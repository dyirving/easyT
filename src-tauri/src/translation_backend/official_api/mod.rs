//! Official API Adapter：封装 OpenAI 兼容协议的 Chat Completions 调用
//!
//! 行为等价于原 `llm::client::translate`，但返回 BackendResult，
//! 错误统一为 BackendError。

pub mod adapter;

pub use adapter::OfficialApiAdapter;
