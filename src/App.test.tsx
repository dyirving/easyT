import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import { useSettingsStore } from "@/stores/settingsStore";
import { startShortcutTranslation } from "@/services/translationCoordinator";
import {
  getTranslationHistoryEntry,
  initializeTranslationHistory,
} from "@/services/tauriCommands";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";

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
      translationHistoryLimit: 5,
      backendMode: "officialApi",
      webGateway: { provider: "qwen", model: "Qwen3.7-Max", saveHistory: false },
    }),
    initializeTranslationHistory: vi.fn(),
    getTranslationHistoryEntry: vi.fn(),
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
    useTranslationHistoryStore.getState().reset();
    registry.handlers.clear();
    vi.clearAllMocks();
    vi.mocked(initializeTranslationHistory).mockResolvedValue({
      state: "ready",
      limit: 5,
      summaries: [],
    });
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

  it("keeps the page gated until the latest history body is restored", async () => {
    const summary = {
      entryId: "latest",
      originalSummary: "source",
      translatedSummary: "译文",
      targetLanguage: "简体中文",
      sourceBackend: "officialApi" as const,
      sourceProvider: "agnes",
      sourceModel: "agnes-2.0-flash",
      fromCache: false,
      totalElapsedMs: 8,
      completedAtUtcMs: Date.now(),
    };
    vi.mocked(initializeTranslationHistory).mockResolvedValue({
      state: "ready",
      limit: 5,
      summaries: [summary],
    });
    vi.mocked(getTranslationHistoryEntry).mockResolvedValue({
      ...summary,
      originalText: "full source",
      translatedText: "restored translation",
    });
    render(<App />);
    expect(screen.getByText("正在加载翻译历史…")).toBeInTheDocument();
    expect(await screen.findByText("restored translation")).toBeInTheDocument();
    expect(screen.queryByText("正在加载翻译历史…")).not.toBeInTheDocument();
  });

  it("degrades to an available manual translator when history is unavailable", async () => {
    vi.mocked(initializeTranslationHistory).mockRejectedValue(new Error("db"));
    render(<App />);
    await waitFor(() =>
      expect(useTranslationHistoryStore.getState().initialization).toBe(
        "unavailable",
      ),
    );
    expect(screen.getByText("翻译历史暂时不可用。")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "手动输入翻译" }));
    expect(screen.getByRole("button", { name: "翻译" })).toBeEnabled();
  });
});
