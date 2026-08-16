# 02 — 添加并登录独立 Qwen 账号

**Source:** [SDD-qwen-multi-account-round-robin.md](../../../SDD-qwen-multi-account-round-robin.md)

**What to build:** 用户可以通过名称 Dialog 添加 Qwen 账号。名称校验和 10 个账号上限生效；新账号拥有独立 UUID、凭证位置和 WebView2 profile；添加后能够开始该账号的登录流程。全局同时只能有一个登录流程，登录成功、取消、失败和重新登录都保持正确的旧状态与凭证行为。设置页能够显示登录中和登录结果。

**Blocked by:** 01 — 安全迁移并展示 Qwen 账号池库存

**Status:** ready-for-human

- [x] 添加账号会先完成名称校验、账号槽位和独立目录初始化，再打开绑定目标账号的登录窗口。
- [x] 第 10 个账号可以创建，第 11 个账号返回 `QW-POOL-002`，非法名称返回 `QW-POOL-011`。
- [x] 两个账号使用不同的凭证路径和 profile；一个账号的登录、取消或失败不会修改另一个账号的数据或状态。
- [x] 全局同一时间最多一个账号处于登录或重新登录流程；其他账号尝试登录返回 `QW-LOGIN-001`。
- [x] 初次登录成功原子保存凭证并进入已登录、健康状态，不额外发送测试请求。
- [x] 重新登录成功前保留旧凭证和登录前状态；取消或失败后恢复原状态，过期账号仍保持登录过期。
- [x] 登录窗口继续使用既有 host allowlist、固定 label 和 capability 边界，远程页面不获得新的 Tauri command 权限。
- [x] 登录 watcher、凭证 zeroize、登录取消和结构化登录错误测试通过；不得使用真实凭证。

Implementation evidence: `cargo test --manifest-path src-tauri/Cargo.toml qwen:: --no-fail-fast`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`, `npm test -- --run src/components/settings/useSettingsController.test.tsx`, and `npm run typecheck` passed on 2026-08-16. The installed Tauri/WebView2 surface could not be verified to expose target-Cookie deletion without a real login. The watcher instead rejects a candidate equal to the account's persisted ticket and continues waiting, so stale profile Cookie data cannot silently count as a new login. This is a deliberate, test-covered deviation; profile Cookie deletion remains for a follow-up once a supported API is verified.
