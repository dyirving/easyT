import { BookPlus, Pencil, Search, Trash2, X } from "lucide-react";
import {
  Button,
  Dialog,
  FormField,
  IconButton,
  Input,
  Select,
  Spinner,
  Switch,
} from "@/components/ui";
import { ConfirmDialog, StatusBanner } from "@/components/patterns";
import type { TermEntry } from "@/types";
import { TARGET_LANGUAGES } from "@/types";
import { useTermbaseController } from "./useTermbaseController";

type Controller = ReturnType<typeof useTermbaseController>;

export function TermbaseDialog({
  open,
  onClose,
  controller,
}: {
  open: boolean;
  onClose: () => void;
  controller: Controller;
}) {
  return (
    <>
      <Dialog
        open={open && !controller.deleting}
        onOpenChange={(next) => {
          if (!next) {
            controller.closeView();
            onClose();
          }
        }}
        title="术语表"
        description="翻译时优先采用以下指定译法；仅在原文术语匹配且语义适用时生效"
      >
        <div className="flex justify-end">
          <IconButton label="关闭术语表" size="sm" onClick={onClose}>
            <X className="h-4 w-4" />
          </IconButton>
        </div>
        {controller.phase === "loading" ? (
          <p className="flex items-center gap-2 py-6 text-sm text-ink-muted">
            <Spinner />
            正在读取术语表…
          </p>
        ) : null}
        {controller.bannerMessage ? (
          <StatusBanner
            tone="danger"
            announcement="assertive"
            description={controller.bannerMessage}
          />
        ) : null}
        {controller.snapshot?.warning ? (
          <StatusBanner
            tone="warning"
            announcement="polite"
            description={controller.snapshot.warning.message}
          />
        ) : null}
        {controller.snapshot ? (
          controller.editing === null ? (
            <TermList controller={controller} />
          ) : (
            <TermEditor controller={controller} />
          )
        ) : null}
      </Dialog>
      <ConfirmDialog
        open={Boolean(controller.deleting)}
        title="删除术语条目？"
        description={`删除“${termLabel(controller, controller.deleting)}”后，后续翻译将不再使用该指定译法。`}
        confirmLabel="删除"
        cancelLabel="取消"
        tone="danger"
        pending={controller.pending}
        onCancel={controller.cancelDelete}
        onConfirm={() => void controller.confirmDelete()}
      />
    </>
  );
}

function TermList({ controller }: { controller: Controller }) {
  const snapshot = controller.snapshot!;
  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-3 rounded-lg border border-line bg-surface-soft/40 px-3 py-2">
        <div className="min-w-0">
          <p className="text-sm font-medium text-ink">启用术语表</p>
          <p className="text-xs text-ink-muted">
            关闭时翻译不使用任何术语约束（已保存 {snapshot.entries.length} 条）
          </p>
        </div>
        <Switch
          checked={snapshot.enabled}
          onCheckedChange={controller.toggleEnabled}
          disabled={controller.pending}
          aria-label="启用术语表"
        />
      </div>
      <div className="relative">
        <Search
          aria-hidden="true"
          className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-muted"
        />
        <Input
          className="pl-9"
          placeholder="搜索源术语或指定译法"
          value={controller.query}
          onChange={(event) => controller.setQuery(event.target.value)}
          aria-label="搜索术语"
        />
      </div>
      <div
        role="list"
        aria-label="术语条目"
        className="max-h-48 space-y-2 overflow-y-auto pr-1"
      >
        {controller.visibleEntries.length === 0 ? (
          <p className="rounded-lg border border-line bg-surface-soft/40 px-3 py-6 text-center text-sm text-ink-muted">
            {snapshot.entries.length === 0
              ? "还没有术语条目，点击“新建术语”添加"
              : "没有匹配的术语条目"}
          </p>
        ) : (
          controller.visibleEntries.map((entry) => (
            <TermRow
              key={entry.id}
              entry={entry}
              controller={controller}
            />
          ))
        )}
      </div>
      <div className="flex items-center justify-between gap-2">
        <Button variant="outline" size="sm" onClick={controller.beginCreate}>
          <BookPlus className="h-4 w-4" />
          新建术语
        </Button>
        {controller.pageCount > 1 ? (
          <div className="flex items-center gap-2 text-sm text-ink-muted">
            <Button
              variant="outline"
              size="sm"
              disabled={controller.page <= 1 || controller.pending}
              onClick={() => controller.setPage(controller.page - 1)}
            >
              上一页
            </Button>
            <span aria-live="polite">
              第 {controller.page} / {controller.pageCount} 页
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={
                controller.page >= controller.pageCount || controller.pending
              }
              onClick={() => controller.setPage(controller.page + 1)}
            >
              下一页
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function TermRow({
  entry,
  controller,
}: {
  entry: TermEntry;
  controller: Controller;
}) {
  return (
    <div
      role="listitem"
      className="flex items-center gap-3 rounded-lg border border-line bg-surface-soft/40 px-3 py-2"
    >
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-ink">
          {entry.sourceTerm}
          {entry.caseSensitive ? (
            <span className="ml-2 rounded bg-surface-soft px-1.5 py-0.5 text-xs text-ink-muted">
              大小写敏感
            </span>
          ) : null}
        </p>
        <p className="truncate text-sm text-ink-soft">
          → {entry.targetTerm}
          <span className="ml-2 text-xs text-ink-muted">
            {entry.targetLanguage}
          </span>
        </p>
      </div>
      <Switch
        checked={entry.enabled}
        onCheckedChange={() => controller.toggleEntry(entry)}
        disabled={controller.pending}
        aria-label={`启用术语 ${entry.sourceTerm}`}
      />
      <IconButton
        label={`编辑术语 ${entry.sourceTerm}`}
        size="sm"
        onClick={() => controller.beginEdit(entry)}
      >
        <Pencil className="h-4 w-4" />
      </IconButton>
      <IconButton
        label={`删除术语 ${entry.sourceTerm}`}
        size="sm"
        onClick={() => controller.requestDelete(entry)}
      >
        <Trash2 className="h-4 w-4" />
      </IconButton>
    </div>
  );
}

function TermEditor({ controller }: { controller: Controller }) {
  const draft = controller.draft ?? {
    sourceTerm: "",
    targetLanguage: "简体中文",
    targetTerm: "",
    caseSensitive: false,
  };
  const canSave =
    draft.sourceTerm.trim().length > 0 && draft.targetTerm.trim().length > 0;
  return (
    <div className="space-y-3">
      <FormField label="源术语" hint="英文术语；字母、数字与下划线组合按完整单词匹配">
        <Input
          value={draft.sourceTerm}
          onChange={(event) => controller.updateDraft({ sourceTerm: event.target.value })}
          placeholder="例如 neural network"
          aria-label="源术语"
        />
      </FormField>
      <FormField label="目标语言">
        <Select
          value={draft.targetLanguage}
          onChange={(event) =>
            controller.updateDraft({ targetLanguage: event.target.value })
          }
          aria-label="目标语言"
        >
          {TARGET_LANGUAGES.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </Select>
      </FormField>
      <FormField label="指定译法">
        <Input
          value={draft.targetTerm}
          onChange={(event) => controller.updateDraft({ targetTerm: event.target.value })}
          placeholder="例如 神经网络"
          aria-label="指定译法"
        />
      </FormField>
      <div className="flex items-center justify-between gap-3 rounded-lg border border-line bg-surface-soft/40 px-3 py-2">
        <div>
          <p className="text-sm font-medium text-ink">大小写敏感</p>
          <p className="text-xs text-ink-muted">
            关闭时忽略大小写匹配（例如 china 与 China 视为同一术语）
          </p>
        </div>
        <Switch
          checked={draft.caseSensitive}
          onCheckedChange={(caseSensitive) =>
            controller.updateDraft({ caseSensitive })
          }
          aria-label="大小写敏感"
        />
      </div>
      <div className="flex justify-end gap-2">
        <Button
          variant="outline"
          onClick={controller.cancelEdit}
          disabled={controller.pending}
        >
          取消
        </Button>
        <Button
          variant="primary"
          onClick={() => void controller.saveDraft()}
          disabled={!canSave}
          loading={controller.pending}
          loadingLabel="正在保存"
        >
          保存
        </Button>
      </div>
    </div>
  );
}

function termLabel(controller: Controller, id: string | null): string {
  const entry = controller.snapshot?.entries.find((e) => e.id === id);
  return entry ? entry.sourceTerm : "该条目";
}
