# 05 — 交付表单基础组件与自动关联

Status: ready-for-human

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-002, FR-003 and Phase 2.

## What to build

实现 `FormField` context 及 `Input`、`Textarea`、native `Select`、`Switch`，统一尺寸、错误状态和 label/hint/error 的可访问关联。组件独立使用时仍须正常工作。

## Blocked by

- 03 — 建立设计 token 与样式分层

## Acceptance criteria

- [x] `FormField` 使用 `useId`，自动关联 label、控件、hint 和 error，并传播 required/invalid 语义。
- [x] 调用方已有 `id` 与 `aria-describedby` 被正确合并而非覆盖；context 缺失时各控件可独立使用。
- [x] Input/Textarea/Select 继承对应原生 props 并 `forwardRef`；Textarea 最小高度 80px，Input/Select 最小高度 36px。
- [x] Select 只封装 native select，不实现自绘弹层。
- [x] Switch 保持 `checked/onCheckedChange/disabled` 合同，支持 ref 与 FormField 关联，尺寸为 36×20、thumb 16。
- [x] 删除 Switch 文件中的通用 `Label` 出口；`Field.tsx` 仅在所有调用方迁移完成后删除，不建立长期兼容层。
- [x] 组件视觉来自 token/组件内部，不依赖全局 `.input` recipe。
- [x] 组件无 store/service/Tauri/AppConfig import。
- [x] UI-004、UI-005 自动测试通过，`npm run typecheck`、`npm test`、`npm run build` 通过。

## Out of scope

- 自定义 combobox、多选或第三方表单库。
- 页面 controller 与领域逻辑迁移。

## Comments

- 2026-08-11：新增内部 FormField context、Textarea、native Select 和 FormControls 行为测试；Input/Switch 改为消费 context，并保留独立使用与原生 ref。Switch 的旧 Label 导出已删除；仍被 SettingsPage 使用的 Field 保留至领域迁移工单。全量验证通过：9 files / 53 tests、typecheck、build；未新增依赖或 UI 层 forbidden import。
