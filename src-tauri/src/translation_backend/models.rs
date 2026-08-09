//! 翻译后端统一类型：BackendMode / BackendRequest / BackendResult / BackendSource
//!
//! 这些是 TranslationBackend 与外部调用方（commands、coordinator）之间的契约。
//! Adapter 内部使用自己的请求/响应 DTO，不在这里暴露。

use serde::{Deserialize, Serialize};

use super::error::BackendError;

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

/// 可见译文的后端增量。reasoning 和协议心跳不应进入此契约。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendProgress {
    ContentDelta(String),
}

/// 翻译后端向上层报告可见正文的最小契约。
pub trait TranslationProgress: Send + Sync {
    fn emit(&self, progress: BackendProgress) -> Result<(), BackendError>;
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

/// 翻译请求选项：把界面意图完整传递到 TranslationBackend。
/// force_refresh=false 为普通翻译（Use 策略）；true 为"重新翻译"（Refresh 策略，
/// 绕过缓存读取并在成功后覆盖共享缓存）。Bypass 策略由后端根据配置自行决定。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TranslationOptions {
    pub force_refresh: bool,
}

/// 结果来源状态：结果如何产生。
/// 本阶段尚未接入实际缓存，后端只产出 Miss / Refreshed / Bypassed；
/// MemoryHit / PersistentHit 由后续缓存垂直切片构造。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CacheStatus {
    Miss,
    MemoryHit,
    PersistentHit,
    Refreshed,
    Bypassed,
}

/// 翻译后端统一结果：完整译文 + 来源状态同行返回。
/// Adapter 合同（BackendResult）不变，来源状态只在 seam 处附加。
#[derive(Debug, Clone)]
pub struct TranslationOutcome {
    pub result: BackendResult,
    pub cache_status: CacheStatus,
}

impl TranslationOutcome {
    /// 前端 fromCache 布尔值的唯一来源；未接入缓存时始终为 false。
    pub fn is_from_cache(&self) -> bool {
        matches!(
            self.cache_status,
            CacheStatus::MemoryHit | CacheStatus::PersistentHit
        )
    }
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

    #[test]
    fn translation_options_default_is_plain_use() {
        assert_eq!(
            TranslationOptions::default(),
            TranslationOptions {
                force_refresh: false
            }
        );
    }

    #[test]
    fn outcome_from_cache_marks_only_cache_hits() {
        let result = || BackendResult {
            translated_text: "你好".to_string(),
            source: BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "agnes".to_string(),
                model: "agnes-2.0-flash".to_string(),
            },
        };

        let miss = TranslationOutcome {
            result: result(),
            cache_status: CacheStatus::Miss,
        };
        let refreshed = TranslationOutcome {
            result: result(),
            cache_status: CacheStatus::Refreshed,
        };
        let bypassed = TranslationOutcome {
            result: result(),
            cache_status: CacheStatus::Bypassed,
        };
        let memory_hit = TranslationOutcome {
            result: result(),
            cache_status: CacheStatus::MemoryHit,
        };
        let persistent_hit = TranslationOutcome {
            result: result(),
            cache_status: CacheStatus::PersistentHit,
        };

        assert!(!miss.is_from_cache());
        assert!(!refreshed.is_from_cache());
        assert!(!bypassed.is_from_cache());
        assert!(memory_hit.is_from_cache());
        assert!(persistent_hit.is_from_cache());
    }
}
