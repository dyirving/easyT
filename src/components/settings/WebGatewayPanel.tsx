import { AlertTriangle, LogIn, LogOut } from "lucide-react";
import { Button, FormField, Select, Switch } from "@/components/ui";
import { QWEN_ALLOWED_MODELS, type AppConfig, type QwenSessionStatus } from "@/types";
import { SettingsRow } from "./SettingsRow";

interface WebGatewayPanelProps {
  config: AppConfig;
  setConfig: (patch: Partial<AppConfig>) => void;
  status: QwenSessionStatus | null;
  pending: boolean;
  onLogin: () => void;
  onLogout: () => void;
}

const phaseLabels: Record<QwenSessionStatus["phase"], string> = {
  loggedOut: "未登录",
  loggingIn: "登录中…",
  ready: "已登录",
  expired: "登录已过期",
};

export function WebGatewayPanel({ config, setConfig, status, pending, onLogin, onLogout }: WebGatewayPanelProps) {
  const phase = status?.phase ?? "loggedOut";
  const canReLogin = phase === "ready" || phase === "expired";
  const canLogout = canReLogin;

  return (
    <section className="space-y-4 rounded-lg border border-warning/40 bg-warning/5 px-3 py-3">
      <div className="flex items-center gap-2 text-warning">
        <AlertTriangle aria-hidden="true" className="h-4 w-4 shrink-0" />
        <span className="text-xs font-medium">实验功能</span>
      </div>
      <FormField label="Qwen 模型" hint="从允许列表中选取">
        <Select value={config.webGateway.model} onChange={(event) => setConfig({ webGateway: { ...config.webGateway, model: event.target.value } })}>
          {QWEN_ALLOWED_MODELS.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}
        </Select>
      </FormField>
      <SettingsRow title="保存到 Qwen 对话记录" description="开启后，翻译原文和结果可能出现在 Qwen 网页端历史中" control={<Switch checked={config.webGateway.saveHistory} onCheckedChange={(saveHistory) => setConfig({ webGateway: { ...config.webGateway, saveHistory } })} aria-label="保存到 Qwen 对话记录" />} />
      <div className="flex items-center justify-between rounded-md border border-line bg-surface px-3 py-2">
        <span className="text-xs text-ink-muted">登录状态</span>
        <span className={phase === "ready" ? "text-xs font-medium text-success" : "text-xs font-medium text-ink-soft"}>{phaseLabels[phase]}</span>
      </div>
      {status?.message ? <p className="text-xs text-ink-muted">{status.message}</p> : null}
      <div className="flex flex-wrap gap-2">
        {phase === "loggedOut" ? <Button variant="primary" onClick={onLogin} loading={pending}><LogIn />登录 Qwen</Button> : null}
        {canReLogin ? <Button variant="outline" onClick={onLogin} disabled={pending}>重新登录</Button> : null}
        {canLogout ? <Button variant="outline" onClick={onLogout} disabled={pending}><LogOut />退出登录</Button> : null}
      </div>
    </section>
  );
}
