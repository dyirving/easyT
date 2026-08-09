# 05 — 交付安全清除与 epoch 防回填

Status: ready-for-human

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

交付用户可控且并发安全的“清除翻译缓存”：操作一次性清理 L1、L2、待处理写入/Touch、隔离文件和统计，同时使用 epoch 保证清除前已经启动的翻译请求即使随后成功，也不能重新填充缓存。清除不取消翻译，也不删除用户正在阅读的译文。

## Blocked by

- 04 — 交付容量管理、Touch 与缓存详情

## Acceptance criteria

- [x] 清除开始时先增加 epoch，并在同一 L1 状态锁内清空条目与进程内统计。
- [x] 所有 Store/Touch 携带 epoch；L1 条件插入和 worker 执行都拒绝旧 epoch。
- [x] worker 关闭 Connection 后删除当前数据库、WAL、SHM 和所有通过严格命名与目录边界校验的隔离文件，再创建空 schema 与统计。
- [x] 不使用 DELETE+VACUUM，不使用宽泛路径、glob 或未验证的递归删除。
- [x] 清除完成后才向界面返回成功统计；重复清除空缓存仍然成功。
- [x] 清除前有请求在途时，该请求可以完成展示，但不能写回 L1/L2。
- [x] 清除成功后当前原文和译文保留、缓存来源提示移除、不自动发起翻译。
- [x] UI 有明确二次确认，说明不会删除设置、Qwen 登录状态或网页对话记录；执行期间不能重复提交。
- [x] 清除失败时 L1 保持为空、L2 进入 Degraded，普通翻译继续可用并显示安全错误。
- [x] clear/in-flight 竞态、队列旧命令、重复清除、路径验证和前端状态测试通过。

## Comments

- 2026-08-10 (implemented): 安全清除链路已交付。
  - `TranslationCache::clear` 在同一 L1 锁内先清空条目并推进 epoch；持久 worker 的 Store/Touch/Lookup 均按 epoch 拒绝旧命令。Clear 使用 await/reply，完成数据库重建后才返回零条目、空命中率统计；重复 Clear 幂等。
  - worker 先关闭唯一 SQLite Connection，再仅删除经解析和目录边界验证的 main/WAL/SHM，以及全部严格命名的隔离文件族，以恢复“最多保留一组”的不变量；不使用 `DELETE+VACUUM`、glob 或递归删除。删除/重建失败会使 L2 进入 Degraded，而 L1 保持已清空且后续翻译仍可用。
  - 审查修复确保乱序旧 Clear 不会使 worker epoch 倒退；同时拒绝 `cache` junction/symlink 越界与符号链接文件删除。
  - 新增 `clear_translation_cache` 本机主窗口 IPC；详情弹窗提供清除/重建二次确认、执行中防重复、返回统计刷新及失败安全文案。成功后由 `App` 调用既有 store 动作，仅移除当前译文的缓存来源提示，不清文本也不自动翻译。
  - 验证：Rust 164 项、Vitest 46 项、TypeScript typecheck、前端 production build、Rust fmt 与 clippy `-D warnings` 全部通过；Standards/Spec 双轴审查发现的 epoch 与路径校验问题已修复并复审通过。
  - 未执行真实账号 E2E 或发布级手工验收；该部分保留给 07 工单。
