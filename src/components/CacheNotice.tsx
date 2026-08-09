import { Info } from "lucide-react";

/**
 * 缓存来源提示：只负责可访问的信息提示，不接收或渲染译文内容。
 * 不参与复制、Markdown/KaTeX 渲染或任何持久化。
 */
export function CacheNotice() {
  return (
    <div
      role="note"
      className="flex items-start gap-2 rounded-lg border border-accent/30 bg-accent/5 px-3 py-2 text-xs text-ink-soft"
    >
      <Info className="mt-0.5 h-3.5 w-3.5 shrink-0 text-accent" />
      <span>
        此译文来自本机缓存，点击“重新翻译”可使用当前模型刷新。
      </span>
    </div>
  );
}