# 04 — 交付操作与反馈基础组件

Status: ready-for-human

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-002 and Phase 2.

## What to build

实现并测试 `Button`、`IconButton` 和 `Spinner`，建立 `src/components/ui/index.ts` seam 的第一部分。组件只拥有通用交互、视觉和可访问性，不读取应用状态或调用服务。

## Blocked by

- 03 — 建立设计 token 与样式分层

## Acceptance criteria

- [x] `Button` 支持 `primary|outline|ghost|danger`、`sm|md`、`loading/loadingLabel`、原生 props 与 `forwardRef`。
- [x] Button loading 时防重复提交并提供正确可访问文案；不实现 `asChild`。
- [x] `IconButton` 支持 `ghost|outline|danger`、`sm|md`、必填 `label`、`pressed`、`loading` 和单图标 child。
- [x] IconButton 不依赖 tooltip 获得可访问名称；装饰图标不重复播报。
- [x] `Spinner` 支持 sm/md 与可选 label，并避免与父级 loading 文案重复播报。
- [x] 组件高度、图标尺寸、focus-visible、disabled 和 reduced-motion 符合 token 合同。
- [x] 移除 Button 的 `size="icon"` 公共合同；生产调用方最终由 IconButton 迁移，不能保留废弃 alias。
- [x] 组件仅依赖 React、Lucide（需要时）与 `cn`；无 store/service/Tauri/AppConfig import。
- [x] UI-001、UI-002、UI-003、UI-009 自动测试通过，`npm run typecheck`、`npm test`、`npm run build` 通过。

## Out of scope

- 批量迁移翻译页和设置页。
- 实现表单、Dialog 或 patterns。

## Comments

- 2026-08-11：按 TDD 新增 `ActionFeedback.test.tsx`，先验证 seam 缺失失败，再实现 `Spinner`、升级 `Button`、新增 `IconButton` 与 `ui/index.ts`。`size="icon"` 已从公开 Button contract 删除，标题栏与缓存详情的 6 个生产调用点迁为 `IconButton size="sm"`，保持原 32px 尺寸。Button/Spinner 视觉 recipe 从旧全局 recipe 收拢到组件，但旧 CSS recipe 仍按 03 的清理清单保留至后续迁移。静态搜索确认无 `size="icon"` 和 `ui` forbidden import；现有 `Field`/`Input`/`Switch` 深路径 import 属后续表单工单范围。`npm run typecheck`、`npm test`（8 files / 51 tests）与 `npm run build` 均通过。生产 JS gzip 从 72.93 KiB 到 73.32 KiB（+0.39 KiB），CSS gzip 从 12.84 KiB 到 12.79 KiB（-0.05 KiB），均在预算内。可见变化仅为 loading 的 spinner/状态播报及新增键盘 focus-visible，均为本票批准的交互与无障碍行为。
