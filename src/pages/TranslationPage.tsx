import { useEffect, useRef, useState } from "react";
import {
  CacheNotice,
  ErrorState,
  ManualTranslationInput,
  OriginalTextPanel,
  TranslationHeader,
  TranslationHistorySection,
  TranslationPanel,
  TranslationProgress,
  TranslationRecord,
  useTranslationController,
} from "@/components/translation";
import { ConfirmDialog, StatusBanner } from "@/components/patterns";
import { Button, Collapsible, Spinner } from "@/components/ui";

interface TranslationPageProps {
  onOpenSettings: () => void;
  onClose: () => void;
}

export function TranslationPage({
  onOpenSettings,
  onClose,
}: TranslationPageProps) {
  const controller = useTranslationController();
  const {
    config,
    copied,
    errorMessage,
    fromCache,
    friendlyError,
    history,
    historyRows,
    historyWarning,
    isBusy,
    isPartial,
    originalText,
    pinned,
    progressBackend,
    progressPhase,
    progressPhaseStartedTotalElapsedMs,
    progressSequence,
    progressSyncedAtMonotonicMs,
    progressSyncedTotalElapsedMs,
    refreshErrorMessage,
    requestId,
    requestStartedAtMonotonicMs,
    status,
    topPersisted,
    topPersistedBody,
    topTranslatedText,
    translatedText,
    totalElapsedMs,
  } = controller;
  const scrollRef = useRef<HTMLDivElement>(null);
  const observedScrollToken = useRef(history.scrollToTopToken);
  const [originalOpen, setOriginalOpen] = useState(true);
  const [translationOpen, setTranslationOpen] = useState(true);

  useEffect(() => {
    setOriginalOpen(true);
    setTranslationOpen(true);
  }, [requestId, topPersisted?.entryId]);

  useEffect(() => {
    const element = scrollRef.current;
    if (element) element.scrollTop = history.scrollTop;
  }, []);

  useEffect(() => {
    if (observedScrollToken.current === history.scrollToTopToken) return;
    observedScrollToken.current = history.scrollToTopToken;
    const element = scrollRef.current;
    if (element) element.scrollTop = 0;
  }, [history.scrollToTopToken]);

  const activeProgress = {
    phase: progressPhase,
    sequence: progressSequence,
    backend: progressBackend,
    phaseStartedTotalElapsedMs: progressPhaseStartedTotalElapsedMs,
    syncedTotalElapsedMs: progressSyncedTotalElapsedMs,
    syncedAtMonotonicMs: progressSyncedAtMonotonicMs,
    requestStartedAtMonotonicMs,
  };

  const canRetry =
    !isBusy && Boolean(originalText || topPersistedBody?.originalText);
  const canCopy = Boolean(
    topTranslatedText &&
      ((status === "success" && !isPartial) || topPersistedBody),
  );
  const captureFailedWithoutText =
    status === "error" && requestId === null && !originalText;
  const offerManualTranslation =
    history.summaries.length === 0 || captureFailedWithoutText;
  const originalSection = (
    <Collapsible
      open={originalOpen}
      onOpenChange={setOriginalOpen}
      title="原文"
      summary={originalText.slice(0, 160)}
    >
      <OriginalTextPanel text={originalText} bare />
    </Collapsible>
  );

  return (
    <div className="flex h-full flex-col">
      <TranslationHeader
        pinned={pinned}
        onTogglePin={controller.togglePin}
        onOpenSettings={onOpenSettings}
        onClose={onClose}
        onRetry={controller.retry}
        canRetry={canRetry}
        copied={copied}
        onCopy={controller.copyTop}
        canCopy={canCopy}
      />

      <div
        ref={scrollRef}
        onScroll={(event) =>
          history.rememberScrollTop(event.currentTarget.scrollTop)
        }
        className="flex-1 overflow-y-auto px-3 py-3"
      >
        {history.initialization === "loading" ? (
          <div className="flex items-center justify-center gap-2 py-8 text-sm text-ink-muted">
            <Spinner label="正在加载翻译历史" />
            正在加载翻译历史…
          </div>
        ) : (
          <div className="space-y-3">
            {history.initializationWarning ? (
              <StatusBanner
                tone="warning"
                announcement="polite"
                description={history.initializationWarning.message}
              />
            ) : null}
            {history.actionError ? (
              <StatusBanner
                tone="danger"
                announcement="assertive"
                description={history.actionError}
              />
            ) : null}

            {history.manualInputOpen ? (
              <ManualTranslationInput
                open
                value={history.manualInput}
                maxLength={config.maxTextLength}
                disabled={isBusy || history.capturePending}
                onOpenChange={history.setManualInputOpen}
                onValueChange={history.setManualInput}
                onTranslate={() => void controller.translate(history.manualInput)}
              />
            ) : offerManualTranslation ? (
              <Button
                variant="outline"
                size="sm"
                disabled={isBusy || history.capturePending}
                onClick={() => history.setManualInputOpen(true)}
              >
                手动输入翻译
              </Button>
            ) : null}

            {status === "idle" && !topPersisted ? (
              <p className="py-3 text-center text-sm text-ink-muted">
                有选区时按{" "}
                <kbd className="rounded bg-surface-soft px-1.5 py-0.5 text-xs">
                  {config.shortcut}
                </kbd>{" "}
                翻译；无选区时显示翻译窗口
              </p>
            ) : null}

            {topPersisted ? (
              <TranslationRecord
                top
                summary={topPersisted}
                body={topPersistedBody}
                loading={history.loadingEntryIds.includes(topPersisted.entryId)}
                pendingAction={history.pendingActionById[topPersisted.entryId]}
                onCopy={() => void controller.copyEntry(topPersisted.entryId, false)}
                onCopyAll={() => void controller.copyEntry(topPersisted.entryId, true)}
                onRetranslate={() =>
                  void controller.retranslateEntry(topPersisted.entryId)
                }
              />
            ) : null}

            {status === "translating" ? (
              <div className="space-y-3">
                {originalSection}
                <TranslationProgress
                  kind="active"
                  snapshot={activeProgress}
                  compact={false}
                />
              </div>
            ) : null}
            {status === "streaming" ? (
              <div className="space-y-3">
                {originalSection}
                <TranslationPanel text={translatedText} mode="streaming" />
                <TranslationProgress
                  kind="active"
                  snapshot={activeProgress}
                  compact
                />
              </div>
            ) : null}
            {status === "refreshing" ? (
              <div className="space-y-3">
                <OriginalTextPanel text={originalText} />
                {fromCache ? <CacheNotice /> : null}
                <TranslationPanel text={translatedText} mode="complete" />
                <TranslationProgress
                  kind="active"
                  snapshot={activeProgress}
                  compact
                />
              </div>
            ) : null}
            {status === "success" ? (
              <div className="space-y-3">
                {originalSection}
                {fromCache ? <CacheNotice /> : null}
                {refreshErrorMessage ? (
                  <StatusBanner
                    tone="danger"
                    announcement="assertive"
                    description={
                      <>
                        重新翻译失败，当前仍显示此前的本机缓存译文。
                        {refreshErrorMessage}
                      </>
                    }
                  />
                ) : null}
                {historyWarning ? (
                  <StatusBanner
                    tone="warning"
                    announcement="polite"
                    description={historyWarning.message}
                  />
                ) : null}
                <Collapsible
                  open={translationOpen}
                  onOpenChange={setTranslationOpen}
                  title="译文"
                  summary={translatedText.slice(0, 160)}
                  unmountOnClose
                >
                  <TranslationPanel text={translatedText} mode="complete" bare />
                </Collapsible>
                {totalElapsedMs !== null ? (
                  <TranslationProgress
                    kind={refreshErrorMessage ? "failure" : "success"}
                    totalElapsedMs={totalElapsedMs}
                  />
                ) : null}
              </div>
            ) : null}
            {status === "error" ? (
              <div className="space-y-3">
                {originalText ? <OriginalTextPanel text={originalText} /> : null}
                {isPartial && translatedText ? (
                  <TranslationPanel text={translatedText} mode="partial" />
                ) : null}
                {isPartial && totalElapsedMs !== null ? (
                  <TranslationProgress
                    kind="interrupted"
                    totalElapsedMs={totalElapsedMs}
                  />
                ) : null}
                <ErrorState
                  message={
                    friendlyError?.friendlyMessage ??
                    errorMessage ??
                    "翻译失败"
                  }
                  hint={friendlyError?.hint}
                  onRetry={
                    friendlyError?.retryable && originalText
                      ? controller.retry
                      : undefined
                  }
                  onOpenSettings={onOpenSettings}
                />
                {!isPartial && totalElapsedMs !== null ? (
                  <TranslationProgress
                    kind="failure"
                    totalElapsedMs={totalElapsedMs}
                  />
                ) : null}
              </div>
            ) : null}

            <TranslationHistorySection
              count={history.summaries.length}
              limit={history.limit}
              records={historyRows}
              bodiesById={history.bodiesById}
              expandedEntryIds={history.expandedEntryIds}
              loadingEntryIds={history.loadingEntryIds}
              pendingActionById={history.pendingActionById}
              clearDisabled={isBusy || history.capturePending}
              onClear={history.requestClearConfirmation}
              onOpenChange={controller.toggleHistoryEntry}
              onCopy={(entryId) => void controller.copyEntry(entryId, false)}
              onCopyAll={(entryId) => void controller.copyEntry(entryId, true)}
              onRetranslate={(entryId) =>
                void controller.retranslateEntry(entryId)
              }
            />
          </div>
        )}
      </div>

      <ConfirmDialog
        open={history.clearStatus !== "idle"}
        title="清空翻译历史"
        description="确定清空全部翻译历史吗？所有已保存的原文和译文都会删除，此操作不会清除翻译缓存。"
        confirmLabel="清空历史"
        cancelLabel="取消"
        tone="danger"
        pending={history.clearStatus === "pending"}
        onCancel={history.cancelClear}
        onConfirm={() => void controller.confirmClear()}
      />
    </div>
  );
}
