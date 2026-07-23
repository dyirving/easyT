// 设置状态管理
// 第一轮：仅维护内存中的配置草稿与保存状态，用于设置页 UI。
// 下一轮将接入 get_config / save_config 命令。
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
  /** 加载配置（第一轮为占位，下一轮接入命令） */
  loadConfig: (config: AppConfig) => void;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  config: { ...DEFAULT_CONFIG },
  saved: false,
  setConfig: (patch) =>
    set((s) => ({ config: { ...s.config, ...patch }, saved: false })),
  resetToDefault: () => set({ config: { ...DEFAULT_CONFIG }, saved: false }),
  markSaved: () => set({ saved: true }),
  loadConfig: (config) => set({ config: { ...config }, saved: true }),
}));
