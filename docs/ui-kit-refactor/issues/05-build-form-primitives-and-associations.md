# 05 — 交付表单基础组件与自动关联

Status: ready-for-agent

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-002, FR-003 and Phase 2.

## What to build

实现 `FormField` context 及 `Input`、`Textarea`、native `Select`、`Switch`，统一尺寸、错误状态和 label/hint/error 的可访问关联。组件独立使用时仍须正常工作。

## Blocked by

- 03 — 建立设计 token 与样式分层

## Acceptance criteria

- [ ] `FormField` 使用 `useId`，自动关联 label、控件、hint 和 error，并传播 required/invalid 语义。
- [ ] 调用方已有 `id` 与 `aria-describedby` 被正确合并而非覆盖；context 缺失时各控件可独立使用。
- [ ] Input/Textarea/Select 继承对应原生 props 并 `forwardRef`；Textarea 最小高度 80px，Input/Select 最小高度 36px。
- [ ] Select 只封装 native select，不实现自绘弹层。
- [ ] Switch 保持 `checked/onCheckedChange/disabled` 合同，支持 ref 与 FormField 关联，尺寸为 36×20、thumb 16。
- [ ] 删除 Switch 文件中的通用 `Label` 出口；`Field.tsx` 仅在所有调用方迁移完成后删除，不建立长期兼容层。
- [ ] 组件视觉来自 token/组件内部，不依赖全局 `.input` recipe。
- [ ] 组件无 store/service/Tauri/AppConfig import。
- [ ] UI-004、UI-005 自动测试通过，`npm run typecheck`、`npm test`、`npm run build` 通过。

## Out of scope

- 自定义 combobox、多选或第三方表单库。
- 页面 controller 与领域逻辑迁移。

## Comments

