import { useEffect, useRef, useState } from "react";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";
import {
  beginWebLogin,
  beginQwenAccountLogin as beginQwenAccountLoginCommand,
  createQwenAccount as createQwenAccountCommand,
  deleteQwenAccount as deleteQwenAccountCommand,
  getConfig,
  getQwenAccountPool,
  getWebLoginStatus,
  logoutQwenAccount as logoutQwenAccountCommand,
  moveQwenAccount as moveQwenAccountCommand,
  renameQwenAccount as renameQwenAccountCommand,
  saveConfig,
  setQwenAccountEnabled as setQwenAccountEnabledCommand,
  testQwenAccount as testQwenAccountCommand,
  formatCommandError,
  testApiConnection,
  toCommandError,
} from "@/services/tauriCommands";
import {
  getProviderPreset,
  type BackendMode,
  type ModelProvider,
  type QwenSessionStatus,
  type QwenAccountPoolSnapshot,
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
  const [qwenAccountPool, setQwenAccountPool] = useState<QwenAccountPoolSnapshot | null>(null);
  const [loginActionPending, setLoginActionPending] = useState(false);
  const [accountDestructiveIntent, setAccountDestructiveIntent] = useState<{
    accountId: string;
    kind: "logout" | "delete";
    displayName: string;
  } | null>(null);
  const [qwenAccountPending, setQwenAccountPending] = useState(false);
  const [qwenAccountError, setQwenAccountError] = useState<string | null>(null);
  const [historyLimitInput, setHistoryLimitInput] = useState(() =>
    String(config.translationHistoryLimit),
  );
  const loginStatusVersion = useRef(0);
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
      const version = loginStatusVersion.current;
      try {
        const status = await getWebLoginStatus(provider);
        if (cancelled || version !== loginStatusVersion.current) return;
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
    if (!isWebGateway) {
      setQwenAccountPool(null);
      return;
    }
    let cancelled = false;
    getQwenAccountPool()
      .then((snapshot) => {
        if (!cancelled) setQwenAccountPool(snapshot);
      })
      .catch(() => {
        if (!cancelled) setQwenAccountPool(null);
      });
    return () => {
      cancelled = true;
    };
  }, [isWebGateway]);

  useEffect(() => {
    const hasDynamicAccountState = qwenAccountPool?.accounts.some((account) =>
      account.status === "loggingIn" || account.status === "busy" || account.status === "coolingDown",
    );
    if (!isWebGateway || !hasDynamicAccountState) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const refresh = async () => {
      try {
        const snapshot = await getQwenAccountPool();
        if (cancelled) return;
        setQwenAccountPool(snapshot);
        if (snapshot.accounts.some((account) =>
          account.status === "loggingIn" || account.status === "busy" || account.status === "coolingDown",
        )) timer = setTimeout(refresh, POLL_MS);
      } catch {
        if (!cancelled) timer = setTimeout(refresh, POLL_MS * 2);
      }
    };
    timer = setTimeout(refresh, POLL_MS);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [isWebGateway, qwenAccountPool]);

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
    const result = await testApiConnection(config);
    setTesting(result.ok ? "ok" : "fail");
    setTestMessage(result.message);
  };

  const beginLogin = async () => {
    if (loginActionPending) return;
    setLoginActionPending(true);
    try {
      loginStatusVersion.current += 1;
      const status = await beginWebLogin(config.webGateway.provider);
      setLoginStatus(status);
    } finally {
      setLoginActionPending(false);
    }
  };

  const createQwenAccount = async (displayName: string) => {
    if (qwenAccountPending) return;
    setQwenAccountPending(true);
    setQwenAccountError(null);
    try {
      setQwenAccountPool(await createQwenAccountCommand(displayName));
    } catch (error) {
      const commandError = toCommandError(error);
      setQwenAccountError(commandError.code ? `${commandError.message} [${commandError.code}]` : commandError.message);
    } finally {
      setQwenAccountPending(false);
    }
  };

  const beginQwenAccountLogin = async (accountId: string) => {
    if (qwenAccountPending) return;
    setQwenAccountPending(true);
    setQwenAccountError(null);
    try {
      setQwenAccountPool(await beginQwenAccountLoginCommand(accountId));
    } catch (error) {
      const commandError = toCommandError(error);
      setQwenAccountError(commandError.code ? `${commandError.message} [${commandError.code}]` : commandError.message);
    } finally {
      setQwenAccountPending(false);
    }
  };

  const applyAccountMutation = async (
    mutation: () => Promise<QwenAccountPoolSnapshot>,
  ) => {
    if (qwenAccountPending) return;
    setQwenAccountPending(true);
    setQwenAccountError(null);
    try {
      setQwenAccountPool(await mutation());
    } catch (error) {
      const commandError = toCommandError(error);
      setQwenAccountError(commandError.code ? `${commandError.message} [${commandError.code}]` : commandError.message);
    } finally {
      setQwenAccountPending(false);
    }
  };

  const renameQwenAccount = (accountId: string, displayName: string) =>
    applyAccountMutation(() => renameQwenAccountCommand(accountId, displayName));

  const setQwenAccountEnabled = (accountId: string, enabled: boolean) =>
    applyAccountMutation(() => setQwenAccountEnabledCommand(accountId, enabled));

  const moveQwenAccount = (accountId: string, direction: "up" | "down") =>
    applyAccountMutation(() => moveQwenAccountCommand(accountId, direction));

  const testQwenAccount = async (accountId: string) => {
    if (qwenAccountPending) return;
    setQwenAccountPending(true);
    setQwenAccountError(null);
    try {
      await testQwenAccountCommand(accountId, config);
      setQwenAccountPool(await getQwenAccountPool());
    } catch (error) {
      setQwenAccountError(formatCommandError(toCommandError(error)));
      setQwenAccountPool(await getQwenAccountPool().catch(() => null));
    } finally {
      setQwenAccountPending(false);
    }
  };

  const confirmAccountDestructiveAction = async () => {
    const intent = accountDestructiveIntent;
    if (!intent) return;
    setAccountDestructiveIntent(null);
    await applyAccountMutation(() => intent.kind === "logout"
      ? logoutQwenAccountCommand(intent.accountId)
      : deleteQwenAccountCommand(intent.accountId));
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
    qwenAccountPool,
    loginActionPending,
    accountDestructiveIntent,
    setAccountDestructiveIntent,
    qwenAccountPending,
    qwenAccountError,
    isWebGateway,
    historyLimitInput,
    historyLimitError,
    changeHistoryLimit,
    save,
    test,
    beginLogin,
    createQwenAccount,
    beginQwenAccountLogin,
    renameQwenAccount,
    setQwenAccountEnabled,
    moveQwenAccount,
    testQwenAccount,
    confirmAccountDestructiveAction,
    changeBackend,
    changeProvider,
    changeApiKey,
  };
}
