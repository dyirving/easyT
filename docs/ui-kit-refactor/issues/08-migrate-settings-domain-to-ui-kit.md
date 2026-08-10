# 08 — 迁移设置领域到 UI Kit

Status: ready-for-human

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), FR-006, FR-007, FR-009 and Phase 5.

## What to build

把设置页拆为领域展示组件、`useSettingsController` 和 `useCacheDetailsController`，迁移全部表单控件、状态反馈、缓存详情及破坏性确认。保留现有配置、Official API、Qwen 登录和缓存操作行为。

## Blocked by

- 06 — 交付原生 Dialog 与共享交互模式

## Acceptance criteria

- [x] ShortcutInput、CacheDetailsDialog 迁入 settings 目录；提取 SettingsHeader、SettingsRow、OfficialApiPanel、WebGatewayPanel。
- [x] `useSettingsController` 管理配置加载/草稿/保存/连接测试、backend/provider change、Qwen 登录状态与轮询、登录/注销意图。
- [x] `useCacheDetailsController` 管理统计 loading/ready/degraded/error、清理/重建确认、防重复、失败恢复和成功通知。
- [x] controller timer/listener 在卸载时清理，保留现有 1 秒登录轮询和取消保护；展示组件不读取 store/service/Tauri。
- [x] SettingsPage 只组合 controller 与 settings UI，不直接 import store/service/Tauri。
- [x] 所有 select/input/switch/icon button 复用 UI Kit；API Key 显隐 IconButton 具有明确 label。
- [x] Qwen 注销与缓存清理/重建统一使用 ConfirmDialog；全仓删除 `window.confirm` 和缓存私有确认 overlay。
- [x] CacheDetailsDialog 使用原生 Dialog，详情与确认不同时保持两个 modal open；Escape、初始焦点与焦点恢复正确。
- [x] 保持 Official/WebGateway 切换、模型选项、saveHistory、登录/重登/退出、保存、连接测试、缓存详情和清除后的通知语义。
- [x] 建立 `src/components/settings/index.ts`；无 settings 深路径 import；新路径通过后删除设置领域旧文件和 `Field.tsx`。
- [x] 迁移原有缓存详情测试，并新增 settings/cache controller 与 SettingsPage 组合测试；typecheck/test/build 通过。

## Out of scope

- 改变配置 schema、Qwen 协议、登录窗口或缓存后端。
- 新增设置项或重新设计设置页。

## Comments
