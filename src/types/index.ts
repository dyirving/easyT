// 应用统一类型定义
// 应用前后端共享的前端类型与配置常量。

/**
 * 翻译状态机
 * - idle: 空闲，等待用户触发
 * - translating: 正在等待流式请求的首段正文，或执行一次性翻译
 * - streaming: 已收到正文增量但尚未完成
 * - refreshing: 保留旧缓存译文的同时重新翻译，失败时回退到旧结果
 * - success: 翻译成功
 * - error: 翻译失败
 */
export type TranslationStatus =
  | "idle"
  | "translating"
  | "streaming"
  | "refreshing"
  | "success"
  | "error";

export type TranslationPhase =
  | "checkingCache"
  | "preparingRequest"
  | "connectingBackend"
  | "waitingForContent"
  | "receivingContent"
  | "savingHistory";

export type HistoryWarningKind =
  | "storageUnavailable"
  | "storageRecovered"
  | "saveFailed"
  | "saveTimedOut"
  | "entryTooLarge"
  | "limitApplyFailed";

export interface HistoryWarning {
  kind: HistoryWarningKind;
  message: string;
}

export interface TranslationHistorySummary {
  entryId: string;
  originalSummary: string;
  translatedSummary: string;
  targetLanguage: string;
  sourceBackend: BackendMode;
  sourceProvider: string;
  sourceModel: string;
  fromCache: boolean;
  totalElapsedMs: number;
  completedAtUtcMs: number;
}

export interface TranslationHistoryEntry extends TranslationHistorySummary {
  originalText: string;
  translatedText: string;
}

export type HistoryInitState = "ready" | "recovered" | "unavailable";

export interface HistorySnapshot {
  state: HistoryInitState;
  limit: number;
  summaries: TranslationHistorySummary[];
  warning?: HistoryWarning;
}

export type HistoryCommitOutcome =
  | {
      status: "saved";
      summary: TranslationHistorySummary;
      replacedEntryId?: string;
      evictedEntryIds: string[];
    }
  | { status: "notSaved"; warning: HistoryWarning };

export type HistoryLimitUpdate =
  | {
      status: "applied";
      summaries: TranslationHistorySummary[];
      evictedEntryIds: string[];
    }
  | { status: "warning"; warning: HistoryWarning };

export interface SaveConfigResult {
  historyLimit: number;
  historyUpdate: HistoryLimitUpdate;
}

export interface TranslationProgressBackend {
  mode: BackendMode;
  provider: string;
}

/**
 * 前端翻译状态
 * errorKind 用于查表得到友好文案与可重试标识
 */
export interface TranslationState {
  requestId: string | null;
  originalText: string;
  translatedText: string;
  status: TranslationStatus;
  errorMessage: string | null;
  errorKind: ErrorKind | null;
  errorCode?: string | null;
  /** 当前译文是否只收到部分正文，不能作为完整译文复制 */
  isPartial: boolean;
  /** 当前译文是否来自本机缓存；未接入缓存时始终为 false */
  fromCache: boolean;
  /** 重新翻译失败时的独立错误提示，保留旧缓存译文 */
  refreshErrorMessage: string | null;
  progressPhase: TranslationPhase | null;
  progressSequence: number | null;
  progressBackend: TranslationProgressBackend | null;
  progressPhaseStartedTotalElapsedMs: number | null;
  progressSyncedTotalElapsedMs: number | null;
  progressSyncedAtMonotonicMs: number | null;
  requestStartedAtMonotonicMs: number | null;
  totalElapsedMs: number | null;
  pinned: boolean;
}

export type PersistentCacheState =
  | "starting"
  | "ready"
  | "degraded"
  | "stopped";

export interface CacheStats {
  state: PersistentCacheState;
  entryCount: number;
  diskBytes: number;
  maxDiskBytes: number;
  hitRate: number | null;
  cachePath: string;
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
 * 翻译后端选择
 * - officialApi：使用 OpenAI 兼容协议调用付费 API（默认）
 * - webGateway：实验功能，使用网页登录态调用 Qwen 私有接口
 */
export type BackendMode = "officialApi" | "webGateway";

/**
 * Web 网关支持的 Provider 种类
 * 第一版仅 Qwen；不预先引入动态注册表
 */
export type WebProviderKind = "qwen";

/**
 * Qwen 登录态阶段（与 Rust 端 QwenSessionPhase 对应）
 * - loggedOut：本地无凭证或已显式注销
 * - loggingIn：正在登录（已创建登录窗口，watcher 运行中）
 * - ready：本地存在可解密凭证（不保证实时有效；首次 401/403 时转 expired）
 * - expired：凭证曾被验证过但已失效，需要重新登录
 */
export type QwenSessionPhase =
  | "loggedOut"
  | "loggingIn"
  | "ready"
  | "expired";

/** QwenSession 当前状态快照（前端可观察） */
export interface QwenSessionStatus {
  phase: QwenSessionPhase;
  message: string | null;
  updatedAt: number | null;
}

export type QwenAccountDisplayStatus =
  | "disabled"
  | "loggingIn"
  | "loggedOut"
  | "expired"
  | "busy"
  | "coolingDown"
  | "pendingVerification"
  | "available";

export interface QwenAccountSnapshot {
  accountId: string;
  displayName: string;
  enabled: boolean;
  order: number;
  status: QwenAccountDisplayStatus;
  cooldownRemainingSeconds?: number;
  message?: string;
  messageCode?: string;
  actions: QwenAccountActions;
}

export interface QwenAccountActions {
  canRename: boolean;
  canToggleEnabled: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  canLogin: boolean;
  canLogout: boolean;
  canTest: boolean;
  canDelete: boolean;
}

export interface QwenAccountPoolSnapshot {
  accounts: QwenAccountSnapshot[];
  maximumAccounts: number;
  loginAccountId?: string;
  warning?: QwenAccountPoolWarning;
}

export interface QwenAccountPoolWarning {
  code: string;
  message: string;
}

/**
 * WebGateway 实验功能配置
 * - provider：第一版仅 qwen
 * - model：必须来自 QWEN_ALLOWED_MODELS 白名单
 */
export interface WebGatewayConfig {
  provider: WebProviderKind;
  model: string;
  /** 是否将翻译请求显示在 Qwen 网页端对话历史中；默认关闭 */
  saveHistory: boolean;
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
  /** 是否在译文生成期间逐步展示正文；默认关闭 */
  streamOutput: boolean;
  shortcut: string;
  targetLanguage: string;
  timeoutSeconds: number;
  autoHide: boolean;
  pinnedByDefault: boolean;
  maxTextLength: number;
  /** 持久化翻译历史总记录数上限，范围 1～20。 */
  translationHistoryLimit: number;
  /** 翻译后端选择；旧配置缺失时默认 officialApi */
  backendMode: BackendMode;
  /** WebGateway 实验功能配置；旧配置缺失时默认 Qwen + Qwen3.7-Max */
  webGateway: WebGatewayConfig;
}

/** Qwen WebGateway 允许的模型白名单（与 Rust 端 QWEN_ALLOWED_MODELS 对齐） */
export const QWEN_ALLOWED_MODELS: { label: string; value: string }[] = [
  { label: "Qwen3.7-千问（综合 AI 助手）", value: "Qwen" },
  { label: "Qwen3.8-Max-Preview（最新旗舰）", value: "Qwen3.8-Max-Preview" },
  { label: "Qwen3.7-Max（默认）", value: "Qwen3.7-Max" },
  { label: "Qwen3.6-Flash（快速）", value: "Qwen3.6-Flash" },
];

export const DEFAULT_CONFIG: AppConfig = {
  provider: DEFAULT_PROVIDER,
  apiKeys: {},
  baseUrl: "https://apihub.agnes-ai.com/v1",
  apiKey: "",
  model: "agnes-2.0-flash",
  enableThinking: false,
  streamOutput: false,
  shortcut: "Ctrl+T",
  targetLanguage: "简体中文",
  timeoutSeconds: 60,
  autoHide: true,
  pinnedByDefault: false,
  maxTextLength: 5000,
  translationHistoryLimit: 5,
  backendMode: "officialApi",
  webGateway: {
    provider: "qwen",
    model: "Qwen3.7-Max",
    saveHistory: false,
  },
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
  WindowError: "WindowError",
  CacheOperationFailed: "CacheOperationFailed",
  HistoryOperationFailed: "HistoryOperationFailed",
  // ===== Backend 错误（来自 TranslationBackend）=====
  /** WebGateway 模式下本地无可用凭证，需要用户先登录 */
  LoginRequired: "LoginRequired",
  /** 凭证曾存在但已过期，需要重新登录 */
  SessionExpired: "SessionExpired",
  /** 翻译请求被新请求取代（latest-wins abort） */
  BackendCancelled: "BackendCancelled",
  /** WebGateway 网络层错误（DNS、连接拒绝、TLS 等） */
  BackendNetwork: "BackendNetwork",
  /** Qwen 私有协议结构已变化 */
  BackendProtocolMismatch: "BackendProtocolMismatch",
  /** 流式响应中断且已收到部分正文，不能作为成功返回 */
  BackendPartialResponse: "BackendPartialResponse",
  /** 响应无法解析或缺少必要字段 */
  BackendInvalidResponse: "BackendInvalidResponse",
  /** 当前后端不支持标准流式输出 */
  BackendStreamingUnsupported: "BackendStreamingUnsupported",
  Internal: "Internal",
} as const;

export type ErrorKind = (typeof ERROR_KIND)[keyof typeof ERROR_KIND];
