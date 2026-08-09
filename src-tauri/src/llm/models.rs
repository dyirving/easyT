//! 翻译结果类型（保留旧路径以最小化前端破坏）
//!
//! TranslationResult 已迁到 translation_backend::models::BackendResult，
//! 但 translate_text 命令继续返回简单 TranslationResult 结构以保持前端契约。

use serde::{Deserialize, Serialize};

/// 翻译结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    #[serde(alias = "translated_text")]
    pub translated_text: String,
    /// 是否来自本机缓存
    pub from_cache: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_result_serializes_for_frontend() {
        let result = TranslationResult {
            translated_text: "你好".to_string(),
            from_cache: false,
        };

        let json = serde_json::to_string(&result).expect("result should serialize");

        assert!(json.contains("translatedText"));
        assert!(json.contains("fromCache"));
        assert!(!json.contains("translated_text"));
    }
}
