# 01 — 安全迁移并展示 Qwen 账号池库存

**Source:** [SDD-qwen-multi-account-round-robin.md](../../../SDD-qwen-multi-account-round-robin.md)

**What to build:** 应用启动时能够安全读取 Qwen 账号注册表；现有单账号凭证和浏览器 profile 能迁移为“默认账号”；迁移中断可以恢复；注册表损坏时保留账号数据并生成禁用的恢复条目。设置页能够读取并展示权威账号池快照和恢复警告。

**Blocked by:** None — can start immediately

**Status:** ready-for-human

- [x] 账号模型、名称和 UUID 校验符合规则，账号池上限为 10，账号元数据不进入 `AppConfig`。
- [x] 注册表使用原子写入和 schema 校验；未知版本不会覆盖原有数据。
- [x] 旧单账号凭证和 profile 通过可恢复 journal 迁移为一个完整的“默认账号”，覆盖每个中断边界和重复启动场景。
- [x] 注册表损坏时隔离注册表、保留所有账号目录和凭证，并生成默认禁用的恢复条目。
- [x] 启动恢复不会恢复轮询游标、占用或冷却截止时间，且不会主动向 Qwen 发请求。
- [x] Rust 能返回不泄漏凭证、ticket、Cookie 或完整注册表正文的账号池快照和存储错误码。
- [x] 账号池快照可由设置页读取并展示，现有单账号登录和翻译行为在尚未启用新调度前保持不变。
- [x] 注册表 round-trip、schema/损坏恢复、迁移故障注入和敏感信息脱敏测试通过。
