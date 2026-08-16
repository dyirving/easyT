//! 翻译 Prompt（Official API 与 WebGateway 共用）
//!
//! 纯逻辑模块：不持有状态，不读取配置，不发请求。

use crate::termbase::EffectiveTermbase;

/// Prompt 版本：内容或格式要求变化时必须手动提升，
/// 使既有缓存键（含 prompt_version）自然失效（《翻译缓存规则》§5.3）。
/// 1 -> 2：共享模板新增条件术语约束块（术语表）。
pub const PROMPT_VERSION: u32 = 2;

/// 根据目标语言动态拼装系统提示词，并在有效术语集非空时追加术语约束块。
pub fn build_system_prompt(target_language: &str, termbase: &EffectiveTermbase) -> String {
    let base = format!(
        r#"你是一名严谨的英文学术文献翻译助手。

请将用户提供的英文内容翻译为{target_language}。

要求：
1. 准确保留原文含义，不擅自增加原文不存在的信息。
2. 使用自然、清晰、符合{target_language}学术表达习惯的语言。
3. 保留公式、变量、引用编号、缩写和特殊符号。
4. 公式必须输出为可被 Markdown 数学渲染器解析的 LaTeX：行内公式使用 `$...$`，独立公式使用 `$$...$$`。
5. 不要把公式写成转义后的普通文本，例如不要输出 `\$X\^2\$`、`\\(X\\)` 或散落的 `\in`；应输出 `$X^2$`、`$X \in \mathbb{{R}}$`。
6. 公式内部保持合法 LaTeX，优先使用 KaTeX 支持的写法，如 `\mathbb{{R}}`、`\times`、`_`、`^`、`\text{{model}}`；包含 `\tag{{n}}` 的编号公式必须使用 `$$...$$` 独立成段，不能使用 `$...$`。
7. 翻译公式周围的自然语言，但不要翻译或改写公式内部的变量名、维度符号、函数名和下标含义。
8. 代码标识符、库名、包名、命令、API 名称等不可翻译部分保留原文，但要翻译其类别或上下文含义；例如 "Python requests" 应译为 "Python requests 库"。
9. 重要专业术语首次出现时，可以使用"中文术语（English Term）"形式。
10. 如果输入是短语、标题或不完整的句子，也必须给出{target_language}表达，不要简单照抄原文。
11. 如果输入确实完全由不可翻译专名组成，可保留专名并补充最小必要的{target_language}说明。
12. 不解释翻译过程。
13. 只输出译文。"#,
        target_language = target_language
    );
    let block = termbase.prompt_block();
    if termbase.is_empty() {
        base
    } else {
        format!("{base}\n\n{block}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_guides_short_technical_phrases() {
        let prompt = build_system_prompt("简体中文", &EffectiveTermbase::empty());

        assert!(prompt.contains("Python requests 库"));
        assert!(prompt.contains("短语"));
        assert!(prompt.contains("不要简单照抄原文"));
    }

    #[test]
    fn prompt_requires_markdown_math_output() {
        let prompt = build_system_prompt("简体中文", &EffectiveTermbase::empty());

        assert!(prompt.contains("行内公式使用 `$...$`"));
        assert!(prompt.contains("独立公式使用 `$$...$$`"));
        assert!(prompt.contains("\\mathbb{R}"));
        assert!(prompt.contains("KaTeX"));
        assert!(prompt.contains("包含 `\\tag{n}` 的编号公式必须使用 `$$...$$`"));
        assert!(prompt.contains("不要输出 `\\$X\\^2\\$`"));
    }

    #[test]
    fn empty_termbase_keeps_prompt_unchanged() {
        let empty = build_system_prompt("简体中文", &EffectiveTermbase::empty());
        assert!(!empty.contains("以下是翻译约束"));
        assert!(!empty.contains("=>"));
        assert!(empty.ends_with("只输出译文。"));
    }

    #[test]
    fn non_empty_termbase_appends_compact_block() {
        let entry = crate::termbase::matcher::resolve(
            "neural network",
            "简体中文",
            true,
            &[crate::termbase::model::TermEntry {
                id: "t-1".to_string(),
                source_term: "neural network".to_string(),
                target_language: "简体中文".to_string(),
                target_term: "神经网络".to_string(),
                enabled: true,
                case_sensitive: false,
                created_at_utc_ms: 0,
                updated_at_utc_ms: 0,
            }],
        );
        let prompt = build_system_prompt("简体中文", &entry);
        assert!(prompt.contains("以下是翻译约束；仅在原文术语匹配且语义适用时优先采用右侧译法："));
        assert!(prompt.contains("neural network => 神经网络"));
        assert!(prompt.ends_with("neural network => 神经网络"), "术语块追加在末尾");
    }
}
