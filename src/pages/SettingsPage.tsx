import { useEffect, useState } from "react";
import { ArrowLeft, Eye, EyeOff, Loader2, CheckCircle2 } from "lucide-react";
import { useSettingsStore } from "@/stores/settingsStore";
import {
  getConfig,
  saveConfig as saveConfigCommand,
  testApiConnection,
  toCommandError,
} from "@/services/tauriCommands";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { Button } from "@/components/ui/Button";
import { Switch } from "@/components/ui/Switch";
import { ShortcutInput } from "@/components/ShortcutInput";
import { TARGET_LANGUAGES } from "@/types";

interface SettingsPageProps {
  onBack: () => void;
}

type TestStatus = "idle" | "testing" | "ok" | "fail";

export function SettingsPage({ onBack }: SettingsPageProps) {
  const { config, setConfig, loadConfig, markSaved } = useSettingsStore();
  const [showKey, setShowKey] = useState(false);
  const [testing, setTesting] = useState<TestStatus>("idle");
  const [testMessage, setTestMessage] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [saveError, setSaveError] = useState(false);
  const [loadingConfig, setLoadingConfig] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  // 挂载时从 Rust 端加载已持久化的配置
  useEffect(() => {
    let cancelled = false;
    setLoadingConfig(true);
    setLoadError(null);
    getConfig()
      .then((cfg) => {
        if (cancelled) return;
        loadConfig(cfg);
      })
      .catch((e) => {
        if (cancelled) return;
        const err = toCommandError(e);
        setLoadError(err.message);
      })
      .finally(() => {
        if (cancelled) return;
        setLoadingConfig(false);
      });
    return () => {
      cancelled = true;
    };
  }, [loadConfig]);

  const handleSave = async () => {
    setSaving(true);
    setSaveError(false);
    setSaveMessage(null);
    try {
      // 校验已下沉到 Rust 端 validate_config；UI 层只做最简拦截，避免无效请求
      await saveConfigCommand(config);
      markSaved();
      setSaveMessage("设置已保存");
    } catch (e) {
      setSaveError(true);
      const err = toCommandError(e);
      setSaveMessage(err.message);
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting("testing");
    setTestMessage(null);
    try {
      const result = await testApiConnection(config);
      setTesting(result.ok ? "ok" : "fail");
      setTestMessage(result.message);
    } catch (e) {
      setTesting("fail");
      const err = toCommandError(e);
      setTestMessage(err.message);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center justify-between border-b border-line px-3 py-2">
        <button
          onClick={onBack}
          className="flex items-center gap-1.5 text-sm text-ink-soft hover:text-ink"
        >
          <ArrowLeft className="h-4 w-4" />
          返回
        </button>
        <div
          className="flex flex-1 justify-center text-sm font-medium text-ink"
          data-tauri-drag-region
        >
          设置
        </div>
        <span className="w-12" data-tauri-drag-region />
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-4">
        <div className="mx-auto max-w-xl space-y-5">
          {/* 加载提示 */}
          {loadingConfig ? (
            <div className="flex items-center justify-center gap-2 py-4 text-sm text-ink-muted">
              <Loader2 className="h-4 w-4 animate-spin" />
              正在加载配置…
            </div>
          ) : null}
          {loadError ? (
            <div className="rounded-lg border border-danger/40 bg-danger/5 px-3 py-2 text-xs text-danger">
              加载配置失败：{loadError}（将使用默认配置）
            </div>
          ) : null}

          <Field
            label="API Base URL"
            htmlFor="baseUrl"
            hint="兼容 OpenAI Chat Completions 的接口地址"
          >
            <Input
              id="baseUrl"
              value={config.baseUrl}
              onChange={(e) => setConfig({ baseUrl: e.target.value })}
              placeholder="https://api.openai.com/v1"
            />
          </Field>

          <Field
            label="API Key"
            htmlFor="apiKey"
            hint="不会在日志或界面中明文展示"
          >
            <div className="relative">
              <Input
                id="apiKey"
                type={showKey ? "text" : "password"}
                value={config.apiKey}
                onChange={(e) => setConfig({ apiKey: e.target.value })}
                placeholder="sk-..."
                className="pr-10"
              />
              <button
                type="button"
                onClick={() => setShowKey((v) => !v)}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-ink-muted hover:text-ink"
                aria-label={showKey ? "隐藏" : "显示"}
              >
                {showKey ? (
                  <EyeOff className="h-4 w-4" />
                ) : (
                  <Eye className="h-4 w-4" />
                )}
              </button>
            </div>
          </Field>

          <Field label="模型名称" htmlFor="model" hint="例如 gpt-4o-mini">
            <Input
              id="model"
              value={config.model}
              onChange={(e) => setConfig({ model: e.target.value })}
              placeholder="gpt-4o-mini"
            />
          </Field>

          <Field
            label="全局快捷键"
            htmlFor="shortcut"
            hint="默认 Ctrl+T，可能与其他软件冲突"
          >
            <ShortcutInput
              value={config.shortcut}
              onChange={(shortcut) => setConfig({ shortcut })}
            />
          </Field>

          <Field label="目标语言" htmlFor="targetLanguage">
            <select
              id="targetLanguage"
              value={config.targetLanguage}
              onChange={(e) => setConfig({ targetLanguage: e.target.value })}
              className="input"
            >
              {TARGET_LANGUAGES.map((l) => (
                <option key={l.value} value={l.value}>
                  {l.label}
                </option>
              ))}
            </select>
          </Field>

          <div className="grid grid-cols-2 gap-4">
            <Field
              label="请求超时（秒）"
              htmlFor="timeoutSeconds"
              hint="5～300"
            >
              <Input
                id="timeoutSeconds"
                type="number"
                min={5}
                max={300}
                value={config.timeoutSeconds}
                onChange={(e) =>
                  setConfig({
                    timeoutSeconds: Number(e.target.value) || 60,
                  })
                }
              />
            </Field>
            <Field
              label="最大翻译字符数"
              htmlFor="maxTextLength"
              hint="默认 5000"
            >
              <Input
                id="maxTextLength"
                type="number"
                min={100}
                max={20000}
                value={config.maxTextLength}
                onChange={(e) =>
                  setConfig({
                    maxTextLength: Number(e.target.value) || 5000,
                  })
                }
              />
            </Field>
          </div>

          <div className="space-y-3 rounded-lg border border-line bg-surface-soft/40 px-3 py-3">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium text-ink">自动隐藏窗口</p>
                <p className="text-xs text-ink-muted">失去焦点后隐藏临时窗口</p>
              </div>
              <Switch
                checked={config.autoHide}
                onCheckedChange={(v) => setConfig({ autoHide: v })}
                aria-label="自动隐藏窗口"
              />
            </div>
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium text-ink">默认常驻窗口</p>
                <p className="text-xs text-ink-muted">首次触发时即固定窗口</p>
              </div>
              <Switch
                checked={config.pinnedByDefault}
                onCheckedChange={(v) => setConfig({ pinnedByDefault: v })}
                aria-label="默认常驻窗口"
              />
            </div>
          </div>

          <div className="flex items-center gap-2 pt-1">
            <Button variant="primary" onClick={handleSave} disabled={saving}>
              {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : "保存"}
            </Button>
            <Button
              variant="outline"
              onClick={handleTest}
              disabled={testing === "testing"}
            >
              {testing === "testing" ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                "测试连接"
              )}
            </Button>
            {testing === "ok" ? (
              <span className="flex items-center gap-1 text-xs text-success">
                <CheckCircle2 className="h-4 w-4" />
                {testMessage ?? "连接成功"}
              </span>
            ) : null}
            {testing === "fail" ? (
              <span className="text-xs text-danger">
                {testMessage ?? "连接失败"}
              </span>
            ) : null}
            {saveMessage ? (
              <span
                className={
                  saveError ? "text-xs text-danger" : "text-xs text-success"
                }
              >
                {saveMessage}
              </span>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
