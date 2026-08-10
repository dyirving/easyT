# 构建与静态实现清单

采集提交：`594528f`。命令均在仓库根目录执行。

## 生产资产

`npm run build` 成功后，对 `dist/assets` 中每个 `.js` / `.css` 文件读取原始字节；gzip 使用 .NET `System.IO.Compression.GzipStream`，压缩级别 `Optimal`，因此不需要新增包。

| 文件 | 原始字节 | gzip 字节 |
| --- | ---: | ---: |
| `dist/assets/index-Aoqzskkf.js` | 231,163 | 73,756 |
| `dist/assets/index-C6Kt63ho.css` | 48,339 | 12,300 |
| `dist/assets/MarkdownTranslation-BYSSuxI3.js` | 395,128 | 120,477 |

## 裸控件与私有交互

以下为当前实现的迁移清单，不代表本票修改了任何实现。

| 搜索目标 | 基线结果 |
| --- | --- |
| `window.confirm` / `role="dialog"` / `role="alertdialog"` / `fixed inset-0` | `SettingsPage.tsx:201` 使用 `window.confirm`；`CacheDetailsDialog.tsx:102,104,145,147` 使用私有遮罩与 dialog / alertdialog。 |
| `<button|input|select|textarea>` | `TranslationPage.tsx:153`；`SettingsPage.tsx:263,300,347,545,577,606,679`；`ShortcutInput.tsx:82`；以及现有 `components/ui` 的 `Button`、`Input`、`Switch` 实现。 |
| UI 层禁止依赖 | `src/components/ui/` 中未找到 `@/stores`、`@/services`、`@tauri-apps` 或 `AppConfig`。`src/components/patterns/` 在此基线不存在。 |
| 旧全局 recipe | `src/index.css:41-108` 定义 `.btn*`、`.input`、`.panel`、`.translation-markdown*`。 |

当前 UI 导入均来自 `@/components/ui/`；`@/components/patterns/`、`@/components/translation/` 与 `@/components/settings/` 尚不存在。这是 Phase 0 的迁移起点。

执行的搜索命令：

```powershell
rg -n 'window\.confirm|role="dialog"|role="alertdialog"|fixed inset-0' src --glob '*.tsx'
rg -n '<(button|input|select|textarea)\b' src --glob '*.tsx'
rg -n '@/stores|@/services|@tauri-apps|AppConfig' src/components/ui --glob '*.ts' --glob '*.tsx'
rg -n '\.btn|\.input|\.panel|\.translation-markdown' src/index.css src/pages src/components --glob '*.css' --glob '*.tsx'
```
