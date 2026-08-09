//! 缓存键与输入规范化（规则文档 §5～§7；FR-004/005/008/015）
//!
//! 纯函数模块：不持有状态、不读取配置、不发网络。
//! 规范化与键编码在锁外完成，把受控的所有权（Vec 字节、len）交给缓存层。

use crate::translation_backend::models::{BackendMode, BackendResult};
use crate::translation_backend::prompt::PROMPT_VERSION;

/// 键编码方案版本：编码、规范化或输出参数集合变化时手动提升。
pub const CACHE_KEY_VERSION: u32 = 1;

/// 单条逻辑字节上限；恰好 1 MiB 可缓存，大于则跳过缓存但照常翻译。
pub const MAX_ENTRY_LOGICAL_BYTES: u64 = 1024 * 1024;

/// 短文本按《翻译缓存规则》§8：规范化后原文 UTF-8 不超过 256 字节且无 LF。
pub const SHORT_TEXT_MAX_BYTES: usize = 256;

/// 键空间领域前缀，固定字符串作为格式标识，不带长度前缀。
const DOMAIN: &[u8] = b"easyT.translation-cache";

/// UTF-8 BOM：只允许出现在输入开头，规范化必须去掉。
const UTF8_BOM: &str = "\u{feff}";

/// BLAKE3 256-bit 精确键（32 字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CacheKey([u8; 32]);

impl CacheKey {
    /// 供 03 工单 L2 持久化层以原始 32 字节作为 SQLite 主键。
    #[allow(dead_code)]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn from_hash(hash: &blake3::Hash) -> Self {
        Self(*hash.as_bytes())
    }
}

#[cfg(test)]
impl CacheKey {
    /// 测试用确定键：按种子字节填充前 4 字节，其余为零。
    pub(crate) fn from_seed(seed: u32) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..4].copy_from_slice(&seed.to_le_bytes());
        Self(bytes)
    }
}

/// 规范化后的缓存输入：受控所有权，锁外构建。
#[derive(Debug, Clone)]
pub struct NormalizedCacheInput {
    pub key: CacheKey,
    /// 规范化后原文的 UTF-8 字节数，用于逻辑大小（原文不落库）。
    pub normalized_source_bytes: usize,
    /// trim 后的目标语言（参与键与逻辑大小）。
    pub target_language: String,
    /// 是否为短文本：进入 L1 短池，否则进入长池。
    pub is_short_text: bool,
}

/// 规范化 → 键与逻辑大小信息的单次入口。
pub fn prepare_cache_input(text: &str, target_language: &str) -> NormalizedCacheInput {
    let normalized = normalize(text);
    // 短文本判定基于规范化后原文（§8：不含 LF 且 ≤256 字节），与键共享同一份规范化结果。
    let is_short_text = !normalized.contains(&b'\n') && normalized.len() <= SHORT_TEXT_MAX_BYTES;
    let target = target_language.trim();

    let mut encoder = KeyEncoder::new();
    encoder.write_raw(DOMAIN);
    encoder.write_u32(CACHE_KEY_VERSION);
    encoder.write_u32(PROMPT_VERSION);
    encoder.write_bytes(&normalized);
    encoder.write_bytes(target.as_bytes());
    // 当前没有输出影响参数；恒为 0，保留字段位置，加入参数时提升 CACHE_KEY_VERSION。
    encoder.write_u32(0);

    NormalizedCacheInput {
        key: CacheKey::from_hash(&blake3::hash(&encoder.finish())),
        normalized_source_bytes: normalized.len(),
        target_language: target.to_string(),
        is_short_text,
    }
}

/// 规范化：
/// - 单行（无 LF/CR）：去开头 BOM；去首尾 Unicode whitespace；内部空白与大小写不变。
/// - 多行（含 LF/CR/CRLF）：去开头 BOM；CRLF/CR 归一为 LF；首尾空白、缩进、空行、Markdown/LaTeX 不变。
///
/// 禁止 lowercase、空白折叠、NFC/NFKC、Markdown 重排与模糊匹配。
fn normalize(text: &str) -> Vec<u8> {
    if text.contains(['\n', '\r']) {
        normalize_multiline(text)
    } else {
        text.strip_prefix(UTF8_BOM)
            .unwrap_or(text)
            .trim()
            .as_bytes()
            .to_vec()
    }
}

fn normalize_multiline(text: &str) -> Vec<u8> {
    let text = text.strip_prefix(UTF8_BOM).unwrap_or(text);
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(b'\n');
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}

/// 键编码器：整数一律 little-endian；变长字段统一 u32 长度前缀 + 原始字节。
/// 编码规则变化必须提升 CACHE_KEY_VERSION 并重新生成固定向量。
struct KeyEncoder {
    buf: Vec<u8>,
}

impl KeyEncoder {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(128),
        }
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    fn write_u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.write_u32(bytes.len() as u32);
        self.write_raw(bytes);
    }

    fn finish(self) -> Vec<u8> {
        self.buf
    }
}

/// 单条逻辑字节（《翻译缓存规则》§7）：
/// 32(键) + 各原文/语言/译文/来源 UTF-8 字节 + 固定 256 开销。
pub fn logical_size(input: &NormalizedCacheInput, result: &BackendResult) -> u64 {
    32 + input.normalized_source_bytes as u64
        + input.target_language.len() as u64
        + result.translated_text.len() as u64
        + backend_label(result.source.backend).len() as u64
        + result.source.provider.len() as u64
        + result.source.model.len() as u64
        + 256
}

/// 最小可判定逻辑大小：不含未知的译文和来源元数据。
/// 若该下界已超过 1 MiB，请求必然不可缓存，Use 路径也不应查 L1/L2。
pub fn is_definitely_oversized(input: &NormalizedCacheInput) -> bool {
    32 + input.normalized_source_bytes as u64 + input.target_language.len() as u64 + 256
        > MAX_ENTRY_LOGICAL_BYTES
}

/// BackendMode 的 serde camelCase 标签长度，与 models.rs 序列化保持一致。
fn backend_label(mode: BackendMode) -> &'static str {
    match mode {
        BackendMode::OfficialApi => "officialApi",
        BackendMode::WebGateway => "webGateway",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 把输入实参按固定流水线转成十六进制键，测试断言用。
    fn key_hex(text: &str, target: &str) -> String {
        let input = prepare_cache_input(text, target);
        input
            .key
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn single_line_trims_only_outer_whitespace() {
        assert_eq!(
            key_hex("  Hello world  ", "en"),
            key_hex("Hello world", "en")
        );
        assert_eq!(key_hex("\tHello \t", "en"), key_hex("Hello", "en"));
        // 含 LF 即多行：首尾空白保留
        assert_ne!(key_hex("\t\nHello\n", "en"), key_hex("Hello", "en"));
    }

    #[test]
    fn single_line_keeps_inner_whitespace() {
        assert_ne!(key_hex("a  b", "en"), key_hex("a b", "en"));
    }

    #[test]
    fn single_line_keeps_case_and_punctuation() {
        assert_ne!(key_hex("A, B!", "en"), key_hex("a, b!", "en"));
        assert_ne!(key_hex("import x", "en"), key_hex("importx", "enxx"));
    }

    #[test]
    fn bom_removed_only_at_start() {
        assert_eq!(key_hex("\u{feff}Hello", "en"), key_hex("Hello", "en"));
        assert_eq!(
            key_hex("a\r\n\u{feff}b", "en"),
            key_hex("a\n\u{feff}b", "en"),
            "多行中的 BOM 不做转义"
        );
        assert_ne!(key_hex("Hello\u{feff}", "en"), key_hex("Hello", "en"));
    }

    #[test]
    fn multiline_line_endings_are_equivalent() {
        let lf = key_hex("line1\nline2", "en");
        let crlf = key_hex("line1\r\nline2", "en");
        let cr = key_hex("line1\rline2", "en");
        assert_eq!(lf, crlf);
        assert_eq!(lf, cr);
    }

    #[test]
    fn multiline_keeps_edges_indentation_and_blank_lines() {
        assert_ne!(key_hex("  a \n  b", "en"), key_hex("a\nb", "en"));
        assert_ne!(key_hex("a\n\n  b", "en"), key_hex("a\nb", "en"));
    }

    #[test]
    fn multiline_keeps_markdown_and_latex() {
        let md = "```rust\nfn main() {\n    let x = 1;\n}\n```";
        let latex = "公式 $X \\in \\mathbb{R}$ 与 $$\\tag{1} X = Y$$";
        assert_eq!(key_hex(md, "简体中文"), key_hex(md, "简体中文"));
        assert_eq!(key_hex(latex, "简体中文"), key_hex(latex, "简体中文"));
        // 反斜杠（LaTeX/代码）不得被折叠或重排
        assert_ne!(
            key_hex(latex, "简体中文"),
            key_hex(&latex.replace("\\", ""), "简体中文")
        );
    }

    #[test]
    fn target_language_participates_and_is_trimmed() {
        assert_ne!(key_hex("hello", "简体中文"), key_hex("hello", "English"));
        assert_eq!(key_hex("hello", " 简体中文 "), key_hex("hello", "简体中文"));
    }

    #[test]
    fn cache_key_and_prompt_versions_are_one() {
        assert_eq!(CACHE_KEY_VERSION, 1);
        assert_eq!(PROMPT_VERSION, 1);
    }

    #[test]
    fn short_text_classification_bounds() {
        assert!(prepare_cache_input("", "en").is_short_text);
        assert!(prepare_cache_input(&"x".repeat(256), "en").is_short_text);
        assert!(!prepare_cache_input(&"x".repeat(257), "en").is_short_text);
        assert!(!prepare_cache_input("a\nb", "en").is_short_text);
        // CR 归一后含 LF → 长文本
        assert!(!prepare_cache_input("a\rb", "en").is_short_text);
        // 多字节 UTF-8 按字节数计
        assert!(prepare_cache_input(&"你".repeat(85), "en").is_short_text); // 255 字节
        assert!(!prepare_cache_input(&"你".repeat(86), "en").is_short_text); // 258 字节
    }

    #[test]
    fn normalized_source_bytes_reflect_normalization() {
        let input = prepare_cache_input("  hello  ", "en");
        assert_eq!(input.normalized_source_bytes, b"hello".len());
        let crlf = prepare_cache_input("a\r\nb", "en");
        assert_eq!(crlf.normalized_source_bytes, 3, "归一为 LF 后的字节数");
    }

    #[test]
    fn logical_size_follows_formula() {
        let input = prepare_cache_input("text", "en");
        let result = BackendResult {
            translated_text: "译文".to_string(),
            source: crate::translation_backend::models::BackendSource {
                backend: BackendMode::OfficialApi,
                provider: "p".to_string(),
                model: "m".to_string(),
            },
        };
        // 32 + 4(raw "text") + 2("en") + 6("译文") + 11("officialApi") + 1 + 1 + 256
        assert_eq!(
            logical_size(&input, &result),
            32 + 4 + 2 + 6 + 11 + 1 + 1 + 256
        );
    }

    #[test]
    fn definitely_oversized_input_is_known_before_lookup() {
        let safe_len = MAX_ENTRY_LOGICAL_BYTES as usize - 32 - 2 - 256;
        let exact_lower_bound = prepare_cache_input(&"x".repeat(safe_len), "en");
        assert!(!is_definitely_oversized(&exact_lower_bound));

        let oversized = prepare_cache_input(&"x".repeat(safe_len + 1), "en");
        assert!(is_definitely_oversized(&oversized));
    }

    #[test]
    fn web_gateway_backend_label_uses_camel_case() {
        assert_eq!(backend_label(BackendMode::WebGateway), "webGateway");
        assert_eq!(backend_label(BackendMode::OfficialApi), "officialApi");
    }

    /// 固定向量：由 blake3 1.8.6 生成，编码规则/版本变化时必须同步更新
    /// 并手动提升 CACHE_KEY_VERSION。
    #[test]
    fn fixed_key_vectors_are_stable_snapshots() {
        let single = key_hex("Hello World!", "简体中文");
        let multi = key_hex(
            "## 标题\n\n第一段 $\\mathbb{R}$\r\n第二段\r第三段",
            "English",
        );
        // 固定向量快照：blake3 1.8.6 生成；编码规则/版本变化时必须同步更新
        // 并手动提升 CACHE_KEY_VERSION。
        assert_eq!(
            single,
            "6f7832409d4d82c35f2fe78eb5bb3237bfc206839511f88f708b5c4f526259be"
        );
        assert_eq!(
            multi,
            "eafd922fcb6daea47fba7f900e5f90a0f6999625b4d9f548af32e67e2df694a3"
        );
        assert_ne!(single, multi);
    }
}
