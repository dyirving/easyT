import {
  CacheNotice,
  ErrorState,
  LoadingState,
  OriginalTextPanel,
  TranslationHeader,
  TranslationPanel,
  useTranslationController,
} from "@/components/translation";
import { Button, Spinner, Textarea } from "@/components/ui";

interface TranslationPageProps {
  onOpenSettings: () => void;
  onClose: () => void;
}

/** Composes translation-domain UI; stateful actions live in the controller. */
export function TranslationPage({ onOpenSettings, onClose }: TranslationPageProps) {
  const controller = useTranslationController();
  const {
    config,
    copied,
    errorMessage,
    fromCache,
    friendlyError,
    isBusy,
    isPartial,
    manualInput,
    originalText,
    pinned,
    refreshErrorMessage,
    status,
    translatedText,
  } = controller;

  return (
    <div className="flex h-full flex-col">
      <TranslationHeader
        pinned={pinned}
        onTogglePin={controller.togglePin}
        onOpenSettings={onOpenSettings}
        onClose={onClose}
        onRetry={controller.retry}
        canRetry={!isBusy && !!originalText}
        copied={copied}
        onCopy={controller.copy}
        canCopy={!!translatedText && status === "success" && !isPartial}
      />

      <div className="flex-1 overflow-y-auto px-3 py-3">
        {status === "idle" ? (
          <div className="flex flex-col items-stretch justify-center gap-3 py-6">
            <p className="text-center text-sm text-ink-muted">
              有选区时按 <kbd className="rounded bg-surface-soft px-1.5 py-0.5 text-xs">{config.shortcut}</kbd> 翻译；
              无选区时显示翻译窗口
            </p>
            <div className="rounded-lg border border-line bg-surface-soft/40 p-3">
              <div className="mb-1.5 text-xs font-medium text-ink-soft">手动输入文本测试翻译</div>
              <Textarea
                value={manualInput}
                onChange={(event) => controller.setManualInput(event.target.value)}
                placeholder="例如：Large language models are trained on massive text corpora."
                rows={3}
                className="resize-none leading-relaxed"
              />
              <div className="mt-2 flex items-center justify-between">
                <span className="text-xs text-ink-muted">{manualInput.length} / {config.maxTextLength}</span>
                <Button variant="primary" size="sm" onClick={() => void controller.translate(manualInput)} disabled={!manualInput.trim() || isBusy}>
                  翻译
                </Button>
              </div>
            </div>
          </div>
        ) : null}

        {status === "translating" ? <div className="space-y-3"><OriginalTextPanel text={originalText} /><LoadingState message="正在翻译…" /></div> : null}
        {status === "streaming" ? <div className="space-y-3"><OriginalTextPanel text={originalText} /><TranslationPanel text={translatedText} mode="streaming" /></div> : null}
        {status === "refreshing" ? (
          <div className="space-y-3">
            <OriginalTextPanel text={originalText} />
            {fromCache ? <CacheNotice /> : null}
            <TranslationPanel text={translatedText} mode="complete" />
            <p className="flex items-center gap-2 text-xs text-ink-muted"><Spinner size="sm" className="text-accent" />正在重新翻译</p>
          </div>
        ) : null}
        {status === "success" ? (
          <div className="space-y-3">
            <OriginalTextPanel text={originalText} />
            {fromCache ? <CacheNotice /> : null}
            {refreshErrorMessage ? <div role="alert" className="rounded-lg border border-danger/30 bg-danger/5 px-3 py-2 text-xs text-danger">重新翻译失败，当前仍显示此前的本机缓存译文。{refreshErrorMessage}</div> : null}
            <TranslationPanel text={translatedText} mode="complete" />
          </div>
        ) : null}
        {status === "error" ? (
          <div className="space-y-3">
            {originalText ? <OriginalTextPanel text={originalText} /> : null}
            {isPartial && translatedText ? <TranslationPanel text={translatedText} mode="partial" /> : null}
            <ErrorState message={friendlyError?.friendlyMessage ?? errorMessage ?? "翻译失败"} hint={friendlyError?.hint} onRetry={friendlyError?.retryable && originalText ? controller.retry : undefined} onOpenSettings={onOpenSettings} />
          </div>
        ) : null}
      </div>
    </div>
  );
}
