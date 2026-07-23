// 应用统一类型定义
// 第一轮（静态 UI）阶段：仅定义前端展示所需类型与占位常量，
// 尚未连接 Tauri Command 与真实后端。

/**
 * 翻译状态机
 * - idle: 空闲，等待用户触发
 * - capturing: 正在获取选中文本（占位，本轮不实际调用）
 * - translating: 正在调用大模型翻译
 * - success: 翻译成功
 * - error: 翻译失败
 */
export type TranslationStatus =
  | "idle"
  | "capturing"
  | "translating"
  | "success"
  | "error";

/**
 * 前端翻译状态
 * 阶段9：errorKind 用于查表得到友好文案与可重试标识
 */
export interface TranslationState {
  requestId: string | null;
  originalText: string;
  translatedText: string;
  status: TranslationStatus;
  errorMessage: string | null;
  errorKind: ErrorKind | null;
  pinned: boolean;
}

/**
 * 目标语言可选项
 */
export interface TargetLanguage {
  label: string;
  value: string;
}

export const TARGET_LANGUAGES: TargetLanguage[] = [
  { label: "简体中文", value: "简体中文" },
  { label: "繁體中文", value: "繁體中文" },
  { label: "English", value: "English" },
  { label: "日本語", value: "日本語" },
];

/**
 * 应用配置（与 Rust 端 AppConfig 对应）
 * 注意：apiKey 在原型阶段暂存于本地 JSON，存在安全风险，后续替换为系统凭据存储。
 */
export interface AppConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
  shortcut: string;
  targetLanguage: string;
  timeoutSeconds: number;
  autoHide: boolean;
  pinnedByDefault: boolean;
  maxTextLength: number;
}

export const DEFAULT_CONFIG: AppConfig = {
  baseUrl: "https://api.openai.com/v1",
  apiKey: "",
  model: "",
  shortcut: "Ctrl+T",
  targetLanguage: "简体中文",
  timeoutSeconds: 60,
  autoHide: true,
  pinnedByDefault: false,
  maxTextLength: 5000,
};

/**
 * 错误类型常量（与 Rust 端 AppError 对应）
 * 第一轮仅用于展示，后续将作为 Command 返回的错误标签。
 */
export const ERROR_KIND = {
  NoSelectedText: "NoSelectedText",
  TextTooLong: "TextTooLong",
  ClipboardError: "ClipboardError",
  ShortcutRegistrationFailed: "ShortcutRegistrationFailed",
  ConfigInvalid: "ConfigInvalid",
  ApiUnauthorized: "ApiUnauthorized",
  ApiRateLimited: "ApiRateLimited",
  ApiTimeout: "ApiTimeout",
  ApiRequestFailed: "ApiRequestFailed",
  ApiResponseInvalid: "ApiResponseInvalid",
  WindowError: "WindowError",
  Internal: "Internal",
} as const;

export type ErrorKind = (typeof ERROR_KIND)[keyof typeof ERROR_KIND];
