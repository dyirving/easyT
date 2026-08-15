use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::translation_backend::models::{BackendMode, WebProviderKind};

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

impl ModelProvider {
    pub fn stable_id(&self) -> &'static str {
        match self {
            Self::Agnes => "agnes",
            Self::Deepseek => "deepseek",
            Self::Qwen => "qwen",
            Self::Glm => "glm",
            Self::Kimi => "kimi",
            Self::Doubao => "doubao",
            Self::Custom => "custom",
        }
    }
}

/// WebGateway 配置（实验功能）
/// - provider：第一版仅 Qwen
/// - model：必须来自内部允许列表，不接受任意字符串
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebGatewayConfig {
    #[serde(default = "default_web_provider")]
    pub provider: WebProviderKind,
    #[serde(default = "default_qwen_model")]
    pub model: String,
    /// 是否让 Qwen 将翻译请求保存到网页端会话历史。
    #[serde(default)]
    pub save_history: bool,
}

impl Default for WebGatewayConfig {
    fn default() -> Self {
        Self {
            provider: default_web_provider(),
            model: default_qwen_model(),
            save_history: false,
        }
    }
}

fn default_web_provider() -> WebProviderKind {
    WebProviderKind::Qwen
}

/// 默认 Qwen 模型：与千问官网当前默认选项保持一致
fn default_qwen_model() -> String {
    "Qwen3.7-Max".to_string()
}

/// Qwen WebGateway 允许的模型白名单
/// 第一版不接受任意字符串，必须从此列表中选取
pub const QWEN_ALLOWED_MODELS: &[&str] = &[
    "Qwen",
    "Qwen3.8-Max-Preview",
    "Qwen3.7-Max",
    "Qwen3.6-Flash",
];

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
    /// 是否在译文生成期间逐步展示正文；缺失时关闭以保持旧行为
    #[serde(default)]
    pub stream_output: bool,
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
    /// 持久化翻译历史的最大记录数（包含最新译文）。
    #[serde(default = "default_translation_history_limit")]
    pub translation_history_limit: u8,
    /// 翻译后端选择
    /// 旧配置文件缺失时默认 OfficialApi，保持行为不变
    #[serde(default)]
    pub backend_mode: BackendMode,
    /// WebGateway 实验功能配置
    /// 旧配置文件缺失时使用默认 Qwen + Qwen3.7-Max
    #[serde(default)]
    pub web_gateway: WebGatewayConfig,
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
        stream_output: false,
        shortcut: "Ctrl+T".to_string(),
        target_language: "简体中文".to_string(),
        timeout_seconds: 60,
        auto_hide: true,
        pinned_by_default: false,
        max_text_length: 5000,
        translation_history_limit: default_translation_history_limit(),
        backend_mode: BackendMode::OfficialApi,
        web_gateway: WebGatewayConfig::default(),
    }
}

pub const fn default_translation_history_limit() -> u8 {
    5
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
            "streamOutput": true,
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
        assert!(cfg.stream_output);
        assert_eq!(cfg.target_language, "简体中文");
        // 旧配置文件缺失 backendMode/webGateway 时回退默认值
        assert_eq!(cfg.backend_mode, BackendMode::OfficialApi);
        assert_eq!(cfg.web_gateway.provider, WebProviderKind::Qwen);
        assert_eq!(cfg.web_gateway.model, "Qwen3.7-Max");
        assert!(!cfg.web_gateway.save_history);
        assert_eq!(cfg.translation_history_limit, 5);
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
        assert!(!cfg.stream_output);
        assert_eq!(cfg.base_url, "https://api.openai.com/v1");
        assert_eq!(cfg.api_key, "sk-test");
        assert_eq!(cfg.model, "gpt-4o-mini");
        // 旧配置文件无 backendMode 时默认 OfficialApi
        assert_eq!(cfg.backend_mode, BackendMode::OfficialApi);
    }

    #[test]
    fn app_config_serializes_for_frontend() {
        let json = serde_json::to_string(&default_config()).expect("config should serialize");

        assert!(json.contains("\"provider\":\"agnes\""));
        assert!(json.contains("apiKeys"));
        assert!(json.contains("baseUrl"));
        assert!(json.contains("apiKey"));
        assert!(json.contains("enableThinking"));
        assert!(json.contains("streamOutput"));
        assert!(json.contains("targetLanguage"));
        assert!(json.contains("backendMode"));
        assert!(json.contains("webGateway"));
        assert!(!json.contains("base_url"));
    }

    #[test]
    fn default_config_uses_agnes_provider() {
        let cfg = default_config();
        assert_eq!(cfg.provider, ModelProvider::Agnes);
        assert!(cfg.api_keys.is_empty());
        assert!(!cfg.enable_thinking);
        assert!(!cfg.stream_output);
        assert_eq!(cfg.base_url, "https://apihub.agnes-ai.com/v1");
        assert_eq!(cfg.model, "agnes-2.0-flash");
        assert_eq!(cfg.backend_mode, BackendMode::OfficialApi);
    }

    #[test]
    fn history_limit_defaults_and_serializes_in_frontend_shape() {
        let config = default_config();
        assert_eq!(config.translation_history_limit, 5);
        let value = serde_json::to_value(config).expect("serialize config");
        assert_eq!(value["translationHistoryLimit"], 5);
        assert!(value.get("translation_history_limit").is_none());
    }

    #[test]
    fn backend_mode_can_be_web_gateway() {
        let json = r#"{
            "provider": "agnes",
            "apiKeys": {},
            "baseUrl": "https://apihub.agnes-ai.com/v1",
            "apiKey": "",
            "model": "agnes-2.0-flash",
            "enableThinking": false,
            "shortcut": "Ctrl+T",
            "targetLanguage": "简体中文",
            "timeoutSeconds": 60,
            "autoHide": true,
            "pinnedByDefault": false,
            "maxTextLength": 5000,
            "backendMode": "webGateway",
            "webGateway": { "provider": "qwen", "model": "Qwen3.6-Flash" }
        }"#;
        let cfg: AppConfig = serde_json::from_str(json).expect("parse");
        assert_eq!(cfg.backend_mode, BackendMode::WebGateway);
        assert_eq!(cfg.web_gateway.model, "Qwen3.6-Flash");
        assert!(!cfg.web_gateway.save_history);
        assert!(!cfg.stream_output);
    }

    #[test]
    fn legacy_config_without_stream_output_defaults_to_disabled() {
        let json = r#"{
            "baseUrl": "https://api.openai.com/v1",
            "apiKey": "sk-test",
            "model": "gpt-4o-mini",
            "shortcut": "Ctrl+T",
            "targetLanguage": "简体中文",
            "timeoutSeconds": 60,
            "autoHide": true,
            "pinnedByDefault": false,
            "maxTextLength": 5000
        }"#;
        let cfg: AppConfig = serde_json::from_str(json).expect("legacy config should parse");

        assert!(!cfg.stream_output);
        assert_eq!(cfg.model, "gpt-4o-mini");
    }

    #[test]
    fn qwen_model_allowlist_matches_current_web_models() {
        assert_eq!(
            QWEN_ALLOWED_MODELS,
            &[
                "Qwen",
                "Qwen3.8-Max-Preview",
                "Qwen3.7-Max",
                "Qwen3.6-Flash",
            ]
        );
    }
}
