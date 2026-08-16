//! 翻译后端深模块
//!
//! TranslationBackend 是翻译能力唯一的外部 seam：
//! - 根据 AppConfig.backend_mode 路由到 OfficialApiAdapter 或 WebGateway
//! - 在进入 Adapter 前执行共同输入校验
//! - 编排缓存策略 Use/Refresh/Bypass（缓存深模块不知道 Adapter/HTTP/Tauri）
//! - 返回统一 BackendResult 与来源状态 TranslationOutcome
//!
//! 它不负责：
//! - latest-wins generation（继续由 TranslationRequestManager 唯一负责）
//! - 创建登录窗口
//! - Cookie 提取或凭证持久化
//! - Qwen Header、请求体、SSE 字段
//! - 前端状态更新

pub mod cache;
pub mod error;
pub mod models;
pub mod official_api;
pub mod progress;
pub mod prompt;
pub mod web_gateway;

pub use cache::entry::{CachePolicy, CacheStatus, TranslationOutcome};
pub use error::BackendError;
pub use models::{BackendMode, BackendRequest, BackendResult, TranslationOptions};
pub use progress::{
    PhaseProgress, ProgressBackendSource, TranslationPhase, TranslationProgress,
    TranslationProgressReporter,
};

use std::future::Future;
use std::sync::Arc;

use crate::config::AppConfig;
use crate::termbase::{EffectiveTermbase, Termbase};

use self::cache::{
    is_definitely_oversized, prepare_cache_input, NormalizedCacheInput, TranslationCache,
};
use self::official_api::OfficialApiAdapter;
use self::prompt::build_system_prompt;
use self::web_gateway::WebGateway;

pub(crate) fn connection_success_message(prefix: &str, result: &BackendResult) -> String {
    format!(
        "{prefix}，返回译文长度 {} 字符",
        result.translated_text.chars().count()
    )
}

/// 翻译后端统一入口
pub struct TranslationBackend {
    official_api: OfficialApiAdapter,
    web_gateway: Arc<WebGateway>,
    cache: Arc<TranslationCache>,
    termbase: Arc<Termbase>,
}

impl TranslationBackend {
    pub fn new(
        http_client: reqwest::Client,
        cache: Arc<TranslationCache>,
        termbase: Arc<Termbase>,
        app_data: &std::path::Path,
    ) -> Result<Self, crate::translation_backend::web_gateway::qwen::QwenError> {
        let official_api = OfficialApiAdapter::new(http_client.clone());
        let web_gateway = Arc::new(WebGateway::open(http_client, app_data)?);
        Ok(Self {
            official_api,
            web_gateway,
            cache,
            termbase,
        })
    }

    /// 共享的 WebGateway 引用（供登录管理命令转发使用）
    pub fn web_gateway(&self) -> Arc<WebGateway> {
        Arc::clone(&self.web_gateway)
    }

    /// 翻译入口
    ///
    /// 在缓存策略查询前解析一次有效术语集（SDD §8.1），同一
    /// `EffectiveTermbase` 生成缓存指纹与共享 Prompt；Adapter 不参与匹配。
    /// 输入校验 → 策略（Use/Refresh/Bypass）→ L1 命中即返 / miss 走 Adapter。
    /// 成功后只有可缓存结果写入；缓存错误不得改变翻译结果语义。
    pub async fn translate(
        &self,
        config: &AppConfig,
        mut request: BackendRequest,
        options: TranslationOptions,
        progress: Arc<TranslationProgressReporter>,
    ) -> Result<TranslationOutcome, BackendError> {
        validate_translate_request(&request, config)?;
        let effective = self
            .termbase
            .resolve(&request.text, &request.target_language);
        request.prompt = build_system_prompt(&request.target_language, &effective);
        let input = prepare_cache_input(
            &request.text,
            &request.target_language,
            effective.fingerprint(),
        );
        let policy = resolve_cache_policy(config, options);
        if policy == CachePolicy::Use && !is_definitely_oversized(&input) {
            progress.phase(TranslationPhase::CheckingCache, None);
        }
        let backend_source = progress_backend_source(config);
        let progress_for_fetch = Arc::clone(&progress);
        let fetch = async move {
            progress_for_fetch.phase(TranslationPhase::PreparingRequest, Some(backend_source));
            match config.backend_mode {
                BackendMode::OfficialApi if config.stream_output => {
                    self.official_api
                        .translate_stream(config, request, progress_for_fetch)
                        .await
                }
                BackendMode::OfficialApi => {
                    self.official_api
                        .translate(config, request, progress_for_fetch)
                        .await
                }
                BackendMode::WebGateway if config.stream_output => {
                    self.web_gateway
                        .translate_stream(config, request, progress_for_fetch)
                        .await
                }
                BackendMode::WebGateway => {
                    self.web_gateway
                        .translate(config, request, progress_for_fetch)
                        .await
                }
            }
        };
        run_translation_with_cache(&self.cache, &input, policy, fetch)
            .await
            .map_err(|error| annotate_termbase_suggestion(&effective, error))
    }

    /// 测试连接：必须通过当前 Adapter 进行真实轻量请求
    /// WebGateway 模式不得仅检查本地 ticket 存在后返回成功
    pub async fn test_connection(&self, config: &AppConfig) -> Result<String, BackendError> {
        validate_test_connection(config)?;
        let progress = Arc::new(TranslationProgressReporter::discard());
        match config.backend_mode {
            BackendMode::OfficialApi if config.stream_output => {
                self.official_api
                    .test_connection_stream(config, progress)
                    .await
            }
            BackendMode::OfficialApi => self.official_api.test_connection(config, progress).await,
            BackendMode::WebGateway if config.stream_output => {
                self.web_gateway
                    .test_connection_stream(config, progress)
                    .await
            }
            BackendMode::WebGateway => self.web_gateway.test_connection(config, progress).await,
        }
    }
}

fn progress_backend_source(config: &AppConfig) -> ProgressBackendSource {
    let provider = if config.backend_mode == BackendMode::WebGateway {
        "qwen"
    } else {
        config.provider.stable_id()
    };
    ProgressBackendSource {
        mode: config.backend_mode,
        provider: provider.to_string(),
    }
}

/// FR-010：非空有效术语集的通用失败只追加非断言建议，错误分类不变。
///
/// - 空有效术语集、取消（Cancelled）与已识别的上下文过长错误不加建议。
/// - Qwen 稳定错误码（认证/限流/超时等）各有专属提示，追加术语表建议会造成噪音。
/// - 识别后的专用文案见 [`TERMBASE_CONTEXT_LENGTH_MESSAGE`]；建议见
///   [`TERMBASE_CONTEXT_SUGGESTION`]。
///
/// 该请求不会保留 Termbase 状态的可变引用：有效集是不可变的
/// `EffectiveTermbase` 快照，fetch 闭包只移动 `request` 与进度回调（§9.1）。
fn annotate_termbase_suggestion(effective: &EffectiveTermbase, error: BackendError) -> BackendError {
    use crate::translation_backend::error::{
        TERMBASE_CONTEXT_LENGTH_MESSAGE, TERMBASE_CONTEXT_SUGGESTION,
    };
    if effective.is_empty() {
        return error;
    }
    if let BackendError::InvalidResponse(message) = &error {
        if message == TERMBASE_CONTEXT_LENGTH_MESSAGE {
            return error;
        }
    }
    let append = |message: String| format!("{message}。{TERMBASE_CONTEXT_SUGGESTION}");
    let annotated = match error {
        BackendError::Cancelled => return error,
        BackendError::Network(message) => BackendError::Network(append(message)),
        BackendError::ProtocolMismatch(message) => BackendError::ProtocolMismatch(append(message)),
        BackendError::PartialResponse(message) => BackendError::PartialResponse(append(message)),
        BackendError::InvalidResponse(message) => BackendError::InvalidResponse(append(message)),
        BackendError::StreamingUnsupported(message) => {
            BackendError::StreamingUnsupported(append(message))
        }
        BackendError::ConfigInvalid(message) => BackendError::ConfigInvalid(append(message)),
        BackendError::Internal(message) => BackendError::Internal(append(message)),
        other => return other,
    };
    log::warn!("termbase_prompt_context_error: kind={:?}", annotated.kind());
    annotated
}

fn validate_translate_request(
    request: &BackendRequest,
    config: &AppConfig,
) -> Result<(), BackendError> {
    if request.text.trim().is_empty() {
        return Err(BackendError::ConfigInvalid("翻译文本不能为空".to_string()));
    }
    if request.text.chars().count() > config.max_text_length {
        return Err(BackendError::ConfigInvalid(format!(
            "文本长度超过最大限制 {}",
            config.max_text_length
        )));
    }
    if request.target_language.trim().is_empty() {
        return Err(BackendError::ConfigInvalid("目标语言不能为空".to_string()));
    }
    Ok(())
}

fn validate_test_connection(config: &AppConfig) -> Result<(), BackendError> {
    if config.timeout_seconds < 5 || config.timeout_seconds > 300 {
        return Err(BackendError::ConfigInvalid(
            "请求超时时间应在 5～300 秒之间".to_string(),
        ));
    }
    if config.max_text_length < 100 || config.max_text_length > 20000 {
        return Err(BackendError::ConfigInvalid(
            "最大翻译字符数应在 100～20000 之间".to_string(),
        ));
    }
    Ok(())
}

/// 缓存策略：唯一决策点（规则文档 §2）。
/// - WebGateway 且保存网页历史：Bypass（测试连接/诊断同样绕行）
/// - 用户显式重新翻译：Refresh
/// - 其余：Use
fn resolve_cache_policy(config: &AppConfig, options: TranslationOptions) -> CachePolicy {
    if config.backend_mode == BackendMode::WebGateway && config.web_gateway.save_history {
        CachePolicy::Bypass
    } else if options.force_refresh {
        CachePolicy::Refresh
    } else {
        CachePolicy::Use
    }
}

/// 缓存感知的翻译编排：Use 命中即返；Refresh 绕过读取成功后覆盖；
/// Bypass 不读不写。epoch 在请求开始时快照，迟到写入被 L1 拒绝。
async fn run_translation_with_cache<F>(
    cache: &TranslationCache,
    input: &NormalizedCacheInput,
    policy: CachePolicy,
    fetch: F,
) -> Result<TranslationOutcome, BackendError>
where
    F: Future<Output = Result<BackendResult, BackendError>>,
{
    let epoch = cache.current_epoch();
    match policy {
        CachePolicy::Use => {
            let input_is_oversized = is_definitely_oversized(input);
            if !input_is_oversized {
                let outcome = cache.lookup(input).await;
                if let Some(result) = outcome.result {
                    return Ok(TranslationOutcome {
                        result: (*result).clone(),
                        cache_status: outcome.status,
                    });
                }
            } else {
                cache.record_oversized_bypass(epoch);
            }
            let result = match fetch.await {
                Ok(result) => result,
                Err(error) => {
                    if !input_is_oversized {
                        cache.record_miss(epoch);
                    }
                    return Err(error);
                }
            };
            let cacheable = is_cacheable_result(&result);
            if !input_is_oversized && cacheable && cache.result_is_oversized(input, &result) {
                cache.record_oversized_bypass(epoch);
            } else if !input_is_oversized {
                cache.record_miss(epoch);
                if cacheable {
                    cache.store(input, &result, epoch);
                }
            }
            Ok(TranslationOutcome {
                result,
                cache_status: CacheStatus::Miss,
            })
        }
        CachePolicy::Refresh => {
            cache.record_refresh(epoch);
            let result = fetch.await?;
            if is_cacheable_result(&result) {
                cache.store(input, &result, epoch);
            }
            Ok(TranslationOutcome {
                result,
                cache_status: CacheStatus::Refreshed,
            })
        }
        CachePolicy::Bypass => {
            cache.record_bypass(epoch);
            let result = fetch.await?;
            Ok(TranslationOutcome {
                result,
                cache_status: CacheStatus::Bypassed,
            })
        }
    }
}

/// 只有完整成功且非空的译文才可缓存；取消/部分/失败根本走不到 store（future 被 abort）。
fn is_cacheable_result(result: &BackendResult) -> bool {
    !result.translated_text.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::termbase::test_support::non_empty_effective;
    use crate::translation_backend::cache::{
        prepare_cache_input, test_support::TestDir, TranslationCache,
    };
    use crate::translation_backend::error::{
        TERMBASE_CONTEXT_LENGTH_MESSAGE, TERMBASE_CONTEXT_SUGGESTION,
    };

    fn sample_result(text: &str) -> BackendResult {
        BackendResult {
            translated_text: text.to_string(),
            source: models::BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "agnes".to_string(),
                model: "agnes-2.0-flash".to_string(),
            },
        }
    }

    /// 计数 fetch：每次轮询恰好产出一次结果，模拟 Adapter 的一次真实调用。
    fn counting_fetch(
        calls: Arc<AtomicUsize>,
        result: BackendResult,
    ) -> impl Future<Output = Result<BackendResult, BackendError>> {
        let calls = Arc::clone(&calls);
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(result)
        }
    }

    fn run_use(
        cache: &TranslationCache,
        input: &NormalizedCacheInput,
        text: &str,
        calls: Arc<AtomicUsize>,
    ) -> TranslationOutcome {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime should build");
        runtime
            .block_on(run_translation_with_cache(
                cache,
                input,
                CachePolicy::Use,
                counting_fetch(calls, sample_result(text)),
            ))
            .expect("use should succeed")
    }

    fn run_refresh(
        cache: &TranslationCache,
        input: &NormalizedCacheInput,
        text: &str,
        calls: Arc<AtomicUsize>,
    ) -> TranslationOutcome {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime should build");
        runtime
            .block_on(run_translation_with_cache(
                cache,
                input,
                CachePolicy::Refresh,
                counting_fetch(calls, sample_result(text)),
            ))
            .expect("refresh should succeed")
    }

    fn run_bypass(
        cache: &TranslationCache,
        input: &NormalizedCacheInput,
        text: &str,
        calls: Arc<AtomicUsize>,
    ) -> TranslationOutcome {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime should build");
        runtime
            .block_on(run_translation_with_cache(
                cache,
                input,
                CachePolicy::Bypass,
                counting_fetch(calls, sample_result(text)),
            ))
            .expect("bypass should succeed")
    }

    #[test]
    fn validate_rejects_empty_text() {
        let mut config = crate::config::default_config();
        config.max_text_length = 100;
        let request = BackendRequest {
            text: "   ".to_string(),
            target_language: "简体中文".to_string(),
            prompt: String::new(),
        };
        let err = validate_translate_request(&request, &config).expect_err("should reject");
        assert!(matches!(err, BackendError::ConfigInvalid(_)));
    }

    #[test]
    fn validate_translate_rejects_oversize_text() {
        let mut config = crate::config::default_config();
        config.max_text_length = 3;
        let request = BackendRequest {
            text: "abcdef".to_string(),
            target_language: "简体中文".to_string(),
            prompt: String::new(),
        };
        let err = validate_translate_request(&request, &config).expect_err("should reject");
        assert!(matches!(err, BackendError::ConfigInvalid(_)));
    }

    #[test]
    fn plain_request_is_use_policy() {
        let config = crate::config::default_config();
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: false
                }
            ),
            CachePolicy::Use
        );
    }

    #[test]
    fn explicit_refresh_is_refresh_policy() {
        let config = crate::config::default_config();
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: true
                }
            ),
            CachePolicy::Refresh
        );
    }

    #[test]
    fn web_gateway_save_history_bypasses_even_when_refreshing() {
        let mut config = crate::config::default_config();
        config.backend_mode = BackendMode::WebGateway;
        config.web_gateway.save_history = true;
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: true
                }
            ),
            CachePolicy::Bypass
        );
    }

    // ===== FR-010 非空术语集通用失败建议（T-012）=====

    #[test]
    fn empty_effective_set_never_appends_suggestion() {
        let error = BackendError::Network("网络请求失败".to_string());
        let annotated =
            annotate_termbase_suggestion(&EffectiveTermbase::empty(), error);
        assert_eq!(annotated.safe_message(), "网络请求失败");
        assert!(!annotated
            .safe_message()
            .contains(TERMBASE_CONTEXT_SUGGESTION));
    }

    #[test]
    fn generic_failure_appends_suggestion_keeping_kind() {
        let error = BackendError::Network("网络请求失败".to_string());
        let annotated = annotate_termbase_suggestion(&non_empty_effective(), error);
        assert!(matches!(annotated, BackendError::Network(_)));
        assert!(annotated.safe_message().contains(TERMBASE_CONTEXT_SUGGESTION));

        let error = BackendError::InvalidResponse("响应格式无效".to_string());
        let annotated = annotate_termbase_suggestion(&non_empty_effective(), error);
        assert!(matches!(annotated, BackendError::InvalidResponse(_)));
        assert!(annotated.safe_message().contains(TERMBASE_CONTEXT_SUGGESTION));
    }

    #[test]
    fn cancelled_and_recognized_context_length_are_not_annotated() {
        let error = annotate_termbase_suggestion(&non_empty_effective(), BackendError::Cancelled);
        assert!(matches!(error, BackendError::Cancelled));

        let error = annotate_termbase_suggestion(
            &non_empty_effective(),
            BackendError::InvalidResponse(TERMBASE_CONTEXT_LENGTH_MESSAGE.to_string()),
        );
        assert!(matches!(error, BackendError::InvalidResponse(_)));
        assert_eq!(error.safe_message(), TERMBASE_CONTEXT_LENGTH_MESSAGE);
    }

    #[test]
    fn stable_qwen_codes_keep_their_own_hints() {
        let error = annotate_termbase_suggestion(
            &non_empty_effective(),
            BackendError::Qwen(
                crate::translation_backend::web_gateway::qwen::QwenError::upstream_rate_limited(),
            ),
        );
        let BackendError::Qwen(qwen) = error else {
            panic!("expected Qwen error");
        };
        assert_eq!(qwen.safe_message(), "Qwen 请求过于频繁");
        assert!(!qwen.safe_message().contains(TERMBASE_CONTEXT_SUGGESTION));
    }

    #[test]
    fn web_gateway_without_save_history_uses_policy() {
        let mut config = crate::config::default_config();
        config.backend_mode = BackendMode::WebGateway;
        config.web_gateway.save_history = false;
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: false
                }
            ),
            CachePolicy::Use
        );
        assert_eq!(
            resolve_cache_policy(
                &config,
                TranslationOptions {
                    force_refresh: true
                }
            ),
            CachePolicy::Refresh
        );
    }

    #[test]
    fn use_policy_miss_fetches_stores_then_hits_without_network() {
        let cache = TranslationCache::memory_only_for_tests();
        let input = prepare_cache_input("hello", "简体中文", &[0u8; 32]);
        let calls = Arc::new(AtomicUsize::new(0));

        let first = run_use(&cache, &input, "你好", Arc::clone(&calls));
        assert_eq!(first.cache_status, CacheStatus::Miss);
        assert_eq!(first.result.translated_text, "你好");

        let second = run_use(&cache, &input, "不应再走网络", Arc::clone(&calls));
        assert_eq!(second.cache_status, CacheStatus::MemoryHit);
        assert_eq!(second.result.translated_text, "你好");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "第二次不发起网络请求");
    }

    #[test]
    fn use_hit_returns_same_content_regardless_of_fetch() {
        let cache = TranslationCache::memory_only_for_tests();
        let input = parse_input("hello", "en");
        let calls = Arc::new(AtomicUsize::new(0));
        run_use(&cache, &input, "A", Arc::clone(&calls));
        // 即使当前 fetch 会返回别的译文，命中路径也不调用它
        let hit = run_use(&cache, &input, "B", Arc::clone(&calls));
        assert_eq!(hit.cache_status, CacheStatus::MemoryHit);
        assert_eq!(hit.result.translated_text, "A");
    }

    #[test]
    fn definitely_oversized_use_skips_cache_lookup() {
        let cache = TranslationCache::memory_only_for_tests();
        let small = parse_input("hello", "en");
        let calls = Arc::new(AtomicUsize::new(0));
        run_use(&cache, &small, "cached", Arc::clone(&calls));

        let oversized_same_key = NormalizedCacheInput {
            key: small.key,
            normalized_source_bytes: crate::translation_backend::cache::key::MAX_ENTRY_LOGICAL_BYTES
                as usize,
            target_language: small.target_language.clone(),
            is_short_text: false,
        };
        let outcome = run_use(&cache, &oversized_same_key, "network", Arc::clone(&calls));

        assert_eq!(outcome.cache_status, CacheStatus::Miss);
        assert_eq!(outcome.result.translated_text, "network");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "oversized Use must not read L1"
        );
    }

    #[tokio::test]
    async fn oversized_network_result_does_not_enter_public_hit_rate() {
        let dir = TestDir::new("oversized-stats");
        let cache = TranslationCache::start(&dir.0);
        cache.wait_until_persistent_ready().await;
        let input = prepare_cache_input("hello", "简体中文", &[0u8; 32]);
        let oversized =
            "译".repeat(crate::translation_backend::cache::key::MAX_ENTRY_LOGICAL_BYTES as usize);

        let outcome = run_translation_with_cache(&cache, &input, CachePolicy::Use, async {
            Ok(sample_result(&oversized))
        })
        .await
        .expect("oversized translation still succeeds");

        assert_eq!(outcome.cache_status, CacheStatus::Miss);
        let stats = cache.stats().await.expect("stats should be readable");
        assert_eq!(stats.entry_count, 0);
        assert_eq!(
            stats.hit_rate, None,
            "oversized results must not add a public miss"
        );
        cache.shutdown().await;
    }

    #[test]
    fn refresh_skips_read_and_overwrites_shared_entry() {
        let cache = TranslationCache::memory_only_for_tests();
        let input = parse_input("hello", "en");
        let calls = Arc::new(AtomicUsize::new(0));

        run_use(&cache, &input, "old", Arc::clone(&calls));
        let refreshed = run_refresh(&cache, &input, "new", Arc::clone(&calls));
        assert_eq!(refreshed.cache_status, CacheStatus::Refreshed);
        assert_eq!(refreshed.result.translated_text, "new");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "Refresh 强制走网络");

        let hit = run_use(&cache, &input, "ignored", Arc::clone(&calls));
        assert_eq!(hit.cache_status, CacheStatus::MemoryHit);
        assert_eq!(hit.result.translated_text, "new");
        assert_eq!(calls.load(Ordering::SeqCst), 2, "覆盖后的新值命中");
    }

    #[test]
    fn bypass_never_reads_or_writes() {
        let cache = TranslationCache::memory_only_for_tests();
        let input = parse_input("hello", "en");
        let calls = Arc::new(AtomicUsize::new(0));

        run_use(&cache, &input, "old", Arc::clone(&calls));
        let bypassed = run_bypass(&cache, &input, "proxy", Arc::clone(&calls));
        assert_eq!(bypassed.cache_status, CacheStatus::Bypassed);
        assert_eq!(bypassed.result.translated_text, "proxy");

        let hit = run_use(&cache, &input, "ignored", Arc::clone(&calls));
        assert_eq!(hit.cache_status, CacheStatus::MemoryHit);
        assert_eq!(hit.result.translated_text, "old", "Bypass 不读写缓存");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "Use 与 Bypass 各一次网络请求"
        );
    }

    #[test]
    fn empty_result_never_stored() {
        let cache = TranslationCache::memory_only_for_tests();
        let input = parse_input("abc", "en");
        let calls = Arc::new(AtomicUsize::new(0));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime should build");
        let calls_for_fetch = Arc::clone(&calls);
        let _ = runtime.block_on(run_translation_with_cache(
            &cache,
            &input,
            CachePolicy::Use,
            async move {
                calls_for_fetch.fetch_add(1, Ordering::SeqCst);
                Ok(sample_result(""))
            },
        ));

        let again = run_use(&cache, &input, "filled", Arc::clone(&calls));
        assert_eq!(again.cache_status, CacheStatus::Miss, "空译文不缓存");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn fetch_error_propagates_and_nothing_is_stored() {
        let cache = TranslationCache::memory_only_for_tests();
        let input = parse_input("abc", "en");
        let calls = Arc::new(AtomicUsize::new(0));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime should build");
        let calls_for_fetch = Arc::clone(&calls);
        let err_outcome = runtime.block_on(run_translation_with_cache(
            &cache,
            &input,
            CachePolicy::Use,
            async move {
                calls_for_fetch.fetch_add(1, Ordering::SeqCst);
                Err(BackendError::Network("模拟网络错误".to_string()))
            },
        ));
        assert!(matches!(err_outcome, Err(BackendError::Network(_))));

        let second = run_use(&cache, &input, "ok", Arc::clone(&calls));
        assert_eq!(second.cache_status, CacheStatus::Miss, "失败结果不缓存");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn different_target_language_misses() {
        let cache = TranslationCache::memory_only_for_tests();
        let zh = parse_input("hello", "zh");
        let en = parse_input("hello", "en");
        let calls = Arc::new(AtomicUsize::new(0));
        run_use(&cache, &zh, "你好", Arc::clone(&calls));
        let second = run_use(&cache, &en, "Hello", Arc::clone(&calls));
        assert_eq!(second.cache_status, CacheStatus::Miss);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    fn parse_input(text: &str, target: &str) -> NormalizedCacheInput {
        prepare_cache_input(text, target, &[0u8; 32])
    }

    #[test]
    fn output_options_participate_in_key_via_cache_key_version() {
        // 目标语言参与键的对外承诺：不同目标语言共享相同原文为不同条目
        assert_ne!(
            parse_input("hello", "zh").key,
            parse_input("hello", "en").key
        );
    }
}
