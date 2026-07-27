//! LLM 兼容层
//!
//! `translate()` 已迁至 translation_backend::TranslationBackend，
//! 这里仅保留前端契约所需的 TranslationResult 类型（通过 `llm::models` 路径访问）。
//! prompt 移到 translation_backend::prompt 共用。

pub mod models;
