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
  "idle" | "capturing" | "translating" | "success" | "error";

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
 * 模型供应商标识
 * - agnes / deepseek / qwen / glm / kimi / doubao：内置供应商，Base URL 与模型列表由前端常量提供
 * - custom：自定义供应商，用户自行填写 Base URL 与模型名称
 */
export type ModelProvider =
  | "agnes"
  | "deepseek"
  | "qwen"
  | "glm"
  | "kimi"
  | "doubao"
  | "custom";

/** 内置供应商的模型可选项 */
export interface ProviderModelOption {
  label: string;
  value: string;
}

/** 内置供应商定义 */
export interface ModelProviderPreset {
  /** 供应商标识，对应 AppConfig.provider */
  value: ModelProvider;
  /** 展示名称 */
  label: string;
  /** 该供应商的 OpenAI 兼容 Base URL */
  baseUrl: string;
  /** 内置模型列表（用户从中选择，无需手动填写） */
  models: ProviderModelOption[];
  /** 是否推荐（用于排序/标记） */
  recommended?: boolean;
  /** 官网地址，供用户获取 API Key */
  website: string;
}

/**
 * 内置模型供应商列表
 * Base URL 与模型名称均来源于各厂商官方文档（OpenAI 兼容入口）：
 * - Agnes:  https://agnes-ai.com/           → https://apihub.agnes-ai.com/v1
 * - DeepSeek: https://api-docs.deepseek.com/ → https://api.deepseek.com
 * - Qwen:   https://help.aliyun.com/zh/model-studio/ → https://dashscope.aliyuncs.com/compatible-mode/v1
 * - GLM:    https://docs.bigmodel.cn/        → https://open.bigmodel.cn/api/paas/v4
 * - Kimi:   https://platform.moonshot.cn/    → https://api.moonshot.cn/v1
 * - DouBao: https://www.volcengine.com/docs/82379/ → https://ark.cn-beijing.volces.com/api/v3
 */
export const MODEL_PROVIDERS: ModelProviderPreset[] = [
  {
    value: "agnes",
    label: "Agnes（推荐）",
    baseUrl: "https://apihub.agnes-ai.com/v1",
    recommended: true,
    website: "https://agnes-ai.com/",
    models: [
      { label: "Agnes-2.0-Flash", value: "agnes-2.0-flash" },
    ],
  },
  {
    value: "deepseek",
    label: "DeepSeek",
    baseUrl: "https://api.deepseek.com",
    website: "https://api-docs.deepseek.com/",
    models: [
      { label: "DeepSeek-V4-Flash（默认非思考）", value: "deepseek-v4-flash" },
      { label: "DeepSeek-V4-Pro", value: "deepseek-v4-pro" },
    ],
  },
  {
    value: "qwen",
    label: "Qwen（通义千问）",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    website: "https://help.aliyun.com/zh/model-studio/",
    models: [
      { label: "qwen3.7-max（旗舰）", value: "qwen3.7-max" },
      { label: "qwen3.7-plus（均衡）", value: "qwen3.7-plus" },
      { label: "qwen3.6-flash（轻量）", value: "qwen3.6-flash" },
    ],
  },
  {
    value: "glm",
    label: "GLM（智谱）",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    website: "https://docs.bigmodel.cn/",
    models: [
      { label: "GLM-5", value: "glm-5" },
      { label: "GLM-5.1", value: "glm-5.1" },
      { label: "GLM-5.2", value: "glm-5.2" },
    ],
  },
  {
    value: "kimi",
    label: "Kimi（月之暗面）",
    baseUrl: "https://api.moonshot.cn/v1",
    website: "https://platform.moonshot.cn/",
    models: [
      { label: "kimi-k2.5（最新多模态）", value: "kimi-k2.5" },
      { label: "kimi-k2-0905-preview", value: "kimi-k2-0905-preview" },
      { label: "kimi-k2-turbo-preview（高速）", value: "kimi-k2-turbo-preview" },
      { label: "moonshot-v1-128k（长文本）", value: "moonshot-v1-128k" },
    ],
  },
  {
    value: "doubao",
    label: "DouBao（豆包）",
    baseUrl: "https://ark.cn-beijing.volces.com/api/v3",
    website: "https://www.volcengine.com/docs/82379/",
    models: [
      { label: "doubao-seed-1.6（250615）", value: "doubao-seed-1-6-250615" },
      { label: "doubao-seed-1.6-flash（250828）", value: "doubao-seed-1-6-flash-250828" },
      { label: "doubao-seed-1.6-thinking（250715）", value: "doubao-seed-1-6-thinking-250715" },
    ],
  },
  {
    value: "custom",
    label: "自定义供应商",
    baseUrl: "",
    website: "",
    models: [],
  },
];

/** 默认供应商标识 */
export const DEFAULT_PROVIDER: ModelProvider = "agnes";

/** 根据 provider 标识查找内置供应商预设 */
export function getProviderPreset(
  provider: ModelProvider,
): ModelProviderPreset | undefined {
  return MODEL_PROVIDERS.find((p) => p.value === provider);
}

/**
 * 应用配置（与 Rust 端 AppConfig 对应）
 */
export interface AppConfig {
  /** 模型供应商标识；非 custom 时 Base URL/模型从内置列表选取 */
  provider: ModelProvider;
  /**
   * 各供应商独立的 API Key 存储
   * key 为 ModelProvider 字符串，value 为该供应商的 API Key
   * `apiKey` 字段始终等于 `apiKeys[provider]`，由前端维护一致性
   */
  apiKeys: Partial<Record<ModelProvider, string>>;
  baseUrl: string;
  /** 当前激活的 API Key（= apiKeys[provider]），供 LLM 客户端直接使用 */
  apiKey: string;
  model: string;
  /**
   * 是否启用模型思考模式
   * false（默认）：翻译场景注入关闭思考参数，省 token、降延迟
   * true：保留各供应商默认思考行为，复杂语境下译文质量可能更好
   */
  enableThinking: boolean;
  shortcut: string;
  targetLanguage: string;
  timeoutSeconds: number;
  autoHide: boolean;
  pinnedByDefault: boolean;
  maxTextLength: number;
}

export const DEFAULT_CONFIG: AppConfig = {
  provider: DEFAULT_PROVIDER,
  apiKeys: {},
  baseUrl: "https://apihub.agnes-ai.com/v1",
  apiKey: "",
  model: "agnes-2.0-flash",
  enableThinking: false,
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
