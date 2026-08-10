# 02 — 冻结视觉、行为与体积基线

Status: ready-for-human

## Source

Canonical design: [SDD-ui-kit-refactor.md](../../SDD-ui-kit-refactor.md), Phase 0 and FR-011.

## What to build

在修改生产 UI 前冻结可复现基线：验证当前构建和测试，记录 JS/CSS 产物体积，枚举裸控件与私有交互实现，并在默认及最小窗口尺寸采集现有视觉状态。该票只生成基线证据，不实施 UI Kit。

## Blocked by

- 01 — 批准 UI Kit 实施合同

## Acceptance criteria

- [x] 记录 `git status --short`、`git rev-parse --short HEAD`，明确保护用户已有未提交改动。
- [x] `npm run typecheck`、`npm test`、`npm run build` 在改造前通过；失败时保留完整基线证据并停止下游实施。
- [x] 记录生产 JS/CSS 每个文件的原始与 gzip 字节，包含使用环境和计算方式，不新增 npm 包。
- [x] 创建 `docs/ui-kit/baselines/README.md`，记录 Windows/WebView2 环境、显示缩放、窗口尺寸、状态准备方式和截图命名。
- [x] 在 `docs/ui-kit/baselines/` 采集 SDD FR-011 指定状态，至少包括 idle 默认/最小尺寸、翻译中/流式输出、成功/缓存、错误、Official 设置、Qwen 设置、缓存详情和破坏性确认。（项目负责人已人工验证全部状态；已留存的截图作为代表性证据。）
- [x] 截图只使用虚构、非敏感数据，不包含真实 API Key、Cookie、ticket 或私人原文/译文。（项目负责人已人工验证。）
- [x] 使用 `rg` 记录裸 button/input/select/textarea、`window.confirm`、私有 dialog/alertdialog/fixed overlay 和旧全局 recipe 的当前清单。
- [x] 人工确认基线足以识别后续视觉漂移；证据路径追加到 Comments。

## Out of scope

- 修改 `src/`、`tailwind.config.js` 或现有测试。
- 把现有缺陷顺带修入基线。

## Comments

- 2026-08-11：已写入 [基线说明](../../ui-kit/baselines/README.md) 与 [构建及静态清单](../../ui-kit/baselines/build-and-inventory.md)。`npm run typecheck`、`npm test`（7 files / 46 tests）和 `npm run build` 均通过。已补入 idle 默认与 Qwen 设置的 `520×390` 截图（125% 缩放下为 649×488 像素）。截图项仍未完成：还需补采 FR-011 的其余状态和最小尺寸，再由人工确认基线。
- 2026-08-11：项目负责人明确确认“全部设置已完成”，并要求以人工验证完结 02 工单。FR-011 的全部状态、非敏感数据约束和视觉基线充分性均已由项目负责人验收；不再要求补交其余截图文件。
