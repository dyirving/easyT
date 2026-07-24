# easyT

轻量级 Windows 桌面划词翻译应用。在浏览器、PDF 阅读器、Word 或其他软件中选中英文文本，按下全局快捷键，自动获取选中文本并调用大模型翻译，结果在鼠标附近以小窗口展示。

基于 Tauri 2 + React + TypeScript 构建，调用 OpenAI-compatible Chat Completions API。

## 功能特性

- **全局划词翻译**：任意应用选中文本后按快捷键触发翻译
- **鼠标附近弹窗**：翻译窗口出现在鼠标光标附近，自动适配多显示器工作区
- **系统托盘驻留**：关闭窗口后应用驻留托盘，不退出
- **窗口固定**：固定后窗口不会因失焦隐藏，可连续翻译多段内容
- **可配置**：支持自定义 API 地址、Key、模型、快捷键、目标语言、超时、字符上限
- **快捷键动态修改**：设置页修改后立即生效，无需重启
- **安全**：API Key 仅出现在请求头，绝不写入日志
- **错误友好提示**：区分可重试与不可重试错误，提供修复建议

## 环境要求

### 运行环境

- Windows 10 或 Windows 11（x64）
- WebView2 Runtime（Win11 已预装，Win10 可能需手动安装）

### 开发环境

- [Node.js](https://nodejs.org/) 18+ 与 npm
- [Rust](https://www.rust-lang.org/tools/install) stable 工具链
- 系统环境变量配置（示例）：
  ```
  RUSTUP_HOME = D:\Rust\rustup
  CARGO_HOME = D:\Rust\cargo
  Path 中包含 D:\Rust\cargo\bin
  ```
- Windows 构建工具（MSVC）：安装 Visual Studio Build Tools 或 Visual Studio，勾选 "C++ 桌面开发"

## 安装

### 方式一：使用安装包

1. 从 [Releases](../../releases) 下载最新安装包：
   - `easyT_0.1.0_x64-setup.exe`（NSIS 安装程序，**推荐**）
   - 或 `easyT_0.1.0_x64_en-US.msi`（MSI 安装包）
2. 双击运行安装
3. 安装完成后从开始菜单启动 easyT

### 方式二：免安装版

直接运行 `src-tauri/target/release/easyt.exe` 即可（需先执行打包命令构建）。

## 开发启动

```bash
# 1. 安装依赖
npm install

# 2. 启动开发模式（同时启动 Vite 与 Tauri）
npm run tauri dev
```

开发模式下：
- 前端热更新通过 Vite 提供（http://localhost:1420）
- Rust 端代码改动会自动重新编译
- 应用窗口启动后可在设置页配置 API Key

## 配置说明

### 首次使用

1. 启动应用后右键托盘图标 → "打开设置"
2. 填写 API 配置（见下表）
3. 点击"保存"，再点"测试连接"验证

### 配置项

| 配置项 | 说明 | 默认值 |
|--------|------|--------|
| API Base URL | 兼容 OpenAI Chat Completions 的接口地址 | `https://api.openai.com/v1` |
| API Key | 访问密钥，不会在日志或界面明文展示 | （空） |
| 模型名称 | 如 `gpt-4o-mini`、`gpt-4o` 等 | （空） |
| 全局快捷键 | 触发翻译的组合键，可能与其他软件冲突 | `Ctrl+T` |
| 目标语言 | 简体中文 / 繁體中文 / English / 日本語 | 简体中文 |
| 请求超时 | 5～300 秒 | 60 |
| 最大翻译字符数 | 100～20000 | 5000 |
| 自动隐藏窗口 | 失焦后隐藏临时窗口 | 开启 |
| 默认常驻窗口 | 首次触发时即固定窗口 | 关闭 |

### 配置文件位置

配置以 JSON 形式持久化在：
```
%APPDATA%\com.easyt.app\config.json
```

采用原子写入（先写 `.tmp` 再 rename），写入中断时回退默认值，不阻塞启动。

### ⚠️ API Key 安全提示

原型阶段 API Key 暂存于本地 JSON 文件（`config.json`），**存在安全风险**。请勿在公共机器或共享环境中配置真实 Key。正式版将切换到系统凭据存储（Windows Credential Manager）。

## 快捷键

| 快捷键 | 作用 |
|--------|------|
| `Ctrl+T`（默认） | 翻译当前选中文本 |
| 左键托盘图标 | 显示翻译窗口 |
| 右键托盘图标 | 打开菜单（显示 / 设置 / 退出） |

修改快捷键：设置页 → "全局快捷键"输入框 → 按下新组合键 → 保存。修改后立即生效，无需重启。

快捷键格式示例：`Ctrl+T`、`Alt+Shift+D`、`Ctrl+Shift+T`。

## 使用流程

1. 在浏览器 / PDF / Word 等任意应用中选中英文文本
2. 按 `Ctrl+T`（或自定义快捷键）
3. 翻译窗口在鼠标附近弹出，显示原文与译文
4. 点击"复制"按钮复制译文到剪贴板
5. 点击"固定"图标可固定窗口（失焦不隐藏，便于连续翻译）
6. 失焦后窗口自动隐藏（除非已固定）

## 打包构建

```bash
# 1. 确认依赖已安装
npm install

# 2. 执行生产构建（自动执行 npm run build + cargo build --release）
npm run tauri build
```

构建产物位于：
```
src-tauri/target/release/
├── easyt.exe                                    # 可执行文件
└── bundle/
    ├── msi/
    │   └── easyT_0.1.0_x64_en-US.msi            # MSI 安装包
    └── nsis/
        └── easyT_0.1.0_x64-setup.exe           # NSIS 安装程序
```

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面框架 | Tauri 2.11 |
| 前端 | React 18 + TypeScript 5 + Vite 5 |
| 状态管理 | Zustand 5 |
| 样式 | Tailwind CSS 3（浅色暖灰主题） |
| 全局快捷键 | tauri-plugin-global-shortcut 2 |
| 剪贴板 | tauri-plugin-clipboard-manager 2 |
| 按键模拟 | enigo 0.2（模拟 Ctrl+C） |
| Windows API | windows-sys 0.59（鼠标位置 + 显示器工作区） |
| HTTP 客户端 | reqwest 0.12 + rustls-tls |
| 后端 | Rust（async with tokio） |

## 常见问题

### 快捷键无响应

- 检查快捷键是否被其他应用占用（如输入法、截图工具）
- 在设置页更换为其他组合键后保存

### 翻译失败"未检测到选中文本"

- 确保在按快捷键前已在其他应用中选中了文本
- 部分应用（如某些 PDF 阅读器）可能不支持 Ctrl+C 复制，尝试换用浏览器或记事本验证

### 翻译失败"API Key 无效（401）"

- 检查设置页的 API Key 与 Base URL 是否正确
- 点击"测试连接"验证配置

### 请求超时

- 在设置页增大"请求超时"时间
- 检查网络是否能访问 API Base URL

### 窗口位置异常

- 多显示器场景下窗口会优先贴当前屏工作区
- 若窗口被固定（pinned），按快捷键不会重新定位，可先取消固定

## 项目结构

```
easyT/
├── src/                          # 前端源码
│   ├── components/               # UI 组件
│   ├── pages/                   # 页面（TranslationPage / SettingsPage）
│   ├── stores/                  # Zustand 状态管理
│   ├── services/                # Tauri Command 调用封装
│   ├── types/                   # 类型定义
│   └── App.tsx                  # 应用入口与路由
├── src-tauri/                    # Rust 后端
│   ├── src/
│   │   ├── commands/             # Tauri Command 实现
│   │   ├── config/               # 配置持久化
│   │   ├── llm/                  # 大模型客户端
│   │   ├── platform/             # 平台抽象（Windows）
│   │   ├── app_error.rs          # 统一错误类型
│   │   ├── lib.rs                # 应用入口
│   │   └── shortcut.rs           # 快捷键管理
│   ├── capabilities/             # Tauri 权限配置
│   ├── icons/                    # 应用图标
│   ├── Cargo.toml                # Rust 依赖
│   └── tauri.conf.json           # Tauri 配置
├── package.json
└── 初始prompt.md                 # 项目需求文档
```

## 开发约束

本项目严格遵循以下约束：
- 不引入后端服务器、数据库、用户登录、支付
- 不实现 OCR、截图翻译、浏览器扩展、翻译历史、自动更新
- 所有依赖均有明确用途说明

## License

原型项目，仅供学习参考。
