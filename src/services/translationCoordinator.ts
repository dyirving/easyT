import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "@/stores/settingsStore";
import {
  createTranslationRequestId,
  useTranslationStore,
} from "@/stores/translationStore";
import {
  captureSelectedText,
  positionWindowNearMouse,
  toCommandError,
  translateText,
} from "@/services/tauriCommands";

type RouteSetter = (route: "translation") => void;

let captureQueue: Promise<void> = Promise.resolve();

const isActiveRequest = (requestId: string) =>
  useTranslationStore.getState().isActiveRequest(requestId);

async function showWindowForRequest(requestId: string) {
  if (!isActiveRequest(requestId)) return false;
  try {
    const win = getCurrentWindow();
    await win.show();
    if (!isActiveRequest(requestId)) return false;
    await win.setFocus();
  } catch (e) {
    console.warn("[easyT] 显示窗口失败:", e);
  }
  return isActiveRequest(requestId);
}

async function captureForRequest(requestId: string) {
  if (!isActiveRequest(requestId)) return null;

  try {
    const { pinned } = useTranslationStore.getState();
    await positionWindowNearMouse(pinned);
  } catch (e) {
    console.warn("[easyT] 重新定位窗口失败:", e);
  }

  if (!isActiveRequest(requestId)) return null;

  try {
    const text = await captureSelectedText();
    return isActiveRequest(requestId) ? text : null;
  } catch (e) {
    if (!(await showWindowForRequest(requestId))) return null;
    const err = toCommandError(e);
    useTranslationStore
      .getState()
      .failRequest(requestId, err.message, err.kind);
    return null;
  }
}

async function translateAfterCapture(requestId: string, text: string) {
  if (!(await showWindowForRequest(requestId))) return;
  if (!useTranslationStore.getState().applyCapturedText(requestId, text)) return;

  const { config } = useSettingsStore.getState();
  try {
    const result = await translateText({
      text,
      targetLanguage: config.targetLanguage,
    });
    useTranslationStore
      .getState()
      .succeedRequest(requestId, result.translatedText);
  } catch (e) {
    const err = toCommandError(e);
    useTranslationStore
      .getState()
      .failRequest(requestId, err.message, err.kind, text);
  }
}

export function startShortcutTranslation(setRoute: RouteSetter) {
  setRoute("translation");

  const requestId = createTranslationRequestId();
  useTranslationStore.getState().beginCapture(requestId);

  const capture = captureQueue
    .catch(() => {
      /* 上一次失败不阻断新的捕获 */
    })
    .then(() => captureForRequest(requestId));

  captureQueue = capture.then(() => undefined, () => undefined);

  void capture.then((text) => {
    if (text) void translateAfterCapture(requestId, text);
  });
}
