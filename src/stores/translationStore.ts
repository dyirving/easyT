// 翻译状态管理
// 第一轮（静态 UI）：提供 idle/loading/success/error 四种状态的切换入口，
// 用于演示界面。下一轮将接入 Tauri Command。
// 阶段9：错误同时记录 errorKind，用于查表得到友好文案与可重试标识
import { create } from "zustand";
import {
  type ErrorKind,
  type TranslationStatus,
  type TranslationState,
} from "@/types";

interface TranslationStore extends TranslationState {
  /** 设置整体状态 */
  setStatus: (status: TranslationStatus) => void;
  /** 设置原文 */
  setOriginalText: (text: string) => void;
  /** 设置译文 */
  setTranslatedText: (text: string) => void;
  /** 设置错误（带 kind）。传 null 清除错误 */
  setError: (message: string | null, kind?: ErrorKind) => void;
  /** 切换固定状态 */
  togglePinned: () => void;
  setPinned: (pinned: boolean) => void;
  /** 重置为 idle */
  reset: () => void;
  /** 开始一次新的翻译请求（生成新 requestId） */
  startRequest: (originalText: string) => string;
}

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
  setStatus: (status) => set({ status }),
  setOriginalText: (originalText) => set({ originalText }),
  setTranslatedText: (translatedText) => set({ translatedText }),
  setError: (errorMessage, kind) =>
    set({
      errorMessage,
      errorKind: errorMessage === null ? null : (kind ?? null),
    }),
  togglePinned: () => set({ pinned: !get().pinned }),
  setPinned: (pinned) => set({ pinned }),
  reset: () => set({ ...initialState, pinned: get().pinned }),
  startRequest: (originalText) => {
    const requestId = `req_${Date.now()}_${Math.random()
      .toString(36)
      .slice(2, 8)}`;
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
}));
