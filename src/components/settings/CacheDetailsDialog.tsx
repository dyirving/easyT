import { X } from "lucide-react";
import { Spinner, Button, Dialog, IconButton } from "@/components/ui";
import { ConfirmDialog, StatusBanner } from "@/components/patterns";
import { useCacheDetailsController } from "./useCacheDetailsController";
import type { CacheStats, PersistentCacheState } from "@/types";

export function CacheDetailsDialog({ open, onClose, onCacheCleared = () => {} }: { open: boolean; onClose: () => void; onCacheCleared?: () => void }) {
  const controller = useCacheDetailsController(open, onCacheCleared);
  const state = controller.details.phase === "ready" ? controller.details.stats.state : "ready";
  return <>
    <Dialog open={open && !controller.confirming} onOpenChange={(next) => !next && onClose()} title="翻译缓存详情">
      <div className="flex justify-end"><IconButton label="关闭缓存详情" size="sm" onClick={onClose}><X className="h-4 w-4" /></IconButton></div>
      {controller.details.phase === "loading" ? <p className="flex items-center gap-2 py-6 text-sm text-ink-muted"><Spinner />正在读取缓存详情…</p> : null}
      {controller.details.phase === "error" ? <StatusBanner tone="danger" announcement="assertive" description={controller.details.message} /> : null}
      {controller.details.phase === "ready" ? <CacheDetails stats={controller.details.stats} clearing={controller.clearing} onClear={controller.requestClear} /> : null}
    </Dialog>
    <ConfirmDialog open={Boolean(controller.confirming)} title={`确认${actionLabel(controller.confirming ?? state)}`} description="这会删除本机翻译缓存，不会删除设置、Qwen 登录状态或网页对话记录。" confirmLabel={controller.confirming === "degraded" ? "确认重建" : "确认清除"} cancelLabel="取消" tone="danger" pending={controller.clearing} onCancel={() => controller.setConfirming(null)} onConfirm={() => void controller.confirmClear()} />
  </>;
}
function CacheDetails({ stats, clearing, onClear }: { stats: CacheStats; clearing: boolean; onClear: () => void }) { return <div className="space-y-3 text-sm"><p className={stats.state === "ready" ? "text-success" : "text-warning"}>{stats.state === "ready" ? "持久化缓存可用" : stats.state === "degraded" ? "持久化缓存不可用" : "持久化缓存正在初始化"}</p><dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-lg border border-line px-3 py-3"><dt className="text-ink-muted">L2 条目</dt><dd className="text-right text-ink">{stats.entryCount} 条</dd><dt className="text-ink-muted">磁盘占用</dt><dd className="text-right text-ink">{formatMiB(stats.diskBytes)} / {formatMiB(stats.maxDiskBytes)}</dd><dt className="text-ink-muted">命中率</dt><dd className="text-right text-ink">{stats.hitRate === null ? "—" : `${(stats.hitRate * 100).toFixed(1)}%`}</dd></dl><div><p className="text-xs text-ink-muted">缓存路径</p><p className="mt-1 break-all rounded bg-surface-soft px-2 py-1.5 text-xs text-ink">{stats.cachePath}</p></div><StatusBanner tone="warning" description="译文以明文保存在本机缓存中；原文不会写入缓存数据库。请仅在可信设备上使用。" /><Button variant="outline" onClick={onClear} loading={clearing} loadingLabel="正在清除">{clearing ? "正在清除…" : actionLabel(stats.state)}</Button></div>; }
function actionLabel(state: PersistentCacheState) { return state === "degraded" ? "重建持久化缓存" : "清除翻译缓存"; }
function formatMiB(bytes: number) { return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`; }
