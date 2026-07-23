use serde::{Deserialize, Serialize};

/// 翻译请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationRequest {
    pub text: String,
    #[serde(alias = "target_language")]
    pub target_language: String,
}

/// 翻译结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    #[serde(alias = "translated_text")]
    pub translated_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_result_serializes_for_frontend() {
        let result = TranslationResult {
            translated_text: "你好".to_string(),
        };

        let json = serde_json::to_string(&result).expect("result should serialize");

        assert!(json.contains("translatedText"));
        assert!(!json.contains("translated_text"));
    }
}
