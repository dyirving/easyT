# 09 — 清理边界并完成 UI Kit 发布验收

Status: ready-for-human

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-010 through FR-012, NFR-001 through NFR-005 and Phase 6.

## What to build

完成旧入口与全局 recipe 清理，验证目录边界、可访问性、视觉一致性、体积预算和全量构建。只修复本需求范围内缺陷，形成可审查的最终证据；真实视觉判断和 Windows release 验收由人工确认。

## Blocked by

- 07 — 迁移翻译领域到 UI Kit
- 08 — 迁移设置领域到 UI Kit

## Acceptance criteria

- [ ] 11 个批准模块已被现有生产 UI 实际使用；不存在旁路实现、旧根组件或临时 re-export。
- [ ] 四个目录 seam 完整，无根 mega barrel、无跨目录深路径 import、无反向依赖。
- [ ] 删除全局 `.btn/.input/.panel` 和 index.css 中 Markdown recipe；实际视觉值只在 tokens 中定义。
- [ ] 生产代码除 UI primitive 实现外无裸 button/input/select/textarea；原生 dialog 只在 Dialog 实现中。
- [ ] 全仓无 `window.confirm`、页面私有 dialog/alertdialog、fixed modal overlay 或重复焦点管理。
- [ ] 页面只组合，controller 持有领域副作用，App 继续持有路由、窗口和全局事件协调。
- [ ] 使用与工单 02 相同环境和尺寸复拍全部视觉状态；人工确认暖灰/蓝灰/紧凑风格及默认/最小窗口无不可接受漂移。
- [ ] 键盘、focus-visible、label/error、announcement、Dialog Escape/焦点恢复以及 360×200 操作可达性手工通过。
- [ ] 记录改造后 JS/CSS 原始与 gzip 字节并和基线比较；JS 增量不超过 10 KiB、CSS 不超过 5 KiB，超出则停止并请求批准。
- [ ] `package.json`/lockfile 无新增或升级 UI 依赖，无泄漏 timer/listener，无敏感日志、测试数据或截图。
- [ ] `npm run typecheck`、`npm test`、`npm run build`、`cargo test --manifest-path src-tauri/Cargo.toml`、`cargo build --release --manifest-path src-tauri/Cargo.toml`、`git diff --check` 全部通过。
- [ ] 最终报告逐项映射 FR/NFR，列出文件变化、测试结果、体积差、截图路径、偏差、剩余风险和 git 状态；未执行项不得声称通过。
- [ ] 人工发布负责人确认视觉比较和 Windows release 证据后，将结果追加到 Comments。

## Out of scope

- 新功能、视觉重设计、依赖升级、版本号或安装包元数据变更。
- 真实 Qwen/Official API 协议改动。

## Comments

