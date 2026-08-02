import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "@/stores/settingsStore";
import { type AppConfig } from "@/types";
import { createTranslationRequestId, useTranslationStore } from "@/stores/translationStore";
import {
  captureSelectedText,
  positionWindowNearMouse,
  toCommandError,
} from "@/services/tauriCommands";
import { runTranslationRequest } from "@/services/translationRunner";

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

async function translateAfterCapture(
  requestId: string,
  text: string,
  config: AppConfig,
) {
  if (!(await showWindowForRequest(requestId))) return;
  if (!useTranslationStore.getState().applyCapturedText(requestId, text)) return;

  await runTranslationRequest(requestId, text, config);
}

export function startShortcutTranslation(setRoute: RouteSetter) {
  setRoute("translation");

  const requestId = createTranslationRequestId();
  // 固定请求启动时的配置，捕获选区期间的设置修改只影响下一请求。
  const requestConfig = { ...useSettingsStore.getState().config };
  useTranslationStore.getState().beginCapture(requestId);

  const capture = captureQueue
    .catch(() => {
      /* 上一次失败不阻断新的捕获 */
    })
    .then(() => captureForRequest(requestId));

  captureQueue = capture.then(() => undefined, () => undefined);

  void capture.then((text) => {
    if (text) void translateAfterCapture(requestId, text, requestConfig);
  });
}
