use std::sync::Mutex;

use crate::app_error::{AppError, AppResult};
use crate::config::{save_config as persist_config, AppConfig};
use crate::shortcut;
use tauri::{AppHandle, State};

/// 全局配置状态：保存当前内存中的配置快照
/// 后续阶段（快捷键、窗口固定）会从这里读取，避免每次都解析文件
pub struct AppState {
    pub config: Mutex<AppConfig>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Mutex::new(config),
        }
    }

    /// 获取配置快照
    pub fn snapshot(&self) -> AppResult<AppConfig> {
        let guard = self.config.lock().map_err(|e| {
            AppError::Internal(format!("配置锁获取失败: {e}"))
        })?;
        Ok(guard.clone())
    }

    /// 更新内存中的配置
    pub fn update(&self, config: AppConfig) -> AppResult<()> {
        let mut guard = self.config.lock().map_err(|e| {
            AppError::Internal(format!("配置锁获取失败: {e}"))
        })?;
        *guard = config;
        Ok(())
    }
}

/// 读取配置：返回内存中的快照
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> AppResult<AppConfig> {
    state.snapshot()
}

/// 保存配置
/// 流程：校验 → 持久化到文件 → 更新内存状态 → 快捷键变更则重新注册
/// 校验失败时不修改原文件与内存状态
#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> AppResult<()> {
    // 1. 校验
    validate_config(&config)?;
    // 2. 检测快捷键是否变更
    let old_shortcut = state.snapshot().map(|c| c.shortcut).unwrap_or_default();
    let shortcut_changed = old_shortcut != config.shortcut;
    // 3. 持久化到磁盘
    persist_config(&app, &config)?;
    // 4. 更新内存状态
    state.update(config)?;
    // 5. 快捷键变更：重新注册
    if shortcut_changed {
        let new_shortcut = state.snapshot()?.shortcut;
        if let Err(e) = shortcut::register(&app, &new_shortcut) {
            // 注册失败不阻断保存流程，但返回警告给前端
            log::warn!("快捷键重新注册失败: {e}");
            return Err(e);
        }
    }
    log::info!("配置已保存");
    Ok(())
}

/// 校验配置
pub fn validate_config(config: &AppConfig) -> AppResult<()> {
    if config.base_url.trim().is_empty() {
        return Err(AppError::ConfigInvalid("Base URL 不能为空".to_string()));
    }
    if config.model.trim().is_empty() {
        return Err(AppError::ConfigInvalid("模型名称不能为空".to_string()));
    }
    if config.timeout_seconds < 5 || config.timeout_seconds > 300 {
        return Err(AppError::ConfigInvalid(
            "请求超时时间应在 5～300 秒之间".to_string(),
        ));
    }
    if config.max_text_length < 100 || config.max_text_length > 20000 {
        return Err(AppError::ConfigInvalid(
            "最大翻译字符数应在 100～20000 之间".to_string(),
        ));
    }
    if config.shortcut.trim().is_empty() {
        return Err(AppError::ConfigInvalid("快捷键不能为空".to_string()));
    }
    Ok(())
}

