# UI Kit 改造前基线

采集日期：2026-08-11（Asia/Shanghai）
基线提交：`594528f`（`docs: 更新 v2.1.0 缓存说明`）

本目录只保存 UI Kit 重构开始前的可复现实证；不包含任何生产代码、样式或测试改动。

## 工作区保护

采集前的 `git status --short`：

```text
 M AGENTS.md
?? docs/SDD-ui-kit-refactor.md
?? docs/UI-Kit需求与架构共识文档.md
?? docs/ui-kit-refactor/
```

以上 4 项为已有未提交改动，采集过程中没有覆盖、暂存或修改它们。本票新增的文件仅位于 `docs/ui-kit/baselines/`，以及本票的 issue 记录。

## 验证结果

执行环境：Windows、PowerShell、Node/npm（项目锁定的依赖），未新增 npm 包。

| 命令 | 结果 |
| --- | --- |
| `npm run typecheck` | 通过 |
| `npm test` | 通过：7 个测试文件、46 个测试 |
| `npm run build` | 通过 |

构建产物明细、gzip 算法和静态搜索结果见 [build-and-inventory.md](build-and-inventory.md)。

## 截图契约

截图应在同一台 Windows/WebView2 环境、同一显示缩放、同一主题和同一套虚构测试数据下采集；本次桌面截图为 649×488 像素，对应 125% 显示缩放下的逻辑窗口 `520×390`。不得出现 API Key、Cookie、真实 ticket、私人原文或译文。命名采用 `<序号>-<状态>-<窗口尺寸>.png`，例如 `01-idle-default-520x390.png`。

待采集的 FR-011 状态：

1. idle（默认 `520×390` 与最小 `360×200`）
2. 翻译中与流式输出
3. 翻译成功与缓存命中提示
4. 普通错误与刷新失败
5. Official 设置
6. Qwen 设置
7. 缓存详情
8. 破坏性清除确认

本机浏览器只能加载 Vite 页面，缺少 Tauri 桥接时会在 `App.tsx:76` 的 `getCurrentWindow()` 失败，不能用于视觉基线。桌面 Tauri 会话已补入以下两张可信截图，均使用虚构英文原文及非敏感配置状态：

- [01-idle-default-520x390.png](01-idle-default-520x390.png)
- [06-settings-qwen-520x390.png](06-settings-qwen-520x390.png)

其余 FR-011 状态与最小尺寸未在本目录逐张留存。2026-08-11，项目负责人已人工验证全部状态、数据脱敏和视觉基线充分性，并明确要求完结 02 工单；因此以现有代表性截图和人工验收记录作为该票的最终基线证据。
