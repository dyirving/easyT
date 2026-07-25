use std::sync::Mutex;

use crate::app_error::{AppError, AppResult};
use crate::config::{save_config as persist_config, AppConfig};
use crate::shortcut;
use tauri::{AppHandle, State};

/// 全局配置状态：保存当前内存中的配置快照
/// 后续阶段（快捷键、窗口固定）会从这里读取，避免每次都解析文件
pub struct AppState {
    pub config: Mutex<AppConfig>,
    save_lock: Mutex<()>,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        Self {
            config: Mutex::new(config),
            save_lock: Mutex::new(()),
        }
    }

    /// 获取配置快照
    pub fn snapshot(&self) -> AppResult<AppConfig> {
        let guard = self
            .config
            .lock()
            .map_err(|e| AppError::Internal(format!("配置锁获取失败: {e}")))?;
        Ok(guard.clone())
    }

    /// 更新内存中的配置
    pub fn update(&self, config: AppConfig) -> AppResult<()> {
        let mut guard = self
            .config
            .lock()
            .map_err(|e| AppError::Internal(format!("配置锁获取失败: {e}")))?;
        *guard = config;
        Ok(())
    }
}

/// 读取配置：返回内存中的快照
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> AppResult<AppConfig> {
    state.snapshot()
}

/// 保存配置。
/// 快捷键变更时使用可回滚替换流程，避免持久化状态与实际快捷键不一致。
#[tauri::command]
pub async fn save_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> AppResult<()> {
    let _save_guard = state
        .save_lock
        .lock()
        .map_err(|e| AppError::Internal(format!("配置保存锁获取失败: {e}")))?;

    validate_config(&config)?;

    let old_config = state.snapshot()?;
    let old_shortcut = old_config.shortcut.clone();
    let shortcut_changed = old_shortcut != config.shortcut;

    let replacement = if shortcut_changed {
        Some(shortcut::prepare_replacement(&app, &config.shortcut)?)
    } else {
        None
    };

    if let Err(e) = persist_config(&app, &config) {
        if let Some(replacement) = replacement {
            if let Err(rollback_err) = shortcut::rollback_replacement(&app, replacement) {
                return Err(AppError::Internal(format!(
                    "配置持久化失败: {e}; 快捷键回滚也失败: {rollback_err}"
                )));
            }
        }
        return Err(e);
    }

    if let Err(e) = state.update(config) {
        let rollback_error = replacement
            .and_then(|replacement| shortcut::rollback_replacement(&app, replacement).err());
        let restore_error = persist_config(&app, &old_config).err();
        return Err(combine_compensation_errors(
            e,
            rollback_error,
            restore_error,
        ));
    }

    if let Some(replacement) = replacement {
        shortcut::commit_replacement(&app, replacement);
    }

    log::info!("配置已保存");
    Ok(())
}

fn combine_compensation_errors(
    primary: AppError,
    rollback_error: Option<AppError>,
    restore_error: Option<AppError>,
) -> AppError {
    if rollback_error.is_none() && restore_error.is_none() {
        return primary;
    }

    let mut details = vec![format!("配置内存更新失败: {primary}")];
    if let Some(e) = rollback_error {
        details.push(format!("快捷键回滚也失败: {e}"));
    }
    if let Some(e) = restore_error {
        details.push(format!("恢复旧配置文件也失败: {e}"));
    }
    AppError::Internal(details.join("; "))
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

#[cfg(test)]
mod tests {
    use super::combine_compensation_errors;
    use crate::app_error::AppError;

    #[test]
    fn compensation_error_reports_every_failed_action() {
        let error = combine_compensation_errors(
            AppError::Internal("更新失败".to_string()),
            Some(AppError::ShortcutRegistrationFailed("回滚失败".to_string())),
            Some(AppError::Internal("恢复失败".to_string())),
        );
        let message = error.to_string();

        assert!(message.contains("更新失败"));
        assert!(message.contains("回滚失败"));
        assert!(message.contains("恢复失败"));
    }

    #[test]
    fn compensation_error_preserves_primary_when_cleanup_succeeds() {
        let error =
            combine_compensation_errors(AppError::Internal("更新失败".to_string()), None, None);

        assert_eq!(error.to_string(), "内部错误: 更新失败");
    }
}
