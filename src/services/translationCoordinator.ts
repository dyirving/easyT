import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "@/stores/settingsStore";
import { type AppConfig, ERROR_KIND } from "@/types";
import { useTranslationStore } from "@/stores/translationStore";
import {
  captureSelectedText,
  positionWindowNearMouse,
  toCommandError,
} from "@/services/tauriCommands";
import { runTranslationRequest } from "@/services/translationRunner";

type RouteSetter = (route: "translation") => void;

let captureQueue: Promise<void> = Promise.resolve();

/** 显示并聚焦翻译窗口；失败只记录 warning，不阻断后续流程 */
async function showAndFocusWindow() {
  try {
    const win = getCurrentWindow();
    await win.show();
    await win.setFocus();
  } catch (e) {
    console.warn("[easyT] 显示窗口失败:", e);
  }
}

/** 无选区显示恢复：切到翻译界面并显示聚焦窗口，不改任何翻译状态 */
async function restoreTranslationWindow(setRoute: RouteSetter) {
  setRoute("translation");
  await showAndFocusWindow();
}

/** 有效文本：立即建立请求并启动翻译，窗口处理失败不阻断翻译 */
async function translateCapturedText(
  text: string,
  config: AppConfig,
  setRoute: RouteSetter,
) {
  const requestId = useTranslationStore.getState().startRequest(text);
  setRoute("translation");

  try {
    const { pinned } = useTranslationStore.getState();
    await positionWindowNearMouse(pinned);
  } catch (e) {
    console.warn("[easyT] 重新定位窗口失败:", e);
  }

  await showAndFocusWindow();
  await runTranslationRequest(requestId, text, config, false);
}

/** 处理捕获结果：有效文本、无选区或其他捕获故障三分支 */
async function handleCaptureResult(
  capture: Promise<string>,
  config: AppConfig,
  setRoute: RouteSetter,
) {
  let text: string;
  try {
    text = await capture;
  } catch (e) {
    const err = toCommandError(e);
    if (err.kind !== ERROR_KIND.NoSelectedText) {
      useTranslationStore.getState().failCapture(err.message, err.kind);
    }
    await restoreTranslationWindow(setRoute);
    return;
  }
  await translateCapturedText(text, config, setRoute);
}

export function startShortcutTranslation(setRoute: RouteSetter) {
  // 固定请求启动时的配置，捕获选区期间的设置修改只影响下一请求。
  const requestConfig = { ...useSettingsStore.getState().config };

  const capture = captureQueue
    .catch(() => {
      /* 上一次失败不阻断新的捕获 */
    })
    .then(() => captureSelectedText());

  captureQueue = capture.then(() => undefined, () => undefined);

  void handleCaptureResult(capture, requestConfig, setRoute);
}