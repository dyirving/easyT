use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 模型供应商标识
/// - agnes / deepseek / qwen / glm / kimi / doubao：内置供应商，Base URL 与模型由前端常量维护
/// - custom：自定义供应商，用户自行填写 Base URL 与模型名称
///
/// 注意：Rust 端只持有该字段的字符串值，不解析内置供应商列表；
/// Base URL 与模型名称仍由 `base_url` / `model` 字段直接传递给 LLM 客户端。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelProvider {
    Agnes,
    Deepseek,
    Qwen,
    Glm,
    Kimi,
    Doubao,
    Custom,
}

impl Default for ModelProvider {
    /// 旧配置文件缺少 provider 字段时回退为 custom，
    /// 保留用户已填写的 base_url / model，避免行为变化。
    fn default() -> Self {
        ModelProvider::Custom
    }
}

/// 应用配置，与前端 types/index.ts 中的 AppConfig 对齐
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub provider: ModelProvider,
    /// 各供应商独立的 API Key 存储
    /// key 为供应商标识字符串（"agnes"/"deepseek"/...，与 ModelProvider 序列化一致）
    /// value 为该供应商的 API Key
    /// `api_key` 字段始终等于 `api_keys[provider]`，由前端维护一致性
    #[serde(default)]
    pub api_keys: HashMap<String, String>,
    #[serde(alias = "base_url")]
    pub base_url: String,
    #[serde(default)]
    #[serde(alias = "api_key")]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    /// 是否启用模型思考模式
    /// false（默认）：翻译场景注入关闭思考参数，省 token、降延迟
    /// true：保留各供应商默认思考行为，复杂语境下译文质量可能更好
    #[serde(default)]
    pub enable_thinking: bool,
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
/// 默认供应商为 Agnes（推荐），与前端 DEFAULT_CONFIG 保持一致。
pub fn default_config() -> AppConfig {
    AppConfig {
        provider: ModelProvider::Agnes,
        api_keys: HashMap::new(),
        base_url: "https://apihub.agnes-ai.com/v1".to_string(),
        api_key: String::new(),
        model: "agnes-2.0-flash".to_string(),
        enable_thinking: false,
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
            "provider": "deepseek",
            "apiKeys": { "deepseek": "sk-deepseek-key", "agnes": "sk-agnes-key" },
            "baseUrl": "https://api.deepseek.com/v1",
            "apiKey": "sk-deepseek-key",
            "model": "deepseek-chat",
            "enableThinking": true,
            "shortcut": "Ctrl+T",
            "targetLanguage": "简体中文",
            "timeoutSeconds": 60,
            "autoHide": true,
            "pinnedByDefault": false,
            "maxTextLength": 5000
        }"#;

        let cfg: AppConfig = serde_json::from_str(json).expect("camelCase config should parse");

        assert_eq!(cfg.provider, ModelProvider::Deepseek);
        assert_eq!(
            cfg.api_keys.get("deepseek").map(String::as_str),
            Some("sk-deepseek-key")
        );
        assert_eq!(
            cfg.api_keys.get("agnes").map(String::as_str),
            Some("sk-agnes-key")
        );
        assert_eq!(cfg.base_url, "https://api.deepseek.com/v1");
        assert_eq!(cfg.api_key, "sk-deepseek-key");
        assert_eq!(cfg.model, "deepseek-chat");
        assert!(cfg.enable_thinking);
        assert_eq!(cfg.target_language, "简体中文");
    }

    #[test]
    fn app_config_still_accepts_existing_snake_case_files() {
        // 旧配置文件没有 provider / apiKeys / enableThinking 字段：
        // provider 回退 custom，api_keys 回退空，enable_thinking 回退 false
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

        assert_eq!(cfg.provider, ModelProvider::Custom);
        assert!(cfg.api_keys.is_empty());
        assert!(!cfg.enable_thinking);
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.model, "gpt-4o-mini");
    }

    #[test]
    fn app_config_serializes_for_frontend() {
        let json = serde_json::to_string(&default_config()).expect("config should serialize");

        assert!(json.contains("\"provider\":\"agnes\""));
        assert!(json.contains("apiKeys"));
        assert!(json.contains("baseUrl"));
        assert!(json.contains("apiKey"));
        assert!(json.contains("enableThinking"));
        assert!(json.contains("targetLanguage"));
        assert!(!json.contains("base_url"));
    }

    #[test]
    fn default_config_uses_agnes_provider() {
        let cfg = default_config();
        assert_eq!(cfg.provider, ModelProvider::Agnes);
        assert!(cfg.api_keys.is_empty());
        assert!(!cfg.enable_thinking);
        assert_eq!(cfg.base_url, "https://apihub.agnes-ai.com/v1");
        assert_eq!(cfg.model, "agnes-2.0-flash");
    }
}
