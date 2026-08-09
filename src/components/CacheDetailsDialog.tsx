import { useEffect, useRef, useState } from "react";
import { Loader2, X } from "lucide-react";

import { Button } from "@/components/ui/Button";
import {
  getTranslationCacheStats,
  toCommandError,
} from "@/services/tauriCommands";
import type { CacheStats, PersistentCacheState } from "@/types";

interface CacheDetailsDialogProps {
  open: boolean;
  onClose: () => void;
}

type DetailsState =
  | { phase: "loading" }
  | { phase: "ready"; stats: CacheStats }
  | { phase: "error"; message: string };

export function CacheDetailsDialog({ open, onClose }: CacheDetailsDialogProps) {
  const [details, setDetails] = useState<DetailsState>({ phase: "loading" });
  const closeButtonRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const returnFocus = document.activeElement;
    closeButtonRef.current?.focus();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      if (returnFocus instanceof HTMLElement) returnFocus.focus();
    };
  }, [open, onClose]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setDetails({ phase: "loading" });
    getTranslationCacheStats()
      .then((stats) => {
        if (!cancelled) setDetails({ phase: "ready", stats });
      })
      .catch((error) => {
        if (cancelled) return;
        setDetails({
          phase: "error",
          message: toCommandError(error).message,
        });
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 px-4">
      <section
        role="dialog"
        aria-modal="true"
        aria-labelledby="cache-details-title"
        className="w-full max-w-md rounded-xl border border-line bg-surface-panel p-4 shadow-soft"
      >
        <header className="flex items-center justify-between gap-3">
          <h2 id="cache-details-title" className="text-base font-semibold text-ink">
            翻译缓存详情
          </h2>
          <Button
            ref={closeButtonRef}
            size="icon"
            aria-label="关闭缓存详情"
            onClick={onClose}
          >
            <X className="h-4 w-4" />
          </Button>
        </header>

        <div className="mt-4">
          {details.phase === "loading" ? (
            <div className="flex items-center gap-2 py-6 text-sm text-ink-muted">
              <Loader2 className="h-4 w-4 animate-spin" />
              正在读取缓存详情…
            </div>
          ) : null}
          {details.phase === "error" ? (
            <div className="rounded-lg border border-danger/40 bg-danger/5 px-3 py-3 text-sm text-danger">
              读取缓存详情失败：{details.message}
            </div>
          ) : null}
          {details.phase === "ready" ? (
            <CacheDetails stats={details.stats} />
          ) : null}
        </div>
      </section>
    </div>
  );
}

function CacheDetails({ stats }: { stats: CacheStats }) {
  const state = statePresentation(stats.state);
  return (
    <div className="space-y-3 text-sm">
      <p className={state.tone}>{state.label}</p>
      <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-lg border border-line px-3 py-3">
        <dt className="text-ink-muted">L2 条目</dt>
        <dd className="text-right text-ink">{stats.entryCount} 条</dd>
        <dt className="text-ink-muted">磁盘占用</dt>
        <dd className="text-right text-ink">
          {formatMiB(stats.diskBytes)} / {formatMiB(stats.maxDiskBytes)}
        </dd>
        <dt className="text-ink-muted">命中率</dt>
        <dd className="text-right text-ink">
          {stats.hitRate === null ? "—" : `${(stats.hitRate * 100).toFixed(1)}%`}
        </dd>
      </dl>
      <div>
        <p className="text-xs text-ink-muted">缓存路径</p>
        <p className="mt-1 break-all rounded bg-surface-soft px-2 py-1.5 text-xs text-ink">
          {stats.cachePath}
        </p>
      </div>
      <p className="rounded-lg bg-warning/5 px-3 py-2 text-xs text-ink-soft">
        译文以明文保存在本机缓存中；原文不会写入缓存数据库。请仅在可信设备上使用。
      </p>
    </div>
  );
}

function formatMiB(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function statePresentation(
  state: PersistentCacheState,
): { label: string; tone: string } {
  switch (state) {
    case "starting":
      return { label: "持久化缓存正在初始化", tone: "text-warning" };
    case "ready":
      return { label: "持久化缓存可用", tone: "text-success" };
    case "degraded":
      return { label: "持久化缓存不可用", tone: "text-warning" };
    case "stopped":
      return { label: "持久化缓存已停止", tone: "text-warning" };
  }
}
