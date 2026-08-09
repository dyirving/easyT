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

