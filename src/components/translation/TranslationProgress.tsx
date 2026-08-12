import { useEffect, useState } from "react";
import { Spinner } from "@/components/ui";
import {
  getProviderPreset,
  type ModelProvider,
  type TranslationPhase,
  type TranslationProgressBackend,
} from "@/types";

export interface ActiveProgressSnapshot {
  phase: TranslationPhase | null;
  sequence: number | null;
  backend: TranslationProgressBackend | null;
  phaseStartedTotalElapsedMs: number | null;
  syncedTotalElapsedMs: number | null;
  syncedAtMonotonicMs: number | null;
  requestStartedAtMonotonicMs: number | null;
}

type TranslationProgressProps =
  | {
      kind: "active";
      snapshot: ActiveProgressSnapshot;
      compact: boolean;
    }
  | {
      kind: "success" | "failure" | "interrupted";
      totalElapsedMs: number;
    };

const PHASE_LABELS: Record<TranslationPhase, string> = {
  checkingCache: "正在查询本机缓存",
  preparingRequest: "正在准备翻译请求",
  connectingBackend: "正在连接翻译服务",
  waitingForContent: "已连接翻译服务，正在等待译文",
  receivingContent: "正在接收译文",
};

function safeDuration(value: number) {
  return Number.isFinite(value) && value > 0 ? value : 0;
}

export function formatActiveDuration(value: number) {
  const milliseconds = safeDuration(value);
  return milliseconds < 1000
    ? "不足 1 秒"
    : `${Math.floor(milliseconds / 1000)} 秒`;
}

export function formatTerminalDuration(value: number) {
  const milliseconds = safeDuration(value);
  if (milliseconds < 100) return "不足 0.1 秒";
  if (milliseconds < 9950) {
    return `${(Math.round(milliseconds / 100) / 10).toFixed(1)} 秒`;
  }
  return `${Math.round(milliseconds / 1000)} 秒`;
}

function backendLabel(backend: TranslationProgressBackend | null) {
  if (!backend) return null;
  if (backend.mode === "webGateway") {
    return backend.provider === "qwen" ? "Qwen 网页实验模式" : null;
  }
  if (backend.provider === "custom") return "Official API · 自定义供应商";
  const preset = getProviderPreset(backend.provider as ModelProvider);
  return preset ? `Official API · ${preset.label}` : "Official API";
}

function ActiveProgress({
  snapshot,
  compact,
}: Extract<TranslationProgressProps, { kind: "active" }>) {
  const [now, setNow] = useState(() => performance.now());

  useEffect(() => {
    setNow(performance.now());
    const timer = setInterval(() => setNow(performance.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  const hasSync =
    snapshot.syncedTotalElapsedMs !== null &&
    snapshot.syncedAtMonotonicMs !== null;
  const totalElapsed = hasSync
    ? safeDuration(snapshot.syncedTotalElapsedMs!) +
      Math.max(0, now - snapshot.syncedAtMonotonicMs!)
    : Math.max(0, now - (snapshot.requestStartedAtMonotonicMs ?? now));
  const phaseElapsed =
    snapshot.phaseStartedTotalElapsedMs === null
      ? null
      : Math.max(0, totalElapsed - snapshot.phaseStartedTotalElapsedMs);
  const phaseLabel = snapshot.phase
    ? PHASE_LABELS[snapshot.phase]
    : "正在处理翻译请求";
  const sourceLabel = snapshot.phase === "checkingCache"
    ? null
    : backendLabel(snapshot.backend);

  return (
    <div
      className={
        compact
          ? "space-y-1 text-xs text-ink-muted"
          : "flex flex-col items-center justify-center gap-2 py-8 text-center text-ink-muted"
      }
    >
      {!compact ? <Spinner size="md" className="text-accent" /> : null}
      <div aria-live="polite" aria-atomic="true" key={snapshot.sequence ?? "fallback"}>
        <p className={compact ? "text-xs text-ink-soft" : "text-sm text-ink-soft"}>
          {phaseLabel}
        </p>
        {sourceLabel ? <p className="text-xs text-ink-muted">{sourceLabel}</p> : null}
      </div>
      <p className="text-xs text-ink-muted">
        {phaseElapsed === null
          ? `总计${formatActiveDuration(totalElapsed)}`
          : `本阶段 ${formatActiveDuration(phaseElapsed)} · 总计 ${formatActiveDuration(totalElapsed)}`}
      </p>
    </div>
  );
}

export function TranslationProgress(props: TranslationProgressProps) {
  if (props.kind === "active") return <ActiveProgress {...props} />;

  const duration = formatTerminalDuration(props.totalElapsedMs);
  const message =
    props.kind === "success"
      ? `本次翻译耗时 ${duration}`
      : props.kind === "interrupted"
        ? `请求在 ${duration} 后中断`
        : `请求在 ${duration} 后失败`;
  return <p className="text-xs text-ink-muted">{message}</p>;
}
