# 02 — 交付 L1 内存缓存完整链路

Status: ready-for-human

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

交付一个可独立演示的进程内翻译缓存：同一规范化原文和目标语言第一次完整翻译成功后，第二次普通翻译无需网络即可返回；用户能够看到独立缓存来源提示，并可使用重新翻译调用当前模型刷新共享结果。缓存必须跨供应商和模型共享，同时严格保护 Markdown、LaTeX 和 latest-wins 行为。

## Blocked by

- 01 — 扩展缓存感知翻译合同

## Acceptance criteria

- [x] 使用版本化、domain-separated、长度前缀的 BLAKE3 256-bit 精确键。
- [x] 单行、多行、BOM、CRLF/CR、目标语言和 Markdown/LaTeX 规范化符合批准规则。
- [x] L1 使用短/长双池 LRU，总容量不超过 10 MiB、1,024 条，长文本不超过 7 MiB、256 条，短文本不超过 768 条。
- [x] 单条逻辑大小超过 1 MiB 时跳过缓存但正常翻译；恰好 1 MiB 可缓存。
- [x] Use、Refresh、Bypass 行为正确；Qwen 保存网页历史、连接测试与诊断请求不读写缓存。
- [x] 模型、供应商和账号不参与键，目标语言与版本参与键。
- [x] 只缓存完整成功的非空结果；取消、部分回答、流式增量和失败结果不缓存。
- [x] 缓存提示位于原文与译文之间，不参与 Markdown/KaTeX、复制、字数或持久化。
- [x] 重新翻译失败时保留旧缓存译文和来源提示，并显示明确失败信息。
- [x] 键、规范化、容量、淘汰、策略、页面和前端状态测试通过。

## Comments

- 2026-08-10 (implemented): L1 完整链路已交付。版本化 BLAKE3 键、单/多行规范化、短/长双池 LRU、1 MiB 边界、Use/Refresh/Bypass、缓存来源提示与刷新失败保留均已有自动测试。
  - 验证：Rust 145 项、Vitest 40 项、TypeScript typecheck、前端 build、Rust fmt 与 clippy `-D warnings` 全部通过。
  - L1 跨池淘汰只比较两个分池的 LRU 尾部，保持插入路径平均 O(1)；命中来源状态从缓存门面透传，为后续 L2 `PersistentHit` 保留合同。
  - 审查修复：锁中毒后校验并恢复 L1 不变量；`access_tick` 溢出前重编号；`Cargo.lock` 只保留 BLAKE3/LRU 必需依赖变化。
  - 未执行真实 Qwen/Official API 账号 E2E；该发布级手工验收保留到 07 工单。
