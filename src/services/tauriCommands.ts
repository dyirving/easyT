// Tauri 命令服务层
// 通过 invoke 调用 Rust 后端命令
// 翻译/捕获/快捷键/窗口相关命令在后续阶段接入真实逻辑
import { invoke } from "@tauri-apps/api/core";
import {
  type AppConfig,
  type ErrorKind,
  type QwenSessionStatus,
  type WebProviderKind,
  ERROR_KIND,
} from "@/types";

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
 * 以下为窗口相关命令
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
    case ERROR_KIND.LoginRequired:
      return {
        friendlyMessage: "请先在设置中登录 Qwen",
        hint: "WebGateway 模式需要登录 Qwen 账号才能翻译",
        retryable: false,
      };
    case ERROR_KIND.SessionExpired:
      return {
        friendlyMessage: "Qwen 登录状态已过期",
        hint: "请到设置页重新登录 Qwen",
        retryable: false,
      };
    case ERROR_KIND.BackendCancelled:
      return {
        friendlyMessage: err.message || "翻译请求已被新请求取代",
        hint: "连续触发翻译时旧请求会被取消，属于正常行为",
        retryable: false,
      };
    case ERROR_KIND.BackendNetwork:
      return {
        friendlyMessage: "网络请求失败",
        hint: "请检查网络连接，或稍后重试",
        retryable: true,
      };
    case ERROR_KIND.BackendProtocolMismatch:
      return {
        friendlyMessage: "Qwen 网页协议已变化",
        hint: "请切换 Official API 或更新 easyT",
        retryable: false,
      };
    case ERROR_KIND.BackendPartialResponse:
      return {
        friendlyMessage: "上游响应不完整",
        hint: "翻译过程被中断，请重试",
        retryable: true,
      };
    case ERROR_KIND.BackendInvalidResponse:
      return {
        friendlyMessage: "响应格式无效",
        hint: "Qwen 返回内容无法解析，请重试或切换 Official API",
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
