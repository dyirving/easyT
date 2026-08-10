import { StatusBanner } from "@/components/patterns";

/**
 * 缓存来源提示：只负责可访问的信息提示，不接收或渲染译文内容。
 * 不参与复制、Markdown/KaTeX 渲染或任何持久化。
 */
export function CacheNotice() {
  return (
    <StatusBanner
      tone="info"
      announcement="polite"
      description="此译文来自本机缓存，点击“重新翻译”可使用当前模型刷新。"
    />
  );
}
