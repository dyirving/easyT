# 06 — 流式失败后台复检与退出清理

**Source:** [SDD-qwen-multi-account-round-robin.md](../../../SDD-qwen-multi-account-round-robin.md)

**What to build:** 流式输出失败时立即向用户返回原错误和既有未完成译文语义，不进行正式重试；原账号继续占用并在后台执行一次使用丢弃 reporter 的复检。复检完成后正确更新健康状态并释放账号占用。应用退出能够取消登录 watcher 和后台复检、关闭登录窗口、释放 lease，并按约定恢复复检前状态。

**Blocked by:** 05 — 错误复检、冷却与一次性正式重试

**Status:** ready-for-human

- [x] 流式正式翻译失败后立即完成用户翻译请求，不发起正式重试，并保留现有未完成译文和错误语义。
- [x] 原账号在后台复检结束前保持使用中，其他正式翻译和测试不能获取该账号；后台复检最长使用当前 `timeoutSeconds`。
- [x] 后台复检固定使用丢弃 reporter，不发送主翻译页的阶段/正文事件，不写入缓存或任何翻译历史。
- [x] 后台复检成功、普通失败和 401/403 分别按健康、冷却和登录过期规则更新账号，然后释放 lease。
- [x] latest-wins 取消、用户取消和应用退出不处罚账号；退出中断复检时恢复复检前健康状态，已完成的健康更新不回滚。
- [x] 应用退出不等待网络完成，能够取消登录 watcher、后台复检、登录窗口和所有 lease，且不遗留可运行的后台任务。
- [x] 流式静默超时、partial response、任务取消、shutdown 和 task 泄漏测试通过。

## Implementation evidence

- `QwenAccountPool::translate_stream` makes one formal streaming attempt. A sent, non-auth Qwen failure returns unchanged to the caller and retains its lease in one fixed-account background `hi` probe. The probe is bounded by the current clamped `timeoutSeconds`, uses `TranslationProgressReporter::discard()`, `saveHistory=false`, and never commits a formal cursor send.
- The pool registers each probe abort handle before permitting it to start. `shutdown()` marks the pool unavailable, cancels the active login watcher, aborts tracked probes without awaiting network, and releases scheduler leases. Interrupted probes have not changed health, while completed health updates remain authoritative.
- Controlled loopback tests cover partial stream semantics and one formal send plus one discard probe, busy exclusion, no probe content delta, successful release/removal, user and latest-wins cancellation without a probe or health change, normal probe failure to cooldown, 401 probe failure to persisted expiration, and shutdown task/lease cleanup. Existing SSE coverage exercises streaming silent timeout and partial response.
- Verification on 2026-08-16: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`; `cargo test --manifest-path src-tauri/Cargo.toml qwen --no-fail-fast` (74 passed); focused translation backend/cache/history/progress Rust tests (16/12/1/3 passed); `npm run typecheck`; targeted settings/service Vitest suites (13 passed); and `git diff --check`. No real Qwen credential or network request was used. Ticket 07 release verification, full test/build/clippy/release-build, and real-account manual validation were not run.
