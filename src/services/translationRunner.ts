import { ERROR_KIND, type AppConfig } from "@/types";
import {
  createTranslationDeltaBuffer,
  useTranslationStore,
} from "@/stores/translationStore";
import { toCommandError, translateText } from "@/services/tauriCommands";

/** 执行已启动的请求，并统一处理增量、终态和未完成译文。 */
export async function runTranslationRequest(
  requestId: string,
  text: string,
  config: AppConfig,
) {
  const deltaBuffer = config.streamOutput
    ? createTranslationDeltaBuffer(requestId)
    : null;

  try {
    const result = await translateText({
      requestId,
      text,
      targetLanguage: config.targetLanguage,
      streamOutput: config.streamOutput,
      onContentDelta: deltaBuffer?.append,
    });
    deltaBuffer?.flush();
    useTranslationStore
      .getState()
      .succeedRequest(requestId, result.translatedText);
  } catch (error) {
    deltaBuffer?.flush();
    const commandError = toCommandError(error);
    const current = useTranslationStore.getState();
    current.failRequest(
      requestId,
      commandError.message,
      commandError.kind,
      text,
      config.streamOutput &&
        commandError.kind !== ERROR_KIND.BackendCancelled &&
        current.isActiveRequest(requestId) &&
        !!current.translatedText,
    );
  } finally {
    deltaBuffer?.dispose();
  }
}
