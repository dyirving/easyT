import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/Button";

interface ErrorStateProps {
  message: string;
  hint?: string;
  onRetry?: () => void;
  onOpenSettings?: () => void;
}

/** 错误状态 */
export function ErrorState({
  message,
  hint,
  onRetry,
  onOpenSettings,
}: ErrorStateProps) {
  return (
    <div className="flex flex-col gap-3 py-5">
      <div className="flex items-start gap-2.5">
        <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-danger" />
        <div className="space-y-1">
          <p className="text-sm font-medium text-danger">{message}</p>
          {hint ? (
            <p className="text-xs text-ink-muted">{hint}</p>
          ) : null}
        </div>
      </div>
      <div className="flex gap-2">
        {onRetry ? (
          <Button variant="outline" size="sm" onClick={onRetry}>
            重试
          </Button>
        ) : null}
        {onOpenSettings ? (
          <Button variant="outline" size="sm" onClick={onOpenSettings}>
            打开设置
          </Button>
        ) : null}
      </div>
    </div>
  );
}
