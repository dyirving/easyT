# 04 — 交付容量管理、Touch 与缓存详情

Status: ready-for-agent

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

让持久化缓存能够长期稳定运行并向用户透明展示状态：L1/L2 命中更新访问统计，L2 根据固定容量和确定性 LRU 自动淘汰；设置页只提供“查看缓存详情”入口，弹窗展示条目、实际磁盘占用、命中率、路径和本机存储说明。

## Blocked by

- 03 — 交付 L2 跨启动持久化

## Acceptance criteria

- [ ] L1 与 L2 命中都更新访问时间和命中数，Touch 按键合并。
- [ ] Touch 在 30 秒、256 个不同键、淘汰前、清除前或关闭前 flush。
- [ ] L2 最大逻辑容量 256 MiB、最大 50,000 条；超限后同时降至 230.4 MiB 和 45,000 条以内。
- [ ] 每个删除批次最多 500 条，淘汰顺序按最近访问、生成时间和缓存键稳定排序。
- [ ] 无 TTL，普通淘汰不执行完整 VACUUM；WAL checkpoint 行为符合批准规则。
- [ ] 公开命中率只使用 L1 hit、L2 hit 和 miss；分母为零时显示“—”。
- [ ] 统计跨应用重启保留；详情条目数只统计 L2，磁盘占用包括数据库、WAL 和 SHM。
- [ ] 设置页不直接铺开统计，只通过按钮打开具备 dialog 语义、键盘关闭和焦点恢复的详情弹窗。
- [ ] 弹窗正确展示 loading、ready、degraded 和查询失败状态，且不新增前端依赖。
- [ ] Touch 合并、确定性淘汰、低水位、统计公式、持久化统计和弹窗可访问性测试通过。

