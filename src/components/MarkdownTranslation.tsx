import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkMath from "remark-math";

interface MarkdownTranslationProps {
  text: string;
}

export function MarkdownTranslation({ text }: MarkdownTranslationProps) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkMath]}
      rehypePlugins={[rehypeKatex]}
      components={{
        a: ({ children }) => <span>{children}</span>,
      }}
    >
      {text}
    </ReactMarkdown>
  );
}
