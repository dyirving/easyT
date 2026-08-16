# 03 — 管理账号生命周期与隔离清理

**Source:** [SDD-qwen-multi-account-round-robin.md](../../../SDD-qwen-multi-account-round-robin.md)

**What to build:** 用户可以重命名、启用/停用、上移/下移、退出登录和删除 Qwen 账号。退出与删除使用确认 Dialog；退出会清理该账号凭证和 profile 但保留账号槽位；删除会清理账号目录和注册表条目；失败时不会伪装成功或影响其他账号。设置页根据 Rust 返回的 `status/actions` 展示可用操作。

**Blocked by:** 02 — 添加并登录独立 Qwen 账号

**Status:** ready-for-human

- [x] 重命名、启用/停用和上移/下移成功后返回完整权威账号池快照，名称、状态和顺序规则保持一致。
- [x] 账号占用时允许重命名和停用，但禁止重新登录、退出登录、删除和逐账号测试，并返回具体错误码。
- [x] 退出登录和删除账号在执行前使用 `ConfirmDialog` 二次确认，不使用 `window.confirm` 或浏览器 prompt。
- [x] 退出登录删除目标账号凭证和 profile、保留账号槽位及其名称/启用状态/顺序，并进入未登录状态。
- [x] 删除账号先建立可恢复清理状态，再提交注册表变更；任一清理失败都保留可诊断数据，不影响其他账号且不能伪装成功。
- [x] 重新登录、退出和删除不得通过残留 profile 或 Cookie 恢复旧身份。
- [x] 前端只依据 Rust 返回的 `status/actions` 渲染操作可用性，不自行复制账号状态机。
- [ ] 账号操作行为、确认交互、键盘焦点、错误码和最小窗口滚动/换行测试通过（自动 controller coverage 已通过；仍需 360x200 人工窗口检查）。

Implementation evidence (2026-08-16): account lifecycle mutations are registry-backed and return the authoritative snapshot; logout/delete hold a fixed non-cursor lease and use staging before destructive cleanup. Settings uses UI Kit `Dialog`, `IconButton`, `Switch`, and `ConfirmDialog`, rendering only Rust-provided `status/actions`. `cargo fmt --manifest-path src-tauri/Cargo.toml`, `cargo test --manifest-path src-tauri/Cargo.toml qwen --no-fail-fast` (64 passed), `npm run typecheck`, and `npm test -- --run src/components/settings/useSettingsController.test.tsx` (11 passed) completed. No real Qwen login/request or 360x200 manual visual check was run.
