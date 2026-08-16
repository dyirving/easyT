// Tauri 命令服务层
// 通过 invoke 调用 Rust 后端命令
// 翻译、捕获、快捷键和窗口相关命令的前端封装。
import { Channel, invoke } from "@tauri-apps/api/core";
import {
  type AppConfig,
  type CacheStats,
  type ErrorKind,
  type HistoryCommitOutcome,
  type HistorySnapshot,
  type SaveConfigResult,
  type TranslationHistoryEntry,
  type QwenSessionStatus,
  type QwenAccountPoolSnapshot,
  type TranslationPhase,
  type TranslationProgressBackend,
  type WebProviderKind,
  ERROR_KIND,
} from "@/types";

/** 统一命令错误 */
export interface CommandError {
  kind: ErrorKind;
  message: string;
  code?: string;
  totalElapsedMs?: number;
}

/**
 * 读取配置（真实持久化）
 */
export async function getConfig(): Promise<AppConfig> {
  return invoke<AppConfig>("get_config");
}

/**
 * 保存配置（校验失败不会覆盖原文件）
 */
export async function saveConfig(config: AppConfig): Promise<SaveConfigResult> {
  return invoke<SaveConfigResult>("save_config", { config });
}

export async function initializeTranslationHistory(): Promise<HistorySnapshot> {
  return invoke<HistorySnapshot>("initialize_translation_history");
}

export async function getTranslationHistoryEntry(
  entryId: string,
): Promise<TranslationHistoryEntry> {
  return invoke<TranslationHistoryEntry>("get_translation_history_entry", {
    entryId,
  });
}

export async function clearTranslationHistory(): Promise<{ clearedCount: number }> {
  return invoke<{ clearedCount: number }>("clear_translation_history");
}

export async function getTranslationCacheStats(): Promise<CacheStats> {
  return invoke<CacheStats>("get_translation_cache_stats");
}

export async function clearTranslationCache(): Promise<CacheStats> {
  return invoke<CacheStats>("clear_translation_cache");
}

/**
 * 捕获选中文本
 * 阶段7已接入：通过 Rust 端模拟 Ctrl+C + 剪贴板读写实现
 */
export async function captureSelectedText(): Promise<string> {
  return invoke<string>("capture_selected_text");
}

/**
 * 翻译文本
 * config 由 Rust 端从 AppState 读取，前端不携带 api_key
 * forceRefresh=true 表示"重新翻译"：绕过缓存读取，成功后覆盖共享缓存。
 */
export interface TranslateTextRequest {
  requestId: string;
  text: string;
  targetLanguage: string;
  forceRefresh: boolean;
  replaceEntryId?: string;
  onPhaseChanged: (event: PhaseChangedEvent) => void;
  onContentDelta?: (delta: string) => void;
}

export interface TranslationResult {
  translatedText: string;
  /** 是否来自本机缓存；未接入缓存时始终为 false */
  fromCache: boolean;
  totalElapsedMs: number;
  /** Rust 在成功返回前始终给出持久化结果。 */
  history: HistoryCommitOutcome;
}

export interface PhaseChangedEvent {
  type: "phaseChanged";
  requestId: string;
  sequence: number;
  phase: TranslationPhase;
  totalElapsedMs: number;
  backend?: TranslationProgressBackend;
}

type TranslationProgressEvent = PhaseChangedEvent | {
  type: "contentDelta";
  requestId: string;
  delta: string;
};

export async function translateText(
  request: TranslateTextRequest,
): Promise<TranslationResult> {
  const channel = new Channel<TranslationProgressEvent>((event) => {
    if (event.requestId !== request.requestId) return;
    if (event.type === "phaseChanged") {
      request.onPhaseChanged(event);
    } else {
      request.onContentDelta?.(event.delta);
    }
  });

  return invoke<TranslationResult>("translate_text", {
    requestId: request.requestId,
    text: request.text,
    targetLanguage: request.targetLanguage,
    forceRefresh: request.forceRefresh,
    replaceEntryId: request.replaceEntryId ?? null,
    onEvent: channel,
  });
}

/**
 * 测试当前草稿配置的 API 连接；Rust 端按 streamOutput 选择测试模式。
 */
export async function testApiConnection(
  config: AppConfig,
): Promise<{ ok: boolean; message: string }> {
  try {
    const message = await invoke<string>("test_api_connection", { config });
    return { ok: true, message };
  } catch (e) {
    const error = toCommandError(e);
    return {
      ok: false,
      message: formatCommandError(error),
    };
  }
}

// ===== WebGateway 登录管理 =====

/**
 * 启动 Qwen 网页登录流程
 * 非阻塞：立即返回当前状态，不等待用户完成登录。
 * 后台 watcher 会读取 tongyi_sso_ticket Cookie，找到后保存并切到 Ready。
 */
export async function beginWebLogin(
  provider: WebProviderKind,
): Promise<QwenSessionStatus> {
  return invoke<QwenSessionStatus>("begin_web_login", { provider });
}

/**
 * 查询当前登录状态
 * 仅在 SettingsPage 可见且状态为 loggingIn 时每 1 秒调用一次
 */
export async function getWebLoginStatus(
  provider: WebProviderKind,
): Promise<QwenSessionStatus> {
  return invoke<QwenSessionStatus>("get_web_login_status", { provider });
}

export async function getQwenAccountPool(): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("get_qwen_account_pool");
}

export async function createQwenAccount(displayName: string): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("create_qwen_account", { displayName });
}

export async function beginQwenAccountLogin(accountId: string): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("begin_qwen_account_login", { accountId });
}

export async function renameQwenAccount(
  accountId: string,
  displayName: string,
): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("rename_qwen_account", { accountId, displayName });
}

export async function setQwenAccountEnabled(
  accountId: string,
  enabled: boolean,
): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("set_qwen_account_enabled", { accountId, enabled });
}

export async function moveQwenAccount(
  accountId: string,
  direction: "up" | "down",
): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("move_qwen_account", { accountId, direction });
}

export async function logoutQwenAccount(accountId: string): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("logout_qwen_account", { accountId });
}

export async function deleteQwenAccount(accountId: string): Promise<QwenAccountPoolSnapshot> {
  return invoke<QwenAccountPoolSnapshot>("delete_qwen_account", { accountId });
}

export async function testQwenAccount(
  accountId: string,
  config: AppConfig,
): Promise<string> {
  return invoke<string>("test_qwen_account", { accountId, config });
}

/**
 * 退出登录：关闭登录窗口、取消 watcher、清除凭证与 profile
 * 显式 destructive 操作，UI 应二次确认
 */
export async function logoutWebAccount(
  provider: WebProviderKind,
): Promise<QwenSessionStatus> {
  return invoke<QwenSessionStatus>("logout_web_account", { provider });
}

/**
 * 把主窗口重新定位到鼠标附近
 * 阶段8：在快捷键触发翻译前调用，让窗口即时出现在鼠标附近
 * pinned=true 时 Rust 端会跳过重新定位
 */
export async function positionWindowNearMouse(pinned: boolean): Promise<void> {
  await invoke<void>("position_window_near_mouse", { pinned });
}
export async function copyTranslation(text: string): Promise<void> {
  await invoke<void>("copy_translation", { text });
}

/**
 * 把 invoke 抛出的错误统一转换为 CommandError
 * 用于在 UI 中展示友好的错误文案
 */
export function toCommandError(e: unknown): CommandError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    const kind = (e as { kind: string }).kind as ErrorKind;
    const message = (e as { message: string }).message;
    const rawCode = (e as { code?: unknown }).code;
    const code = typeof rawCode === "string"
      ? rawCode
      : undefined;
    const rawElapsed = (e as { totalElapsedMs?: unknown }).totalElapsedMs;
    const totalElapsedMs =
      typeof rawElapsed === "number" &&
      Number.isFinite(rawElapsed) &&
      rawElapsed >= 0
        ? rawElapsed
        : undefined;
    return { kind, message, code, totalElapsedMs };
  }
  return {
    kind: ERROR_KIND.Internal,
    message: e instanceof Error ? e.message : "未知错误",
  };
}

/**
 * 阶段9：把 CommandError 转换为面向用户的友好文案 + 可重试标识
 * - friendlyMessage：用于 ErrorState 主文案
 * - hint：副提示（如何修复）
 * - retryable：true 时显示"重试"按钮，false 时仅显示"打开设置"
 *
 * 区分原则：
 * - 网络/超时/响应解析：可重试（瞬时故障）
 * - 配置/未授权/限流：不可重试（需用户介入修改配置）
 */
export interface FriendlyError {
  friendlyMessage: string;
  hint?: string;
  retryable: boolean;
  code?: string;
}

export function formatCommandError(err: Pick<CommandError, "message" | "code">): string {
  return err.code ? `${err.message} [${err.code}]` : err.message;
}

export function toFriendlyError(err: CommandError): FriendlyError {
  const withCode = (friendly: Omit<FriendlyError, "code">): FriendlyError => ({
    ...friendly,
    friendlyMessage: err.code ? err.message : friendly.friendlyMessage,
    code: err.code,
  });
  switch (err.kind) {
    case ERROR_KIND.NoSelectedText:
      return withCode({
        friendlyMessage: "未检测到选中文本",
        hint: "请在其他应用中选中英文文本后再按快捷键",
        retryable: false,
      });
    case ERROR_KIND.TextTooLong:
      return withCode({
        friendlyMessage: err.message || "文本过长",
        hint: "可在设置中调大最大翻译字符数",
        retryable: false,
      });
    case ERROR_KIND.ClipboardError:
      return withCode({
        friendlyMessage: err.message || "剪贴板操作失败",
        hint: "可能被其他应用占用，请稍后重试",
        retryable: true,
      });
    case ERROR_KIND.ApiUnauthorized:
      return withCode({
        friendlyMessage: "API Key 无效或未配置 (401)",
        hint: "请到设置页检查 API Key 与 Base URL",
        retryable: false,
      });
    case ERROR_KIND.ApiRateLimited:
      return withCode({
        friendlyMessage: "请求过于频繁 (429)",
        hint: "请稍后再试，或检查账户额度",
        retryable: true,
      });
    case ERROR_KIND.ApiTimeout:
      return withCode({
        friendlyMessage: "请求超时",
        hint: "请检查网络，或在设置中增大超时时间",
        retryable: true,
      });
    case ERROR_KIND.LoginRequired:
      return withCode({
        friendlyMessage: "请先在设置中登录 Qwen",
        hint: "WebGateway 模式需要登录 Qwen 账号才能翻译",
        retryable: false,
      });
    case ERROR_KIND.SessionExpired:
      return withCode({
        friendlyMessage: "Qwen 登录状态已过期",
        hint: "请到设置页重新登录 Qwen",
        retryable: false,
      });
    case ERROR_KIND.BackendCancelled:
      return withCode({
        friendlyMessage: err.message || "翻译请求已被新请求取代",
        hint: "连续触发翻译时旧请求会被取消，属于正常行为",
        retryable: false,
      });
    case ERROR_KIND.BackendNetwork:
      return withCode({
        friendlyMessage: "网络请求失败",
        hint: "请检查网络连接，或稍后重试",
        retryable: true,
      });
    case ERROR_KIND.BackendProtocolMismatch:
      return withCode({
        friendlyMessage: "Qwen 网页协议已变化",
        hint: "请切换 Official API 或更新 easyT",
        retryable: false,
      });
    case ERROR_KIND.BackendPartialResponse:
      return withCode({
        friendlyMessage: "上游响应不完整",
        hint: "翻译过程被中断，请重试",
        retryable: true,
      });
    case ERROR_KIND.BackendInvalidResponse:
      return withCode({
        friendlyMessage: "响应格式无效",
        hint: "Qwen 返回内容无法解析，请重试或切换 Official API",
        retryable: true,
      });
    case ERROR_KIND.BackendStreamingUnsupported:
      return withCode({
        friendlyMessage: "当前后端不支持流式输出",
        hint: "请在设置中关闭“流式输出”后重试",
        retryable: false,
      });
    case ERROR_KIND.ConfigInvalid:
      return withCode({
        friendlyMessage: err.message || "配置无效",
        hint: "请到设置页修正配置后保存",
        retryable: false,
      });
    case ERROR_KIND.ShortcutRegistrationFailed:
      return withCode({
        friendlyMessage: err.message || "快捷键注册失败",
        hint: "可能被其他应用占用，请尝试更换组合键",
        retryable: false,
      });
    case ERROR_KIND.WindowError:
      return withCode({
        friendlyMessage: err.message || "窗口操作失败",
        retryable: false,
      });
    case ERROR_KIND.HistoryOperationFailed:
      return withCode({
        friendlyMessage: err.message || "翻译历史操作失败",
        hint: "翻译仍可继续，请稍后重试历史操作",
        retryable: true,
      });
    case ERROR_KIND.Internal:
    default:
      return {
        friendlyMessage: err.message || "内部错误",
        hint: "请重启应用，若问题持续请反馈",
        retryable: false,
      };
  }
}
