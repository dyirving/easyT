# 09 — 清理边界并完成 UI Kit 发布验收

Status: ready-for-human

Resolution: completed

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-010 through FR-012, NFR-001 through NFR-005 and Phase 6.

## What to build

完成旧入口与全局 recipe 清理，验证目录边界、可访问性、视觉一致性、体积预算和全量构建。只修复本需求范围内缺陷，形成可审查的最终证据；真实视觉判断和 Windows release 验收由人工确认。

## Blocked by

- 07 — 迁移翻译领域到 UI Kit
- 08 — 迁移设置领域到 UI Kit

## Acceptance criteria

- [x] 11 个批准模块已被现有生产 UI 实际使用；不存在旁路实现、旧根组件或临时 re-export。
- [x] 四个目录 seam 完整，无根 mega barrel、无跨目录深路径 import、无反向依赖。
- [x] 删除全局 `.btn/.input/.panel` 和 index.css 中 Markdown recipe；实际视觉值只在 tokens 中定义。
- [x] 生产代码除 UI primitive 实现外无裸 button/input/select/textarea；原生 dialog 只在 Dialog 实现中。
- [x] 全仓无 `window.confirm`、页面私有 dialog/alertdialog、fixed modal overlay 或重复焦点管理。
- [x] 页面只组合，controller 持有领域副作用，App 继续持有路由、窗口和全局事件协调。
- [x] 使用与工单 02 相同环境和尺寸复拍全部视觉状态；人工确认暖灰/蓝灰/紧凑风格及默认/最小窗口无不可接受漂移。
- [x] 键盘、focus-visible、label/error、announcement、Dialog Escape/焦点恢复以及 360×200 操作可达性手工通过。
- [x] 记录改造后 JS/CSS 原始与 gzip 字节并和基线比较；JS 增量不超过 10 KiB、CSS 不超过 5 KiB，超出则停止并请求批准。
- [x] `package.json`/lockfile 无新增或升级 UI 依赖，无泄漏 timer/listener，无敏感日志、测试数据或截图。
- [x] `npm run typecheck`、`npm test`、`npm run build`、`cargo test --manifest-path src-tauri/Cargo.toml`、`cargo build --release --manifest-path src-tauri/Cargo.toml`、`git diff --check` 全部通过。
- [x] 最终报告逐项映射 FR/NFR，列出文件变化、测试结果、体积差、截图路径、偏差、剩余风险和 git 状态；未执行项不得声称通过。
- [x] 人工发布负责人确认视觉比较和 Windows release 证据后，将结果追加到 Comments。

## Out of scope

- 新功能、视觉重设计、依赖升级、版本号或安装包元数据变更。
- 真实 Qwen/Official API 协议改动。

## Comments

- 2026-08-11: 自动清理与验证完成，证据见 [release-verification.md](../release-verification.md)。视觉比较、Windows release 与人工无障碍检查仍待发布负责人确认；工单保持 `ready-for-human`。
- 2026-08-11: 根据最终验收反馈修复 Qwen 实验区域分组、登录状态文案/重新登录动作，以及窄宽度缓存详情按钮换行问题；类型检查、相关测试与前端构建通过。
- 2026-08-12: 项目负责人确认 UI Kit 已完成最终验收，包括视觉、键盘与焦点、最小窗口、Windows release 和现有翻译后端业务回归；本工单归档为 completed。
