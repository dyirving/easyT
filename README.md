# easyT

easyT 是一款轻量级 Windows 桌面划词翻译工具。用户可在浏览器、PDF 阅读器、Word 等应用中选中文本，通过全局快捷键获取选区并调用大模型翻译，结果会显示在鼠标附近的置顶窗口中。

当前版本为 **2.0.0**，基于 Tauri 2、React 18、TypeScript 和 Rust 构建，提供两种翻译后端：

- **Official API**：调用 OpenAI-compatible Chat Completions API，支持多个内置供应商及自定义接口。
- **Qwen 网页实验模式**：登录 Qwen 网页账号后，使用网页登录态调用 Qwen 私有接口，无需填写 API Key。

> Qwen 网页模式依赖非公开协议，可能随 Qwen 网站更新而失效，且不会自动回退到 Official API。该功能仅适合在可信设备上实验使用。

## 功能特性

- 任意应用选中文本后，通过可配置的全局快捷键触发翻译；无选区时按快捷键会显示翻译窗口并保留当前状态
- 自动在鼠标附近显示窗口，并适配多显示器工作区
- 支持手动输入文本进行翻译
- 支持 Official API 与 Qwen 网页实验模式切换
- 内置 Agnes、DeepSeek、Qwen、GLM、Kimi、DouBao 供应商配置
- 支持自定义 OpenAI-compatible Base URL 和模型名称
- 每个 Official API 供应商独立保存 API Key
- 支持关闭或启用模型思考模式
- 支持可选的流式输出：生成期间逐步展示译文
- 支持简体中文、繁體中文、English、日本語四种目标语言
- 译文支持 Markdown、行内公式和块级公式渲染
- 支持复制译文、重试翻译、窗口固定和失焦自动隐藏
- 关闭主窗口后驻留系统托盘，窗口尺寸会自动保存
- 修改全局快捷键后立即生效，无需重启
- 区分鉴权、限流、超时、登录过期、协议变化等错误并提供提示

## 环境要求

### 运行环境

- Windows 10 或 Windows 11（x64）
- Microsoft Edge WebView2 Runtime（Windows 11 通常已预装）
- 可访问所选模型供应商的网络环境

### 开发环境

- [Node.js](https://nodejs.org/) 18+ 与 npm
- [Rust](https://www.rust-lang.org/tools/install) 1.77.2+ stable 工具链
- Visual Studio 2022 或 Visual Studio Build Tools，并安装“使用 C++ 的桌面开发”工作负载
- WebView2 开发环境，详见 [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

## 安装

从项目 [Releases](../../releases) 下载 2.0.0 或更新版本：

- `easyT_{version}_x64-setup.exe`：NSIS 安装程序（**推荐**）
- `easyT_{version}_x64_en-US.msi`：MSI 安装包

也可以在本地完成生产构建后，直接运行 `src-tauri/target/release/easyt.exe`。

## 快速开始

1. 启动 easyT。
2. 从翻译窗口打开设置，或右键托盘图标选择“打开设置”。
3. 选择翻译后端并完成对应配置。
4. 点击“保存”，再点击“测试连接”。
5. 在其他应用中选中文本，按 `Ctrl+T` 开始翻译；未选中任何文本时按快捷键会显示翻译窗口。

主窗口关闭后只会隐藏到托盘。需要完全退出时，请右键托盘图标并选择“退出 easyT”。

## 翻译后端

### Official API

Official API 是默认模式。内置供应商会自动填写 Base URL，并提供预设模型列表；每个供应商的 API Key 分开保存，切换供应商时会恢复对应 Key。

| 供应商 | 默认或可选模型 | Base URL |
|---|---|---|
| Agnes（默认） | `agnes-2.0-flash` | `https://apihub.agnes-ai.com/v1` |
| DeepSeek | `deepseek-v4-flash`、`deepseek-v4-pro` | `https://api.deepseek.com` |
| Qwen | `qwen3.7-max`、`qwen3.7-plus`、`qwen3.6-flash` | `https://dashscope.aliyuncs.com/compatible-mode/v1` |
| GLM | `glm-5`、`glm-5.1`、`glm-5.2` | `https://open.bigmodel.cn/api/paas/v4` |
| Kimi | Kimi K2、K2.5、Moonshot 等预设模型 | `https://api.moonshot.cn/v1` |
| DouBao | DouBao Seed 1.6 系列 | `https://ark.cn-beijing.volces.com/api/v3` |
| 自定义供应商 | 用户填写模型 ID | 用户填写 |

自定义接口必须兼容 OpenAI Chat Completions，请求地址按以下规则生成：

```text
{API Base URL}/chat/completions
```

关闭“启用思考模式”时，easyT 会针对已知供应商注入对应的关闭思考参数，以降低延迟和 Token 消耗；自定义供应商不会注入供应商专用参数。

开启“流式输出”后，Official API 必须返回标准 Chat Completions SSE，Qwen 网页实验模式会使用其已有的私有 SSE。流式输出期间只展示正文纯文本，收到完整结束信号后才切换为 Markdown 并启用复制。若 Official API 端点不支持标准流式响应，easyT 会提示关闭该开关，不会自动改发一次性请求。

### Qwen 网页实验模式

该模式通过独立 WebView2 窗口登录 Qwen，并使用登录态访问 Qwen 私有接口。

1. 在设置中将“翻译后端”切换为“Qwen 网页实验模式”。
2. 选择允许的 Qwen 模型。
3. 点击“登录 Qwen”，在新窗口中完成登录。
4. 登录状态显示“已登录”后保存设置并测试连接。

当前允许的模型：

- `Qwen`
- `Qwen3.8-Max-Preview`
- `Qwen3.7-Max`（默认）
- `Qwen3.6-Flash`

“保存到 Qwen 对话记录”默认关闭。开启后，翻译原文和结果可能出现在 Qwen 网页端历史记录中。

此模式有以下限制：

- 依赖 Qwen 非公开接口，协议变化可能导致翻译失败。
- 仅支持 Qwen，不接受任意 WebGateway 地址或请求头。
- 登录态失效后需要重新登录。
- 不会在失败时自动调用付费 Official API。
- 流式输出期间只展示正文，不展示模型思考内容；中途失败时会保留并标记未完成译文，但不能复制。

## 配置项

| 配置项 | 说明 | 默认值 |
|---|---|---|
| 翻译后端 | Official API / Qwen 网页实验模式 | Official API |
| 模型供应商 | Official API 的供应商预设 | Agnes |
| API Key | 当前 Official API 供应商的访问密钥 | 空 |
| 模型 | 所选供应商或 Qwen 网页模式使用的模型 | 随后端变化 |
| 启用思考模式 | 保留供应商默认思考行为 | 关闭 |
| 流式输出 | 生成期间逐步展示正文；Official API 需支持标准 SSE | 关闭 |
| 全局快捷键 | 有选区时捕获并翻译，无选区时显示翻译窗口 | `Ctrl+T` |
| 目标语言 | 简体中文 / 繁體中文 / English / 日本語 | 简体中文 |
| 请求超时 | 5 至 300 秒 | 60 秒 |
| 最大翻译字符数 | 100 至 20000 | 5000 |
| 自动隐藏窗口 | 非固定窗口失焦后自动隐藏 | 开启 |
| 默认常驻窗口 | 启动后默认固定翻译窗口 | 关闭 |
| 保存到 Qwen 对话记录 | 仅影响 Qwen 网页实验模式 | 关闭 |

快捷键格式示例：`Ctrl+T`、`Alt+Shift+D`、`Ctrl+Shift+T`。

## 本地数据与安全

配置和 WebView2 数据位于可执行文件同级的 `easyT_Data` 目录：

```text
easyT_Data/
├── config.json
├── window-state.json
├── webview/
├── web_gateway/
│   └── qwen/
│       ├── credentials.bin
│       └── profile/
└── logs/                 # 仅开发构建
```

安全注意事项：

- Official API Key 保存在本地 `config.json` 中，不会写入应用日志。
- Qwen 登录凭证以明文保存在 `web_gateway/qwen/credentials.bin` 中，不会写入 `config.json` 或日志。
- Qwen 请求期间使用的凭证内存副本会在释放时清理，但本地凭证文件本身未加密。
- 点击“退出登录”会删除 Qwen 凭证和专用浏览器 profile。
- 请勿在公共、共享或不可信设备上配置真实 API Key 或登录 Qwen。

配置和凭证采用临时文件加原子替换方式写入。由于数据目录与可执行文件同级，安装到 `Program Files` 等受保护目录时可能没有写入权限；如遇配置保存失败，请使用具备写权限的安装目录或免安装版本。

## 开发

安装依赖并启动 Tauri 开发模式：

```bash
npm install
npm run tauri dev
```

开发模式会启动 Vite 开发服务器 `http://localhost:1420`，并自动编译、运行 Rust 桌面端。

常用检查命令：

```bash
# TypeScript 类型检查
npm run typecheck

# 前端生产构建
npm run build

# Rust 测试
cargo test --manifest-path src-tauri/Cargo.toml
```

## 打包

```bash
npm install
npm run tauri build
```

Tauri 会先执行 `npm run build`，再编译 Rust Release 版本并生成安装包：

```text
src-tauri/target/release/
├── easyt.exe
└── bundle/
    ├── msi/
    │   └── easyT_2.0.0_x64_en-US.msi
    └── nsis/
        └── easyT_2.0.0_x64-setup.exe
```

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2.11 |
| 前端 | React 18、TypeScript 5.6、Vite 5 |
| 状态管理 | Zustand 5 |
| 样式 | Tailwind CSS 3 |
| 富文本渲染 | react-markdown、remark-math、KaTeX |
| 全局快捷键 | tauri-plugin-global-shortcut 2 |
| 剪贴板 | tauri-plugin-clipboard-manager 2 |
| 按键模拟 | enigo 0.2 |
| HTTP 与流式响应 | reqwest 0.12、futures-util、SSE |
| Windows API | windows-sys 0.59 |
| 后端 | Rust、Tokio |

## 项目结构

```text
easyT/
├── src/
│   ├── components/                 # 翻译界面、状态组件和基础 UI
│   ├── pages/                      # 翻译页与设置页
│   ├── services/                   # Tauri Command 封装与快捷键翻译协调
│   ├── stores/                     # Zustand 状态
│   ├── types/                      # 配置、后端和错误类型
│   ├── App.tsx                     # 页面切换、托盘事件和窗口行为
│   └── main.tsx                    # React 入口
├── src-tauri/
│   ├── capabilities/               # Tauri 权限配置
│   ├── icons/                      # 桌面应用图标
│   ├── src/
│   │   ├── commands/               # 前端可调用的 Tauri Commands
│   │   ├── config/                 # 配置模型、校验与持久化
│   │   ├── platform/               # Windows 选区、剪贴板和窗口定位
│   │   ├── translation_backend/    # Official API 与 WebGateway 后端
│   │   ├── lib.rs                  # 应用、窗口、托盘及插件初始化
│   │   ├── shortcut.rs             # 全局快捷键管理
│   │   └── window_state.rs         # 窗口尺寸持久化
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
└── README.md
```

## 常见问题

### 全局快捷键无响应

- 检查快捷键是否被输入法、截图工具或其他应用占用。
- 在设置页录入新的组合键并保存，修改会立即生效。
- 某些以管理员身份运行的应用可能无法被普通权限进程捕获选区，可尝试以相同权限运行 easyT。

### 快捷键未捕获到选区

- 按快捷键时没有任何选中文本属于正常行为：easyT 会显示翻译窗口并保留当前译文，不会清除已有内容。
- 需要重新翻译时，请返回其他应用选中文本后再按快捷键。
- 部分 PDF 阅读器或特殊控件不支持通过 `Ctrl+C` 获取选区，可先在浏览器或记事本中验证。
- easyT 会暂时读取复制结果并恢复原剪贴板；如果目标应用复制较慢，可再次尝试。

### Official API 连接失败

- 检查供应商、模型和 API Key 是否匹配。
- 自定义供应商需确认 Base URL 兼容 OpenAI Chat Completions。
- 检查网络、账户额度和供应商限流状态。
- 在设置中点击“测试连接”获取更具体的错误提示。

### Qwen 网页模式要求重新登录

- 登录态可能已过期，点击“重新登录”完成认证。
- 如果 Qwen 网页或私有协议已经变化，请切换回 Official API。
- 退出登录会清除本地凭证和 Qwen 专用浏览器数据。

### 配置无法保存

- 确认 `easyt.exe` 所在目录对当前用户可写。
- 避免将免安装版放在 `Program Files` 等受保护目录。

## 项目范围

当前项目不包含后端服务器、数据库、用户系统、OCR、截图翻译、浏览器扩展、翻译历史和自动更新。

## License

原型项目，仅供学习与技术验证。
