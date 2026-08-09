import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  Eye,
  EyeOff,
  Loader2,
  CheckCircle2,
  AlertTriangle,
  LogIn,
  LogOut,
  Database,
} from "lucide-react";
import { useSettingsStore } from "@/stores/settingsStore";
import {
  getConfig,
  saveConfig as saveConfigCommand,
  testApiConnection,
  beginWebLogin,
  getWebLoginStatus,
  logoutWebAccount,
  toCommandError,
} from "@/services/tauriCommands";
import { Field } from "@/components/ui/Field";
import { Input } from "@/components/ui/Input";
import { Button } from "@/components/ui/Button";
import { Switch } from "@/components/ui/Switch";
import { ShortcutInput } from "@/components/ShortcutInput";
import { CacheDetailsDialog } from "@/components/CacheDetailsDialog";
import {
  TARGET_LANGUAGES,
  MODEL_PROVIDERS,
  QWEN_ALLOWED_MODELS,
  getProviderPreset,
  type BackendMode,
  type ModelProvider,
  type QwenSessionStatus,
  type WebProviderKind,
} from "@/types";

interface SettingsPageProps {
  onBack: () => void;
  onCacheCleared?: () => void;
}

type TestStatus = "idle" | "testing" | "ok" | "fail";

/** 登录轮询间隔：仅 SettingsPage 可见且状态为 loggingIn 时启动 */
const LOGIN_POLL_INTERVAL_MS = 1000;

export function SettingsPage({ onBack, onCacheCleared }: SettingsPageProps) {
  const { config, setConfig, loadConfig, markSaved } = useSettingsStore();
  const [showKey, setShowKey] = useState(false);
  const [testing, setTesting] = useState<TestStatus>("idle");
  const [testMessage, setTestMessage] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [saveError, setSaveError] = useState(false);
  const [loadingConfig, setLoadingConfig] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [cacheDetailsOpen, setCacheDetailsOpen] = useState(false);

  // WebGateway 登录状态
  const [loginStatus, setLoginStatus] = useState<QwenSessionStatus | null>(
    null,
  );
  const [loginActionPending, setLoginActionPending] = useState(false);
  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const isWebGateway = config.backendMode === "webGateway";

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

  // 进入 WebGateway 模式时立即获取一次登录状态
  useEffect(() => {
    if (!isWebGateway) {
      setLoginStatus(null);
      return;
    }
    let cancelled = false;
    const provider: WebProviderKind = config.webGateway.provider;
    getWebLoginStatus(provider)
      .then((status) => {
        if (cancelled) return;
        setLoginStatus(status);
      })
      .catch((e) => {
        if (cancelled) return;
        logWarn("获取 Qwen 登录状态失败", e);
      });
    return () => {
      cancelled = true;
    };
  }, [isWebGateway, config.webGateway.provider]);

  // 登录轮询：仅在 WebGateway 模式且状态为 loggingIn 时启动
  useEffect(() => {
    if (!isWebGateway) return;
    if (loginStatus?.phase !== "loggingIn") return;

    let cancelled = false;
    const provider: WebProviderKind = config.webGateway.provider;

    const poll = async () => {
      if (cancelled) return;
      try {
        const status = await getWebLoginStatus(provider);
        if (cancelled) return;
        setLoginStatus(status);
        if (status.phase === "loggingIn") {
          pollTimerRef.current = setTimeout(poll, LOGIN_POLL_INTERVAL_MS);
        }
      } catch (e) {
        if (cancelled) return;
        logWarn("轮询 Qwen 登录状态失败", e);
        // 出错时退避后继续轮询
        pollTimerRef.current = setTimeout(poll, LOGIN_POLL_INTERVAL_MS * 2);
      }
    };

    pollTimerRef.current = setTimeout(poll, LOGIN_POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      if (pollTimerRef.current) {
        clearTimeout(pollTimerRef.current);
        pollTimerRef.current = null;
      }
    };
  }, [isWebGateway, loginStatus?.phase, config.webGateway.provider]);

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

  const handleBeginLogin = async () => {
    if (loginActionPending) return;
    setLoginActionPending(true);
    try {
      const status = await beginWebLogin(config.webGateway.provider);
      setLoginStatus(status);
    } catch (e) {
      const err = toCommandError(e);
      logWarn("启动 Qwen 登录失败", err.message);
    } finally {
      setLoginActionPending(false);
    }
  };

  const handleLogout = async () => {
    if (loginActionPending) return;
    // 显式 destructive 操作：UI 二次确认
    const confirmed = window.confirm(
      "确定退出 Qwen 登录？\n\n退出后：\n- 删除本地保存的登录凭证\n- 清除 Qwen 浏览器 profile\n- 翻译将不可用，直到重新登录",
    );
    if (!confirmed) return;
    setLoginActionPending(true);
    try {
      const status = await logoutWebAccount(config.webGateway.provider);
      setLoginStatus(status);
    } catch (e) {
      const err = toCommandError(e);
      logWarn("退出 Qwen 登录失败", err.message);
    } finally {
      setLoginActionPending(false);
    }
  };

  const handleBackendModeChange = (mode: BackendMode) => {
    setConfig({ backendMode: mode });
    // 切换后端时重置测试状态
    setTesting("idle");
    setTestMessage(null);
  };

  // 切换供应商：内置供应商自动填入 Base URL，并把模型重置为该供应商首个模型；
  // 自定义供应商则清空 Base URL 与模型，交由用户填写。
  // 同时从 apiKeys 存储池恢复该供应商的 API Key（未填写则为空）。
  const handleProviderChange = (provider: ModelProvider) => {
    const preset = getProviderPreset(provider);
    if (!preset) return;
    const restoredKey = config.apiKeys[provider] ?? "";
    if (provider === "custom") {
      setConfig({
        provider,
        baseUrl: "",
        model: "",
        apiKey: restoredKey,
      });
    } else {
      const firstModel = preset.models[0]?.value ?? "";
      setConfig({
        provider,
        baseUrl: preset.baseUrl,
        model: firstModel,
        apiKey: restoredKey,
      });
    }
  };

  // 编辑当前供应商的 API Key：同步写入 apiKey 与 apiKeys[provider]
  const handleApiKeyChange = (value: string) => {
    setConfig({
      apiKey: value,
      apiKeys: { ...config.apiKeys, [config.provider]: value },
    });
  };

  const isCustom = config.provider === "custom";
  const currentPreset = getProviderPreset(config.provider);

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

          {/* 翻译后端选择 */}
          <Field
            label="翻译后端"
            htmlFor="backendMode"
            hint="Official API 使用付费接口；WebGateway 为实验功能"
          >
            <select
              id="backendMode"
              value={config.backendMode}
              onChange={(e) =>
                handleBackendModeChange(e.target.value as BackendMode)
              }
              className="input"
            >
              <option value="officialApi">Official API（付费）</option>
              <option value="webGateway">Qwen 网页实验模式</option>
            </select>
          </Field>

          {isWebGateway ? (
            <WebGatewayPanel
              config={config}
              setConfig={setConfig}
              loginStatus={loginStatus}
              loginActionPending={loginActionPending}
              onBeginLogin={handleBeginLogin}
              onLogout={handleLogout}
            />
          ) : (
            <OfficialApiPanel
              config={config}
              setConfig={setConfig}
              showKey={showKey}
              setShowKey={setShowKey}
              isCustom={isCustom}
              currentPreset={currentPreset}
              onProviderChange={handleProviderChange}
              onApiKeyChange={handleApiKeyChange}
            />
          )}

          <Field
            label="全局快捷键"
            htmlFor="shortcut"
            hint="有选区时翻译，无选区时显示翻译窗口；默认 Ctrl+T，可能与其他软件冲突"
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
                <p className="text-sm font-medium text-ink">启用思考模式</p>
                <p className="text-xs text-ink-muted">
                  关闭（默认）省 token、更快；开启可在复杂语境下提升译文质量
                </p>
              </div>
              <Switch
                checked={config.enableThinking}
                onCheckedChange={(v) => setConfig({ enableThinking: v })}
                aria-label="启用思考模式"
              />
            </div>
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm font-medium text-ink">流式输出</p>
                <p className="text-xs text-ink-muted">
                  生成时逐步显示译文；Official API 端点需支持标准流式响应
                </p>
              </div>
              <Switch
                checked={config.streamOutput}
                onCheckedChange={(v) => setConfig({ streamOutput: v })}
                aria-label="流式输出"
              />
            </div>
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

          <div className="flex items-center justify-between gap-4 rounded-lg border border-line bg-surface-soft/40 px-3 py-3">
            <div>
              <p className="text-sm font-medium text-ink">翻译缓存</p>
              <p className="text-xs text-ink-muted">
                查看本机缓存的条目、磁盘占用、命中率和保存位置
              </p>
            </div>
            <Button variant="outline" onClick={() => setCacheDetailsOpen(true)}>
              <Database className="h-4 w-4" />
              查看缓存详情
            </Button>
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
      <CacheDetailsDialog
        open={cacheDetailsOpen}
        onClose={() => setCacheDetailsOpen(false)}
        onCacheCleared={onCacheCleared}
      />
    </div>
  );
}

// ===== Official API 面板 =====

interface OfficialApiPanelProps {
  config: import("@/types").AppConfig;
  setConfig: (patch: Partial<import("@/types").AppConfig>) => void;
  showKey: boolean;
  setShowKey: (v: boolean | ((prev: boolean) => boolean)) => void;
  isCustom: boolean;
  currentPreset: ReturnType<typeof getProviderPreset>;
  onProviderChange: (provider: ModelProvider) => void;
  onApiKeyChange: (value: string) => void;
}

function OfficialApiPanel({
  config,
  setConfig,
  showKey,
  setShowKey,
  isCustom,
  currentPreset,
  onProviderChange,
  onApiKeyChange,
}: OfficialApiPanelProps) {
  return (
    <>
      <Field
        label="模型供应商"
        htmlFor="provider"
        hint="选择内置供应商后无需填写 Base URL 与模型名称"
      >
        <select
          id="provider"
          value={config.provider}
          onChange={(e) => onProviderChange(e.target.value as ModelProvider)}
          className="input"
        >
          {MODEL_PROVIDERS.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </Field>

      {/* 模型名称：内置供应商用下拉选择，自定义用文本输入 */}
      <Field
        label="模型名称"
        htmlFor="model"
        hint={
          isCustom
            ? "兼容 OpenAI Chat Completions 的模型 ID"
            : "从该供应商的内置模型中选择"
        }
      >
        {isCustom ? (
          <Input
            id="model"
            value={config.model}
            onChange={(e) => setConfig({ model: e.target.value })}
            placeholder="gpt-4o-mini"
          />
        ) : (
          <select
            id="model"
            value={config.model}
            onChange={(e) => setConfig({ model: e.target.value })}
            className="input"
          >
            {currentPreset?.models.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </select>
        )}
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
            onChange={(e) => onApiKeyChange(e.target.value)}
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

      {/* Base URL：仅自定义供应商时显示并允许编辑 */}
      {isCustom ? (
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
      ) : null}
    </>
  );
}

// ===== WebGateway 面板 =====

interface WebGatewayPanelProps {
  config: import("@/types").AppConfig;
  setConfig: (patch: Partial<import("@/types").AppConfig>) => void;
  loginStatus: QwenSessionStatus | null;
  loginActionPending: boolean;
  onBeginLogin: () => void;
  onLogout: () => void;
}

function WebGatewayPanel({
  config,
  setConfig,
  loginStatus,
  loginActionPending,
  onBeginLogin,
  onLogout,
}: WebGatewayPanelProps) {
  const phase = loginStatus?.phase ?? "loggedOut";
  const phaseLabel = getPhaseLabel(phase);
  const phaseTone = getPhaseTone(phase);
  const canLogin = phase === "loggedOut";
  const canReLogin = phase === "ready" || phase === "expired";
  const canLogout = phase === "ready" || phase === "expired";

  return (
    <div className="space-y-4 rounded-lg border border-warning/40 bg-warning/5 px-3 py-3">
      {/* 实验功能警告 */}
      <div className="flex items-start gap-2">
        <AlertTriangle className="mt-0.5 h-4 w-4 flex-shrink-0 text-warning" />
        <div className="text-xs text-ink-soft">
          <p className="font-medium text-warning">实验功能</p>
          <p className="mt-0.5">
            Qwen 网页模式使用网页登录态调用 Qwen 私有接口。可能因 Qwen
            协议变化而失效；不会自动回退到付费 Official API。登录凭证会以明文保存在
            easyT_Data 目录，请仅在可信设备上使用。
          </p>
        </div>
      </div>

      <Field
        label="Qwen 模型"
        htmlFor="webGatewayModel"
        hint="从允许列表中选取"
      >
        <select
          id="webGatewayModel"
          value={config.webGateway.model}
          onChange={(e) =>
            setConfig({
              webGateway: { ...config.webGateway, model: e.target.value },
            })
          }
          className="input"
        >
          {QWEN_ALLOWED_MODELS.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </select>
      </Field>

      <div className="flex items-center justify-between gap-4 rounded-md border border-line bg-surface px-3 py-2">
        <div>
          <p className="text-sm font-medium text-ink">保存到 Qwen 对话记录</p>
          <p className="text-xs text-ink-muted">
            开启后，翻译原文和结果可能出现在 Qwen 网页端历史中
          </p>
        </div>
        <Switch
          checked={config.webGateway.saveHistory}
          onCheckedChange={(saveHistory) =>
            setConfig({
              webGateway: { ...config.webGateway, saveHistory },
            })
          }
          aria-label="保存到 Qwen 对话记录"
        />
      </div>

      {/* 登录状态展示 */}
      <div className="space-y-2 rounded-md border border-line bg-surface px-3 py-2">
        <div className="flex items-center justify-between">
          <span className="text-xs text-ink-muted">登录状态</span>
          <span className={`text-xs font-medium ${phaseTone}`}>
            {phaseLabel}
          </span>
        </div>
        {loginStatus?.message ? (
          <p className="text-xs text-ink-muted">{loginStatus.message}</p>
        ) : null}
      </div>

      {/* 操作按钮 */}
      <div className="flex flex-wrap items-center gap-2">
        {canLogin ? (
          <Button
            variant="primary"
            onClick={onBeginLogin}
            disabled={loginActionPending}
          >
            {loginActionPending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <LogIn className="h-4 w-4" />
            )}
            登录 Qwen
          </Button>
        ) : null}
        {canReLogin ? (
          <Button
            variant="outline"
            onClick={onBeginLogin}
            disabled={loginActionPending}
          >
            重新登录
          </Button>
        ) : null}
        {canLogout ? (
          <Button
            variant="outline"
            onClick={onLogout}
            disabled={loginActionPending}
          >
            <LogOut className="h-4 w-4" />
            退出登录
          </Button>
        ) : null}
      </div>

      {/* 登录中提示 */}
      {phase === "loggingIn" ? (
        <p className="text-xs text-ink-muted">
          已打开 Qwen 登录窗口，请在窗口中完成登录。登录成功后窗口会自动关闭。
        </p>
      ) : null}
    </div>
  );
}

function getPhaseLabel(phase: QwenSessionStatus["phase"]): string {
  switch (phase) {
    case "loggedOut":
      return "未登录";
    case "loggingIn":
      return "登录中…";
    case "ready":
      return "已登录";
    case "expired":
      return "已过期";
  }
}

function getPhaseTone(phase: QwenSessionStatus["phase"]): string {
  switch (phase) {
    case "loggedOut":
      return "text-ink-muted";
    case "loggingIn":
      return "text-warning";
    case "ready":
      return "text-success";
    case "expired":
      return "text-danger";
  }
}

function logWarn(prefix: string, e: unknown) {
  // 控制台兜底；Rust 端错误已通过 toCommandError 处理
  // eslint-disable-next-line no-console
  console.warn(`[easyT] ${prefix}:`, e);
}
