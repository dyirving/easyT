// 翻译状态管理
import { create } from "zustand";
import { type ErrorKind, type TranslationState } from "@/types";
import {
  type PhaseChangedEvent,
  type TranslationResult,
} from "@/services/tauriCommands";

interface TranslationStore extends TranslationState {
  /** 切换固定状态 */
  togglePinned: () => void;
  setPinned: (pinned: boolean) => void;
  /** 重置为 idle */
  reset: () => void;
  /**
   * 开始一次新的翻译请求（生成新 requestId）
   * forceRefresh=true 表示"重新翻译"：
   * - 当前为同一原文的完整缓存结果时保留译文并进入 refreshing
   * - 否则等价普通 start，仍向后端传递 true
   */
  startRequest: (originalText: string, forceRefresh?: boolean) => string;
  /** 捕获故障：原子切换到无原文错误态并使旧请求失效 */
  failCapture: (message: string, kind?: ErrorKind) => void;
  /** 仅当 requestId 仍是最新请求时写入成功结果 */
  succeedRequest: (requestId: string, result: TranslationResult) => boolean;
  /** 仅当 requestId 仍是最新请求时追加正文增量 */
  appendTranslationDelta: (requestId: string, delta: string) => boolean;
  /** 接受当前请求严格递增的真实后端阶段。 */
  applyProgressPhase: (requestId: string, event: PhaseChangedEvent) => boolean;
  /** 仅当 requestId 仍是最新请求时写入错误结果 */
  failRequest: (
    requestId: string,
    message: string,
    kind?: ErrorKind,
    originalText?: string,
    preservePartial?: boolean,
    totalElapsedMs?: number,
  ) => boolean;
  /** 重新翻译失败：仍是最新请求且处于 refreshing 时回退到旧缓存译文 */
  failRefreshRequest: (
    requestId: string,
    message: string,
    kind?: ErrorKind,
    totalElapsedMs?: number,
  ) => boolean;
  /** 清除成功：保留当前译文，只移除缓存来源提示 */
  clearCacheSourceNotice: () => void;
  /** 判断 requestId 是否仍是最新请求 */
  isActiveRequest: (requestId: string) => boolean;
}

export const createTranslationRequestId = () =>
  `req_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

const initialState: TranslationState = {
  requestId: null,
  originalText: "",
  translatedText: "",
  status: "idle",
  errorMessage: null,
  errorKind: null,
  isPartial: false,
  fromCache: false,
  refreshErrorMessage: null,
  progressPhase: null,
  progressSequence: null,
  progressBackend: null,
  progressPhaseStartedTotalElapsedMs: null,
  progressSyncedTotalElapsedMs: null,
  progressSyncedAtMonotonicMs: null,
  requestStartedAtMonotonicMs: null,
  totalElapsedMs: null,
  pinned: false,
};

const phaseRank = {
  checkingCache: 0,
  preparingRequest: 1,
  connectingBackend: 2,
  waitingForContent: 3,
  receivingContent: 4,
} as const;

const clearedActiveProgress = {
  progressPhase: null,
  progressSequence: null,
  progressBackend: null,
  progressPhaseStartedTotalElapsedMs: null,
  progressSyncedTotalElapsedMs: null,
  progressSyncedAtMonotonicMs: null,
  requestStartedAtMonotonicMs: null,
};

export const useTranslationStore = create<TranslationStore>((set, get) => ({
  ...initialState,
  togglePinned: () => set({ pinned: !get().pinned }),
  setPinned: (pinned) => set({ pinned }),
  reset: () => set({ ...initialState, pinned: get().pinned }),
  startRequest: (originalText, forceRefresh = false) => {
    const requestId = createTranslationRequestId();
    const current = get();
    const preserveCached =
      forceRefresh &&
      current.status === "success" &&
      current.fromCache &&
      current.originalText === originalText;
    set({
      requestId,
      originalText,
      translatedText: preserveCached ? current.translatedText : "",
      fromCache: preserveCached ? current.fromCache : false,
      status: preserveCached ? "refreshing" : "translating",
      errorMessage: null,
      errorKind: null,
      refreshErrorMessage: null,
      isPartial: false,
      ...clearedActiveProgress,
      requestStartedAtMonotonicMs: performance.now(),
      totalElapsedMs: null,
    });
    return requestId;
  },
  failCapture: (message, kind) => {
    set({
      requestId: null,
      originalText: "",
      translatedText: "",
      status: "error",
      errorMessage: message,
      errorKind: kind ?? null,
      isPartial: false,
      fromCache: false,
      refreshErrorMessage: null,
      ...clearedActiveProgress,
      totalElapsedMs: null,
    });
  },
  appendTranslationDelta: (requestId, delta) => {
    const current = get();
    if (
      current.requestId !== requestId ||
      !delta ||
      (current.status !== "translating" && current.status !== "streaming")
    ) {
      return false;
    }
    set((state) => ({
      translatedText: state.translatedText + delta,
      status: "streaming",
      errorMessage: null,
      errorKind: null,
      isPartial: false,
    }));
    return true;
  },
  applyProgressPhase: (requestId, event) => {
    const current = get();
    if (
      current.requestId !== requestId ||
      !Number.isInteger(event.sequence) ||
      event.sequence <= 0 ||
      !Number.isFinite(event.totalElapsedMs) ||
      event.totalElapsedMs < 0 ||
      (current.progressSequence !== null &&
        event.sequence <= current.progressSequence) ||
      (current.progressPhase !== null &&
        phaseRank[event.phase] < phaseRank[current.progressPhase])
    ) {
      return false;
    }
    const syncedAt = performance.now();
    set({
      progressPhase: event.phase,
      progressSequence: event.sequence,
      progressBackend:
        event.phase === "checkingCache" ? null : (event.backend ?? null),
      progressPhaseStartedTotalElapsedMs: event.totalElapsedMs,
      progressSyncedTotalElapsedMs: event.totalElapsedMs,
      progressSyncedAtMonotonicMs: syncedAt,
      totalElapsedMs: event.totalElapsedMs,
    });
    return true;
  },
  succeedRequest: (requestId, result) => {
    if (get().requestId !== requestId) return false;
    set({
      translatedText: result.translatedText,
      status: "success",
      isPartial: false,
      fromCache: result.fromCache,
      refreshErrorMessage: null,
      ...clearedActiveProgress,
      totalElapsedMs: result.totalElapsedMs,
    });
    return true;
  },
  failRequest: (
    requestId,
    message,
    kind,
    originalText,
    preservePartial = false,
    totalElapsedMs,
  ) => {
    if (get().requestId !== requestId) return false;
    set({
      originalText: originalText ?? get().originalText,
      translatedText: preservePartial ? get().translatedText : "",
      status: "error",
      errorMessage: message,
      errorKind: kind ?? null,
      isPartial: preservePartial && get().translatedText.length > 0,
      fromCache: false,
      refreshErrorMessage: null,
      ...clearedActiveProgress,
      totalElapsedMs: totalElapsedMs ?? null,
    });
    return true;
  },
  failRefreshRequest: (requestId, message, _kind, totalElapsedMs) => {
    const current = get();
    if (current.requestId !== requestId || current.status !== "refreshing") {
      return false;
    }
    set({
      status: "success",
      errorMessage: null,
      errorKind: null,
      refreshErrorMessage: message,
      ...clearedActiveProgress,
      totalElapsedMs: totalElapsedMs ?? null,
    });
    return true;
  },
  clearCacheSourceNotice: () => {
    set({ fromCache: false });
  },
  isActiveRequest: (requestId) => get().requestId === requestId,
}));

/**
 * 合并高频正文增量，避免每个上游 token 都触发一次 React 更新。
 * flush 仍会通过 requestId 检查，因此旧请求的定时器不会污染新请求。
 */
export function createTranslationDeltaBuffer(requestId: string) {
  let pending = "";
  let timer: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (!pending) return;
    const delta = pending;
    pending = "";
    useTranslationStore.getState().appendTranslationDelta(requestId, delta);
  };

  return {
    append(delta: string) {
      const current = useTranslationStore.getState();
      if (
        !delta ||
        !current.isActiveRequest(requestId) ||
        (current.status !== "translating" && current.status !== "streaming")
      ) {
        return;
      }
      pending += delta;
      if (!timer) {
        timer = setTimeout(flush, 50);
      }
    },
    flush,
    dispose() {
      if (timer) clearTimeout(timer);
      timer = null;
      pending = "";
    },
  };
}
