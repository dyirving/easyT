import { renderHook, act } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "@/stores/settingsStore";
import { useSettingsController } from "./useSettingsController";

vi.mock("@/services/tauriCommands", () => ({ getConfig: vi.fn(() => new Promise(() => {})), getWebLoginStatus: vi.fn(), beginWebLogin: vi.fn(), logoutWebAccount: vi.fn(), saveConfig: vi.fn(), testApiConnection: vi.fn(), toCommandError: (error: unknown) => ({ message: error instanceof Error ? error.message : "失败" }) }));

describe("useSettingsController", () => {
  beforeEach(() => useSettingsStore.getState().resetToDefault());
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
});
