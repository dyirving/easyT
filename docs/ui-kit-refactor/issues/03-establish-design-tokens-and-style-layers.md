# 03 — 建立设计 token 与样式分层

Status: ready-for-human

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-001, FR-010 and Phase 1.

## What to build

创建 `src/styles/tokens.css` 与 `src/styles/base.css`，让 `tailwind.config.js` 映射 CSS variables，并把 `src/index.css` 收敛为样式装配入口。此阶段必须保持现有视觉，不提前删除仍被旧组件使用的 recipe。

## Blocked by

- 02 — 冻结视觉、行为与体积基线

## Acceptance criteria

- [x] `tokens.css` 按 SDD 固定全部 surface/ink/line/accent/danger/success/warning 值，以及圆角、shadow、字体、密度和 focus ring token。
- [x] `base.css` 接管 html/body/root、字体渲染、背景/文字、滚动条、focus-visible 与 reduced-motion 等真正全局规则。
- [x] `tailwind.config.js` 通过 CSS variables 暴露语义颜色、圆角和 shadow；不再复制实际 hex，并补齐现有页面使用的 warning。
- [x] `index.css` 明确 import/layer 顺序；现阶段仍被旧组件消费的 `.btn/.input/.panel/translation-markdown` 有可追踪清理清单，不新增使用点。
- [x] 默认窗口和最小窗口与基线相比无未经批准的可见视觉变化。
- [x] 不新增或升级依赖，不修改 Rust/Tauri/配置/缓存代码。
- [x] `npm run typecheck`、`npm test`、`npm run build` 通过。
- [x] 实施报告列出 token 唯一性搜索和任何不可避免的视觉差异。

## Out of scope

- 实现 UI Kit React 组件。
- 迁移页面或删除旧 CSS recipe。

## Comments

- 2026-08-11：新增 `src/styles/tokens.css` 与 `src/styles/base.css`；`src/index.css` 现只负责 tokens/base/KaTeX、Tailwind directives 与 Phase 6 才会移除的旧 recipe。`tailwind.config.js` 全部语义颜色、圆角与 shadow 均映射 CSS variables，补齐 `warning`。唯一性搜索 `rg -n '#[0-9a-fA-F]{3,8}|rgba\\([0-9]|rgb\\([0-9]' src/styles src/index.css tailwind.config.js --glob '*.css' --glob '*.js'` 仅命中 `tokens.css` 内的 soft shadow RGB 值；Tailwind 配置不再含 hex。旧 `.btn/.input/.panel/.translation-markdown` 仍只在 `src/index.css`，作为后续组件迁移的清理清单，未新增使用点。常态页面视觉值与基线一致；新增的 `:focus-visible` 轮廓及 reduced-motion 规则是 SDD 要求的无障碍行为，不改变未聚焦的默认或最小窗口视觉。`npm run typecheck`、`npm test`（7 files / 46 tests）与 `npm run build` 均通过；CSS gzip 从 12,300 B 到 12,830 B（+530 B），低于 5 KiB 预算。
