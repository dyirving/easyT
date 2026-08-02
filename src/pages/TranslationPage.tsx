import { useState } from "react";
import { useTranslationStore } from "@/stores/translationStore";
import { useSettingsStore } from "@/stores/settingsStore";
import {
  copyTranslation,
  setWindowPinned,
  toCommandError,
  toFriendlyError,
} from "@/services/tauriCommands";
import { runTranslationRequest } from "@/services/translationRunner";
import { TranslationHeader } from "@/components/TranslationHeader";
import { OriginalTextPanel } from "@/components/OriginalTextPanel";
import { TranslationPanel } from "@/components/TranslationPanel";
import { LoadingState } from "@/components/LoadingState";
import { ErrorState } from "@/components/ErrorState";
import { Button } from "@/components/ui/Button";

interface TranslationPageProps {
  onOpenSettings: () => void;
  onClose: () => void;
}

/**
 * 翻译页：组合各组件并渲染等待、生成中、成功和错误状态。
 */
export function TranslationPage({ onOpenSettings, onClose }: TranslationPageProps) {
  const {
    originalText,
    translatedText,
    status,
    errorMessage,
    errorKind,
    isPartial,
    pinned,
    startRequest,
    failRequest,
    togglePinned,
  } = useTranslationStore();
  const { config } = useSettingsStore();

  const [copied, setCopied] = useState(false);
  const [manualInput, setManualInput] = useState(
    "Large language models are trained on massive text corpora."
  );

  const isBusy =
    status === "translating" || status === "capturing" || status === "streaming";

  // 触发一次翻译
  // config 由 Rust 端从 AppState 读取，前端只校验本地配置中的 maxTextLength 用于预拦截
  const handleTranslate = async (text: string) => {
    const requestId = startRequest(text);

    if (!text.trim()) {
      failRequest(
        requestId,
        "未检测到选中文本，请在其他应用中选中英文后再按快捷键。",
        "NoSelectedText",
      );
      return;
    }
    if (text.length > config.maxTextLength) {
      failRequest(
        requestId,
        `文本过长（${text.length} 字符），已超过配置上限 ${config.maxTextLength}。`,
        "TextTooLong",
        text,
      );
      return;
    }

    await runTranslationRequest(requestId, text, { ...config });
  };

  const handleRetry = () => {
    if (originalText) void handleTranslate(originalText);
  };

  // 切换固定状态：同步到 store 与 Rust 端
  // Rust 端在阶段8会根据此状态决定是否重新定位窗口
  const handleTogglePin = () => {
    const next = !pinned;
    togglePinned();
    setWindowPinned(next).catch((e) => {
      const err = toCommandError(e);
      console.warn("[easyT] set_window_pinned 失败:", err.message);
    });
  };

  const handleCopy = async () => {
    if (!translatedText) return;
    try {
      // 通过 Rust 端命令写入剪贴板（统一通过 platform::current::write_clipboard_text）
      await copyTranslation(translatedText);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      // 失败回退到 Web Clipboard API
      const err = toCommandError(e);
      console.warn("[easyT] copy_translation 失败:", err.message);
      try {
        await navigator.clipboard.writeText(translatedText);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      } catch {
        /* 忽略 */
      }
    }
  };

  // 阶段9：根据 errorKind 查表得到友好文案与可重试标识
  const friendlyError = (() => {
    if (status !== "error") return null;
    return toFriendlyError({
      kind: errorKind ?? "Internal",
      message: errorMessage ?? "",
    });
  })();

  return (
    <div className="flex h-full flex-col">
      <TranslationHeader
        pinned={pinned}
        onTogglePin={handleTogglePin}
        onOpenSettings={onOpenSettings}
        onClose={onClose}
        onRetry={handleRetry}
        canRetry={!isBusy && !!originalText}
        copied={copied}
        onCopy={handleCopy}
        canCopy={!!translatedText && status === "success" && !isPartial}
      />

      <div className="flex-1 overflow-y-auto px-3 py-3">
        {status === "idle" ? (
          <div className="flex flex-col items-stretch justify-center gap-3 py-6">
            <p className="text-center text-sm text-ink-muted">
              选中英文文本后按 <kbd className="rounded bg-surface-soft px-1.5 py-0.5 text-xs">Ctrl+T</kbd> 开始翻译
            </p>

            {/* 手动输入区：用于在尚未接入选中文本捕获前验证翻译链路 */}
            <div className="rounded-lg border border-line bg-surface-soft/40 p-3">
              <div className="mb-1.5 text-xs font-medium text-ink-soft">
                手动输入文本测试翻译
              </div>
              <textarea
                value={manualInput}
                onChange={(e) => setManualInput(e.target.value)}
                placeholder="例如：Large language models are trained on massive text corpora."
                rows={3}
                className="input resize-none text-sm leading-relaxed"
              />
              <div className="mt-2 flex items-center justify-between">
                <span className="text-xs text-ink-muted">
                  {manualInput.length} / {config.maxTextLength}
                </span>
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() => void handleTranslate(manualInput)}
                  disabled={!manualInput.trim() || isBusy}
                >
                  翻译
                </Button>
              </div>
            </div>
          </div>
        ) : null}

        {(status === "capturing" || status === "translating") && originalText ? (
          <div className="space-y-3">
            <OriginalTextPanel text={originalText} />
            <LoadingState message={status === "capturing" ? "正在获取选中文本…" : "正在翻译…"} />
          </div>
        ) : null}

        {status === "streaming" ? (
          <div className="space-y-3">
            <OriginalTextPanel text={originalText} />
            <TranslationPanel text={translatedText} mode="streaming" />
          </div>
        ) : null}

        {(status === "capturing" || status === "translating") && !originalText ? (
          <LoadingState message={status === "capturing" ? "正在获取选中文本…" : "正在翻译…"} />
        ) : null}

        {status === "success" ? (
          <div className="space-y-3">
            <OriginalTextPanel text={originalText} />
            <TranslationPanel text={translatedText} mode="complete" />
          </div>
        ) : null}

        {status === "error" ? (
          <div className="space-y-3">
            {originalText ? <OriginalTextPanel text={originalText} /> : null}
            {isPartial && translatedText ? (
              <TranslationPanel text={translatedText} mode="partial" />
            ) : null}
            <ErrorState
              message={friendlyError?.friendlyMessage ?? errorMessage ?? "翻译失败"}
              hint={friendlyError?.hint}
              onRetry={friendlyError?.retryable ? handleRetry : undefined}
              onOpenSettings={onOpenSettings}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}
