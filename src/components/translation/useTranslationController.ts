import { useState } from "react";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationStore } from "@/stores/translationStore";
import {
  copyTranslation,
  setWindowPinned,
  toCommandError,
  toFriendlyError,
} from "@/services/tauriCommands";
import { runTranslationRequest } from "@/services/translationRunner";

const DEFAULT_MANUAL_INPUT =
  "Large language models are trained on massive text corpora.";

/** Owns translation actions and state so the page remains a UI composition. */
export function useTranslationController() {
  const translation = useTranslationStore();
  const { config } = useSettingsStore();
  const [copied, setCopied] = useState(false);
  const [manualInput, setManualInput] = useState(DEFAULT_MANUAL_INPUT);
  const isBusy = ["translating", "streaming", "refreshing"].includes(
    translation.status,
  );

  const translate = async (text: string, forceRefresh = false) => {
    const requestId = translation.startRequest(text, forceRefresh);
    if (!text.trim()) {
      translation.failRequest(
        requestId,
        "未检测到选中文本，请在其他应用中选中英文后再按快捷键。",
        "NoSelectedText",
      );
      return;
    }
    if (text.length > config.maxTextLength) {
      translation.failRequest(
        requestId,
        `文本过长（${text.length} 字符），已超过配置上限 ${config.maxTextLength}。`,
        "TextTooLong",
        text,
      );
      return;
    }
    await runTranslationRequest(requestId, text, { ...config }, forceRefresh);
  };

  const retry = () => {
    if (translation.originalText) void translate(translation.originalText, true);
  };

  const togglePin = () => {
    const next = !translation.pinned;
    translation.togglePinned();
    setWindowPinned(next).catch((error) => {
      const commandError = toCommandError(error);
      console.warn("[easyT] set_window_pinned 失败:", commandError.message);
    });
  };

  const copy = async () => {
    if (!translation.translatedText) return;
    const markCopied = () => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    };
    try {
      await copyTranslation(translation.translatedText);
      markCopied();
    } catch (error) {
      const commandError = toCommandError(error);
      console.warn("[easyT] copy_translation 失败:", commandError.message);
      try {
        await navigator.clipboard.writeText(translation.translatedText);
        markCopied();
      } catch {
        // Both native and browser clipboard access can be unavailable.
      }
    }
  };

  const friendlyError =
    translation.status === "error"
      ? toFriendlyError({
          kind: translation.errorKind ?? "Internal",
          message: translation.errorMessage ?? "",
        })
      : null;

  return {
    ...translation,
    config,
    copied,
    manualInput,
    setManualInput,
    isBusy,
    friendlyError,
    translate,
    retry,
    togglePin,
    copy,
  };
}
