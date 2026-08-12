import { renderHook, act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";
import { saveConfig } from "@/services/tauriCommands";
import { useSettingsController } from "./useSettingsController";

vi.mock("@/services/tauriCommands", () => ({ getConfig: vi.fn(() => new Promise(() => {})), getWebLoginStatus: vi.fn(), beginWebLogin: vi.fn(), logoutWebAccount: vi.fn(), saveConfig: vi.fn(), testApiConnection: vi.fn(), toCommandError: (error: unknown) => ({ message: error instanceof Error ? error.message : "失败" }) }));

describe("useSettingsController", () => {
  beforeEach(() => {
    useSettingsStore.getState().resetToDefault();
    useTranslationHistoryStore.getState().reset();
    vi.mocked(saveConfig).mockReset();
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
