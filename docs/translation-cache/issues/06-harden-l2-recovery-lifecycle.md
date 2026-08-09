# 06 — 交付 L2 故障恢复与生命周期

Status: ready-for-human

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

完善 L2 在真实 Windows 环境中的恢复和生命周期行为：临时锁与 I/O 错误透明降级，非法行局部删除，损坏或不兼容数据库隔离重建，永久错误进入 Degraded；用户可以从详情弹窗显式重建，应用退出在有限时间内 flush，而任何缓存故障都不阻断翻译。

## Blocked by

- 03 — 交付 L2 跨启动持久化
- 05 — 交付安全清除与 epoch 防回填

## Acceptance criteria

- [x] BUSY、LOCKED、临时 I/O、队列满和查询超时只让当前请求走网络，worker 保持 Ready。
- [x] 非法单行记录被删除并按 miss 处理，不导致整库重建。
- [x] CORRUPT、NOTADB、schema/迁移失败或不支持的新版本被隔离并尝试创建新库。
- [x] 隔离文件带时间戳且最多保留一组；清除/重建覆盖这些文件。
- [x] 权限、磁盘和永久初始化失败进入 Degraded，本次运行不循环重连，L1 和网络仍可用。
- [x] Degraded 时弹窗显示“持久化缓存不可用”，操作变为“重建持久化缓存”，成功后回到 Ready。
- [x] 启动不等待 SQLite；退出最多等待 1 秒 flush Touch/Store、checkpoint 和关闭，超时不阻止退出。
- [x] 日志只含操作、状态、错误类别、条数和字节数，不记录原文、译文、完整键、凭证、请求或响应。
- [x] 新缓存命令不会向远程 Qwen WebView 暴露 Tauri 权限。
- [x] 故障注入、迁移、损坏隔离、Degraded/重建、启动和退出预算测试通过。
