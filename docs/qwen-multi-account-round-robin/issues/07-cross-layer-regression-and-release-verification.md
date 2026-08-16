# 07 — 跨层回归与发布验证

**Source:** [SDD-qwen-multi-account-round-robin.md](../../../SDD-qwen-multi-account-round-robin.md)

**What to build:** 完整功能在缓存、翻译历史、主进度、Official API、登录窗口 capability、日志脱敏、设置页可访问性和窗口尺寸约束下通过回归验证。发布前能够提供迁移故障注入、账号/profile 隔离、A/B/A 轮询、错误码和 release build 的证据；未经明确授权的真实 Qwen 请求明确标记为未执行。

**Blocked by:** 03 — 管理账号生命周期与隔离清理；06 — 流式失败后台复检与退出清理

**Status:** ready-for-human

- [x] 全量 Rust 测试（226）、前端测试（95）、typecheck、frontend build、clippy、fmt、release build 和 `git diff --check` 通过。
- [ ] 回归证明缓存命中不推进游标，跨账号正式重试只创建一条 easyT 翻译历史，测试/复检不污染缓存、历史或主进度。
- [x] Official API、Prompt、Qwen 私有 DTO、模型白名单、AppConfig/WebGatewayConfig schema 和既有 latest-wins 行为保持不变；完整回归测试通过。
- [x] `src-tauri/capabilities/default.json` 仍只授权主窗口；结构化错误和日志审查确认不包含凭证、账号名称、完整 UUID、原文、译文或 response body。
- [x] 受控 loopback/fixture 验证核心认证、429/5xx、网络/超时、协议错误、流式中断和账号池状态；真实 Qwen 请求未执行。
- [ ] 在 `520x390` 和 `360x200` 窗口验证账号名称、状态、错误码、Dialog、滚动、键盘和焦点无重叠或溢出。
- [x] 已记录 production bundle 体积；无新增 UI dependency 或 capability。
- [x] 真实双账号/profile 隔离和轮询人工验收明确标记为未执行，未宣称真实账号验收完成。
- [x] SDD 实施记录同步完成，列出实现证据、验证命令、未执行真实账号检查及 Cookie 清理 API deviation。

## Implementation evidence

- 2026-08-16 full verification: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`; `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`; `cargo test --manifest-path src-tauri/Cargo.toml` (226 passed); `npm run typecheck`; `npm test -- --run` (18 files, 95 passed); `npm run build`; `cargo build --release --manifest-path src-tauri/Cargo.toml` (passed in 57.54s); and `git diff --check`.
- `src-tauri/capabilities/default.json` still lists only `main`. No real Qwen credential/network request or manual `360x200` visual inspection was performed; those remain explicit follow-ups requiring authorization/access.
