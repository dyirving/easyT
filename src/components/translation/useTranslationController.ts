import { useState } from "react";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationStore } from "@/stores/translationStore";
import { useTranslationHistoryStore } from "@/stores/translationHistoryStore";
import {
  clearTranslationHistory,
  copyTranslation,
  getTranslationHistoryEntry,
  toCommandError,
  toFriendlyError,
} from "@/services/tauriCommands";
import { runTranslationRequest } from "@/services/translationRunner";

async function writeClipboard(text: string) {
  try {
    await copyTranslation(text);
  } catch {
    await navigator.clipboard.writeText(text);
  }
}

/** Owns active translation and persistent-history orchestration. */
export function useTranslationController() {
  const translation = useTranslationStore();
  const history = useTranslationHistoryStore();
  const { config } = useSettingsStore();
  const [copied, setCopied] = useState(false);
  const isBusy = ["translating", "streaming", "refreshing"].includes(
    translation.status,
  );

  const translate = async (
    text: string,
    forceRefresh = false,
    replaceEntryId?: string,
  ) => {
    history.prepareForNewRequest();
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
    if (replaceEntryId) {
      await runTranslationRequest(
        requestId,
        text,
        { ...config },
        forceRefresh,
        replaceEntryId,
      );
    } else {
      await runTranslationRequest(requestId, text, { ...config }, forceRefresh);
    }
  };

  const latestSummary = history.summaries[0];
  const latestBody = latestSummary
    ? history.bodiesById[latestSummary.entryId]
    : undefined;
  const hasActiveView = translation.status !== "idle";
  const topPersisted = !hasActiveView ? latestSummary : undefined;
  const historyRows = topPersisted
    ? history.summaries.slice(1)
    : history.summaries;

  const loadEntry = async (entryId: string) => {
    const cached = useTranslationHistoryStore.getState().bodiesById[entryId];
    if (cached) return cached;
    history.setEntryLoading(entryId, true);
    history.setActionError(null);
    try {
      const entry = await getTranslationHistoryEntry(entryId);
      history.cacheBody(entry);
      return entry;
    } catch (error) {
      history.setActionError(toCommandError(error).message);
      return null;
    } finally {
      history.setEntryLoading(entryId, false);
    }
  };

  const toggleHistoryEntry = (entryId: string, open: boolean) => {
    history.setExpanded(entryId, open);
    if (open && !history.bodiesById[entryId]) void loadEntry(entryId);
  };

  const copyEntry = async (entryId: string) => {
    history.setPendingAction(entryId, "copyAll");
    history.setActionError(null);
    try {
      const entry = await loadEntry(entryId);
      if (!entry) return;
      await writeClipboard(`${entry.originalText}\n\n${entry.translatedText}`);
    } catch (error) {
      history.setActionError(toCommandError(error).message);
    } finally {
      history.setPendingAction(entryId, undefined);
    }
  };

  const retranslateEntry = async (entryId: string) => {
    history.setPendingAction(entryId, "retranslate");
    const entry = await loadEntry(entryId);
    history.setPendingAction(entryId, undefined);
    if (entry) await translate(entry.originalText, true, entryId);
  };

  const retry = () => {
    if (translation.originalText) {
      void translate(translation.originalText, true);
    } else if (latestSummary) {
      void retranslateEntry(latestSummary.entryId);
    }
  };

  const togglePin = () => {
    translation.togglePinned();
  };

  const topTranslatedText =
    translation.status === "success"
      ? translation.translatedText
      : topPersisted
        ? latestBody?.translatedText ?? ""
        : "";

  const copyTop = async () => {
    if (!topTranslatedText) return;
    try {
      await writeClipboard(topTranslatedText);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (error) {
      console.warn(
        "[easyT] copy_translation 失败:",
        toCommandError(error).message,
      );
      history.setActionError(toCommandError(error).message);
    }
  };

  const confirmClear = async () => {
    history.setClearPending();
    history.setActionError(null);
    try {
      await clearTranslationHistory();
      history.clearSucceeded();
      translation.reset();
    } catch (error) {
      history.cancelClear();
      history.setActionError(toCommandError(error).message);
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
    history,
    copied,
    isBusy,
    friendlyError,
    topPersisted,
    topPersistedBody: topPersisted ? latestBody : undefined,
    historyRows,
    topTranslatedText,
    translate,
    retry,
    togglePin,
    copyTop,
    toggleHistoryEntry,
    copyEntry,
    retranslateEntry,
    confirmClear,
  };
}
