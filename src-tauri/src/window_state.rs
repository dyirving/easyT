use std::fs;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalSize};

use crate::app_error::{AppError, AppResult};
use crate::config::app_data_dir;

const WINDOW_STATE_FILE_NAME: &str = "window-state.json";
const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Debug)]
struct PendingWindowSize {
    latest: Option<PhysicalSize<u32>>,
    last_update: Instant,
    worker_running: bool,
}

impl Default for PendingWindowSize {
    fn default() -> Self {
        Self {
            latest: None,
            last_update: Instant::now(),
            worker_running: false,
        }
    }
}

static PENDING_SIZE: OnceLock<Mutex<PendingWindowSize>> = OnceLock::new();

#[derive(Debug, Deserialize, Serialize)]
struct SavedWindowSize {
    width: u32,
    height: u32,
}

fn window_state_path() -> AppResult<std::path::PathBuf> {
    Ok(app_data_dir()?.join(WINDOW_STATE_FILE_NAME))
}

/// 在窗口显示前恢复上次保存的主窗口尺寸。
pub fn restore_main_window_size(app: &AppHandle) {
    let result = (|| -> AppResult<()> {
        let path = window_state_path()?;
        if !path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(path)
            .map_err(|e| AppError::Internal(format!("读取窗口状态失败: {e}")))?;
        let saved: SavedWindowSize = serde_json::from_str(&content)
            .map_err(|e| AppError::Internal(format!("解析窗口状态失败: {e}")))?;
        let window = app
            .get_webview_window("main")
            .ok_or_else(|| AppError::WindowError("未找到主窗口".to_string()))?;
        window
            .set_size(PhysicalSize::new(saved.width, saved.height))
            .map_err(|e| AppError::WindowError(format!("恢复窗口尺寸失败: {e}")))
    })();

    if let Err(e) = result {
        log::warn!("恢复窗口尺寸失败，继续使用默认尺寸: {e}");
    }
}

/// 合并连续 resize 事件，并在用户停止拖动后于阻塞线程落盘。
pub fn schedule_main_window_size_save(size: PhysicalSize<u32>) {
    let state = PENDING_SIZE.get_or_init(|| Mutex::new(PendingWindowSize::default()));
    let should_start_worker = {
        let mut pending = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.latest = Some(size);
        pending.last_update = Instant::now();
        if pending.worker_running {
            false
        } else {
            pending.worker_running = true;
            true
        }
    };

    if !should_start_worker {
        return;
    }

    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(SAVE_DEBOUNCE).await;
            let size_to_save = {
                let mut pending = state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if pending.last_update.elapsed() < SAVE_DEBOUNCE {
                    None
                } else {
                    pending.worker_running = false;
                    pending.latest.take()
                }
            };

            let Some(size) = size_to_save else {
                continue;
            };
            match tokio::task::spawn_blocking(move || save_main_window_size(size)).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => log::warn!("保存窗口尺寸失败: {error}"),
                Err(error) => log::warn!("窗口尺寸保存任务失败: {error}"),
            }
            break;
        }
    });
}

/// 以原子写入方式保存主窗口尺寸。
pub fn save_main_window_size(size: PhysicalSize<u32>) -> AppResult<()> {
    let path = window_state_path()?;
    let data = serde_json::to_vec_pretty(&SavedWindowSize {
        width: size.width,
        height: size.height,
    })
    .map_err(|e| AppError::Internal(format!("序列化窗口状态失败: {e}")))?;
    let temporary_path = path.with_extension("json.tmp");
    {
        let mut file = fs::File::create(&temporary_path)
            .map_err(|e| AppError::Internal(format!("创建窗口状态临时文件失败: {e}")))?;
        file.write_all(&data)
            .map_err(|e| AppError::Internal(format!("写入窗口状态失败: {e}")))?;
        file.sync_all()
            .map_err(|e| AppError::Internal(format!("刷新窗口状态失败: {e}")))?;
    }
    fs::rename(&temporary_path, path).map_err(|e| {
        let _ = fs::remove_file(&temporary_path);
        AppError::Internal(format!("提交窗口状态失败: {e}"))
    })
}
