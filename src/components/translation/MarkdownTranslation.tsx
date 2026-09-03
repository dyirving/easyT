import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import "./MarkdownTranslation.css";

interface MarkdownTranslationProps {
  text: string;
}

/** KaTeX 只允许块公式使用 \tag；容错处理模型偶发的独立行内编号公式。 */
function normalizeTaggedEquations(markdown: string) {
  return markdown
    .split(/(\r?\n)/)
    .map((line) => {
      const match =
        line.match(
          /^(\s*)\$\$([^\r\n]*\\tag\s*\{[^}]+\}[^\r\n]*)\$\$(\s*)$/,
        ) ??
        line.match(
          /^(\s*)\$([^$\r\n]*\\tag\s*\{[^}]+\}[^$\r\n]*)\$(\s*)$/,
        );
      return match
        ? `${match[1]}$$\n${match[1]}${match[2]}\n${match[1]}$$${match[3]}`
        : line;
    })
    .join("");
}

export function MarkdownTranslation({ text }: MarkdownTranslationProps) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkMath]}
      rehypePlugins={[rehypeKatex]}
      components={{
        a: ({ children }) => <span>{children}</span>,
        table: ({ children }) => (
          <div className="translation-markdown-table-wrap">
            <table>{children}</table>
          </div>
        ),
      }}
    >
      {normalizeTaggedEquations(text)}
    </ReactMarkdown>
  );
}
