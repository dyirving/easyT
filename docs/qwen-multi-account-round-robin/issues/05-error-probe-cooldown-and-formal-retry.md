# 05 — 错误复检、冷却与一次性正式重试

**Source:** [SDD-qwen-multi-account-round-robin.md](../../../SDD-qwen-multi-account-round-robin.md)

**What to build:** 一次性输出遇到 429/5xx 时最多保留一次正式重试，第二次重新 Round Robin；网络、超时、协议和其他一次性错误不扩大正式重试范围。出错账号按规则固定复检，复检成功保持健康，失败进入五分钟冷却，冷却后进入待验证；401/403 直接变为登录过期。用户可以逐账号测试并提前恢复账号。所有结果通过稳定错误码返回并在设置页显示。

**Blocked by:** 04 — 健康账号 Round Robin 翻译

**Status:** ready-for-human

- [x] 单次 Qwen executor 恰好执行一次请求；一次性输出仅在第一次 429/5xx 后允许一次正式重试，第二次重新 Round Robin。
- [x] 网络错误、超时、协议错误和其他非 429/5xx 错误不触发新的正式重试；没有其他健康账号时允许按既有规则再次选择原账号。
- [x] 正式请求已发送且发生可复检错误时，固定原账号最多执行一次轻量 `hi` 复检，不推进游标、不写缓存、历史或主进度，并强制关闭 Qwen 网页历史。
- [x] 一次性输出的账号等待、正式请求、backoff、复检和正式重试共享原总超时；无剩余时间时不复检/重试并将账号转为待验证。
- [x] 复检失败进入五分钟冷却，冷却结束转为待验证；待验证账号被正式调度命中时先复检，失败后重新冷却。
- [x] 401/403 不执行复检，分别持久化为登录过期并返回 `QW-AUTH-401`/`QW-AUTH-403`。
- [x] 全局测试和逐账号测试遵守各自的游标、lease、健康更新和历史/缓存隔离规则；逐账号测试忙碌时立即返回 `QW-POOL-009`。
- [x] Rust/TypeScript 错误合同包含独立 `code`；前端以 `{message} [{code}]` 展示 Qwen 错误且不解析中文消息。
- [ ] 429/5xx、网络、超时、认证、冷却、待验证、测试和敏感 response body 脱敏测试通过。

## Implementation evidence

- `QwenRequestExecutor` still performs one request only. `QwenAccountPool` owns one-shot selection, bounded probe, 250 ms backoff, and at most one newly selected formal retry; streaming remains one formal attempt and does not start Ticket 06 background probing.
- The controlled loopback fixture covers a formal 429, fixed-account `hi` probe, `temporary=true` probe request, discard progress, and second formal request on the next account. It uses synthetic local tickets only.
- Runtime cooldown state is authoritative in Rust snapshots and lazily becomes `pendingVerification`; the settings controller polls snapshots only while Rust reports logging in, busy, or cooling down. No permanent timer is added.
- `cargo test --manifest-path src-tauri/Cargo.toml qwen --no-fail-fast` passed 68 tests. Focused cache/history/progress/backend tests, `npm run typecheck`, and 19 targeted frontend tests also passed on 2026-08-16.
- No real Qwen credential or network request was used. Controlled tests do not yet separately exercise every network, timeout, 401, 403, 5xx, and sensitive upstream-response-body path; retain the final validation checkbox for that coverage.
