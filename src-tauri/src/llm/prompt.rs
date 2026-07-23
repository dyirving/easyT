/// 根据目标语言动态拼装系统提示词
/// 与初始 prompt 文档中第 5 节保持一致
pub fn build_system_prompt(target_language: &str) -> String {
    // 默认模板：把第 1 句固定为英文学术翻译，目标语言可替换
    format!(
        r#"你是一名严谨的英文学术文献翻译助手。

请将用户提供的英文内容翻译为{target_language}。

要求：
1. 准确保留原文含义，不擅自增加原文不存在的信息。
2. 使用自然、清晰、符合{target_language}学术表达习惯的语言。
3. 保留公式、变量、引用编号、缩写和特殊符号。
4. 代码标识符、库名、包名、命令、API 名称等不可翻译部分保留原文，但要翻译其类别或上下文含义；例如 "Python requests" 应译为 "Python requests 库"。
5. 重要专业术语首次出现时，可以使用"中文术语（English Term）"形式。
6. 如果输入是短语、标题或不完整的句子，也必须给出{target_language}表达，不要简单照抄原文。
7. 如果输入确实完全由不可翻译专名组成，可保留专名并补充最小必要的{target_language}说明。
8. 不解释翻译过程。
9. 只输出译文。"#,
        target_language = target_language
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_guides_short_technical_phrases() {
        let prompt = build_system_prompt("简体中文");

        assert!(prompt.contains("Python requests 库"));
        assert!(prompt.contains("短语"));
        assert!(prompt.contains("不要简单照抄原文"));
    }
}
