# 04 — 交付操作与反馈基础组件

Status: ready-for-agent

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-002 and Phase 2.

## What to build

实现并测试 `Button`、`IconButton` 和 `Spinner`，建立 `src/components/ui/index.ts` seam 的第一部分。组件只拥有通用交互、视觉和可访问性，不读取应用状态或调用服务。

## Blocked by

- 03 — 建立设计 token 与样式分层

## Acceptance criteria

- [ ] `Button` 支持 `primary|outline|ghost|danger`、`sm|md`、`loading/loadingLabel`、原生 props 与 `forwardRef`。
- [ ] Button loading 时防重复提交并提供正确可访问文案；不实现 `asChild`。
- [ ] `IconButton` 支持 `ghost|outline|danger`、`sm|md`、必填 `label`、`pressed`、`loading` 和单图标 child。
- [ ] IconButton 不依赖 tooltip 获得可访问名称；装饰图标不重复播报。
- [ ] `Spinner` 支持 sm/md 与可选 label，并避免与父级 loading 文案重复播报。
- [ ] 组件高度、图标尺寸、focus-visible、disabled 和 reduced-motion 符合 token 合同。
- [ ] 移除 Button 的 `size="icon"` 公共合同；生产调用方最终由 IconButton 迁移，不能保留废弃 alias。
- [ ] 组件仅依赖 React、Lucide（需要时）与 `cn`；无 store/service/Tauri/AppConfig import。
- [ ] UI-001、UI-002、UI-003、UI-009 自动测试通过，`npm run typecheck`、`npm test`、`npm run build` 通过。

## Out of scope

- 批量迁移翻译页和设置页。
- 实现表单、Dialog 或 patterns。

## Comments

