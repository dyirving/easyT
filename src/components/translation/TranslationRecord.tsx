import { useEffect, useState } from "react";
import { Button, Collapsible, Spinner } from "@/components/ui";
import type {
  TranslationHistoryEntry,
  TranslationHistorySummary,
} from "@/types";
import { CacheNotice } from "./CacheNotice";
import { OriginalTextPanel } from "./OriginalTextPanel";
import { TranslationPanel } from "./TranslationPanel";
import { TranslationProgress } from "./TranslationProgress";
import { formatHistoryTime } from "./historyFormatting";

interface TranslationRecordContentProps {
  summary: TranslationHistorySummary;
  body?: TranslationHistoryEntry;
  loading?: boolean;
  pendingAction?: "copy" | "copyAll" | "retranslate";
  onCopy(): void;
  onCopyAll(): void;
  onRetranslate(): void;
}

type TranslationRecordProps = TranslationRecordContentProps &
  (
    | { top: true }
    | { top?: false; open: boolean; onOpenChange(open: boolean): void }
  );

function RecordContent({
  summary,
  body,
  loading,
  pendingAction,
  onCopy,
  onCopyAll,
  onRetranslate,
}: TranslationRecordContentProps) {
  const [originalOpen, setOriginalOpen] = useState(true);
  const [translationOpen, setTranslationOpen] = useState(true);
  useEffect(() => {
    setOriginalOpen(true);
    setTranslationOpen(true);
  }, [summary.entryId]);

  if (loading) {
    return (
      <div className="flex items-center gap-2 py-2 text-xs text-ink-muted">
        <Spinner label="正在加载翻译记录正文" />
      </div>
    );
  }
  if (!body) {
    return (
      <p className="py-2 text-xs text-ink-muted">
        正文暂时无法读取，请稍后重试。
      </p>
    );
  }

  return (
    <div className="space-y-2">
      <Collapsible
        open={originalOpen}
        onOpenChange={setOriginalOpen}
        title="原文"
        summary={summary.originalSummary}
      >
        <OriginalTextPanel text={body.originalText} bare />
      </Collapsible>
      {summary.fromCache ? <CacheNotice /> : null}
      <Collapsible
        open={translationOpen}
        onOpenChange={setTranslationOpen}
        title="译文"
        summary={summary.translatedSummary}
        unmountOnClose
      >
        <TranslationPanel text={body.translatedText} mode="complete" bare />
      </Collapsible>
      <div className="flex flex-wrap items-center gap-1.5">
        <Button
          size="sm"
          variant="outline"
          loading={pendingAction === "copy"}
          loadingLabel="正在复制"
          onClick={onCopy}
        >
          复制译文
        </Button>
        <Button
          size="sm"
          variant="outline"
          loading={pendingAction === "copyAll"}
          loadingLabel="正在复制"
          onClick={onCopyAll}
        >
          全部复制
        </Button>
        <Button
          size="sm"
          variant="ghost"
          loading={pendingAction === "retranslate"}
          loadingLabel="正在读取"
          onClick={onRetranslate}
        >
          使用当前设置重新翻译
        </Button>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-ink-muted">
        <span>
          {summary.targetLanguage} · {formatHistoryTime(summary.completedAtUtcMs)}
          {summary.fromCache ? " · 来自缓存" : ""}
        </span>
        <TranslationProgress kind="success" totalElapsedMs={summary.totalElapsedMs} />
      </div>
    </div>
  );
}

export function TranslationRecord(props: TranslationRecordProps) {
  const content = <RecordContent {...props} />;
  if (props.top) return <div className="space-y-2">{content}</div>;
  return (
    <Collapsible
      open={props.open}
      onOpenChange={props.onOpenChange}
      title={formatHistoryTime(props.summary.completedAtUtcMs)}
      summary={
        <span className="space-y-0.5">
          <span className="block truncate">原文：{props.summary.originalSummary}</span>
          <span className="block truncate">译文：{props.summary.translatedSummary}</span>
        </span>
      }
      unmountOnClose
    >
      {content}
    </Collapsible>
  );
}
