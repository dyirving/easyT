import { Pin, PinOff, Settings, X, RefreshCw, Copy, Check } from "lucide-react";
import { Button } from "@/components/ui/Button";

interface TranslationHeaderProps {
  pinned: boolean;
  onTogglePin: () => void;
  onOpenSettings: () => void;
  onClose: () => void;
  onRetry: () => void;
  canRetry: boolean;
  copied: boolean;
  onCopy: () => void;
  canCopy: boolean;
}

/** 翻译窗口顶部标题栏与操作按钮 */
export function TranslationHeader({
  pinned,
  onTogglePin,
  onOpenSettings,
  onClose,
  onRetry,
  canRetry,
  copied,
  onCopy,
  canCopy,
}: TranslationHeaderProps) {
  return (
    <div
      className="flex items-center justify-between border-b border-line px-3 py-2"
      data-tauri-drag-region
    >
      <div className="flex items-center gap-1.5 text-sm font-medium text-ink">
        <span className="inline-block h-2 w-2 rounded-full bg-accent" />
        easyT
      </div>
      <div className="flex items-center gap-0.5">
        <Button
          variant="ghost"
          size="icon"
          onClick={onCopy}
          disabled={!canCopy}
          title={copied ? "已复制" : "复制译文"}
          aria-label={copied ? "已复制" : "复制译文"}
        >
          {copied ? (
            <Check className="h-4 w-4 text-success" />
          ) : (
            <Copy className="h-4 w-4" />
          )}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={onRetry}
          disabled={!canRetry}
          title="重新翻译"
          aria-label="重新翻译"
        >
          <RefreshCw className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={onTogglePin}
          title={pinned ? "取消固定" : "固定窗口"}
          aria-label={pinned ? "取消固定" : "固定窗口"}
        >
          {pinned ? (
            <PinOff className="h-4 w-4 text-accent" />
          ) : (
            <Pin className="h-4 w-4" />
          )}
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={onOpenSettings}
          title="打开设置"
          aria-label="打开设置"
        >
          <Settings className="h-4 w-4" />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          onClick={onClose}
          title="关闭"
          aria-label="关闭"
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
