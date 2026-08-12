use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::app_error::{AppError, AppResult};
use crate::config::{default_config, AppConfig, QWEN_ALLOWED_MODELS};

/// 配置文件名
const CONFIG_FILE_NAME: &str = "config.json";
pub const DATA_DIR_NAME: &str = "easyT_Data";

/// 返回可执行文件同级的应用数据目录。
/// 发布版会落在安装目录下的 `easyT_Data`；开发版则落在调试可执行文件同级目录。
pub fn app_data_dir() -> AppResult<PathBuf> {
    let executable = std::env::current_exe()
        .map_err(|e| AppError::Internal(format!("解析应用可执行文件路径失败: {e}")))?;
    app_data_dir_from_executable(&executable)
}

fn app_data_dir_from_executable(executable: &Path) -> AppResult<PathBuf> {
    let dir = app_data_dir_path_from_executable(executable)?;
    fs::create_dir_all(&dir).map_err(|e| AppError::Internal(format!("创建配置目录失败: {e}")))?;
    Ok(dir)
}

fn app_data_dir_path_from_executable(executable: &Path) -> AppResult<PathBuf> {
    let executable_dir = executable
        .parent()
        .ok_or_else(|| AppError::Internal("应用可执行文件路径不包含父目录".to_string()))?;
    Ok(executable_dir.join(DATA_DIR_NAME))
}

pub fn config_path() -> AppResult<PathBuf> {
    Ok(app_data_dir()?.join(CONFIG_FILE_NAME))
}

/// 加载配置：文件不存在时写入默认配置并返回
/// 解析失败时回退到默认配置，避免单次损坏导致应用无法启动
pub fn load_config() -> AppResult<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        let cfg = default_config();
        // 写入失败不致命：仍返回内存中的默认配置
        let _ = write_config_inner(&path, &cfg);
        return Ok(cfg);
    }
    let content = fs::read_to_string(&path)
        .map_err(|e| AppError::Internal(format!("读取配置文件失败: {e}")))?;
    if content.trim().is_empty() {
        return Ok(default_config());
    }
    // 配置损坏时回退到默认值，避免阻塞启动
    let cfg: AppConfig = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("配置解析失败，回退默认配置: {e}");
            default_config()
        }
    };
    let (cfg, migrated) = normalize_config(cfg);
    if migrated {
        let _ = write_config_inner(&path, &cfg);
    }
    Ok(cfg)
}

fn normalize_config(mut config: AppConfig) -> (AppConfig, bool) {
    let mut changed = false;
    if !(1..=20).contains(&config.translation_history_limit) {
        log::info!(
            "已保存的翻译历史上限无效，运行时回退默认值: old={}",
            config.translation_history_limit
        );
        config.translation_history_limit = default_config().translation_history_limit;
        // 历史上限只做运行时兜底；等用户下一次显式保存设置时再写回，
        // 避免仅因读取旧配置就改写磁盘。
    }
    if !QWEN_ALLOWED_MODELS.contains(&config.web_gateway.model.as_str()) {
        log::info!(
            "已保存的 Qwen 模型不再受支持，迁移到官网默认模型: old={}",
            config.web_gateway.model
        );
        config.web_gateway.model = default_config().web_gateway.model;
        changed = true;
    }
    (config, changed)
}

/// 保存配置：先写入临时文件再原子替换，避免写入中断导致损坏
pub fn save_config(config: &AppConfig) -> AppResult<()> {
    let path = config_path()?;
    write_config_inner(&path, config)
}

fn write_config_inner(path: &PathBuf, config: &AppConfig) -> AppResult<()> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| AppError::Internal(format!("序列化配置失败: {e}")))?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| AppError::Internal(format!("创建临时文件失败: {e}")))?;
        f.write_all(json.as_bytes())
            .map_err(|e| AppError::Internal(format!("写入配置失败: {e}")))?;
        f.sync_all()
            .map_err(|e| AppError::Internal(format!("刷新配置到磁盘失败: {e}")))?;
    }
    // 原子替换（Windows 上 replace 会覆盖目标）
    fs::rename(&tmp, path).map_err(|e| {
        // rename 跨卷可能失败，退化为直接写入目标
        let _ = fs::remove_file(&tmp);
        AppError::Internal(format!("提交配置文件失败: {e}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{app_data_dir_path_from_executable, normalize_config, DATA_DIR_NAME};
    use crate::config::default_config;
    use std::path::Path;

    #[test]
    fn data_dir_is_sibling_of_executable() {
        let path = app_data_dir_path_from_executable(Path::new(r"D:\easyT\easyt.exe"))
            .expect("test path should resolve");
        assert_eq!(path, Path::new(r"D:\easyT").join(DATA_DIR_NAME));
    }

    #[test]
    fn obsolete_qwen_model_migrates_to_current_default() {
        let mut config = default_config();
        config.web_gateway.model = "Qwen3.5-Flash".to_string();
        let (migrated, changed) = normalize_config(config);
        assert!(changed);
        assert_eq!(migrated.web_gateway.model, "Qwen3.7-Max");
    }

    #[test]
    fn current_qwen_model_is_not_migrated() {
        let mut config = default_config();
        config.web_gateway.model = "Qwen3.8-Max-Preview".to_string();
        let (normalized, changed) = normalize_config(config);
        assert!(!changed);
        assert_eq!(normalized.web_gateway.model, "Qwen3.8-Max-Preview");
    }

    #[test]
    fn invalid_history_limit_falls_back_without_eager_persistence() {
        let mut config = default_config();
        config.translation_history_limit = 0;
        let (normalized, changed) = normalize_config(config);
        assert_eq!(normalized.translation_history_limit, 5);
        assert!(!changed);
    }
}
