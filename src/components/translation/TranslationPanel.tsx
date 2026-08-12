import { lazy, Suspense } from "react";

const MarkdownTranslation = lazy(() =>
  import("./MarkdownTranslation").then((module) => ({
    default: module.MarkdownTranslation,
  }))
);

interface TranslationPanelProps {
  text: string;
  mode?: "streaming" | "complete" | "partial";
  bare?: boolean;
}

/** 译文区域，更突出 */
export function TranslationPanel({ text, mode = "complete", bare = false }: TranslationPanelProps) {
  const isComplete = mode === "complete";
  const isPartial = mode === "partial";

  const content = (
    <>
      <div className="mb-1 flex items-center gap-2 text-xs font-medium uppercase tracking-wide text-accent">
        <span>{isPartial ? "未完成译文" : "译文"}</span>
        {!isComplete && !isPartial ? (
          <span className="normal-case tracking-normal text-ink-muted">生成中</span>
        ) : null}
      </div>
      {isComplete ? (
        <div className="translation-markdown text-[15px] leading-relaxed text-ink">
          <Suspense
            fallback={
              <p className="whitespace-pre-wrap break-words text-[15px] leading-relaxed text-ink">
                {text}
              </p>
            }
          >
            <MarkdownTranslation text={text} />
          </Suspense>
        </div>
      ) : (
        <p className="whitespace-pre-wrap break-words text-[15px] leading-relaxed text-ink">
          {text}
        </p>
      )}
    </>
  );
  return bare ? <div>{content}</div> : <div className="rounded-lg bg-surface-panel px-3 py-2.5">{content}</div>;
}
