import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";

interface LoadingStateProps {
  message?: string;
  /** 超过该秒数后展示"已等待 N 秒"提示，默认 5 */
  slowThresholdSeconds?: number;
}

/**
 * 加载状态
 * 阶段9：增加"已等待 N 秒"提示，让用户知道仍在等待而非卡死
 */
export function LoadingState({
  message = "正在翻译…",
  slowThresholdSeconds = 5,
}: LoadingStateProps) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    const startedAt = Date.now();
    const timer = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startedAt) / 1000));
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  const showSlowHint = elapsed >= slowThresholdSeconds;

  return (
    <div className="flex flex-col items-center justify-center gap-2 py-8 text-ink-muted">
      <Loader2 className="h-5 w-5 animate-spin text-accent" />
      <p className="text-sm">{message}</p>
      {showSlowHint ? (
        <p className="text-xs text-ink-muted">
          已等待 {elapsed} 秒，如长时间无响应请检查网络与 API 配置
        </p>
      ) : null}
    </div>
  );
}
