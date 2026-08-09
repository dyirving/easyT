# 07 — 完成全链路验收与发布构建

Status: ready-for-agent

## Source

Canonical design: [SDD-translation-cache.md](../../SDD-translation-cache.md)

## What to build

对完整 L1-L2 翻译缓存进行最终集成、回归和发布验收。修复范围内缺陷，证明所有需求、异常路径和兼容约束均已覆盖，并产出可审查的验证与体积证据；不得借此进行无关重构。

## Blocked by

- 04 — 交付容量管理、Touch 与缓存详情
- 05 — 交付安全清除与 epoch 防回填
- 06 — 交付 L2 故障恢复与生命周期

## Acceptance criteria

- [ ] SDD 中 FR-001 至 FR-018、NFR-001 至 NFR-011 均有通过的自动测试或可复现手工证据。
- [ ] Markdown 表格、代码块、行内/块公式、反斜杠、空行与缩进首次结果和缓存命中结果一致。
- [ ] 跨模型/供应商共享、不同目标语言隔离、prompt/key 版本失效、saveHistory Bypass 均通过。
- [ ] latest-wins、流式中断、部分译文、复制、Qwen 登录和 Official API 现有测试无回归。
- [ ] TypeScript typecheck、前端测试、前端构建、Rust format/test/clippy/release 构建全部通过，或对既有问题提供明确证据。
- [ ] 完成 Tauri 安装包构建并记录 release 可执行文件、安装包大小及可获得的前后差值，不编造缺失 baseline。
- [ ] 有凭证时按 SDD 执行真实 Qwen/Official API 手工验证；无凭证时明确标记未执行。
- [ ] 用户原有 Qwen Adapter 修改未被覆盖，工作树中每项变更都能对应本功能或已批准偏差。
- [ ] 所有偏差按 DEV 协议记录并获得必要批准；SDD 和需求文档随批准变更同步。
- [ ] 最终报告包含 outcome、变更文件、需求覆盖、命令证据、体积、偏差、保留的用户工作及剩余事项。

