import { renderHook, act } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";
import {
  beginWebLogin,
  getWebLoginStatus,
  saveConfig,
} from "@/services/tauriCommands";
import { useSettingsController } from "./useSettingsController";

vi.mock("@/services/tauriCommands", () => ({ getConfig: vi.fn(() => new Promise(() => {})), getWebLoginStatus: vi.fn(), beginWebLogin: vi.fn(), logoutWebAccount: vi.fn(), saveConfig: vi.fn(), testApiConnection: vi.fn(), toCommandError: (error: unknown) => ({ message: error instanceof Error ? error.message : "失败" }) }));

describe("useSettingsController", () => {
  beforeEach(() => {
    useSettingsStore.getState().resetToDefault();
    useTranslationHistoryStore.getState().reset();
    vi.mocked(getWebLoginStatus).mockReset();
    vi.mocked(beginWebLogin).mockReset();
    vi.mocked(saveConfig).mockReset();
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
