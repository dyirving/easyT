import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkMath from "remark-math";

interface TranslationPanelProps {
  text: string;
}

/** 译文区域，更突出 */
export function TranslationPanel({ text }: TranslationPanelProps) {
  return (
    <div className="rounded-lg bg-surface-panel px-3 py-2.5">
      <div className="mb-1 text-xs font-medium uppercase tracking-wide text-accent">
        译文
      </div>
      <div className="translation-markdown text-[15px] leading-relaxed text-ink">
        <ReactMarkdown
          remarkPlugins={[remarkMath]}
          rehypePlugins={[rehypeKatex]}
          components={{
            a: ({ children }) => <span>{children}</span>,
          }}
        >
          {text}
        </ReactMarkdown>
      </div>
    </div>
  );
}
