import {
  AlertTriangle,
  ArrowDown,
  ArrowUp,
  LogIn,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";
import { useRef, useState } from "react";
import { StatusBanner } from "@/components/patterns";
import {
  Button,
  Dialog,
  FormField,
  IconButton,
  Input,
  Select,
  Switch,
} from "@/components/ui";
import {
  QWEN_ALLOWED_MODELS,
  type AppConfig,
  type QwenAccountDisplayStatus,
  type QwenAccountPoolSnapshot,
} from "@/types";
import { SettingsRow } from "./SettingsRow";

interface DestructiveAccountIntent {
  accountId: string;
  displayName: string;
  kind: "logout" | "delete";
}

interface WebGatewayPanelProps {
  config: AppConfig;
  setConfig: (patch: Partial<AppConfig>) => void;
  accountPool: QwenAccountPoolSnapshot | null;
  pending: boolean;
  error: string | null;
  onCreateAccount: (displayName: string) => Promise<void>;
  onBeginAccountLogin: (accountId: string) => Promise<void>;
  onRenameAccount: (accountId: string, displayName: string) => Promise<void>;
  onSetAccountEnabled: (accountId: string, enabled: boolean) => Promise<void>;
  onMoveAccount: (accountId: string, direction: "up" | "down") => Promise<void>;
  onTestAccount: (accountId: string) => Promise<void>;
  onRequestDestructiveAction: (intent: DestructiveAccountIntent) => void;
}

const accountStatusLabels: Record<QwenAccountDisplayStatus, string> = {
  disabled: "已停用",
  loggingIn: "登录中…",
  loggedOut: "未登录",
  expired: "登录已过期",
  busy: "使用中",
  coolingDown: "冷却中",
  pendingVerification: "待验证",
  available: "可用",
};

export function WebGatewayPanel({
  config,
  setConfig,
  accountPool,
  pending,
  error,
  onCreateAccount,
  onBeginAccountLogin,
  onRenameAccount,
  onSetAccountEnabled,
  onMoveAccount,
  onTestAccount,
  onRequestDestructiveAction,
}: WebGatewayPanelProps) {
  const [addOpen, setAddOpen] = useState(false);
  const [renameAccountId, setRenameAccountId] = useState<string | null>(null);
  const [displayName, setDisplayName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const canAdd = !accountPool || accountPool.accounts.length < accountPool.maximumAccounts;
  const accountToRename = accountPool?.accounts.find(
    (account) => account.accountId === renameAccountId,
  );

  const createAccount = async () => {
    if (!displayName.trim()) return;
    await onCreateAccount(displayName);
    setDisplayName("");
    setAddOpen(false);
  };

  const renameAccount = async () => {
    if (!accountToRename || !displayName.trim()) return;
    await onRenameAccount(accountToRename.accountId, displayName);
    setDisplayName("");
    setRenameAccountId(null);
  };

  return (
    <section className="space-y-4 rounded-lg border border-warning/40 bg-warning/5 px-3 py-3">
      <div className="flex items-center gap-2 text-warning">
        <AlertTriangle aria-hidden="true" className="h-4 w-4 shrink-0" />
        <span className="text-xs font-medium">实验功能</span>
      </div>
      <FormField label="Qwen 模型" hint="从允许列表中选取">
        <Select
          value={config.webGateway.model}
          onChange={(event) =>
            setConfig({
              webGateway: { ...config.webGateway, model: event.target.value },
            })
          }
        >
          {QWEN_ALLOWED_MODELS.map((item) => (
            <option key={item.value} value={item.value}>{item.label}</option>
          ))}
        </Select>
      </FormField>
      <SettingsRow
        title="保存到 Qwen 对话记录"
        description="开启后，翻译原文和结果可能出现在 Qwen 网页端历史中"
        control={<Switch checked={config.webGateway.saveHistory} onCheckedChange={(saveHistory) => setConfig({ webGateway: { ...config.webGateway, saveHistory } })} aria-label="保存到 Qwen 对话记录" />}
      />
      {accountPool ? (
        <div className="space-y-2 border-t border-warning/30 pt-3">
          <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
            <span className="font-medium text-ink-soft">Qwen 账号</span>
            <div className="flex items-center gap-2">
              <span className="text-ink-muted">{accountPool.accounts.length} / {accountPool.maximumAccounts}</span>
              <Button size="sm" variant="outline" onClick={() => setAddOpen(true)} disabled={!canAdd || pending}><Plus />添加账号</Button>
            </div>
          </div>
          {accountPool.warning ? <StatusBanner tone="warning" announcement="polite" description={`${accountPool.warning.message} [${accountPool.warning.code}]`} /> : null}
          {error ? <StatusBanner tone="danger" announcement="polite" description={error} /> : null}
          {accountPool.accounts.length > 0 ? (
            <ul className="space-y-1" aria-label="Qwen 账号池">
              {accountPool.accounts.map((account) => (
                <li key={account.accountId} className="space-y-2 border-b border-line/70 py-2 text-xs">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="min-w-0 break-words text-ink">{account.displayName}</span>
                    <span className="shrink-0 text-ink-soft">{accountStatusLabels[account.status]}{account.cooldownRemainingSeconds !== undefined ? ` ${account.cooldownRemainingSeconds}s` : ""}</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <Switch checked={account.enabled} disabled={!account.actions.canToggleEnabled || pending} onCheckedChange={(enabled) => void onSetAccountEnabled(account.accountId, enabled)} aria-label={`启用 ${account.displayName}`} />
                    <IconButton label="重命名账号" variant="ghost" size="sm" disabled={!account.actions.canRename || pending} onClick={() => { setRenameAccountId(account.accountId); setDisplayName(account.displayName); }}><Pencil /></IconButton>
                    <IconButton label="上移账号" variant="ghost" size="sm" disabled={!account.actions.canMoveUp || pending} onClick={() => void onMoveAccount(account.accountId, "up")}><ArrowUp /></IconButton>
                    <IconButton label="下移账号" variant="ghost" size="sm" disabled={!account.actions.canMoveDown || pending} onClick={() => void onMoveAccount(account.accountId, "down")}><ArrowDown /></IconButton>
                  </div>
                  <div className="flex flex-wrap items-center gap-2">
                    {account.actions.canLogin ? <Button size="sm" variant={account.status === "loggedOut" ? "primary" : "outline"} onClick={() => void onBeginAccountLogin(account.accountId)} disabled={pending}><LogIn />{account.status === "loggedOut" ? "登录" : "重新登录"}</Button> : null}
                    {account.actions.canTest ? <Button size="sm" variant="outline" onClick={() => void onTestAccount(account.accountId)} disabled={pending}>测试</Button> : null}
                    {account.actions.canLogout ? <Button size="sm" variant="outline" onClick={() => onRequestDestructiveAction({ accountId: account.accountId, displayName: account.displayName, kind: "logout" })} disabled={pending}>退出登录</Button> : null}
                    {account.actions.canDelete ? <IconButton label="删除账号" variant="danger" size="sm" disabled={pending} onClick={() => onRequestDestructiveAction({ accountId: account.accountId, displayName: account.displayName, kind: "delete" })}><Trash2 /></IconButton> : null}
                  </div>
                  {account.message ? <p className="break-words text-ink-muted">{account.message}{account.messageCode ? ` [${account.messageCode}]` : ""}</p> : null}
                </li>
              ))}
            </ul>
          ) : <p className="text-xs text-ink-muted">暂无 Qwen 账号</p>}
        </div>
      ) : null}
      <Dialog open={addOpen} onOpenChange={setAddOpen} title="添加 Qwen 账号" description="名称仅用于本机区分账号。" initialFocusRef={inputRef}>
        <form className="space-y-4" onSubmit={(event) => { event.preventDefault(); void createAccount(); }}>
          <FormField label="账号名称" hint="1 至 40 个字符" required>
            <Input ref={inputRef} value={displayName} onChange={(event) => setDisplayName(event.target.value)} maxLength={40} />
          </FormField>
          <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={() => setAddOpen(false)}>取消</Button><Button type="submit" variant="primary" loading={pending} disabled={!displayName.trim()}>添加并登录</Button></div>
        </form>
      </Dialog>
      <Dialog open={Boolean(accountToRename)} onOpenChange={(open) => { if (!open) setRenameAccountId(null); }} title="重命名 Qwen 账号" description="名称仅用于本机区分账号。" initialFocusRef={inputRef}>
        <form className="space-y-4" onSubmit={(event) => { event.preventDefault(); void renameAccount(); }}>
          <FormField label="账号名称" hint="1 至 40 个字符" required>
            <Input ref={inputRef} value={displayName} onChange={(event) => setDisplayName(event.target.value)} maxLength={40} />
          </FormField>
          <div className="flex justify-end gap-2"><Button type="button" variant="ghost" onClick={() => setRenameAccountId(null)}>取消</Button><Button type="submit" variant="primary" loading={pending} disabled={!displayName.trim()}>保存</Button></div>
        </form>
      </Dialog>
    </section>
  );
}
