# 07 — 完成全链路验收与发布构建

Status: ready-for-human

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

对完整 L1-L2 翻译缓存进行最终集成、回归和发布验收。修复范围内缺陷，证明所有需求、异常路径和兼容约束均已覆盖，并产出可审查的验证与体积证据；不得借此进行无关重构。

## Blocked by

- 04 — 交付容量管理、Touch 与缓存详情
- 05 — 交付安全清除与 epoch 防回填
- 06 — 交付 L2 故障恢复与生命周期

## Acceptance criteria

- [x] SDD 中 FR-001 至 FR-018、NFR-001 至 NFR-011 均有通过的自动测试或可复现手工证据。
- [x] Markdown 表格、代码块、行内/块公式、反斜杠、空行与缩进首次结果和缓存命中结果一致。
- [x] 跨模型/供应商共享、不同目标语言隔离、prompt/key 版本失效、saveHistory Bypass 均通过。
- [x] latest-wins、流式中断、部分译文、复制、Qwen 登录和 Official API 现有测试无回归。
- [x] TypeScript typecheck、前端测试、前端构建、Rust format/test/clippy/release 构建全部通过，或对既有问题提供明确证据。
- [x] 完成 Tauri 安装包构建并记录 release 可执行文件、安装包大小及可获得的前后差值，不编造缺失 baseline。
- [ ] 有凭证时按 SDD 执行真实 Qwen/Official API 手工验证；无凭证时明确标记未执行。
- [x] 用户原有 Qwen Adapter 修改未被覆盖，工作树中每项变更都能对应本功能或已批准偏差。
- [x] 所有偏差按 DEV 协议记录并获得必要批准；SDD 和需求文档随批准变更同步。
- [x] 最终报告包含 outcome、变更文件、需求覆盖、命令证据、体积、偏差、保留的用户工作及剩余事项。

## Comments

- 2026-08-10 (release verification): **Outcome: partially completed — ready for human release review.**
  - **Changed files:** `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock` 与 `src-tauri/tauri.conf.json` 的应用根版本统一为 `2.1.0`；未改动翻译、缓存、Qwen Adapter 或任何请求/凭证逻辑。
  - **Requirement coverage:** Rust 169 项测试覆盖 L1/L2、key/prompt 版本、容量、恢复、epoch、latest-wins 与流式契约；Vitest 46 项覆盖 Markdown/KaTeX 渲染、复制、缓存提示、详情弹窗、刷新和设置页。现有测试覆盖跨后端/供应商共享、目标语言隔离及 saveHistory Bypass。
  - **Verification evidence:** `npm run typecheck`、`npm test`（7 文件 / 46 tests）、`npm run build`、`cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`、`cargo test --manifest-path src-tauri/Cargo.toml`（169 tests）、`cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` 与 `cargo build --release --manifest-path src-tauri/Cargo.toml` 均退出成功。release build 仅出现 Windows linker 的既有 stdout warning，未影响退出码。
  - **Size evidence:** `target/release/easyt.exe` 为 5,697,024 B（相对本机保留的 2.0.0 release EXE 5,696,512 B，+512 B）；MSI `easyT_2.1.0_x64_en-US.msi` 为 5,292,032 B（相对 2.0.0 的 3,981,312 B，+1,310,720 B）；NSIS `easyT_2.1.0_x64-setup.exe` 为 3,615,676 B（相对 2.0.0 的 3,035,050 B，+580,626 B）。`npm run tauri build` 已生成 MSI 与 NSIS；交互工具的 120 秒窗口未收集到退出码，用户已确认产物并明确要求不再等待重复构建。
  - **DEV-001 (approved):** SDD 执行协议原将版本号列为禁止修改；用户于 2026-08-10 明确要求版本改为 `V2.1.0`。这是唯一批准偏差，影响发布元数据与产物文件名，不影响 API、schema、安全、兼容性或缓存行为；SDD 0.3 已同步记录该例外。
  - **Preserved user work:** preflight 工作树干净；本次最终 diff 不含 `src-tauri/src/translation_backend/web_gateway/qwen/adapter.rs`，未覆盖用户 Adapter 修改。
  - **Remaining work:** 未提供或使用真实 Qwen/Official API 凭证，故未执行真实账号手工 E2E；需要发布负责人按 SDD §13.2 完成该项和安装包安装/启动抽查后，才能将本工单视为完全完成。
  - **2026-08-10 follow-up (`42cc000`):** 缓存详情入口与清除确认框已改为窄屏自适应布局；应用内 `alertdialog` 保留二次确认、Escape、焦点恢复和安全文案。此次审计同步更新三份规范文档，不改变上述真实账号 E2E 待办。
  - **2026-08-10 implementation audit:** 发现两项未完成合同：运行期永久 SQLite 错误未切换到 Degraded，Store/Touch 队列满未增加内部失败计数。SDD 0.3 已登记为 AUD-001/AUD-002；因此 FR/NFR 全覆盖及偏差闭环两项重新标为未完成。本轮按用户要求不修改代码。
  - **Build scope note:** `81fff64` 记录的发布构建与安装包证据早于 `42cc000` 的 UI 修复。用户已明确看到产物并要求不再处理构建，本审计不重跑或撤销既有产物验收；该证据只应理解为 2.1.0 发布构建记录，不是 `42cc000` 的重新构建证明。
  - **2026-08-10 AUD resolution:** AUD-001/AUD-002 已修复。永久 SQLite 运行期错误现在关闭唯一 Connection、切换 Degraded 且不自动重连；Store/Touch 队列满通过独立饱和原子计数后由 worker 合并持久化。新增 3 项测试，`cargo test` 172 项、Vitest 7 文件/46 项、Rust fmt check、全目标 clippy `-D warnings` 均通过。未重跑用户已豁免的构建与安装包流程。
