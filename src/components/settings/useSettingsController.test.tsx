import { renderHook, act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";
import {
  beginWebLogin,
  beginQwenAccountLogin,
  createQwenAccount,
  deleteQwenAccount,
  getWebLoginStatus,
  getQwenAccountPool,
  moveQwenAccount,
  renameQwenAccount,
  saveConfig,
  setQwenAccountEnabled,
} from "@/services/tauriCommands";
import { useSettingsController } from "./useSettingsController";

vi.mock("@/services/tauriCommands", () => ({ getConfig: vi.fn(() => new Promise(() => {})), getWebLoginStatus: vi.fn(), getQwenAccountPool: vi.fn(), beginWebLogin: vi.fn(), beginQwenAccountLogin: vi.fn(), createQwenAccount: vi.fn(), renameQwenAccount: vi.fn(), setQwenAccountEnabled: vi.fn(), moveQwenAccount: vi.fn(), deleteQwenAccount: vi.fn(), logoutWebAccount: vi.fn(), saveConfig: vi.fn(), testApiConnection: vi.fn(), toCommandError: (error: unknown) => ({ message: error instanceof Error ? error.message : "失败" }) }));

describe("useSettingsController", () => {
  beforeEach(() => {
    useSettingsStore.getState().resetToDefault();
    useTranslationHistoryStore.getState().reset();
    vi.mocked(getWebLoginStatus).mockReset();
    vi.mocked(getQwenAccountPool).mockReset();
    vi.mocked(getQwenAccountPool).mockResolvedValue({
      accounts: [],
      maximumAccounts: 10,
    });
    vi.mocked(beginWebLogin).mockReset();
    vi.mocked(beginQwenAccountLogin).mockReset();
    vi.mocked(createQwenAccount).mockReset();
    vi.mocked(renameQwenAccount).mockReset();
    vi.mocked(setQwenAccountEnabled).mockReset();
    vi.mocked(moveQwenAccount).mockReset();
    vi.mocked(deleteQwenAccount).mockReset();
    vi.mocked(saveConfig).mockReset();
  });

  it("loads the authoritative Qwen account inventory only in WebGateway settings", async () => {
    vi.mocked(getQwenAccountPool).mockResolvedValue({
      accounts: [],
      maximumAccounts: 10,
    });
    useSettingsStore.getState().setConfig({ backendMode: "webGateway" });

    const { result } = renderHook(() => useSettingsController());
    await act(async () => Promise.resolve());

    expect(getQwenAccountPool).toHaveBeenCalledTimes(1);
    expect(result.current.qwenAccountPool?.maximumAccounts).toBe(10);
  });

  it("uses the returned authoritative account snapshot after adding an account", async () => {
    const snapshot = {
      accounts: [{ accountId: "550e8400-e29b-41d4-a716-446655440000", displayName: "Personal", enabled: true, order: 0, status: "loggingIn" as const, actions: { canRename: true, canToggleEnabled: true, canMoveUp: false, canMoveDown: false, canLogin: false, canLogout: false, canTest: false, canDelete: false } }],
      maximumAccounts: 10,
      loginAccountId: "550e8400-e29b-41d4-a716-446655440000",
    };
    vi.mocked(createQwenAccount).mockResolvedValue(snapshot);
    useSettingsStore.getState().setConfig({ backendMode: "webGateway" });
    const { result } = renderHook(() => useSettingsController());

    await act(async () => result.current.createQwenAccount("Personal"));
    expect(createQwenAccount).toHaveBeenCalledWith("Personal");
    expect(result.current.qwenAccountPool).toEqual(snapshot);
  });

  it("uses the returned authoritative snapshot for lifecycle mutations", async () => {
    const snapshot = {
      accounts: [{ accountId: "550e8400-e29b-41d4-a716-446655440000", displayName: "Renamed", enabled: false, order: 0, status: "disabled" as const, actions: { canRename: true, canToggleEnabled: true, canMoveUp: false, canMoveDown: false, canLogin: true, canLogout: true, canTest: false, canDelete: true } }],
      maximumAccounts: 10,
    };
    vi.mocked(renameQwenAccount).mockResolvedValue(snapshot);
    useSettingsStore.getState().setConfig({ backendMode: "webGateway" });
    const { result } = renderHook(() => useSettingsController());

    await act(async () => result.current.renameQwenAccount("550e8400-e29b-41d4-a716-446655440000", "Renamed"));
    expect(renameQwenAccount).toHaveBeenCalledWith("550e8400-e29b-41d4-a716-446655440000", "Renamed");
    expect(result.current.qwenAccountPool).toEqual(snapshot);
  });

  it("uses authoritative snapshots for toggle, ordering, and destructive account actions", async () => {
    const accountId = "550e8400-e29b-41d4-a716-446655440000";
    const snapshot = {
      accounts: [{ accountId, displayName: "Personal", enabled: false, order: 0, status: "disabled" as const, actions: { canRename: true, canToggleEnabled: true, canMoveUp: false, canMoveDown: false, canLogin: true, canLogout: false, canTest: false, canDelete: true } }],
      maximumAccounts: 10,
    };
    vi.mocked(setQwenAccountEnabled).mockResolvedValue(snapshot);
    vi.mocked(moveQwenAccount).mockResolvedValue(snapshot);
    vi.mocked(deleteQwenAccount).mockResolvedValue(snapshot);
    useSettingsStore.getState().setConfig({ backendMode: "webGateway" });
    const { result } = renderHook(() => useSettingsController());

    await act(async () => result.current.setQwenAccountEnabled(accountId, false));
    await act(async () => result.current.moveQwenAccount(accountId, "down"));
    act(() => result.current.setAccountDestructiveIntent({ accountId, displayName: "Personal", kind: "delete" }));
    await act(async () => result.current.confirmAccountDestructiveAction());

    expect(setQwenAccountEnabled).toHaveBeenCalledWith(accountId, false);
    expect(moveQwenAccount).toHaveBeenCalledWith(accountId, "down");
    expect(deleteQwenAccount).toHaveBeenCalledWith(accountId);
    expect(result.current.qwenAccountPool).toEqual(snapshot);
  });
  afterEach(() => vi.useRealTimers());

  it.each(["loggedOut", "ready"] as const)(
    "polls until a login started from %s completes",
    async (initialPhase) => {
      vi.useFakeTimers();
      vi.mocked(getWebLoginStatus)
        .mockResolvedValueOnce({
          phase: initialPhase,
          message: null,
          updatedAt: 1,
        })
        .mockResolvedValueOnce({ phase: "ready", message: null, updatedAt: 2 });
      vi.mocked(beginWebLogin).mockResolvedValue({
        phase: "loggingIn",
        message: "正在登录...",
        updatedAt: 1,
      });
      useSettingsStore.getState().setConfig({ backendMode: "webGateway" });

      const { result } = renderHook(() => useSettingsController());
      await act(async () => Promise.resolve());
      expect(result.current.loginStatus?.phase).toBe(initialPhase);

      await act(async () => result.current.beginLogin());
      expect(result.current.loginStatus?.phase).toBe("loggingIn");

      await act(async () => {
        await vi.advanceTimersByTimeAsync(1000);
      });
      expect(getWebLoginStatus).toHaveBeenCalledTimes(2);
      expect(result.current.loginStatus?.phase).toBe("ready");
    },
  );

  it("ignores an initial status response that predates a new login", async () => {
    vi.useFakeTimers();
    let resolveInitialStatus: ((value: {
      phase: "loggedOut";
      message: null;
      updatedAt: number;
    }) => void) | undefined;
    vi.mocked(getWebLoginStatus)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveInitialStatus = resolve;
          }),
      )
      .mockResolvedValueOnce({ phase: "ready", message: null, updatedAt: 2 });
    vi.mocked(beginWebLogin).mockResolvedValue({
      phase: "loggingIn",
      message: "正在登录...",
      updatedAt: 1,
    });
    useSettingsStore.getState().setConfig({ backendMode: "webGateway" });

    const { result } = renderHook(() => useSettingsController());
    await act(async () => result.current.beginLogin());
    expect(result.current.loginStatus?.phase).toBe("loggingIn");

    await act(async () => {
      resolveInitialStatus?.({ phase: "loggedOut", message: null, updatedAt: 0 });
    });
    expect(result.current.loginStatus?.phase).toBe("loggingIn");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(result.current.loginStatus?.phase).toBe("ready");
  });

  it("restores provider API keys and applies provider defaults", () => {
    useSettingsStore.getState().setConfig({ apiKeys: { deepseek: "saved-key" } });
    const { result } = renderHook(() => useSettingsController());
    act(() => result.current.changeProvider("deepseek"));
    expect(result.current.config).toMatchObject({ provider: "deepseek", apiKey: "saved-key" });
  });
  it("keeps the current provider API key pool synchronized", () => {
    const { result } = renderHook(() => useSettingsController());
    act(() => result.current.changeApiKey("new-key"));
    expect(result.current.config.apiKeys[result.current.config.provider]).toBe("new-key");
  });
  it("rejects non-canonical or out-of-range history limits without saving", async () => {
    const { result } = renderHook(() => useSettingsController());
    act(() => result.current.changeHistoryLimit("01"));
    expect(result.current.historyLimitError).toBe("请输入 1～20 的整数");
    await act(async () => result.current.save());
    expect(saveConfig).not.toHaveBeenCalled();
    act(() => result.current.changeHistoryLimit("21"));
    expect(result.current.historyLimitError).toBeTruthy();
  });
  it("applies the authoritative summaries after a valid save", async () => {
    vi.mocked(saveConfig).mockResolvedValue({
      historyLimit: 20,
      historyUpdate: {
        status: "applied",
        summaries: [],
        evictedEntryIds: ["old"],
      },
    });
    const { result } = renderHook(() => useSettingsController());
    act(() => result.current.changeHistoryLimit("20"));
    await act(async () => result.current.save());
    expect(saveConfig).toHaveBeenCalledWith(
      expect.objectContaining({ translationHistoryLimit: 20 }),
    );
    expect(result.current.saveMessage).toBe("设置已保存");
    expect(useTranslationHistoryStore.getState().limit).toBe(20);
  });
});
