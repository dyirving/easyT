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
      <p className="whitespace-pre-wrap break-words text-[15px] leading-relaxed text-ink">
        {text}
      </p>
    </div>
  );
}
