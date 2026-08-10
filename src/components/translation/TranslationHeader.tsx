import { Pin, PinOff, Settings, X, RefreshCw, Copy, Check } from "lucide-react";
import { IconButton } from "@/components/ui";

const charaLogo = new URL(
  "../../assets/chara_logo_titlebar.png",
  import.meta.url,
).href;

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
    <div className="flex items-center justify-between border-b border-line px-3 py-2">
      <div
        className="flex min-w-0 flex-1 items-center gap-1.5 self-stretch text-sm font-medium text-ink"
        data-tauri-drag-region
      >
        <span className="inline-block h-2 w-2 rounded-full bg-accent" />
        <img
          src={charaLogo}
          alt="easyT"
          className="pointer-events-none h-5 w-auto select-none object-contain"
          draggable={false}
        />
      </div>
      <div className="flex items-center gap-0.5">
        <IconButton
          label={copied ? "已复制" : "复制译文"}
          size="sm"
          onClick={onCopy}
          disabled={!canCopy}
        >
          {copied ? (
            <Check className="h-4 w-4 text-success" />
          ) : (
            <Copy className="h-4 w-4" />
          )}
        </IconButton>
        <IconButton
          label="重新翻译"
          size="sm"
          onClick={onRetry}
          disabled={!canRetry}
        >
          <RefreshCw className="h-4 w-4" />
        </IconButton>
        <IconButton
          label={pinned ? "取消固定" : "固定窗口"}
          size="sm"
          pressed={pinned}
          onClick={onTogglePin}
        >
          {pinned ? (
            <PinOff className="h-4 w-4 text-accent" />
          ) : (
            <Pin className="h-4 w-4" />
          )}
        </IconButton>
        <IconButton
          label="打开设置"
          size="sm"
          onClick={onOpenSettings}
        >
          <Settings className="h-4 w-4" />
        </IconButton>
        <IconButton
          label="关闭"
          size="sm"
          onClick={onClose}
        >
          <X className="h-4 w-4" />
        </IconButton>
      </div>
    </div>
  );
}
