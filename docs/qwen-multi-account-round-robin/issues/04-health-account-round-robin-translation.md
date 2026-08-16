# 04 — 健康账号 Round Robin 翻译

**Source:** [SDD-qwen-multi-account-round-robin.md](../../../SDD-qwen-multi-account-round-robin.md)

**What to build:** 两个或更多健康 Qwen 账号可以为真实一次性输出按设置页顺序执行 A/B/A Round Robin。单账号同一时间只能有一项 Qwen 网络操作；多个账号可以并行。缓存命中不选择账号、不占用账号、不推进游标；真实发送前取消或本地失败也不推进游标。全局“测试连接”能够选择并使用下一个健康账号。

**Blocked by:** 02 — 添加并登录独立 Qwen 账号

**Status:** ready-for-human

- [x] `QwenAccountPool` 成为 WebGateway 的 Qwen 入口，TranslationBackend 和 Official API 边界保持不变。
- [x] 健康、已登录、已启用且空闲的账号按显示顺序执行连续 A/B/A Round Robin；跳过不可用账号。
- [x] 单账号 lease 保证同一时间最多一项正式翻译、复检或测试；不同账号可以并行；lease drop/abort 能唤醒等待者。
- [x] 全部候选账号忙碌时按当前操作期限等待，超时返回 `QW-POOL-007`；不把忙碌误判为不健康。
- [x] 轮询游标只在首个真实网络请求即将发送时提交；发送前取消、请求构造失败或凭证借用失败不推进游标。
- [x] 缓存命中不选择账号、不获取 lease、不推进游标；Refresh/Bypass 的真实正式翻译参与轮询。
- [x] 全局“测试连接”使用下一个健康账号并只推进一次；不写入缓存、easyT 翻译历史或主翻译进度。
- [ ] 单元、异步并发和受控 Qwen executor 测试通过；缓存决策继续在 `TranslationBackend` 账号选择之前，现有翻译与 Official API 合同未修改（focused scheduler/cache coverage 已通过；完整 translation/Official API regression remains pending).

Implementation evidence (2026-08-16): `WebGateway` delegates both output modes to `QwenAccountPool`; the executor prepares all local request data before `AccountLease::commit_send()` and executes exactly one HTTP request. Scheduler unit tests cover A/B/A, skipped entries, no cursor advance for an uncommitted lease, busy timeout `QW-POOL-007`, and lease-drop wakeup without network access. Global testing uses the same selection with `save_history=false` and a discard reporter. Cache behavior is covered by `cargo test --manifest-path src-tauri/Cargo.toml translation_backend::tests --no-fail-fast` (16 passed), where cache hits return before the fetch closure and pool selection. `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml qwen --no-fail-fast` (64 passed), `npm run typecheck`, and `npm test -- --run src/components/settings/useSettingsController.test.tsx` (11 passed) completed. Ticket 05 retry/probe/cooldown policy is intentionally not implemented or tested here; no real Qwen request was issued.
