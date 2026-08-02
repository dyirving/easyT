// 设置状态管理
// 维护内存中的配置草稿与保存状态，用于设置页 UI。
// apiKeys 为各供应商独立的 API Key 存储；apiKey 始终等于 apiKeys[provider]，
// 切换供应商时由 SettingsPage 负责同步 apiKey。
import { create } from "zustand";
import { type AppConfig, DEFAULT_CONFIG } from "@/types";

interface SettingsStore {
  config: AppConfig;
  saved: boolean;
  /** 设置草稿配置（未保存） */
  setConfig: (patch: Partial<AppConfig>) => void;
  /** 恢复为默认配置 */
  resetToDefault: () => void;
  /** 标记保存状态 */
  markSaved: () => void;
  /** 加载配置：含旧文件迁移（无 apiKeys 时用 apiKey 填充当前供应商） */
  loadConfig: (config: AppConfig) => void;
}

/**
 * 迁移并规整配置：
 * - 旧配置文件无 apiKeys 字段（Rust 端 #[serde(default)] 返回空对象）：
 *   若 apiKey 非空，则将其写入 apiKeys[provider]，避免丢失已填写的 Key
 * - 始终保证 apiKey === apiKeys[provider]（ ?? ""）
 */
function migrateConfig(input: AppConfig): AppConfig {
  const config: AppConfig = {
    ...input,
    apiKeys: { ...input.apiKeys },
    streamOutput: input.streamOutput ?? false,
  };
  if (!config.apiKeys || Object.keys(config.apiKeys).length === 0) {
    if (config.apiKey) {
      config.apiKeys = { [config.provider]: config.apiKey };
    } else {
      config.apiKeys = {};
    }
  }
  // 保证 apiKey 与当前供应商的存储值一致
  config.apiKey = config.apiKeys[config.provider] ?? "";
  return config;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  config: { ...DEFAULT_CONFIG },
  saved: false,
  setConfig: (patch) =>
    set((s) => ({ config: { ...s.config, ...patch }, saved: false })),
  resetToDefault: () => set({ config: { ...DEFAULT_CONFIG }, saved: false }),
  markSaved: () => set({ saved: true }),
  loadConfig: (config) =>
    set({ config: migrateConfig(config), saved: true }),
}));
