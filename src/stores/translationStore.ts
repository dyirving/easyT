// 翻译状态管理
import { create } from "zustand";
import { type ErrorKind, type TranslationState } from "@/types";

interface TranslationStore extends TranslationState {
  /** 切换固定状态 */
  togglePinned: () => void;
  setPinned: (pinned: boolean) => void;
  /** 重置为 idle */
  reset: () => void;
  /** 开始一次新的翻译请求（生成新 requestId） */
  startRequest: (originalText: string) => string;
  /** 捕获故障：原子切换到无原文错误态并使旧请求失效 */
  failCapture: (message: string, kind?: ErrorKind) => void;
  /** 仅当 requestId 仍是最新请求时写入成功结果 */
  succeedRequest: (requestId: string, translatedText: string) => boolean;
  /** 仅当 requestId 仍是最新请求时追加正文增量 */
  appendTranslationDelta: (requestId: string, delta: string) => boolean;
  /** 仅当 requestId 仍是最新请求时写入错误结果 */
  failRequest: (
    requestId: string,
    message: string,
    kind?: ErrorKind,
    originalText?: string,
    preservePartial?: boolean,
  ) => boolean;
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
  pinned: false,
};

export const useTranslationStore = create<TranslationStore>((set, get) => ({
  ...initialState,
  togglePinned: () => set({ pinned: !get().pinned }),
  setPinned: (pinned) => set({ pinned }),
  reset: () => set({ ...initialState, pinned: get().pinned }),
  startRequest: (originalText) => {
    const requestId = createTranslationRequestId();
    set({
      requestId,
      originalText,
      translatedText: "",
      status: "translating",
      errorMessage: null,
      errorKind: null,
      isPartial: false,
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
  succeedRequest: (requestId, translatedText) => {
    if (get().requestId !== requestId) return false;
    set({ translatedText, status: "success", isPartial: false });
    return true;
  },
  failRequest: (requestId, message, kind, originalText, preservePartial = false) => {
    if (get().requestId !== requestId) return false;
    set({
      originalText: originalText ?? get().originalText,
      translatedText: preservePartial ? get().translatedText : "",
      status: "error",
      errorMessage: message,
      errorKind: kind ?? null,
      isPartial: preservePartial && get().translatedText.length > 0,
    });
    return true;
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
