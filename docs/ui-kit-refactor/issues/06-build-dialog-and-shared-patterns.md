# 06 — 交付原生 Dialog 与共享交互模式

Status: ready-for-agent

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-004, FR-005 and Phase 3.

## What to build

以原生 `<dialog>` 实现唯一 Dialog 基础，并基于 UI primitives 交付 `StatusBanner` 与 `ConfirmDialog`。为 jsdom 添加最小、幂等的测试 polyfill，但不迁移具体领域弹窗。

## Blocked by

- 04 — 交付操作与反馈基础组件
- 05 — 交付表单基础组件与自动关联

## Acceptance criteria

- [ ] `Dialog` 受控处理 `open/onOpenChange`，使用 `showModal/close`，正确处理 cancel/Escape。
- [ ] Dialog 自动关联标题/描述，支持 initial focus，关闭/卸载后恢复有效触发元素焦点。
- [ ] Dialog 内容在 `360×200` 下可滚动且关闭操作可达，backdrop 由 `dialog::backdrop` 统一拥有。
- [ ] 对嵌套 modal 有明确防护；组件不创建页面 fixed overlay。
- [ ] `src/test/setup.ts` 仅在 jsdom 缺失时 polyfill `showModal/close` 并同步 `open` 属性；生产代码无测试分支。
- [ ] `StatusBanner` 支持四 tone、title/description/action 和 off/polite/assertive announcement，状态不只靠颜色表达。
- [ ] `ConfirmDialog` 只组合 Dialog/Button/Spinner，支持 default/danger、pending、防重复提交、confirm/cancel。
- [ ] 建立 `src/components/patterns/index.ts`；patterns 只从 `@/components/ui` seam 导入。
- [ ] UI-006～UI-008、PT-001～PT-004 自动测试通过。
- [ ] `ui`/`patterns` 无 store/service/Tauri/领域类型 import；typecheck/test/build 通过。

## Out of scope

- 迁移 CacheDetailsDialog 或 Qwen 注销确认。
- 引入第三方 Dialog、portal 或 UI 依赖。

## Comments

