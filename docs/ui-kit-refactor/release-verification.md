# UI Kit Refactor — 发布验证记录

日期：2026-08-11

## 自动验证

| 项目 | 结果 |
|---|---|
| `npm run typecheck` | 通过 |
| `npm test` | 61 项通过 |
| `npm run build` | 通过；主 JS gzip 73.23 KiB，主 CSS gzip 12.78 KiB |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 172 项通过 |
| `cargo build --release --manifest-path src-tauri/Cargo.toml` | 通过（仅 linker message warning） |
| `git diff --check` | 通过 |
| 旧入口与私有 modal 扫描 | 未发现 `window.confirm`、`fixed inset-0` 或 `components/ui/Field` 使用 |

## 迁移映射

- 翻译领域：`components/translation/`，由 `useTranslationController` 协调。
- 设置领域：`components/settings/`，由 `useSettingsController` 与 `useCacheDetailsController` 协调。
- 表单、按钮、Dialog、Spinner：`components/ui/`。
- 确认与状态提示：`components/patterns/`。

## 人工发布验收待办

- 用工单 02 相同环境复拍默认 520×390 与最小 360×200 状态截图并进行视觉对比。
- 验证键盘焦点、Dialog Escape/焦点恢复、窄窗口操作可达性。
- 在 Windows 环境核对 release 安装包与实际 Qwen/Official API 流程。

这些项目需由发布负责人确认后写入 09 工单 Comments；在此之前，09 保持 `ready-for-human`。
