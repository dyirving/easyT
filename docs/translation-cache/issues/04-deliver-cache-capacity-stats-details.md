# 04 — 交付容量管理、Touch 与缓存详情

Status: ready-for-human

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

让持久化缓存能够长期稳定运行并向用户透明展示状态：L1/L2 命中更新访问统计，L2 根据固定容量和确定性 LRU 自动淘汰；设置页只提供“查看缓存详情”入口，弹窗展示条目、实际磁盘占用、命中率、路径和本机存储说明。

## Blocked by

- 03 — 交付 L2 跨启动持久化

## Acceptance criteria

- [x] L1 与 L2 命中都更新访问时间和命中数，Touch 按键合并。
- [x] Touch 在 30 秒、256 个不同键、淘汰前、清除前或关闭前 flush。
- [x] L2 最大逻辑容量 256 MiB、最大 50,000 条；超限后同时降至 230.4 MiB 和 45,000 条以内。
- [x] 每个删除批次最多 500 条，淘汰顺序按最近访问、生成时间和缓存键稳定排序。
- [x] 无 TTL，普通淘汰不执行完整 VACUUM；WAL checkpoint 行为符合批准规则。
- [x] 公开命中率只使用 L1 hit、L2 hit 和 miss；分母为零时显示“—”。
- [x] 统计跨应用重启保留；详情条目数只统计 L2，磁盘占用包括数据库、WAL 和 SHM。
- [x] 设置页不直接铺开统计，只通过按钮打开具备 dialog 语义、键盘关闭和焦点恢复的详情弹窗。
- [x] 弹窗正确展示 loading、ready、degraded 和查询失败状态，且不新增前端依赖。
- [x] Touch 合并、确定性淘汰、低水位、统计公式、持久化统计和弹窗可访问性测试通过。

## Comments

- 2026-08-10 (implemented): 容量管理、Touch、持久统计与只读缓存详情已交付。
  - L1/L2 命中通过单一有界 worker 按键合并 Touch；30 秒、256 个不同键、淘汰前和关闭前会事务化 flush。清除命令及其 flush 接点按工单依赖保留给 05，不在本工单提前扩大权限或 UI 操作范围。
  - L2 使用 256 MiB/50,000 条硬上限和 230.4 MiB/45,000 条双低水位，按 `last_accessed_at_ms → generated_at_ms → cache_key` 稳定淘汰，每批最多 500 条；普通淘汰不执行 VACUUM，容量淘汰后及 WAL 超阈值时执行被动 checkpoint。
  - 统计在 SQLite 中跨重启保留；公开命中率仅使用 L1 hit、L2 hit、miss，零分母返回空值供界面显示“—”。审查修复补充了网络译文最终超 1 MiB 时只计 oversized、不误计公开 miss 的回归测试。
  - 设置页仅新增“查看缓存详情”入口；弹窗展示 L2 条目、main+WAL+SHM 实际占用、上限、命中率、绝对路径、状态和本机明文译文说明，并覆盖 loading/ready/degraded/error、Escape 关闭与焦点恢复。
  - 验证：Rust 160 项、Vitest 44 项、TypeScript typecheck、前端 production build、Rust fmt 与 clippy `-D warnings` 全部通过；Standards/Spec 双轴审查发现的 1 个规格问题和 2 个可维护性判断项已修复，复审通过。
  - 未执行真实账号 E2E 或发布体积测量；发布级手工验收仍由 07 工单负责。
