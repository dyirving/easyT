import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_CONFIG, type TranslationHistorySummary } from "@/types";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";
import { useTranslationStore } from "@/stores/translationStore";
import { translateText } from "./tauriCommands";
import { runTranslationRequest } from "./translationRunner";

vi.mock("./tauriCommands", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./tauriCommands")>();
  return {
    ...actual,
    translateText: vi.fn(),
    toCommandError: vi.fn((error: unknown) => ({
      kind: "Internal",
      message: error instanceof Error ? error.message : "失败",
    })),
  };
});

const mockedTranslateText = vi.mocked(translateText);
const savedSummary: TranslationHistorySummary = {
  entryId: "saved-id",
  originalSummary: "source",
  translatedSummary: "译文",
  targetLanguage: "简体中文",
  sourceBackend: "officialApi",
  sourceProvider: "agnes",
  sourceModel: "agnes-2.0-flash",
  fromCache: false,
  totalElapsedMs: 12,
  completedAtUtcMs: 1,
};

describe("runTranslationRequest history orchestration", () => {
  beforeEach(() => {
    useTranslationStore.getState().reset();
    useTranslationHistoryStore.getState().reset();
    useTranslationHistoryStore.setState({ initialization: "ready" });
    mockedTranslateText.mockReset();
  });

  it("moves a saved success into history and returns the active store to idle", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    mockedTranslateText.mockResolvedValue({
      translatedText: "译文",
      fromCache: false,
      totalElapsedMs: 12,
      history: {
        status: "saved",
        summary: savedSummary,
        evictedEntryIds: [],
      },
    });
    await runTranslationRequest(requestId, "source", DEFAULT_CONFIG);
    expect(useTranslationStore.getState().status).toBe("idle");
    expect(useTranslationHistoryStore.getState().summaries).toEqual([
      savedSummary,
    ]);
    expect(useTranslationHistoryStore.getState().bodiesById["saved-id"]).toMatchObject({
      originalText: "source",
      translatedText: "译文",
    });
  });

  it("keeps a complete temporary result and weak warning when saving fails", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    mockedTranslateText.mockResolvedValue({
      translatedText: "临时译文",
      fromCache: true,
      totalElapsedMs: 20,
      history: {
        status: "notSaved",
        warning: { kind: "saveTimedOut", message: "保存超时" },
      },
    });
    await runTranslationRequest(requestId, "source", DEFAULT_CONFIG, true);
    expect(mockedTranslateText).toHaveBeenCalledWith(
      expect.objectContaining({ forceRefresh: true }),
    );
    expect(useTranslationStore.getState()).toMatchObject({
      status: "success",
      translatedText: "临时译文",
      fromCache: true,
      historyWarning: { kind: "saveTimedOut", message: "保存超时" },
    });
    expect(useTranslationHistoryStore.getState().summaries).toEqual([]);
  });
});
