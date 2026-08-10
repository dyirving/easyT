import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationStore } from "@/stores/translationStore";
import { copyTranslation, setWindowPinned } from "@/services/tauriCommands";
import { runTranslationRequest } from "@/services/translationRunner";
import { useTranslationController } from "./useTranslationController";

vi.mock("@/services/tauriCommands", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/services/tauriCommands")>()),
  copyTranslation: vi.fn(),
  setWindowPinned: vi.fn(),
}));
vi.mock("@/services/translationRunner", () => ({ runTranslationRequest: vi.fn() }));

const mockedCopy = vi.mocked(copyTranslation);
const mockedSetPinned = vi.mocked(setWindowPinned);
const mockedRun = vi.mocked(runTranslationRequest);

describe("useTranslationController", () => {
  beforeEach(() => {
    useTranslationStore.getState().reset();
    useSettingsStore.getState().resetToDefault();
    mockedCopy.mockReset().mockResolvedValue(undefined);
    mockedSetPinned.mockReset().mockResolvedValue(undefined);
    mockedRun.mockReset().mockResolvedValue(undefined);
  });

  it("rejects empty text before invoking the translation runner", async () => {
    const { result } = renderHook(() => useTranslationController());

    await act(() => result.current.translate("   "));

    expect(mockedRun).not.toHaveBeenCalled();
    expect(useTranslationStore.getState()).toMatchObject({
      status: "error",
      errorKind: "NoSelectedText",
    });
  });

  it("enforces the configured text-length limit locally", async () => {
    useSettingsStore.getState().setConfig({ maxTextLength: 3 });
    const { result } = renderHook(() => useTranslationController());

    await act(() => result.current.translate("four"));

    expect(mockedRun).not.toHaveBeenCalled();
    expect(useTranslationStore.getState()).toMatchObject({
      status: "error",
      errorKind: "TextTooLong",
      originalText: "four",
    });
  });

  it("retries the original text with forceRefresh", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore.getState().succeedRequest(requestId, { translatedText: "译文", fromCache: true });
    const { result } = renderHook(() => useTranslationController());

    act(() => result.current.retry());

    expect(mockedRun).toHaveBeenCalledWith(expect.any(String), "source", expect.any(Object), true);
  });

  it("copies completed text and reflects the copied state", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore.getState().succeedRequest(requestId, { translatedText: "译文", fromCache: false });
    const { result } = renderHook(() => useTranslationController());

    await act(() => result.current.copy());

    expect(mockedCopy).toHaveBeenCalledWith("译文");
    expect(result.current.copied).toBe(true);
  });

  it("synchronizes pin changes to the native window command", () => {
    const { result } = renderHook(() => useTranslationController());

    act(() => result.current.togglePin());

    expect(useTranslationStore.getState().pinned).toBe(true);
    expect(mockedSetPinned).toHaveBeenCalledWith(true);
  });
});
