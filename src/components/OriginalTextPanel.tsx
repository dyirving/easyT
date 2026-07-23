interface OriginalTextPanelProps {
  text: string;
}

/** 原文区域，颜色更弱，突出译文 */
export function OriginalTextPanel({ text }: OriginalTextPanelProps) {
  if (!text) {
    return null;
  }
  return (
    <div className="rounded-lg bg-surface-soft/60 px-3 py-2.5">
      <div className="mb-1 text-xs font-medium uppercase tracking-wide text-ink-muted">
        原文
      </div>
      <p className="whitespace-pre-wrap break-words text-sm leading-relaxed text-ink-soft">
        {text}
      </p>
    </div>
  );
}
