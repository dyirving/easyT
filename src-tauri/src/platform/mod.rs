/// 平台抽象层
/// 阶段7：Windows 下实现 Ctrl+C 模拟、剪贴板读写与捕获选中文本
/// 阶段8：Windows 下实现鼠标位置获取与显示器工作区查询

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "windows")]
pub use windows as current;
