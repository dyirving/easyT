import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationStore } from "@/stores/translationStore";
import {
  captureSelectedText,
  positionWindowNearMouse,
} from "@/services/tauriCommands";
import { runTranslationRequest } from "@/services/translationRunner";
import { startShortcutTranslation } from "./translationCoordinator";

const mockedCaptureSelectedText = vi.mocked(captureSelectedText);
const mockedPositionWindowNearMouse = vi.mocked(positionWindowNearMouse);
const mockedRunTranslationRequest = vi.mocked(runTranslationRequest);

const win = vi.hoisted(() => ({
  show: vi.fn(),
  setFocus: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => win,
}));

vi.mock("@/services/tauriCommands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/services/tauriCommands")>();
  return {
    ...actual,
    captureSelectedText: vi.fn(),
    positionWindowNearMouse: vi.fn(),
  };
});

vi.mock("@/services/translationRunner", () => ({
  runTranslationRequest: vi.fn(),
}));

const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
const noSelectedTextError = {
  kind: "NoSelectedText",
  message: "未检测到选中文本",
};
const beforeState = () => ({ ...useTranslationStore.getState() });

describe("startShortcutTranslation", () => {
  const setRoute = vi.fn<(route: "translation") => void>();

  beforeEach(() => {
    useTranslationStore.getState().reset();
    useSettingsStore.getState().resetToDefault();
    setRoute.mockReset();
    win.show.mockReset();
    win.setFocus.mockReset();
    mockedCaptureSelectedText.mockReset();
    mockedPositionWindowNearMouse.mockReset().mockResolvedValue(undefined);
    mockedRunTranslationRequest.mockReset().mockResolvedValue(undefined);
  });

it("T-006 leaves route, window and store untouched while capture is pending", async () => {
    let resolveCapture: (text: string) => void = () => {};
    mockedCaptureSelectedText.mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          resolveCapture = resolve;
        }),
    );

    startShortcutTranslation(setRoute);
    await flush();

    expect(captureSelectedText).toHaveBeenCalledTimes(1);
    expect(setRoute).not.toHaveBeenCalled();
    expect(win.show).not.toHaveBeenCalled();
    expect(win.setFocus).not.toHaveBeenCalled();
    expect(useTranslationStore.getState()).toMatchObject({
      requestId: null,
      originalText: "",
      status: "idle",
    });
    // NFR-001：捕获等待期间完整 store 快照不变
    expect(useTranslationStore.getState()).toEqual(beforeState());

    resolveCapture("text");
    await flush();
  });

  it("NFR-004 allows a later capture after a rejected one", async () => {
    mockedCaptureSelectedText.mockRejectedValueOnce({
      kind: "ClipboardError",
      message: "剪贴板被占用",
    });

    startShortcutTranslation(setRoute);
    await flush();

    expect(useTranslationStore.getState()).toMatchObject({
      status: "error",
      errorKind: "ClipboardError",
    });

    mockedCaptureSelectedText.mockResolvedValueOnce("Recovered");
    startShortcutTranslation(setRoute);
    await flush();

    expect(useTranslationStore.getState()).toMatchObject({
      originalText: "Recovered",
      status: "translating",
    });
    expect(positionWindowNearMouse).toHaveBeenCalledOnce();
    expect(win.show).toHaveBeenCalledTimes(2);
  });

  it("T-007 starts a request for captured text, then positions, shows and runs translation", async () => {
    mockedCaptureSelectedText.mockResolvedValueOnce("Hello World");
    const order: string[] = [];
    setRoute.mockImplementation((route) => {
      order.push(`route:${route}`);
    });
    mockedPositionWindowNearMouse.mockImplementation(async () => {
      order.push("position");
    });
    win.show.mockImplementation(async () => {
      order.push("show");
    });
    win.setFocus.mockImplementation(async () => {
      order.push("focus");
    });
    mockedRunTranslationRequest.mockImplementation(async () => {
      order.push("run");
    });

    startShortcutTranslation(setRoute);
    await flush();

    const state = useTranslationStore.getState();
    expect(state).toMatchObject({
      originalText: "Hello World",
      status: "translating",
    });
    expect(state.requestId).not.toBeNull();
    expect(positionWindowNearMouse).toHaveBeenCalledTimes(1);
    expect(positionWindowNearMouse).toHaveBeenCalledWith(false);
    expect(win.show).toHaveBeenCalledTimes(1);
    expect(win.setFocus).toHaveBeenCalledTimes(1);
expect(runTranslationRequest).toHaveBeenCalledTimes(1);
    expect(runTranslationRequest).toHaveBeenCalledWith(
      state.requestId,
      "Hello World",
      expect.objectContaining({ shortcut: "Ctrl+T" }),
      false,
    );
    expect(order).toEqual([
      "route:translation",
      "position",
      "show",
      "focus",
      "run",
    ]);
  });

  it("T-008 treats NoSelectedText as a pure display restore in every state", async () => {
    const setups: Record<string, () => void> = {
      idle: () => {},
success: () => {
        const id = useTranslationStore.getState().startRequest("source");
        useTranslationStore
          .getState()
          .succeedRequest(id, { translatedText: "译文", fromCache: false });
      },
      streaming: () => {
        const id = useTranslationStore.getState().startRequest("source");
        useTranslationStore.getState().appendTranslationDelta(id, "部分");
      },
      error: () => {
        const id = useTranslationStore.getState().startRequest("source");
        useTranslationStore.getState().failRequest(id, "失败", "ApiTimeout");
      },
    };

    for (const [name, setup] of Object.entries(setups)) {
      setup();
      const before = useTranslationStore.getState();
      mockedCaptureSelectedText.mockRejectedValueOnce(noSelectedTextError);

      startShortcutTranslation(setRoute);
      await flush();

      expect(useTranslationStore.getState()).toEqual(before);
      expect(setRoute).toHaveBeenLastCalledWith("translation");
      expect(win.show).toHaveBeenCalledOnce();
      expect(win.setFocus).toHaveBeenCalledOnce();
      expect(positionWindowNearMouse).not.toHaveBeenCalled();
      expect(runTranslationRequest).not.toHaveBeenCalled();
      expect(name).toBeTruthy();

      setRoute.mockClear();
      win.show.mockClear();
      win.setFocus.mockClear();
    }
  });

  it("T-009 surfaces other capture failures without positioning or translating", async () => {
    const clipboardError = { kind: "ClipboardError", message: "剪贴板被占用" };
    mockedCaptureSelectedText.mockRejectedValueOnce(clipboardError);

    startShortcutTranslation(setRoute);
    await flush();

    expect(useTranslationStore.getState()).toMatchObject({
      requestId: null,
      originalText: "",
      translatedText: "",
      status: "error",
      errorMessage: "剪贴板被占用",
      errorKind: "ClipboardError",
      isPartial: false,
    });
    expect(setRoute).toHaveBeenCalledWith("translation");
    expect(win.show).toHaveBeenCalledTimes(1);
    expect(win.setFocus).toHaveBeenCalledTimes(1);
    expect(positionWindowNearMouse).not.toHaveBeenCalled();
    expect(runTranslationRequest).not.toHaveBeenCalled();
  });

  it("T-010 serializes captures and lets the later valid text own the state", async () => {
    const order: string[] = [];
    let resolveFirst: (text: string) => void = () => {};
    let resolveSecond: (text: string) => void = () => {};
    mockedCaptureSelectedText
      .mockImplementationOnce(
        () =>
          new Promise<string>((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockImplementationOnce(
        () =>
          new Promise<string>((resolve) => {
            resolveSecond = resolve;
          }),
      );
    const requestIds: string[] = [];
    useTranslationStore.subscribe((state, prev) => {
      if (state.requestId && state.requestId !== prev.requestId) {
        requestIds.push(state.requestId);
      }
    });
mockedRunTranslationRequest.mockImplementation(async (requestId, text) => {
      order.push(`run:${text}`);
      useTranslationStore
        .getState()
        .succeedRequest(requestId, {
          translatedText: `译文(${text})`,
          fromCache: false,
        });
    });

    startShortcutTranslation(setRoute);
    await flush();
    startShortcutTranslation(setRoute);
    await flush();
    expect(captureSelectedText).toHaveBeenCalledTimes(1);

    resolveFirst("A");
    await flush();
    expect(captureSelectedText).toHaveBeenCalledTimes(2);

    resolveSecond("B");
    await flush();

    expect(order).toEqual(["run:A", "run:B"]);
    expect(requestIds).toHaveLength(2);
    expect(useTranslationStore.getState()).toMatchObject({
      originalText: "B",
      translatedText: "译文(B)",
      status: "success",
    });
expect(
      useTranslationStore
        .getState()
        .succeedRequest(requestIds[0], { translatedText: "迟到", fromCache: false }),
    ).toBe(false);
  });
});

