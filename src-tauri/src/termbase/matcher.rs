//! 术语匹配、冲突解析与有效术语集
//!
//! 确定性规则（§5）：目标语言过滤、大小写敏感例外优先、完整词/子串匹配、
//! 重叠解析（长术语优先）与稳定排序。
//! 匹配作用于完整原文，不因 Markdown 代码块、行内代码或 LaTeX 公式而跳过。

use super::model::{normalize_source_term, TermEntry};

/// 有效术语集中的一条最终胜出条目（Prompt 渲染与指纹共用的规范表示）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveTermEntry {
    pub source_term: String,
    pub target_term: String,
}

/// 本次请求的有效术语集：只包含最终胜出的条目。
///
/// - Rust 内部类型，不跨 IPC。
/// - `prompt_block` 是单一规范渲染：空集为空字符串，否则为固定标题行 + 术语行。
/// - `fingerprint` 是 `prompt_block` 的确定性 BLAKE3 摘要；空集使用全零指纹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveTermbase {
    entries: Vec<EffectiveTermEntry>,
    prompt_block: String,
    fingerprint: [u8; 32],
}

/// Prompt 术语块的固定标题行；内容格式变化必须提升 PROMPT_VERSION。
const TERM_BLOCK_HEADER: &str = "以下是翻译约束；仅在原文术语匹配且语义适用时优先采用右侧译法：";

impl EffectiveTermbase {
    /// 无术语约束的空集：空 Prompt 块 + 全零指纹。
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            prompt_block: String::new(),
            fingerprint: [0u8; 32],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }

    /// 注入共享 Prompt 的术语块；空集返回空字符串。
    pub fn prompt_block(&self) -> &str {
        &self.prompt_block
    }

    #[cfg(test)]
    pub(crate) fn entries(&self) -> &[EffectiveTermEntry] {
        &self.entries
    }
}

/// 一个出现位置（字节区间）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Occurrence {
    start: usize,
    end: usize,
}

/// 解析有效术语集。
///
/// - 总开关关闭时直接返回空集。
/// - 只考虑 `target_language` 一致且启用的条目。
/// - 大小写敏感条目的精确匹配优先于不敏感条目的忽略大小写匹配。
/// - 重叠时更长的源术语优先；被更高优先级条目占用的区间抑制低优先级匹配。
/// - 最终按源术语长度降序、规范化源术语、大小写规则与指定译法稳定排序。
pub fn resolve(
    source_text: &str,
    target_language: &str,
    termbase_enabled: bool,
    entries: &[TermEntry],
) -> EffectiveTermbase {
    if !termbase_enabled {
        return EffectiveTermbase::empty();
    }

    let mut candidates: Vec<(&TermEntry, Vec<Occurrence>)> = entries
        .iter()
        .filter(|entry| entry.enabled && entry.target_language == target_language)
        .map(|entry| {
            let occurrences = find_occurrences(source_text, &entry.source_term, entry.case_sensitive);
            (entry, occurrences)
        })
        .filter(|(_, occurrences)| !occurrences.is_empty())
        .collect();

    candidates.sort_by(candidate_priority);

    let mut claimed: Vec<Occurrence> = Vec::new();
    let mut winners: Vec<&TermEntry> = Vec::new();
    for (entry, occurrences) in &candidates {
        let survivors: Vec<Occurrence> = occurrences
            .iter()
            .copied()
            .filter(|occurrence| {
                !claimed
                    .iter()
                    .any(|claimed| spans_overlap(claimed, occurrence))
            })
            .collect();
        if survivors.is_empty() {
            continue;
        }
        winners.push(entry);
        claimed.extend(survivors);
    }

    winners.sort_by(winner_order);

    let effective_entries: Vec<EffectiveTermEntry> = winners
        .iter()
        .map(|entry| EffectiveTermEntry {
            source_term: entry.source_term.clone(),
            target_term: entry.target_term.clone(),
        })
        .collect();

    if effective_entries.is_empty() {
        return EffectiveTermbase::empty();
    }

    let block = render_block(&effective_entries);
    let fingerprint = *blake3::hash(block.as_bytes()).as_bytes();
    EffectiveTermbase {
        entries: effective_entries,
        prompt_block: block,
        fingerprint,
    }
}

/// 候选处理顺序：大小写敏感（精确匹配）优先，其次按源术语长度降序，再按内容稳定排序。
fn candidate_priority(
    a: &(&TermEntry, Vec<Occurrence>),
    b: &(&TermEntry, Vec<Occurrence>),
) -> std::cmp::Ordering {
    let a = a.0;
    let b = b.0;
    b.case_sensitive
        .cmp(&a.case_sensitive)
        .then_with(|| {
            b.source_term
                .chars()
                .count()
                .cmp(&a.source_term.chars().count())
        })
        .then_with(|| normalize_source_term(&a.source_term).cmp(&normalize_source_term(&b.source_term)))
        .then_with(|| a.source_term.cmp(&b.source_term))
        .then_with(|| a.target_term.cmp(&b.target_term))
}

/// 最终胜出条目顺序（§5.3）：源术语长度降序、规范化源术语、大小写规则、指定译法。
fn winner_order(a: &&TermEntry, b: &&TermEntry) -> std::cmp::Ordering {
    b.source_term
        .chars()
        .count()
        .cmp(&a.source_term.chars().count())
        .then_with(|| normalize_source_term(&a.source_term).cmp(&normalize_source_term(&b.source_term)))
        .then_with(|| b.case_sensitive.cmp(&a.case_sensitive))
        .then_with(|| a.source_term.cmp(&b.source_term))
        .then_with(|| a.target_term.cmp(&b.target_term))
}

fn render_block(entries: &[EffectiveTermEntry]) -> String {
    let mut lines: Vec<String> = entries
        .iter()
        .map(|entry| format!("{} => {}", entry.source_term, entry.target_term))
        .collect();
    lines.insert(0, TERM_BLOCK_HEADER.to_string());
    lines.join("\n")
}

fn spans_overlap(a: &Occurrence, b: &Occurrence) -> bool {
    a.start < b.end && b.start < a.end
}

/// 英文、数字和下划线组成的源术语按完整单词匹配；
/// 包含空格、连字符或标点的术语按精确子串匹配。
fn is_word_like(term: &str) -> bool {
    !term.is_empty() && term.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn chars_eq_folded(a: &[char], b: &str) -> bool {
    a.iter()
        .flat_map(|c| c.to_lowercase())
        .eq(b.chars().flat_map(char::to_lowercase))
}

fn find_occurrences(text: &str, term: &str, case_sensitive: bool) -> Vec<Occurrence> {
    if term.is_empty() || text.is_empty() {
        return Vec::new();
    }
    if is_word_like(term) {
        find_word_occurrences(text, term, case_sensitive)
    } else {
        find_substring_occurrences(text, term, case_sensitive)
    }
}

fn find_word_occurrences(text: &str, term: &str, case_sensitive: bool) -> Vec<Occurrence> {
    let chars: Vec<char> = text.chars().collect();
    let term_len = term.chars().count();
    let mut occurrences = Vec::new();
    let mut index = 0;
    while index + term_len <= chars.len() {
        let window = &chars[index..index + term_len];
        let matched = if case_sensitive {
            window.iter().collect::<String>() == term
        } else {
            chars_eq_folded(window, term)
        };
        if matched {
            let before = index.checked_sub(1).map(|i| chars[i]);
            let after = (index + term_len < chars.len()).then(|| chars[index + term_len]);
            if before.map_or(true, |c| !is_word_char(c))
                && after.map_or(true, |c| !is_word_char(c))
            {
                let start = chars[..index].iter().map(|c| c.len_utf8()).sum();
                let end = chars[..index + term_len].iter().map(|c| c.len_utf8()).sum();
                occurrences.push(Occurrence { start, end });
                index += term_len;
                continue;
            }
        }
        index += 1;
    }
    occurrences
}

fn find_substring_occurrences(text: &str, term: &str, case_sensitive: bool) -> Vec<Occurrence> {
    if case_sensitive {
        text.match_indices(term)
            .map(|(start, matched)| Occurrence {
                start,
                end: start + matched.len(),
            })
            .collect()
    } else {
        let chars: Vec<char> = text.chars().collect();
        let term_len = term.chars().count();
        let mut occurrences = Vec::new();
        let mut index = 0;
        while index + term_len <= chars.len() {
            let window = &chars[index..index + term_len];
            if chars_eq_folded(window, term) {
                let start = chars[..index].iter().map(|c| c.len_utf8()).sum();
                let end = chars[..index + term_len].iter().map(|c| c.len_utf8()).sum();
                occurrences.push(Occurrence { start, end });
            }
            index += 1;
        }
        occurrences
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::termbase::model::TermEntry;

    fn entry(
        source: &str,
        language: &str,
        target: &str,
        sensitive: bool,
        enabled: bool,
    ) -> TermEntry {
        TermEntry {
            id: format!("{source}-{language}-{target}"),
            source_term: source.to_string(),
            target_language: language.to_string(),
            target_term: target.to_string(),
            enabled,
            case_sensitive: sensitive,
            created_at_utc_ms: 0,
            updated_at_utc_ms: 0,
        }
    }

    fn resolve_into(
        text: &str,
        language: &str,
        entries: &[TermEntry],
    ) -> Vec<(String, String)> {
        resolve(text, language, true, entries)
            .entries()
            .iter()
            .map(|e| (e.source_term.clone(), e.target_term.clone()))
            .collect()
    }

    #[test]
    fn disabled_switch_returns_empty_set() {
        let entries = vec![entry("function", "简体中文", "函数", false, true)];
        let effective = resolve("function", "简体中文", false, &entries);
        assert!(effective.is_empty());
        assert_eq!(effective.prompt_block(), "");
        assert_eq!(*effective.fingerprint(), [0u8; 32]);
    }

    #[test]
    fn target_language_filters_entries() {
        let entries = vec![
            entry("function", "简体中文", "函数", false, true),
            entry("function", "English", "函数", false, true),
        ];
        assert_eq!(
            resolve_into("function", "简体中文", &entries),
            vec![("function".to_string(), "函数".to_string())]
        );
        assert!(resolve_into("function", "日本語", &entries).is_empty());
    }

    #[test]
    fn disabled_entries_are_ignored() {
        let entries = vec![entry("function", "简体中文", "函数", false, false)];
        assert!(resolve_into("function", "简体中文", &entries).is_empty());
    }

    #[test]
    fn word_like_terms_match_only_whole_words() {
        let entries = vec![entry("function", "简体中文", "函数", false, true)];
        assert_eq!(
            resolve_into("a function is useful", "简体中文", &entries),
            vec![("function".to_string(), "函数".to_string())]
        );
        assert!(resolve_into("functional programming", "简体中文", &entries).is_empty());
        assert!(resolve_into("function_x", "简体中文", &entries).is_empty());
        assert_eq!(
            resolve_into("non-function", "简体中文", &entries).len(),
            1,
            "连字符是词边界，function 在 non-function 中是完整单词"
        );
        assert_eq!(
            resolve_into("function, function!", "简体中文", &entries).len(),
            1,
            "一个条目只注入一次"
        );
    }

    #[test]
    fn word_like_terms_match_adjacent_cjk_boundaries() {
        let entries = vec![entry("code", "简体中文", "代码", false, true)];
        assert_eq!(
            resolve_into("代码code代码", "简体中文", &entries),
            vec![("code".to_string(), "代码".to_string())]
        );
    }

    #[test]
    fn multi_word_and_punctuation_terms_use_exact_substring() {
        let entries = vec![
            entry("neural network", "简体中文", "神经网络", false, true),
            entry("state-of-the-art", "简体中文", "最先进的", false, true),
        ];
        assert_eq!(
            resolve_into("we study neural network models", "简体中文", &entries),
            vec![("neural network".to_string(), "神经网络".to_string())]
        );
        // 子串匹配不要求词边界
        assert_eq!(
            resolve_into("neural network-is-state-of-the-art-now", "简体中文", &entries).len(),
            2
        );
    }

    #[test]
    fn matching_does_not_skip_markdown_or_latex() {
        let entries = vec![
            entry("function", "简体中文", "函数", false, true),
            entry("X", "简体中文", "X变量", true, true),
        ];
        let text = "```rust\nfn function() {}\n```\n行内 `function` 与 $X \\in R$";
        assert_eq!(
            resolve_into(text, "简体中文", &entries),
            vec![
                ("function".to_string(), "函数".to_string()),
                ("X".to_string(), "X变量".to_string()),
            ],
            "匹配作用于完整原文，包括代码块、行内代码与 LaTeX 公式"
        );
    }

    #[test]
    fn case_sensitive_exception_wins_over_insensitive_default() {
        let entries = vec![
            entry("china", "简体中文", "瓷器", false, true),
            entry("China", "简体中文", "中国", true, true),
        ];
        assert_eq!(
            resolve_into("China", "简体中文", &entries),
            vec![("China".to_string(), "中国".to_string())],
            "原文 China 只使用敏感条目"
        );
        assert_eq!(
            resolve_into("china", "简体中文", &entries),
            vec![("china".to_string(), "瓷器".to_string())]
        );
        assert_eq!(
            resolve_into("CHINA", "简体中文", &entries),
            vec![("china".to_string(), "瓷器".to_string())]
        );
        // 两个敏感条目不冲突地命中各自出现
        let entries = vec![
            entry("china", "简体中文", "瓷器", true, true),
            entry("China", "简体中文", "中国", true, true),
        ];
        assert_eq!(resolve_into("China china", "简体中文", &entries).len(), 2);
    }

    #[test]
    fn longer_overlapping_term_wins_and_suppresses_conflict() {
        let entries = vec![
            entry("neural network", "简体中文", "神经网络", false, true),
            entry("network", "简体中文", "网络", false, true),
        ];
        let winners = resolve_into("neural network", "简体中文", &entries);
        assert_eq!(
            winners,
            vec![("neural network".to_string(), "神经网络".to_string())]
        );
        // 不重叠的出现保留
        let winners = resolve_into("network neural network", "简体中文", &entries);
        assert_eq!(
            winners,
            vec![
                ("neural network".to_string(), "神经网络".to_string()),
                ("network".to_string(), "网络".to_string()),
            ]
        );
    }

    #[test]
    fn sensitive_shorter_term_still_beats_insensitive_longer_term() {
        let entries = vec![
            entry("CNN", "简体中文", "卷积神经网络", true, true),
            entry("cnn news", "简体中文", "CNN 新闻", false, true),
        ];
        let winners = resolve_into("CNN news", "简体中文", &entries);
        assert_eq!(
            winners,
            vec![("CNN".to_string(), "卷积神经网络".to_string())],
            "敏感精确命中占用区间后抑制不敏感子串"
        );
    }

    #[test]
    fn winner_order_is_stable_by_content_only() {
        let mut entries = vec![
            entry("network", "简体中文", "网络", false, true),
            entry("neural network", "简体中文", "神经网络", false, true),
            entry("China", "简体中文", "中国", true, true),
            entry("china", "简体中文", "瓷器", false, true),
        ];
        let text = "China china network neural network";
        let a = resolve_into(text, "简体中文", &entries);
        entries.reverse();
        let b = resolve_into(text, "简体中文", &entries);
        assert_eq!(a, b, "排序不依赖插入顺序");
        assert_eq!(a[0].0, "neural network", "长术语在前");
        assert_eq!(a[1].0, "network");
        assert_eq!(a[2].0, "China", "敏感条目在大小写折叠相同项前");
        assert_eq!(a[3].0, "china");
    }

    #[test]
    fn fingerprint_ignores_metadata_and_non_matching_entries() {
        let matching = vec![entry("function", "简体中文", "函数", false, true)];
        let mut with_metadata = vec![entry("function", "简体中文", "函数", false, true)];
        with_metadata[0].id = "different-uuid".to_string();
        with_metadata[0].created_at_utc_ms = 999;
        with_metadata[0].updated_at_utc_ms = 999;
        let effective_a = resolve("function()", "简体中文", true, &matching);
        let effective_b = resolve("function()", "简体中文", true, &with_metadata);
        assert_eq!(effective_a.fingerprint(), effective_b.fingerprint());

        // 未命中条目不改变指纹
        let mut with_unrelated = vec![entry("function", "简体中文", "函数", false, true)];
        with_unrelated.push(entry("unrelated", "简体中文", "无关", false, true));
        let effective_c = resolve("function()", "简体中文", true, &with_unrelated);
        assert_eq!(effective_a.fingerprint(), effective_c.fingerprint());
    }

    #[test]
    fn fingerprint_differs_for_conflicting_terms_and_is_deterministic() {
        let plain = vec![entry("function", "简体中文", "函数", false, true)];
        let conflicting = vec![entry("function", "简体中文", "功能", false, true)];
        let a = resolve("function", "简体中文", true, &plain);
        let b = resolve("function", "简体中文", true, &conflicting);
        assert_ne!(a.fingerprint(), b.fingerprint());

        let a2 = resolve("function", "简体中文", true, &plain);
        assert_eq!(a.fingerprint(), a2.fingerprint());
    }

    #[test]
    fn prompt_block_renders_compact_structured_block() {
        let entries = vec![
            entry("function", "简体中文", "函数", false, true),
            entry("neural network", "简体中文", "神经网络", false, true),
        ];
        let effective = resolve("neural network function", "简体中文", true, &entries);
        let block = effective.prompt_block();
        assert!(block.starts_with("以下是翻译约束；仅在原文术语匹配且语义适用时优先采用右侧译法："));
        assert!(block.contains("neural network => 神经网络"));
        assert!(block.contains("function => 函数"));
        assert_eq!(effective.entries().len(), 2);
    }

    #[test]
    fn empty_and_disabled_sets_share_zero_fingerprint() {
        let effective_a = resolve("x", "简体中文", false, &[]);
        let effective_b = resolve("x", "简体中文", true, &[]);
        assert_eq!(effective_a.fingerprint(), effective_b.fingerprint());
        assert_eq!(*effective_a.fingerprint(), [0u8; 32]);
    }

    #[test]
    fn english_digit_underscore_terms_are_word_like() {
        assert!(is_word_like("function"));
        assert!(is_word_like("foo_bar"));
        assert!(is_word_like("2.0".split('.').next().unwrap()));
        assert!(is_word_like("GPT4"));
        assert!(!is_word_like("neural network"));
        assert!(!is_word_like("state-of-the-art"));
        assert!(!is_word_like("foo,bar"));
    }

    #[test]
    fn underscore_word_like_term_does_not_match_inside_identifier() {
        let entries = vec![entry("foo", "简体中文", "条", false, true)];
        assert!(resolve_into("my_foo_bar", "简体中文", &entries).is_empty());
        assert_eq!(
            resolve_into("foo bar", "简体中文", &entries).len(),
            1,
            "空格两侧都是词边界"
        );
    }

    #[test]
    fn helper_terms_never_panics_on_empty_input() {
        let effective = resolve("", "简体中文", true, &[]);
        assert!(effective.is_empty());
        let entries = vec![entry("", "简体中文", "空", false, true)];
        assert!(resolve_into("text", "简体中文", &entries).is_empty());
    }

    #[test]
    fn repeated_same_entry_occurrences_do_not_duplicate_injection() {
        let entries = vec![entry("network", "简体中文", "网络", false, true)];
        assert_eq!(resolve_into("network network network", "简体中文", &entries).len(), 1);
    }
}
