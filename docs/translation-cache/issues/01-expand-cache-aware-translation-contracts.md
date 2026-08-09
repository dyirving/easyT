# 01 — 扩展缓存感知翻译合同

Status: ready-for-agent

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

在不启用实际缓存的前提下，扩展翻译请求、统一结果和前端状态，使普通翻译与重新翻译意图能够从界面完整传递到 TranslationBackend，并让结果能够携带“是否来自缓存”。这是一次 expand/prefactor：完成后现有翻译行为必须不变，所有结果暂时都报告为非缓存结果，为后续 L1 垂直切片建立稳定合同。

## Blocked by

None — can start immediately.

## Acceptance criteria

- [ ] 普通快捷键和手动输入明确传递普通翻译意图，右上角重新翻译与错误态重试明确传递强制刷新意图。
- [ ] 一次性输出和流式输出使用相同的强制刷新与缓存来源合同。
- [ ] TranslationBackend 返回统一 outcome，供应商 Adapter 的 BackendResult 合同保持不变。
- [ ] Tauri 返回结果新增缓存来源布尔值；未接入缓存时始终为 false。
- [ ] 前端状态能够表达 refreshing，并继续使用 requestId 阻止旧翻译请求覆盖新界面。
- [ ] 当前翻译、流式输出、latest-wins、复制和错误展示行为保持不变。
- [ ] 不修改 Qwen/Official API 私有协议、登录逻辑或应用配置 schema。
- [ ] 合同与状态机的 Rust、TypeScript 单元测试通过，完整现有测试保持绿色。

## Comments

- 2026-08-09 (implemented): 全部验收标准已完成。Rust 104 测试、Vitest 40 测试、typecheck/build/clippy/fmt 均绿。
  - 合同归属：`TranslationOptions`/`CacheStatus`/`TranslationOutcome` 放在 `translation_backend/models.rs`（SDD 6.6 允许的唯一归属），L1/L2 切片落地时再按需迁移到 `cache/entry.rs`。
  - 策略 seam：`translation_backend/mod.rs::resolve_cache_policy`（Use/Refresh/Bypass）先于实际缓存实现，L1 切片只替换 Use 分支。
  - `failRefreshRequest` 的 `kind` 参数按 SDD 6.10 签名保留；SDD 6.9/6.11 未定义 refreshErrorKind 存储字段，故仅用于未来提示路由。
  - `clearCacheSourceNotice` 暂无 UI 调用方，随设置页清除入口（后期工单）接入。
  - 本次未接入实际缓存：`fromCache` 恒为 false，`refresh` 状态在真实运行中不会触发（仅供测试与后续切片）。

