import { ERROR_KIND, type AppConfig } from "@/types";
import {
  createTranslationDeltaBuffer,
  useTranslationStore,
} from "@/stores/translationStore";
import { toCommandError, translateText } from "@/services/tauriCommands";

/**
 * 执行已启动的请求，并统一处理增量、终态和未完成译文。
 * forceRefresh=true 表示"重新翻译"（与 store 的 refreshing 状态配套）。
 */
export async function runTranslationRequest(
  requestId: string,
  text: string,
  config: AppConfig,
  forceRefresh = false,
) {
  const deltaBuffer = config.streamOutput
    ? createTranslationDeltaBuffer(requestId)
    : null;

  try {
    const result = await translateText({
      requestId,
      text,
      targetLanguage: config.targetLanguage,
      forceRefresh,
      onPhaseChanged: (event) =>
        useTranslationStore.getState().applyProgressPhase(requestId, event),
      onContentDelta: deltaBuffer?.append,
    });
    deltaBuffer?.flush();
    useTranslationStore.getState().succeedRequest(requestId, result);
  } catch (error) {
    deltaBuffer?.flush();
    const commandError = toCommandError(error);
    const current = useTranslationStore.getState();
    if (forceRefresh && current.status === "refreshing") {
      // 保留旧缓存译文与来源提示，单独记录刷新失败
      current.failRefreshRequest(
        requestId,
        commandError.message,
        commandError.kind,
        commandError.totalElapsedMs,
      );
      return;
    }
    current.failRequest(
      requestId,
      commandError.message,
      commandError.kind,
      text,
      config.streamOutput &&
        commandError.kind !== ERROR_KIND.BackendCancelled &&
        current.isActiveRequest(requestId) &&
        !!current.translatedText,
      commandError.totalElapsedMs,
    );
  } finally {
    deltaBuffer?.dispose();
  }
}
