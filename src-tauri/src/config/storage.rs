use std::fs;
use std::io::Write;
use std::path::PathBuf;

use tauri::Manager;

use crate::app_error::{AppError, AppResult};
use crate::config::{default_config, AppConfig};

/// 配置文件名
const CONFIG_FILE_NAME: &str = "config.json";

/// 解析配置文件路径
/// 路径示例（Windows）：%APPDATA%\<bundle_identifier>\config.json
pub fn config_path(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Internal(format!("解析配置目录失败: {e}")))?;
    fs::create_dir_all(&dir).map_err(|e| AppError::Internal(format!("创建配置目录失败: {e}")))?;
    Ok(dir.join(CONFIG_FILE_NAME))
}

/// 加载配置：文件不存在时写入默认配置并返回
/// 解析失败时回退到默认配置，避免单次损坏导致应用无法启动
pub fn load_config(app: &tauri::AppHandle) -> AppResult<AppConfig> {
    let path = config_path(app)?;
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
    Ok(cfg)
}

/// 保存配置：先写入临时文件再原子替换，避免写入中断导致损坏
pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> AppResult<()> {
    let path = config_path(app)?;
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
