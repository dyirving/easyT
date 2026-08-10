# 07 — 迁移翻译领域到 UI Kit

Status: ready-for-agent

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-006 through FR-008 and Phase 4.

## What to build

把现有翻译领域组件和 Markdown 样式迁入 `src/components/translation/`，提取 `useTranslationController`，并让 `TranslationPage` 只组合领域展示组件。保持翻译请求、latest-wins、流式输出、未完成译文、缓存来源和强制刷新语义不变。

## Blocked by

- 04 — 交付操作与反馈基础组件
- 05 — 交付表单基础组件与自动关联
- 06 — 交付原生 Dialog 与共享交互模式

## Acceptance criteria

- [ ] CacheNotice、ErrorState、LoadingState、MarkdownTranslation、OriginalTextPanel、TranslationHeader、TranslationPanel 及其测试迁入 translation 目录。
- [ ] `MarkdownTranslation.css` 与组件同目录并显式加载；KaTeX CSS 全仓只有一个明确入口，无重复产物。
- [ ] `useTranslationController` 集中 store/service 协调、派生状态、普通翻译、强制刷新、重试、复制和固定窗口动作，不复制 store 状态机。
- [ ] `TranslationPage` 不直接 import store/service/Tauri，只组合 controller 返回的 view state/actions。
- [ ] 手动输入使用 Textarea；按钮、图标按钮、loading 和状态反馈复用 UI Kit/patterns。
- [ ] 建立 `src/components/translation/index.ts`；跨目录只经四个目录 seam import，无 translation 深路径 import。
- [ ] idle、快捷键提示、翻译中、流式输出、未完成译文、普通错误、refresh error、复制、pin、缓存来源和“重新翻译”行为保持。
- [ ] 缓存来源提示继续与译文 DOM/可复制内容分离。
- [ ] 原有 TranslationPage、TranslationPanel、MarkdownTranslation、translationStore、translationCoordinator 测试全部保留并通过；新增 controller 空文本、长度上限、forceRefresh、复制/pin 测试。
- [ ] 新路径通过后删除翻译领域旧文件，不保留旧路径 re-export；typecheck/test/build 通过。

## Out of scope

- 修改 translationStore、translationRunner、translationCoordinator 的业务语义。
- 修改缓存、模型、翻译后端或 Tauri command。

## Comments

