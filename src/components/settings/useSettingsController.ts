import { useEffect, useState } from "react";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";
import {
  beginWebLogin,
  getConfig,
  getWebLoginStatus,
  logoutWebAccount,
  saveConfig,
  testApiConnection,
  toCommandError,
} from "@/services/tauriCommands";
import {
  getProviderPreset,
  type BackendMode,
  type ModelProvider,
  type QwenSessionStatus,
} from "@/types";

const POLL_MS = 1000;
const VALID_HISTORY_LIMIT = /^(?:[1-9]|1\d|20)$/;

export function useSettingsController() {
  const { config, setConfig, loadConfig } = useSettingsStore();
  const [loadingConfig, setLoadingConfig] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveMessage, setSaveMessage] = useState<string | null>(null);
  const [saveWarning, setSaveWarning] = useState<string | null>(null);
  const [saveError, setSaveError] = useState(false);
  const [testing, setTesting] = useState<"idle" | "testing" | "ok" | "fail">(
    "idle",
  );
  const [testMessage, setTestMessage] = useState<string | null>(null);
  const [loginStatus, setLoginStatus] = useState<QwenSessionStatus | null>(null);
  const [loginActionPending, setLoginActionPending] = useState(false);
  const [logoutIntent, setLogoutIntent] = useState(false);
  const [historyLimitInput, setHistoryLimitInput] = useState(() =>
    String(config.translationHistoryLimit),
  );
  const isWebGateway = config.backendMode === "webGateway";
  const historyLimitError = VALID_HISTORY_LIMIT.test(historyLimitInput)
    ? undefined
    : "请输入 1～20 的整数";

  useEffect(() => {
    let cancelled = false;
    setLoadingConfig(true);
    getConfig()
      .then((value) => {
        if (!cancelled) {
          loadConfig(value);
          setHistoryLimitInput(String(value.translationHistoryLimit ?? 5));
        }
      })
      .catch((error) => {
        if (!cancelled) setLoadError(toCommandError(error).message);
      })
      .finally(() => {
        if (!cancelled) setLoadingConfig(false);
      });
    return () => {
      cancelled = true;
    };
  }, [loadConfig]);

  useEffect(() => {
    if (!isWebGateway) {
      setLoginStatus(null);
      return;
    }
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;
    const provider = config.webGateway.provider;
    const check = async () => {
      try {
        const status = await getWebLoginStatus(provider);
        if (cancelled) return;
        setLoginStatus(status);
      } catch {
        if (!cancelled) retryTimer = setTimeout(check, POLL_MS * 2);
      }
    };
    void check();
    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [isWebGateway, config.webGateway.provider]);

  useEffect(() => {
    if (!isWebGateway || loginStatus?.phase !== "loggingIn") return;
    let cancelled = false;
    let pollTimer: ReturnType<typeof setTimeout> | null = null;
    const provider = config.webGateway.provider;
    const check = async () => {
      try {
        const status = await getWebLoginStatus(provider);
        if (cancelled) return;
        setLoginStatus(status);
        if (status.phase === "loggingIn") {
          pollTimer = setTimeout(check, POLL_MS);
        }
      } catch {
        if (!cancelled) pollTimer = setTimeout(check, POLL_MS * 2);
      }
    };
    pollTimer = setTimeout(check, POLL_MS);
    return () => {
      cancelled = true;
      if (pollTimer) clearTimeout(pollTimer);
    };
  }, [isWebGateway, config.webGateway.provider, loginStatus?.phase]);

  const changeHistoryLimit = (value: string) => {
    setHistoryLimitInput(value);
    if (VALID_HISTORY_LIMIT.test(value)) {
      setConfig({ translationHistoryLimit: Number(value) });
    }
  };

  const save = async () => {
    if (historyLimitError) return;
    setSaving(true);
    setSaveError(false);
    setSaveMessage(null);
    setSaveWarning(null);
    try {
      const result = await saveConfig({
        ...config,
        translationHistoryLimit: Number(historyLimitInput),
      });
      setSaveMessage("设置已保存");
      if (result?.historyUpdate?.status === "applied") {
        useTranslationHistoryStore.getState().applyLimitUpdate(
          result.historyLimit,
          result.historyUpdate.summaries,
          result.historyUpdate.evictedEntryIds,
        );
      } else if (result?.historyUpdate?.status === "warning") {
        useTranslationHistoryStore.setState({ limit: result.historyLimit });
        setSaveWarning(result.historyUpdate.warning.message);
      }
    } catch (error) {
      setSaveError(true);
      setSaveMessage(toCommandError(error).message);
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    setTesting("testing");
    setTestMessage(null);
    try {
      const result = await testApiConnection(config);
      setTesting(result.ok ? "ok" : "fail");
      setTestMessage(result.message);
    } catch (error) {
      setTesting("fail");
      setTestMessage(toCommandError(error).message);
    }
  };

  const beginLogin = async () => {
    if (loginActionPending) return;
    setLoginActionPending(true);
    try {
      const status = await beginWebLogin(config.webGateway.provider);
      setLoginStatus(status);
    } finally {
      setLoginActionPending(false);
    }
  };

  const logout = async () => {
    if (loginActionPending) return;
    setLogoutIntent(false);
    setLoginActionPending(true);
    try {
      setLoginStatus(await logoutWebAccount(config.webGateway.provider));
    } finally {
      setLoginActionPending(false);
    }
  };

  const changeBackend = (backendMode: BackendMode) => {
    setConfig({ backendMode });
    setTesting("idle");
    setTestMessage(null);
  };

  const changeProvider = (provider: ModelProvider) => {
    const preset = getProviderPreset(provider);
    if (!preset) return;
    const apiKey = config.apiKeys[provider] ?? "";
    setConfig(
      provider === "custom"
        ? { provider, baseUrl: "", model: "", apiKey }
        : {
            provider,
            baseUrl: preset.baseUrl,
            model: preset.models[0]?.value ?? "",
            apiKey,
          },
    );
  };

  const changeApiKey = (apiKey: string) =>
    setConfig({
      apiKey,
      apiKeys: { ...config.apiKeys, [config.provider]: apiKey },
    });

  return {
    config,
    setConfig,
    loadingConfig,
    loadError,
    saving,
    saveMessage,
    saveWarning,
    saveError,
    testing,
    testMessage,
    loginStatus,
    loginActionPending,
    logoutIntent,
    setLogoutIntent,
    isWebGateway,
    historyLimitInput,
    historyLimitError,
    changeHistoryLimit,
    save,
    test,
    beginLogin,
    logout,
    changeBackend,
    changeProvider,
    changeApiKey,
  };
}
