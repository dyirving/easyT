import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { useSettingsStore } from "@/stores/settingsStore";
import { startShortcutTranslation } from "@/services/translationCoordinator";

vi.mock("@/services/translationCoordinator", () => ({
  startShortcutTranslation: vi.fn(),
}));

const registry = vi.hoisted(() => {
  const handlers = new Map<string, (payload: unknown) => void>();
  return {
    handlers,
    emit: (event: string, payload?: unknown) => handlers.get(event)?.(payload),
  };
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (payload: unknown) => void) => {
    registry.handlers.set(event, handler);
    return Promise.resolve(() => registry.handlers.delete(event));
  }),
}));

vi.mock("@/services/tauriCommands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriCommands")>();
  return {
    ...actual,
    getConfig: vi.fn().mockResolvedValue({
      provider: "agnes",
      apiKeys: {},
      baseUrl: "https://apihub.agnes-ai.com/v1",
      apiKey: "",
      model: "agnes-2.0-flash",
      enableThinking: false,
      streamOutput: false,
      shortcut: "Ctrl+T",
      targetLanguage: "简体中文",
      timeoutSeconds: 60,
      autoHide: true,
      pinnedByDefault: false,
      maxTextLength: 5000,
      backendMode: "officialApi",
      webGateway: { provider: "qwen", model: "Qwen3.7-Max", saveHistory: false },
    }),
  };
});

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onFocusChanged: () => Promise.resolve(() => {}),
    onResized: () => Promise.resolve(() => {}),
    hide: () => Promise.resolve(),
    close: () => Promise.resolve(),
  }),
}));

describe("App shortcut route gating", () => {
  beforeEach(() => {
    useSettingsStore.getState().resetToDefault();
    registry.handlers.clear();
    vi.clearAllMocks();
  });

  it("T-011 ignores the shortcut while the settings page is open", async () => {
    const user = { click: (el: Element) => el.dispatchEvent(new MouseEvent("click", { bubbles: true })) };
    render(<App />);
    user.click(await screen.findByRole("button", { name: "打开设置" }));
    await screen.findByText(/全局快捷键/);

    registry.emit("shortcut://translate");

    expect(startShortcutTranslation).not.toHaveBeenCalled();
    expect(screen.getByText(/全局快捷键/)).toBeInTheDocument();
  });
});
