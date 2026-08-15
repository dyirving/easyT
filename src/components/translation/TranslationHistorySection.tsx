import { Button } from "@/components/ui";
import type {
  TranslationHistoryEntry,
  TranslationHistorySummary,
} from "@/types";
import { TranslationRecord } from "./TranslationRecord";

interface TranslationHistorySectionProps {
  count: number;
  limit: number;
  records: TranslationHistorySummary[];
  bodiesById: Record<string, TranslationHistoryEntry>;
  expandedEntryIds: string[];
  loadingEntryIds: string[];
  pendingActionById: Record<
    string,
    "copyAll" | "retranslate" | undefined
  >;
  clearDisabled: boolean;
  onClear(): void;
  onOpenChange(entryId: string, open: boolean): void;
  onCopyAll(entryId: string): void;
}

export function TranslationHistorySection({
  count,
  limit,
  records,
  bodiesById,
  expandedEntryIds,
  loadingEntryIds,
  pendingActionById,
  clearDisabled,
  onClear,
  onOpenChange,
  onCopyAll,
}: TranslationHistorySectionProps) {
  if (count === 0) return null;
  return (
    <section className="space-y-2" aria-label="翻译历史">
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-medium text-ink">
          翻译历史 {count} / {limit}
        </h2>
        <Button
          variant="danger"
          size="sm"
          disabled={clearDisabled}
          title={clearDisabled ? "翻译完成后可清空历史记录。" : undefined}
          onClick={onClear}
        >
          清空历史
        </Button>
      </div>
      <div className="space-y-2">
        {records.map((summary) => (
          <TranslationRecord
            key={summary.entryId}
            summary={summary}
            body={bodiesById[summary.entryId]}
            open={expandedEntryIds.includes(summary.entryId)}
            loading={loadingEntryIds.includes(summary.entryId)}
            pendingAction={pendingActionById[summary.entryId]}
            onOpenChange={(open) => onOpenChange(summary.entryId, open)}
            onCopyAll={() => onCopyAll(summary.entryId)}
          />
        ))}
      </div>
    </section>
  );
}
