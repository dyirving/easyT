// Tauri 命令服务层
// 通过 invoke 调用 Rust 后端命令
// 翻译/捕获/快捷键/窗口相关命令在后续阶段接入真实逻辑
import { invoke } from "@tauri-apps/api/core";
import { type AppConfig, type ErrorKind, ERROR_KIND } from "@/types";

/** 统一命令错误 */
export interface CommandError {
  kind: ErrorKind;
  message: string;
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
export async function saveConfig(config: AppConfig): Promise<void> {
  await invoke<void>("save_config", { config });
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
 */
export interface TranslateTextRequest {
  text: string;
  targetLanguage: string;
}

export interface TranslationResult {
  translatedText: string;
}

export async function translateText(
  request: TranslateTextRequest,
): Promise<TranslationResult> {
  return invoke<TranslationResult>("translate_text", {
    text: request.text,
    targetLanguage: request.targetLanguage,
  });
}

/**
 * 测试 API 连接（占位，下一轮接入）
 */
export async function testApiConnection(
  config: AppConfig,
): Promise<{ ok: boolean; message: string }> {
  try {
    const message = await invoke<string>("test_api_connection", { config });
    return { ok: true, message };
  } catch (e) {
    return {
      ok: false,
      message:
        e instanceof Object && "message" in e
          ? String((e as { message: string }).message)
          : "连接失败",
    };
  }
}

/**
 * 以下为窗口/快捷键相关命令，第一轮暂不调用，签名已对齐
 */
export async function showTranslationWindow(): Promise<void> {
  await invoke<void>("show_translation_window");
}
export async function hideTranslationWindow(): Promise<void> {
  await invoke<void>("hide_translation_window");
}
export async function setWindowPinned(pinned: boolean): Promise<void> {
  await invoke<void>("set_window_pinned", { pinned });
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
export async function registerShortcut(shortcut: string): Promise<void> {
  await invoke<void>("register_shortcut", { shortcut });
}

/**
 * 把 invoke 抛出的错误统一转换为 CommandError
 * 用于在 UI 中展示友好的错误文案
 */
export function toCommandError(e: unknown): CommandError {
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    const kind = (e as { kind: string }).kind as ErrorKind;
    const message = (e as { message: string }).message;
    return { kind, message };
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
}

export function toFriendlyError(err: CommandError): FriendlyError {
  switch (err.kind) {
    case ERROR_KIND.NoSelectedText:
      return {
        friendlyMessage: "未检测到选中文本",
        hint: "请在其他应用中选中英文文本后再按快捷键",
        retryable: false,
      };
    case ERROR_KIND.TextTooLong:
      return {
        friendlyMessage: err.message || "文本过长",
        hint: "可在设置中调大最大翻译字符数",
        retryable: false,
      };
    case ERROR_KIND.ClipboardError:
      return {
        friendlyMessage: err.message || "剪贴板操作失败",
        hint: "可能被其他应用占用，请稍后重试",
        retryable: true,
      };
    case ERROR_KIND.ApiUnauthorized:
      return {
        friendlyMessage: "API Key 无效或未配置 (401)",
        hint: "请到设置页检查 API Key 与 Base URL",
        retryable: false,
      };
    case ERROR_KIND.ApiRateLimited:
      return {
        friendlyMessage: "请求过于频繁 (429)",
        hint: "请稍后再试，或检查账户额度",
        retryable: true,
      };
    case ERROR_KIND.ApiTimeout:
      return {
        friendlyMessage: "请求超时",
        hint: "请检查网络，或在设置中增大超时时间",
        retryable: true,
      };
    case ERROR_KIND.ApiRequestFailed:
      return {
        friendlyMessage: err.message || "网络请求失败",
        hint: "请检查网络与 Base URL 是否可达",
        retryable: true,
      };
    case ERROR_KIND.ApiResponseInvalid:
      return {
        friendlyMessage: err.message || "响应格式无效",
        hint: "服务端返回内容不符合 OpenAI 协议",
        retryable: true,
      };
    case ERROR_KIND.ConfigInvalid:
      return {
        friendlyMessage: err.message || "配置无效",
        hint: "请到设置页修正配置后保存",
        retryable: false,
      };
    case ERROR_KIND.ShortcutRegistrationFailed:
      return {
        friendlyMessage: err.message || "快捷键注册失败",
        hint: "可能被其他应用占用，请尝试更换组合键",
        retryable: false,
      };
    case ERROR_KIND.WindowError:
      return {
        friendlyMessage: err.message || "窗口操作失败",
        retryable: false,
      };
    case ERROR_KIND.Internal:
    default:
      return {
        friendlyMessage: err.message || "内部错误",
        hint: "请重启应用，若问题持续请反馈",
        retryable: false,
      };
  }
}
