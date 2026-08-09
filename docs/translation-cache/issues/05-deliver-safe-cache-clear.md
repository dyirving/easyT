# 05 — 交付安全清除与 epoch 防回填

Status: ready-for-agent

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

交付用户可控且并发安全的“清除翻译缓存”：操作一次性清理 L1、L2、待处理写入/Touch、隔离文件和统计，同时使用 epoch 保证清除前已经启动的翻译请求即使随后成功，也不能重新填充缓存。清除不取消翻译，也不删除用户正在阅读的译文。

## Blocked by

- 04 — 交付容量管理、Touch 与缓存详情

## Acceptance criteria

- [ ] 清除开始时先增加 epoch，并在同一 L1 状态锁内清空条目与进程内统计。
- [ ] 所有 Store/Touch 携带 epoch；L1 条件插入和 worker 执行都拒绝旧 epoch。
- [ ] worker 关闭 Connection 后删除当前数据库、WAL、SHM 和最多一组隔离文件，再创建空 schema 与统计。
- [ ] 不使用 DELETE+VACUUM，不使用宽泛路径、glob 或未验证的递归删除。
- [ ] 清除完成后才向界面返回成功统计；重复清除空缓存仍然成功。
- [ ] 清除前有请求在途时，该请求可以完成展示，但不能写回 L1/L2。
- [ ] 清除成功后当前原文和译文保留、缓存来源提示移除、不自动发起翻译。
- [ ] UI 有明确二次确认，说明不会删除设置、Qwen 登录状态或网页对话记录；执行期间不能重复提交。
- [ ] 清除失败时 L1 保持为空、L2 进入 Degraded，普通翻译继续可用并显示安全错误。
- [ ] clear/in-flight 竞态、队列旧命令、重复清除、路径验证和前端状态测试通过。

