import { create } from "zustand";
import type {
  HistorySnapshot,
  HistoryWarning,
  TranslationHistoryEntry,
  TranslationHistorySummary,
} from "@/types";

export const DEFAULT_MANUAL_INPUT =
  "Large language models are trained on massive text corpora.";

type PendingHistoryAction = "copyAll";

interface TranslationHistoryStore {
  initialization: "loading" | "ready" | "unavailable";
  initializationWarning: HistoryWarning | null;
  limit: number;
  summaries: TranslationHistorySummary[];
  bodiesById: Record<string, TranslationHistoryEntry>;
  expandedEntryIds: string[];
  loadingEntryIds: string[];
  pendingActionById: Record<string, PendingHistoryAction | undefined>;
  clearStatus: "idle" | "confirming" | "pending";
  actionError: string | null;
  manualInput: string;
  manualInputOpen: boolean;
  scrollTop: number;
  scrollToTopToken: number;
  capturePending: boolean;
  hydrate: (
    snapshot: HistorySnapshot,
    latestEntry?: TranslationHistoryEntry,
  ) => void;
  markUnavailable: (warning?: HistoryWarning) => void;
  applySavedCommit: (
    summary: TranslationHistorySummary,
    originalText: string,
    translatedText: string,
    evictedEntryIds: string[],
  ) => void;
  applyLimitUpdate: (
    limit: number,
    summaries: TranslationHistorySummary[],
    evictedEntryIds: string[],
  ) => void;
  cacheBody: (entry: TranslationHistoryEntry) => void;
  setEntryLoading: (entryId: string, loading: boolean) => void;
  setExpanded: (entryId: string, open: boolean) => void;
  setPendingAction: (
    entryId: string,
    action: PendingHistoryAction | undefined,
  ) => void;
  prepareForNewRequest: () => void;
  requestClearConfirmation: () => void;
  cancelClear: () => void;
  setClearPending: () => void;
  clearSucceeded: () => void;
  setActionError: (message: string | null) => void;
  setManualInput: (value: string) => void;
  setManualInputOpen: (open: boolean) => void;
  rememberScrollTop: (value: number) => void;
  setCapturePending: (pending: boolean) => void;
  reset: () => void;
}

const initialState = {
  initialization: "loading" as const,
  initializationWarning: null,
  limit: 5,
  summaries: [] as TranslationHistorySummary[],
  bodiesById: {} as Record<string, TranslationHistoryEntry>,
  expandedEntryIds: [] as string[],
  loadingEntryIds: [] as string[],
  pendingActionById: {} as Record<
    string,
    PendingHistoryAction | undefined
  >,
  clearStatus: "idle" as const,
  actionError: null,
  manualInput: DEFAULT_MANUAL_INPUT,
  manualInputOpen: false,
  scrollTop: 0,
  scrollToTopToken: 0,
  capturePending: false,
};

function removeIds<T>(record: Record<string, T>, ids: Set<string>) {
  return Object.fromEntries(
    Object.entries(record).filter(([entryId]) => !ids.has(entryId)),
  ) as Record<string, T>;
}

export const useTranslationHistoryStore = create<TranslationHistoryStore>(
  (set, get) => ({
    ...initialState,
    hydrate: (snapshot, latestEntry) => {
      const summaries = snapshot.summaries;
      set({
        initialization:
          snapshot.state === "unavailable" ? "unavailable" : "ready",
        initializationWarning: snapshot.warning ?? null,
        limit: snapshot.limit,
        summaries,
        bodiesById: latestEntry
          ? { [latestEntry.entryId]: latestEntry }
          : {},
        expandedEntryIds: latestEntry ? [latestEntry.entryId] : [],
        loadingEntryIds: [],
        pendingActionById: {},
        manualInputOpen: false,
        actionError: null,
      });
    },
    markUnavailable: (warning) =>
      set({
        initialization: "unavailable",
        initializationWarning:
          warning ?? {
            kind: "storageUnavailable",
            message: "翻译历史暂时不可用。",
          },
        manualInputOpen: false,
      }),
    applySavedCommit: (
      summary,
      originalText,
      translatedText,
      evictedEntryIds,
    ) => {
      const removed = new Set(evictedEntryIds);
      const prior = get().summaries.filter(
        (item) => item.entryId !== summary.entryId && !removed.has(item.entryId),
      );
      const bodies = removeIds(get().bodiesById, removed);
      bodies[summary.entryId] = {
        ...summary,
        originalText,
        translatedText,
      };
      set({
        summaries: [summary, ...prior],
        bodiesById: bodies,
        expandedEntryIds: [summary.entryId],
        loadingEntryIds: get().loadingEntryIds.filter(
          (id) => !removed.has(id),
        ),
        pendingActionById: removeIds(get().pendingActionById, removed),
        initialization: "ready",
        initializationWarning: null,
        manualInputOpen: false,
      });
    },
    applyLimitUpdate: (limit, summaries, evictedEntryIds) => {
      const removed = new Set(evictedEntryIds);
      set({
        limit,
        summaries,
        bodiesById: removeIds(get().bodiesById, removed),
        expandedEntryIds: get().expandedEntryIds.filter(
          (id) => !removed.has(id),
        ),
        loadingEntryIds: get().loadingEntryIds.filter(
          (id) => !removed.has(id),
        ),
        pendingActionById: removeIds(get().pendingActionById, removed),
      });
    },
    cacheBody: (entry) =>
      set({ bodiesById: { ...get().bodiesById, [entry.entryId]: entry } }),
    setEntryLoading: (entryId, loading) =>
      set({
        loadingEntryIds: loading
          ? Array.from(new Set([...get().loadingEntryIds, entryId]))
          : get().loadingEntryIds.filter((id) => id !== entryId),
      }),
    setExpanded: (entryId, open) =>
      set({
        expandedEntryIds: open
          ? Array.from(new Set([...get().expandedEntryIds, entryId]))
          : get().expandedEntryIds.filter((id) => id !== entryId),
      }),
    setPendingAction: (entryId, action) =>
      set({
        pendingActionById: {
          ...get().pendingActionById,
          [entryId]: action,
        },
      }),
    prepareForNewRequest: () =>
      set({
        expandedEntryIds: [],
        manualInputOpen: false,
        actionError: null,
        scrollToTopToken: get().scrollToTopToken + 1,
      }),
    requestClearConfirmation: () => set({ clearStatus: "confirming" }),
    cancelClear: () => set({ clearStatus: "idle" }),
    setClearPending: () => set({ clearStatus: "pending" }),
    clearSucceeded: () =>
      set({
        summaries: [],
        bodiesById: {},
        expandedEntryIds: [],
        loadingEntryIds: [],
        pendingActionById: {},
        clearStatus: "idle",
        actionError: null,
        manualInputOpen: false,
        scrollToTopToken: get().scrollToTopToken + 1,
      }),
    setActionError: (actionError) => set({ actionError }),
    setManualInput: (manualInput) => set({ manualInput }),
    setManualInputOpen: (manualInputOpen) => set({ manualInputOpen }),
    rememberScrollTop: (scrollTop) => set({ scrollTop }),
    setCapturePending: (capturePending) => set({ capturePending }),
    reset: () => set({ ...initialState }),
  }),
);
