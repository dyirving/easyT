// 翻译状态管理
import { create } from "zustand";
import { type ErrorKind, type TranslationState } from "@/types";

interface TranslationStore extends TranslationState {
  /** 切换固定状态 */
  togglePinned: () => void;
  setPinned: (pinned: boolean) => void;
  /** 重置为 idle */
  reset: () => void;
  /** 开始捕获选中文本 */
  beginCapture: (requestId: string) => void;
  /** 开始一次新的翻译请求（生成新 requestId） */
  startRequest: (originalText: string) => string;
  /** 将捕获到的文本绑定到当前请求 */
  applyCapturedText: (requestId: string, originalText: string) => boolean;
  /** 仅当 requestId 仍是最新请求时写入成功结果 */
  succeedRequest: (requestId: string, translatedText: string) => boolean;
  /** 仅当 requestId 仍是最新请求时写入错误结果 */
  failRequest: (
    requestId: string,
    message: string,
    kind?: ErrorKind,
    originalText?: string,
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
  pinned: false,
};

export const useTranslationStore = create<TranslationStore>((set, get) => ({
  ...initialState,
  togglePinned: () => set({ pinned: !get().pinned }),
  setPinned: (pinned) => set({ pinned }),
  reset: () => set({ ...initialState, pinned: get().pinned }),
  beginCapture: (requestId) => {
    set({
      requestId,
      originalText: "",
      translatedText: "",
      status: "capturing",
      errorMessage: null,
      errorKind: null,
    });
  },
  startRequest: (originalText) => {
    const requestId = createTranslationRequestId();
    set({
      requestId,
      originalText,
      translatedText: "",
      status: "translating",
      errorMessage: null,
      errorKind: null,
    });
    return requestId;
  },
  applyCapturedText: (requestId, originalText) => {
    if (get().requestId !== requestId) return false;
    set({
      originalText,
      translatedText: "",
      status: "translating",
      errorMessage: null,
      errorKind: null,
    });
    return true;
  },
  succeedRequest: (requestId, translatedText) => {
    if (get().requestId !== requestId) return false;
    set({ translatedText, status: "success" });
    return true;
  },
  failRequest: (requestId, message, kind, originalText) => {
    if (get().requestId !== requestId) return false;
    set({
      originalText: originalText ?? get().originalText,
      translatedText: "",
      status: "error",
      errorMessage: message,
      errorKind: kind ?? null,
    });
    return true;
  },
  isActiveRequest: (requestId) => get().requestId === requestId,
}));
