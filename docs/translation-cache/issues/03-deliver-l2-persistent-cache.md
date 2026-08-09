# 03 — 交付 L2 跨启动持久化

Status: ready-for-agent

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

把已经可用的 L1 翻译缓存扩展为跨启动的 L2 SQLite 缓存。首次网络翻译应先立即进入 L1 并向用户返回，再异步持久化；应用重启后，相同普通翻译应在固定查询预算内从 L2 返回并提升到 L1。SQLite 必须由单一专用 worker 所有，不能阻塞主窗口或 Tokio core。

## Blocked by

- 02 — 交付 L1 内存缓存完整链路

## Acceptance criteria

- [ ] 使用兼容项目 MSRV 的批准依赖版本，并启用 bundled SQLite；不得升级 Rust MSRV。
- [ ] 数据库固定创建在 easyT_Data/cache 下，schema 与 user_version=1 符合批准设计，原文不落库。
- [ ] 单一命名 worker 线程独占唯一 Connection，使用容量 512 的有界命令队列和 512 KiB 栈。
- [ ] 数据库异步初始化；Starting 或不可用期间 L1 与网络翻译继续工作。
- [ ] L2 Lookup 成功入队后的端到端预算不超过 50 ms；超时、队列满或未就绪时立即走网络，迟到结果被丢弃。
- [ ] 完整成功结果同步写 L1、异步 UPSERT L2；L2 写失败不撤销译文或 L1。
- [ ] L2 命中返回完整结果、提升 L1，并且流式配置下不伪造增量事件。
- [ ] 关闭并重新启动缓存后，相同键能从 L2 命中。
- [ ] 测试只使用隔离临时目录，不接触真实安装目录或用户缓存。
- [ ] schema、UPSERT、重启命中、查询预算、队列满和 write-behind 测试通过。

## Comments

- 2026-08-10 (implemented): L2 跨启动持久化链路已交付。`TranslationCache` 现在按 L1 → 50 ms L2 → 网络查找，完整成功结果同步进入 L1 并通过 512 槽有界队列异步 UPSERT；重启命中会返回完整来源并提升到 L1。
  - SQLite 使用批准的 `rusqlite 0.40.2`、`default-features=false`、`bundled`；数据库位于传入的 `easyT_Data/cache/translation_cache.sqlite3`，v1 schema 测试覆盖精确列、索引、`WITHOUT ROWID`、统计表和原文不落库。
  - worker 使用唯一命名线程、512 KiB 栈和单一 Connection；Starting、Degraded、50 ms 超时、迟到 reply、队列满、UPSERT、write-behind、关闭重开及 L1 提升均有隔离临时目录测试。
  - 验证：Rust 153 项、Vitest 40 项、TypeScript typecheck、前端 build、Rust fmt 与 clippy `-D warnings` 全部通过；Standards/Spec 双轴审查及修复复审通过。
  - 项目 `rust-version=1.77.2` 未修改。当前 Rust 1.97.1 的 locked 构建通过；尝试安装 1.77.2 做精确 MSRV 实测时，官方 rustup 下载多次只留下缺失 rustc manifest 的不完整工具链，已清理，因此这项旧工具链实测证据仍需在可用环境补跑。
  - 未执行真实 Qwen/Official API 账号 E2E；发布级手工验收保留到 07 工单。
