use serde::{Deserialize, Serialize};

/// 应用配置，与前端 types/index.ts 中的 AppConfig 对齐
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(alias = "base_url")]
    pub base_url: String,
    #[serde(default)]
    #[serde(alias = "api_key")]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    pub shortcut: String,
    #[serde(alias = "target_language")]
    pub target_language: String,
    #[serde(alias = "timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(alias = "auto_hide")]
    pub auto_hide: bool,
    #[serde(alias = "pinned_by_default")]
    pub pinned_by_default: bool,
    #[serde(alias = "max_text_length")]
    pub max_text_length: usize,
}

/// 默认配置（函数形式，因为 String 无法在 const 中构造）
/// 注意：不在此处写入任何真实密钥，api_key 默认为空。
pub fn default_config() -> AppConfig {
    AppConfig {
        base_url: "https://api.openai.com/v1".to_string(),
        api_key: String::new(),
        model: String::new(),
        shortcut: "Ctrl+T".to_string(),
        target_language: "简体中文".to_string(),
        timeout_seconds: 60,
        auto_hide: true,
        pinned_by_default: false,
        max_text_length: 5000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_accepts_frontend_camel_case() {
        let json = r#"{
            "baseUrl": "https://api.deepseek.com/v1",
            "apiKey": "sk-test",
            "model": "deepseek-chat",
            "shortcut": "Ctrl+T",
            "targetLanguage": "简体中文",
            "timeoutSeconds": 60,
            "autoHide": true,
            "pinnedByDefault": false,
            "maxTextLength": 5000
        }"#;

        let cfg: AppConfig = serde_json::from_str(json).expect("camelCase config should parse");

        assert_eq!(cfg.base_url, "https://api.deepseek.com/v1");
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.model, "deepseek-chat");
        assert_eq!(cfg.target_language, "简体中文");
    }

    #[test]
    fn app_config_still_accepts_existing_snake_case_files() {
        let json = r#"{
            "base_url": "https://api.openai.com/v1",
            "api_key": "sk-test",
            "model": "gpt-4o-mini",
            "shortcut": "Ctrl+T",
            "target_language": "简体中文",
            "timeout_seconds": 60,
            "auto_hide": true,
            "pinned_by_default": false,
            "max_text_length": 5000
        }"#;

        let cfg: AppConfig = serde_json::from_str(json).expect("snake_case config should parse");

        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.model, "gpt-4o-mini");
    }

    #[test]
    fn app_config_serializes_for_frontend() {
        let json = serde_json::to_string(&default_config()).expect("config should serialize");

        assert!(json.contains("baseUrl"));
        assert!(json.contains("apiKey"));
        assert!(json.contains("targetLanguage"));
        assert!(!json.contains("base_url"));
    }
}
