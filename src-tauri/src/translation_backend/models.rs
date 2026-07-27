//! 翻译后端统一类型：BackendMode / BackendRequest / BackendResult / BackendSource
//!
//! 这些是 TranslationBackend 与外部调用方（commands、coordinator）之间的契约。
//! Adapter 内部使用自己的请求/响应 DTO，不在这里暴露。

use serde::{Deserialize, Serialize};

/// 翻译后端选择
/// - OfficialApi：使用 OpenAI 兼容协议调用付费 API
/// - WebGateway：实验功能，使用网页登录态调用 Qwen 私有接口
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackendMode {
    OfficialApi,
    WebGateway,
}

impl Default for BackendMode {
    /// 旧配置文件缺少 backendMode 时默认 OfficialApi，保持行为不变
    fn default() -> Self {
        BackendMode::OfficialApi
    }
}

/// Web 网关支持的 Provider 种类
/// 第一版仅 Qwen；不要为假设中的未来供应商预先引入动态注册表
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebProviderKind {
    Qwen,
}

/// 翻译后端统一请求
/// 由 commands 层从 AppConfig + 用户输入构造，不暴露 API Key 或凭证
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendRequest {
    pub text: String,
    pub target_language: String,
}

/// 翻译后端统一结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendResult {
    pub translated_text: String,
    pub source: BackendSource,
}

/// 结果来源元数据，只读，前端不应据此分叉核心成功流程
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSource {
    pub backend: BackendMode,
    pub provider: String,
    pub model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_mode_default_is_official_api() {
        assert_eq!(BackendMode::default(), BackendMode::OfficialApi);
    }

    #[test]
    fn backend_mode_serializes_camel_case() {
        let json = serde_json::to_string(&BackendMode::WebGateway).expect("serialize");
        assert_eq!(json, "\"webGateway\"");
    }

    #[test]
    fn backend_result_serializes_camel_case() {
        let result = BackendResult {
            translated_text: "你好".to_string(),
            source: BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "agnes".to_string(),
                model: "agnes-2.0-flash".to_string(),
            },
        };
        let json = serde_json::to_string(&result).expect("serialize");
        assert!(json.contains("translatedText"));
        assert!(!json.contains("translated_text"));
    }

    #[test]
    fn backend_mode_deserializes_legacy_string() {
        let json = "\"officialApi\"";
        let mode: BackendMode = serde_json::from_str(json).expect("deserialize");
        assert_eq!(mode, BackendMode::OfficialApi);
    }
}
