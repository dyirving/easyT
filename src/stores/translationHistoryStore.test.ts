import { beforeEach, describe, expect, it } from "vitest";
import type {
  HistorySnapshot,
  TranslationHistoryEntry,
  TranslationHistorySummary,
} from "@/types";
import { useTranslationHistoryStore } from "./translationHistoryStore";

const summary = (entryId: string, completedAtUtcMs: number): TranslationHistorySummary => ({
  entryId,
  originalSummary: `source-${entryId}`,
  translatedSummary: `target-${entryId}`,
  targetLanguage: "简体中文",
  sourceBackend: "officialApi",
  sourceProvider: "agnes",
  sourceModel: "agnes-2.0-flash",
  fromCache: false,
  totalElapsedMs: 100,
  completedAtUtcMs,
});

const entry = (value: TranslationHistorySummary): TranslationHistoryEntry => ({
  ...value,
  originalText: `full-${value.entryId}`,
  translatedText: `translated-${value.entryId}`,
});

describe("translationHistoryStore", () => {
  beforeEach(() => useTranslationHistoryStore.getState().reset());

  it("hydrates summaries and only the latest full body", () => {
    const newest = summary("new", 2);
    const older = summary("old", 1);
    const snapshot: HistorySnapshot = {
      state: "ready",
      limit: 5,
      summaries: [newest, older],
    };
    useTranslationHistoryStore.getState().hydrate(snapshot, entry(newest));
    expect(useTranslationHistoryStore.getState()).toMatchObject({
      initialization: "ready",
      summaries: [newest, older],
      expandedEntryIds: ["new"],
      manualInputOpen: false,
    });
    expect(useTranslationHistoryStore.getState().bodiesById).toEqual({
      new: entry(newest),
    });
  });

  it("atomically upserts, replaces and evicts records", () => {
    const first = summary("first", 1);
    const replaced = summary("replaced", 2);
    const evicted = summary("evicted", 0);
    useTranslationHistoryStore.getState().hydrate({
      state: "ready",
      limit: 5,
      summaries: [replaced, first, evicted],
    });
    useTranslationHistoryStore.getState().cacheBody(entry(replaced));
    useTranslationHistoryStore.getState().cacheBody(entry(evicted));
    const next = summary("next", 3);
    useTranslationHistoryStore
      .getState()
      .applySavedCommit(next, "source", "target", "replaced", ["evicted"]);
    const state = useTranslationHistoryStore.getState();
    expect(state.summaries.map((item) => item.entryId)).toEqual(["next", "first"]);
    expect(Object.keys(state.bodiesById)).toEqual(["next"]);
    expect(state.expandedEntryIds).toEqual(["next"]);
  });

  it("applies limit cleanup, clear and one-shot scroll intent", () => {
    const first = summary("first", 1);
    const second = summary("second", 2);
    useTranslationHistoryStore.getState().hydrate({
      state: "ready",
      limit: 5,
      summaries: [second, first],
    });
    useTranslationHistoryStore.getState().cacheBody(entry(first));
    useTranslationHistoryStore.getState().setExpanded("first", true);
    useTranslationHistoryStore
      .getState()
      .applyLimitUpdate(1, [second], ["first"]);
    expect(useTranslationHistoryStore.getState()).toMatchObject({
      limit: 1,
      summaries: [second],
      bodiesById: {},
      expandedEntryIds: [],
    });
    const token = useTranslationHistoryStore.getState().scrollToTopToken;
    useTranslationHistoryStore.getState().clearSucceeded();
    expect(useTranslationHistoryStore.getState()).toMatchObject({
      summaries: [],
      manualInputOpen: false,
      scrollToTopToken: token + 1,
    });
  });
});
