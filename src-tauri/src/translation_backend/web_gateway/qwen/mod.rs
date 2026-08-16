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

pub mod account;
pub mod adapter;
pub mod error;
pub mod executor;
pub mod migration;
pub mod pool;
pub mod registry;
pub mod scheduler;
pub mod session;
pub mod sse_decoder;

pub use account::{AccountId, AccountMoveDirection, QwenAccountPoolSnapshot};
pub use adapter::{
    LOGIN_WATCHER_INTERVAL, LOGIN_WATCHER_TIMEOUT, QWEN_LOGIN_URL, QWEN_LOGIN_WINDOW_LABEL,
    QWEN_TICKET_COOKIE_NAME,
};
pub use error::QwenError;
pub use migration::reconcile_legacy_migration;
pub use pool::QwenAccountPool;
pub use session::{QwenSession, QwenSessionStatus};

#[cfg(test)]
pub(crate) mod test_support {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    pub(crate) struct TestDir(PathBuf);

    impl TestDir {
        pub(crate) fn new(label: &str) -> Self {
            let number = COUNTER.fetch_add(1, Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!(
                "easyt-qwen-{label}-{}-{number}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock should be after epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }

        pub(crate) fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
