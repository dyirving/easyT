# 01 — 批准 UI Kit 实施合同

Status: ready-for-human

Resolution: completed

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md)

Product and visual consensus: [UI-Kit需求与架构共识文档.md](../../UI-Kit需求与架构共识文档.md)

## What to decide

审阅并明确批准 UI Kit SDD，使后续编码工单可以开始。该批准只覆盖既定的 easyT 内部 UI Kit 提取、领域组件迁移和行为保持重构，不授权新增依赖、修改后端、改变视觉风格或扩大产品范围。

## Blocked by

- None

## Acceptance criteria

- [x] 项目负责人确认 SDD 中 11 个模块、四目录 seam、controller 边界和六阶段实施顺序。
- [x] 项目负责人确认不新增 UI 依赖、不修改 Rust/Tauri 接口、不改变配置/缓存/翻译状态机。
- [x] 项目负责人确认默认 `520×390`、最小 `360×200`、最大 `900×700` 窗口合同保持不变。
- [x] 项目负责人确认原生 Dialog、统一 ConfirmDialog、无 `window.confirm` 和无页面私有 modal 的约束。
- [x] 项目负责人明确回复批准实施；批准证据与日期追加到 Comments。
- [x] 本次无需调整 SDD；下游工单必须按当前权威 SDD 实施，任何后续偏差均须先更新并重新审阅。

## Out of scope

- 修改任何生产代码、测试或样式。
- 创建视觉基线或执行 UI 迁移。
- 批准 SDD 明确排除的范围。

## Comments

- 2026-08-11: ticket 创建时 SDD 状态为 `In Review`，代码实施仍未授权。
- 2026-08-11: 项目负责人明确回复“批准”。SDD 升级为 `Approved` v0.2，后续 UI Kit 工单可按既定范围开始；未批准任何口头偏差或新增依赖。
- 2026-08-12: UI Kit 最终验收通过，本工单随完整实施链路归档为 completed。
