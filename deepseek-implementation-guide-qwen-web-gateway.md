# easyT Qwen A+B WebGateway Implementation Guide

## Goal

在 easyT 中增加一种实验性的 Qwen WebGateway 翻译后端，采用“A+B”组合：Tauri WebView2 仅用于用户交互式登录和获取 `tongyi_sso_ticket`，实际翻译由 Rust 使用 HTTP Cookie 重放调用 Qwen 网页私有接口，并解析 SSE 响应。

必须保留现有 Official API 路径、Ctrl+T latest-wins、剪贴板捕获、窗口展示和配置保存行为。`translate()` 不得自动创建登录窗口或等待用户登录；网页登录由设置页通过独立命令显式管理。

本功能第一版只支持 Qwen。不要为了假设中的未来供应商预先引入动态注册表、插件系统或复杂 trait；等第二个 Web 供应商真正实现时，再根据实际差异提取公共 interface。

## Current Context

### 已确认的现有实现

- Rust 后端位于 `src-tauri/src/`。
- 当前翻译入口是 `src-tauri/src/commands/translate.rs::translate_text`。
- `TranslationRequestManager::run_latest` 已负责：
  - 为请求分配 generation。
  - 新请求到来时 abort 旧翻译 Future。
  - 阻止旧请求继续占用翻译任务。
- 前端 `src/services/translationCoordinator.ts` 已负责：
  - 捕获串行。
  - HTTP 翻译在捕获队列外执行。
  - request-aware 状态更新。
- Official API 当前实现位于：
  - `src-tauri/src/llm/client.rs`
  - `src-tauri/src/llm/error.rs`
  - `src-tauri/src/llm/models.rs`
  - `src-tauri/src/llm/prompt.rs`
- HTTP Client 已通过 `OnceLock<reqwest::Client>` 复用。
- 应用持久化根目录由 `src-tauri/src/config/storage.rs::app_data_dir()` 提供，指向可执行文件同级的 `easyT_Data`。
- 主窗口由 `src-tauri/src/lib.rs` 中的 `WebviewWindowBuilder` 创建，WebView2 数据目录是 `easyT_Data/webview`。
- 当前 `on_window_event` 会对所有窗口的 `CloseRequested` 执行 `prevent_close + hide`。新增 Qwen 登录窗口后必须修改为只隐藏 `main`，否则登录窗口无法真正关闭。
- `src-tauri/capabilities/default.json` 当前只授权 `main` 窗口。应继续保持这一点，不能把本地 Tauri Command 权限授予远程 Qwen 页面。
- `reqwest` 当前启用 `json`、`rustls-tls`，尚未启用流式响应所需的 `stream`。
- `tokio` 当前仅启用 `rt`。如果实现重试延迟或后台登录轮询，需要按实际使用补充最小的 `time` feature。
- Windows 依赖继续按最小 feature 使用；Qwen 凭证按产品决策明文持久化，不接入 DPAPI。

### ProxyAgent 参考实现

可将 `D:/PythonCode/ProxyAgent/app/model_gateway/providers/qwen_web.py` 作为行为参考，重点参考：

- Qwen 私有 `/api/v2/chat` 请求形状。
- `session_id`、`req_id` 和请求参数生成。
- Qwen 累积式 SSE 内容转增量内容的逻辑。
- reasoning 与普通 answer 的提取规则。
- 上游错误归一化。

不要把 ProxyAgent 的整体 FastAPI、Provider Registry、账号路由、负载均衡、Python 浏览器框架或 JSON Credential Store 搬入 easyT。easyT 是单用户桌面应用，不需要本地 HTTP Server。

ProxyAgent 当前是 GPLv3 项目，并注明部分行为由 Chat2API 移植。除非已经确认相关代码版权和兼容许可，不要逐行复制；应根据协议行为重新实现，并使用脱敏测试样本验证。

### 明确假设

- 第一版目标上游沿用 ProxyAgent 已验证过的中国站 Qwen 网页链路：
  - 登录入口以 `https://www.qianwen.com/` 为起点。
  - 翻译上游以 `https://chat2.qianwen.com/api/v2/chat` 为起点。
  - 凭证名为 `tongyi_sso_ticket`。
- 登录过程中可能跳转到阿里账号域名。实现前必须用开发环境实际记录合法跳转域名，并将其写入显式 allowlist；不得使用 `*.aliyun.com`、`*.taobao.com` 或“允许所有 HTTPS”这类宽泛规则。
- `Ready` 表示本地存在格式有效的凭证，不表示凭证已实时验证。首次真实请求收到 401/403 时转为 `Expired`。
- WebGateway 是实验功能。不得静默回退到可能产生费用的 Official API。

## Implementation Plan

### 1. 建立 TranslationBackend 深模块

#### 文件

- 新建 `src-tauri/src/translation_backend/mod.rs`
- 新建 `src-tauri/src/translation_backend/models.rs`
- 新建 `src-tauri/src/translation_backend/error.rs`
- 可新增 `src-tauri/src/translation_backend/prompt.rs`

#### 名称与职责

`TranslationBackend` 是翻译能力唯一的外部 seam。它负责：

- 根据 `AppConfig.backend_mode` 路由到 `OfficialApiAdapter` 或 `WebGateway`。
- 在进入 Adapter 前执行共同输入校验。
- 返回统一 `BackendResult`。
- 将 Adapter 的错误统一为 `BackendError`。

它不负责：

- latest-wins generation；继续由 `TranslationRequestManager` 唯一负责。
- 创建登录窗口。
- Cookie 提取或加密。
- Qwen Header、请求体、SSE 字段。
- 前端状态更新。

#### 建议数据结构

```rust
pub enum BackendMode {
    OfficialApi,
    WebGateway,
}

pub enum WebProviderKind {
    Qwen,
}

pub struct BackendRequest {
    pub text: String,
    pub target_language: String,
}

pub struct BackendResult {
    pub translated_text: String,
    pub source: BackendSource,
}

pub struct BackendSource {
    pub backend: BackendMode,
    pub provider: String,
    pub model: String,
}
```

这些类型需要遵循项目现有 `serde(rename_all = "camelCase")` 约定。`BackendResult.source` 是只读元数据，不允许前端根据它分叉核心成功流程。

#### 建议接口

```rust
impl TranslationBackend {
    pub fn new(/* 注入 OfficialApiAdapter、WebGateway */) -> Self;

    pub async fn translate(
        &self,
        config: &AppConfig,
        request: BackendRequest,
    ) -> Result<BackendResult, BackendError>;

    pub async fn test_connection(
        &self,
        config: &AppConfig,
    ) -> Result<BackendHealth, BackendError>;
}
```

`test_connection()` 必须通过当前选中的 Adapter 进行真实轻量请求。WebGateway 模式不得仅检查本地 ticket 是否存在后就返回成功。

### 2. 定义 BackendError 并与 AppError 映射

#### 文件

- `src-tauri/src/translation_backend/error.rs`
- 修改 `src-tauri/src/app_error.rs`
- 修改 `src/types/index.ts`

#### BackendError

至少包含：

```rust
pub enum BackendError {
    LoginRequired,
    SessionExpired,
    Unauthorized,
    RateLimited,
    Timeout,
    Cancelled,
    Network(String),
    ProtocolMismatch(String),
    PartialResponse(String),
    InvalidResponse(String),
    ConfigInvalid(String),
}
```

要求：

- 内部错误可以携带可诊断上下文，但不能含 Cookie、ticket、完整 Header、完整上游响应或用户正文。
- `ProtocolMismatch` 表示私有协议结构已变化，不应归类为普通网络错误。
- `PartialResponse` 不能作为成功返回。
- `Cancelled` 不应把 QwenSession 标记为 `Expired` 或错误。

在 `app_error.rs` 增加前端可识别的通用 Backend 错误类型。为了兼容已有前端，可保留现有 `ApiUnauthorized`、`ApiRateLimited` 等枚举，但新的 TranslationBackend 应统一通过明确的 `From<BackendError> for AppError` 映射，不允许各 Adapter 自行拼前端错误字符串。

前端 `ERROR_KIND` 增加：

- `LoginRequired`
- `SessionExpired`
- `BackendCancelled`
- `BackendNetwork`
- `BackendProtocolMismatch`
- `BackendPartialResponse`
- `BackendInvalidResponse`

为这些错误补充用户可操作提示，例如：

- LoginRequired：请先在设置中登录 Qwen。
- SessionExpired：Qwen 登录状态已过期，请重新登录。
- ProtocolMismatch：Qwen 网页协议已变化，请切换 Official API 或更新 easyT。

### 3. 将现有 Official API 实现迁入 Adapter

#### 文件

- 新建 `src-tauri/src/translation_backend/official_api/mod.rs`
- 新建 `src-tauri/src/translation_backend/official_api/adapter.rs`
- 修改或逐步退役 `src-tauri/src/llm/client.rs`
- 评估迁移 `src-tauri/src/llm/error.rs`
- 保留或迁移 `src-tauri/src/llm/prompt.rs`

#### 要求

`OfficialApiAdapter` 封装当前 `llm::client::translate` 行为：

- 复用一个 HTTP Client。
- 保留超时、401、429、5xx 和响应解析逻辑。
- 保留不同 Official Provider 的 thinking 参数差异。
- 输出 `BackendResult`，source.backend 为 `OfficialApi`。
- 不改变当前 Official API 用户的配置语义和翻译结果。

迁移过程中不要长期保留两套 Official API 请求实现。可以先让 `llm::client::translate` 成为到 `OfficialApiAdapter` 的薄兼容调用，完成所有调用点迁移后删除旧入口。

翻译 Prompt 是 Official API 与 Qwen WebGateway 共用的纯逻辑。推荐移到 `translation_backend/prompt.rs`，由两个 Adapter 共同调用。不要分别维护两份 Prompt。

### 4. 实现 WebGateway Adapter

#### 文件

- 新建 `src-tauri/src/translation_backend/web_gateway/mod.rs`
- 新建 `src-tauri/src/translation_backend/web_gateway/account.rs`
- 新建 `src-tauri/src/translation_backend/web_gateway/credential_store.rs`

#### WebGateway 职责

- 根据 `WebProviderKind` 路由。
- 检查凭证状态。
- 复用 HTTP Client。
- 应用统一请求超时。
- 执行有限重试。
- 对日志进行敏感信息过滤。
- 将 Qwen 错误转换为 BackendError。

第一版内部可以使用显式 `match WebProviderKind::Qwen`。不要创建面向未来的动态 Provider Registry。

#### HTTP Client

- 优先由 `TranslationBackend` 构造并注入，共享给 OfficialApiAdapter 和 WebGateway。
- 不要在每次请求中创建 `reqwest::Client`。
- `Cargo.toml` 为 reqwest 增加最小 `stream` feature。
- 如果使用 `tokio::time::sleep` 做 429/5xx 重试，为 tokio 增加最小 `time` feature。
- 不设置无限超时。
- 使用 `config.timeout_seconds.clamp(5, 300)`。
- 最多执行一次对明确可重试错误的重试；第一版不要实现复杂指数退避。
- 401/403 不重试，立即把 QwenSession 标记为 `Expired`。
- 429 可以等待短暂、带上限的延迟后重试一次。
- abort Future 时必须让 `reqwest::Response` 和流被 drop，从而关闭当前读取。

#### 禁止隐式付费回退

第一版不实现 WebGateway → Official API 自动回退。若以后增加：

- 必须新增显式配置开关。
- 默认关闭。
- UI 必须说明可能产生 API 费用。
- 不能因为协议解析失败就自动付费请求。

### 5. 实现 QwenSession

#### 文件

- 新建 `src-tauri/src/translation_backend/web_gateway/qwen/mod.rs`
- 新建 `src-tauri/src/translation_backend/web_gateway/qwen/session.rs`
- 修改 `src-tauri/src/lib.rs`
- 修改 `src-tauri/capabilities/default.json` 时必须保持远程窗口无权限

#### 状态结构

```rust
pub enum QwenSessionPhase {
    LoggedOut,
    LoggingIn,
    Ready,
    Expired,
}

pub struct QwenSessionStatus {
    pub phase: QwenSessionPhase,
    pub message: Option<String>,
    pub updated_at: Option<u64>,
}
```

`QwenSession` 内部可以使用一个 `std::sync::Mutex` 保护短时内存状态。不得在持有 MutexGuard 时执行：

- 创建 WebView。
- 读取 Cookie。
- 文件 I/O。
- await。
- HTTP 请求。

需要先在锁内完成状态判断和状态切换，然后释放锁，再执行外部操作。

#### 登录窗口

登录窗口：

- label 固定为 `qwen-login`。
- 按需创建，默认只允许一个实例。
- 使用 `WebviewUrl::External` 打开 Qwen 登录地址。
- `data_directory` 设置为：
  `app_data_dir()/web_gateway/qwen/profile`
- 设置合理的最小尺寸，例如 900×700；它不应继承主翻译窗口的 always-on-top 或无边框属性。
- 使用 `on_navigation` 实施精确 host allowlist。
- 对不在 allowlist 的导航返回 false，必要时用系统默认浏览器打开帮助/隐私链接。
- 不使用 initialization script。
- 不向 `qwen-login` 授予 Tauri capability。
- 不允许远程页面调用配置、剪贴板、文件或登录管理 Command。

登录域名 allowlist 必须根据一次真实登录流程确认后写为常量并配测试。不要写通配顶级域名。

#### 非阻塞登录流程

`begin_web_login()`：

1. 在短锁内确认当前不是 `LoggingIn`。
2. 切换为 `LoggingIn`。
3. 创建并显示登录窗口。
4. 启动一个后台 watcher。
5. 立即向前端返回当前状态，不等待用户完成登录。

后台 watcher：

- 总等待上限建议 5 分钟。
- 以 500～1000ms 间隔检查 `cookies_for_url`。
- Tauri 文档提示 Cookie 读取应在异步命令/独立线程中进行；按 Tauri 2.11 的约束使用 `tauri::async_runtime::spawn_blocking` 或等效安全方式，避免在同步 Command 或窗口事件回调中直接阻塞 WebView2。
- 找到 `tongyi_sso_ticket` 后立即复制必要值、关闭 watcher、持久化凭证并切换为 `Ready`。
- 不保存完整 Cookie Jar，除非真实接口验证证明只保存 ticket 无法工作。
- 如果用户关闭登录窗口，恢复到登录前状态；没有旧凭证时为 `LoggedOut`，旧凭证过期时为 `Expired`。
- watcher 结束后必须释放窗口、任务和临时明文。

#### 当前窗口关闭逻辑修正

修改 `src-tauri/src/lib.rs::on_window_event`：

- 只有 `window.label() == "main"` 的 CloseRequested 才 `prevent_close + hide`。
- `qwen-login` 的 CloseRequested 必须允许真正关闭，并通知 QwenSession 结束登录。
- 不能让 Qwen 登录窗口触发主窗口尺寸持久化。
- 托盘退出前关闭登录窗口并取消 watcher，再清理快捷键。

### 6. 明文持久化凭证

#### 文件

- `src-tauri/src/translation_backend/web_gateway/credential_store.rs`
- 修改 `src-tauri/Cargo.toml`

#### 持久化位置

```text
easyT_Data/
└── web_gateway/
    └── qwen/
        ├── profile/
        └── credentials.bin
```

`credentials.bin` 直接保存 UTF-8 明文 ticket。`config.json` 中仍不得出现：

- `tongyi_sso_ticket`
- Cookie
- Authorization Header
- Web 凭证

约束：

- 写入使用临时文件 + flush/sync + 原子替换，已有凭证必须可覆盖。
- 空文件、超限内容或非 UTF-8 内容返回 `CredentialCorrupted`，不自动删除原文件。
- logout 时删除 `credentials.bin`，并清除内存明文。
- 敏感字节缓冲区使用 `zeroize` 或等效显式清理；不要把 ticket clone 到多个 String。
- 日志只记录状态、provider、HTTP 状态码和长度，不能记录 ticket、Cookie、请求正文或完整响应。
- 设置页必须明确提示凭证采用明文存储，只应在可信设备使用。

### 7. 实现 QwenWebAdapter

#### 文件

- 新建 `src-tauri/src/translation_backend/web_gateway/qwen/adapter.rs`

#### 常量与协议

所有 Qwen 私有协议知识必须只存在于 `web_gateway/qwen`：

- 登录 URL。
- Chat Base URL。
- `/api/v2/chat`。
- Origin/Referer。
- 必要 Query 参数。
- 模型映射。
- 请求 JSON DTO。
- 响应 JSON DTO 或受控 `serde_json::Value` 解析。

其他模块不得判断：

- `tongyi_sso_ticket`
- `deep_think`
- `multi_load`
- Qwen 私有 event 名称。

当前网页模型映射：

- `Qwen3.7-千问` → `Qwen`
- `Qwen3.8-Max-Preview` → `Qwen3.8-Max-Preview`
- `Qwen3.7-Max` → `Qwen3.7-Max`（默认）
- `Qwen3.6-Flash` → `Qwen3.6-Flash`

#### 凭证使用

- 每次请求通过 QwenSession 获取一个最短生命周期的凭证副本。
- 优先只发送 `tongyi_sso_ticket=<value>`，不要复制浏览器的全部 Cookie。
- 请求结束或失败后尽快清理 ticket。
- Header 必须逐项构造，禁止允许配置注入任意额外 Header。

#### 请求身份

- 每个请求生成新的 `session_id`。
- 每个请求生成新的 `req_id`。
- Device ID 不能是所有安装共享的硬编码常量。
- Device ID 可以首次使用时随机生成并持久化为非敏感安装标识，或按账号生成；不要从 ticket 派生。
- UUID 实现优先选择体积较小的方案。若新增 `uuid` crate，只启用 `v4` feature；也可以复用操作系统随机源生成符合上游要求的十六进制 ID。

#### Prompt 与对话

- 使用与 Official API 相同的翻译 Prompt。
- 每次翻译创建独立 session，不携带此前网页对话。
- 不把旧请求的回答作为下一次输入。
- 明确设置关闭 thinking 的参数，前提是当前 Qwen 私有协议仍支持。
- `saveHistory=false` 时使用 temporary 模式；只有用户显式开启 `saveHistory` 时才创建网页端可见记录。

#### 错误规则

- 401/403：`SessionExpired`，并更新 QwenSession。
- 429：`RateLimited`。
- 5xx：`Network` 或上游失败，可有限重试一次。
- 超时：`Timeout`。
- JSON/SSE 结构变化：`ProtocolMismatch`。
- 流中断且已有正文：`PartialResponse`，不得返回成功。
- 用户 abort：`Cancelled`，不得改变登录状态。

### 8. 实现 QwenSseDecoder

#### 文件

- 新建 `src-tauri/src/translation_backend/web_gateway/qwen/sse_decoder.rs`

#### 设计要求

这是纯计算模块：

- 不持有 AppHandle。
- 不访问配置。
- 不读取 Cookie。
- 不发 HTTP。
- 不写日志中的原始正文。

建议状态：

```rust
pub enum DecodeOutcome {
    Delta(QwenDelta),
    Completed,
    UpstreamError { code: String, message: String },
}

pub struct QwenDelta {
    pub reasoning_delta: Option<String>,
    pub content_delta: Option<String>,
}
```

Decoder 内部维护：

- 尚未消费完的 byte buffer。
- 上一次累积 reasoning。
- 上一次累积 content。
- 是否收到明确完成事件。
- 是否观察到有效 Qwen 消息结构。

处理要求：

1. 正确处理一个 SSE event 被拆成多个网络 chunk。
2. 正确处理一个 chunk 内包含多个 event。
3. 支持 `\n\n` 和 `\r\n\r\n` 分隔。
4. 忽略 SSE 注释行。
5. 拼接同一 event 的多个 `data:` 行。
6. 将 Qwen 返回的累积文本转换为 delta。
7. 如果新累积内容不是旧内容前缀，不得用长度盲切；返回 `ProtocolMismatch`。
8. reasoning 和 answer 分开累计。
9. 收到上游业务 error 时返回结构化错误。
10. HTTP 流 EOF 但没有 Completed：
    - 已有正文：PartialResponse。
    - 没有正文：InvalidResponse 或 Network。
11. 最终 `BackendResult` 只使用普通 answer，不把 reasoning 拼进译文。

不要伪造 token usage。Qwen 网页上游未提供可信统计时，结果中不返回 usage，或将其定义为 `Option`。

### 9. 增加独立登录管理 Commands

#### 文件

- 新建 `src-tauri/src/commands/web_gateway.rs`
- 修改 `src-tauri/src/commands/mod.rs`
- 修改 `src-tauri/src/lib.rs`

#### Commands

```rust
#[tauri::command]
pub async fn begin_web_login(
    app: AppHandle,
    backend: State<'_, TranslationBackend>,
    provider: WebProviderKind,
) -> AppResult<QwenSessionStatus>;

#[tauri::command]
pub async fn get_web_login_status(
    backend: State<'_, TranslationBackend>,
    provider: WebProviderKind,
) -> AppResult<QwenSessionStatus>;

#[tauri::command]
pub async fn logout_web_account(
    app: AppHandle,
    backend: State<'_, TranslationBackend>,
    provider: WebProviderKind,
) -> AppResult<QwenSessionStatus>;
```

具体方法可由 `TranslationBackend` 转发到 WebGateway，但不要让 `translate()` 隐式触发它们。

安全要求：

- Commands 只能被 `main` 窗口调用。
- `qwen-login` 不在 capability 的 windows 列表中。
- provider 只接受枚举，不接受任意 URL。
- 前端不能传 Cookie、ticket、Base URL 或 Header。
- logout 是显式 destructive 操作：关闭登录窗口、取消 watcher、清除凭证和 Qwen profile。UI 应二次确认或至少明确说明需要重新登录。

### 10. 接入现有 translate_text 与 test_connection

#### 文件

- 修改 `src-tauri/src/commands/translate.rs`
- 修改 `src-tauri/src/lib.rs`

#### 修改方式

- 在 setup 中创建并 `app.manage(TranslationBackend::new(...))`。
- `translate_text` 从 `State<TranslationBackend>` 获取统一入口。
- 保留现有：
  - 空文本检查。
  - 最大长度检查。
  - `TranslationRequestManager::run_latest`。
- 把现有 `llm::translate(&config, request)` 替换为：
  `translation_backend.translate(&config, BackendRequest).await`
- `TranslationRequestManager` 仍然是唯一 latest-wins 所有者。
- WebGateway 内不要再建立 generation 或 active request。
- `test_api_connection` 重命名为后端语义更准确的 `test_connection`，或者保留原 Tauri command 名作为兼容 wrapper；前端应逐步使用新名称。

注意 `run_latest` 当前把 abort 映射为 `ApiRequestFailed("翻译请求已被新请求取代")`。迁移后应改为通用 `BackendCancelled`，但不得改变前端 latest-wins 的最终界面行为。

### 11. 扩展配置模型且保持旧配置兼容

#### 文件

- 修改 `src-tauri/src/config/models.rs`
- 修改 `src-tauri/src/commands/config.rs`
- 修改 `src/types/index.ts`
- 修改 `src/stores/settingsStore.ts`

#### 建议配置

```rust
pub enum BackendMode {
    OfficialApi,
    WebGateway,
}

pub struct WebGatewayConfig {
    pub provider: WebProviderKind,
    pub model: String,
    pub save_history: bool,
}

pub struct AppConfig {
    // 现有字段保持不变
    pub backend_mode: BackendMode,
    pub web_gateway: WebGatewayConfig,
}
```

Serde 要求：

- `backendMode` 缺失时默认 `officialApi`，确保旧配置行为不变。
- `webGateway` 缺失时默认 Qwen 和已验证的默认网页模型。
- `saveHistory` 缺失时默认 `false`；只有用户显式开启时才发送非临时会话。
- 不把 Web Credential 放进 AppConfig。
- 切换 WebGateway 不清空 Official API provider、model 或 apiKeys。
- 切回 Official API 时恢复之前的 Official API 设置。

配置验证：

- OfficialApi 模式继续验证 base URL、API Key、model。
- WebGateway 模式不要求 API Key。
- WebGateway 模式只允许 Qwen。
- WebGateway model 必须来自内部允许列表，第一版不接受任意字符串。
- 保存配置不触发登录、不访问网络。

### 12. 更新设置页

#### 文件

- 修改 `src/pages/SettingsPage.tsx`
- 修改 `src/services/tauriCommands.ts`
- 修改 `src/types/index.ts`
- 可新增 `src/components/WebGatewaySettings.tsx`

#### UI 行为

增加“翻译后端”选择：

- Official API
- Qwen 网页实验模式

Official API 模式：

- 保持当前供应商、模型、API Key 和测试连接 UI。

WebGateway 模式：

- 显示“实验功能”标记。
- 显示 Qwen model 选择。
- 显示登录状态：未登录、登录中、已登录、已过期。
- 提供“登录 Qwen”“重新登录”“退出登录”。
- 提供“测试连接”。
- 不显示 API Key 输入框。
- 明确提示不会自动回退到付费 API。

前端调用：

```typescript
beginWebLogin(provider)
getWebLoginStatus(provider)
logoutWebAccount(provider)
testConnection(config)
```

登录轮询：

- 仅在 SettingsPage 可见且状态为 LoggingIn 时每 1 秒调用一次状态查询。
- 页面卸载时停止轮询。
- 不在全局 store 常驻定时器。
- 登录完成、失败或过期后停止轮询。

不要让翻译页面自动弹出 Qwen 登录窗口。收到 `LoginRequired` 或 `SessionExpired` 时显示可操作错误，并提供跳转设置页的入口。

### 13. 生命周期与退出清理

#### 文件

- 修改 `src-tauri/src/lib.rs`
- 修改 QwenSession

要求：

- 应用启动时只检查 `credentials.bin` 是否存在且格式有效，不创建登录 WebView。
- Qwen profile 不应由主窗口加载。
- 托盘退出时：
  1. 取消登录 watcher。
  2. 关闭 `qwen-login`。
  3. 清理内存中的明文凭证。
  4. 保存主窗口状态。
  5. 注销快捷键。
  6. 退出应用。
- 普通关闭主窗口仍隐藏到托盘。
- 用户关闭 Qwen 登录窗口不隐藏到托盘。

## Detailed Instructions

按以下顺序实施，避免大爆炸式改动：

1. 先增加 `BackendMode`、`BackendRequest`、`BackendResult`、`BackendError`，不接 WebGateway。
2. 创建 `TranslationBackend` 和 `OfficialApiAdapter`，将现有 Official API 调用迁到新 seam。
3. 修改 `translate_text` 和测试连接命令只调用 TranslationBackend。
4. 运行全部现有测试，确认 Official API 行为无回归。
5. 增加 WebGateway 配置字段，默认保持 OfficialApi。
6. 实现 `credential_store.rs` 及其明文 round-trip、损坏文件、覆盖写入测试。
7. 实现 QwenSseDecoder，先用脱敏 fixture 完成纯测试，不接真实网络。
8. 实现 QwenWebAdapter，并使用 mock HTTP transport 验证请求与错误分类。
9. 实现 QwenSession 状态机和独立登录 Commands。
10. 创建 `qwen-login` WebView2，完成 Cookie 获取和导航 allowlist。
11. 接入 SettingsPage 登录状态 UI。
12. 最后打开 WebGateway 的实际翻译路由。
13. 使用测试账号进行手工端到端验证。
14. 确认失败、过期、取消和协议变化时 Official API 路径仍可用。

建议控制流：

```text
Ctrl+T
  → translationCoordinator 捕获文本
  → translate_text
  → TranslationRequestManager.run_latest
  → TranslationBackend.translate
      ├─ OfficialApi → OfficialApiAdapter
      └─ WebGateway
           → 检查 QwenSession
           → 没凭证：LoginRequired
           → 有凭证：QwenWebAdapter
                → reqwest stream
                → QwenSseDecoder
                → BackendResult
```

独立登录流：

```text
SettingsPage 点击登录
  → begin_web_login
  → QwenSession: LoggedOut/Expired → LoggingIn
  → 创建 qwen-login WebView2
  → 后台 watcher 读取 tongyi_sso_ticket
  → 明文原子保存
  → QwenSession: Ready
  → 关闭登录窗口
```

## Extra Requirements

### 功能兼容

- 不改变 Ctrl+T 捕获顺序。
- 不改变 latest-wins。
- 不改变主窗口 show/focus 的 active request 复查。
- 不改变快捷键事务和退出清理。
- 不改变 Official API 默认行为。
- 旧 `config.json` 必须能直接加载。

### 性能和体积

- 禁止 Playwright、Chromium、headless_chrome、chromiumoxide。
- 登录 WebView 按需创建，登录完成后关闭。
- 不启动 localhost Server。
- 不引入数据库。
- 不为一个 Provider 引入动态插件框架。
- HTTP Client 全局复用。
- SSE 增量解析，避免反复 clone 整个响应。
- 新依赖只启用最小 features。

### 安全

- Cookie/ticket 禁止明文落盘。
- 禁止输出敏感 Header。
- 禁止把远程网页加入 main capability。
- 禁止前端传任意上游 URL。
- 登录导航采用显式 allowlist。
- 解密凭证仅在请求期间存在。
- 不从 ticket 派生公开账号 ID、Device ID 或日志 ID。
- 不记录用户选中文本。
- 不把 Qwen 原始错误 body 直接返回前端。

### 可靠性

- 所有重试有上限。
- 所有等待有超时。
- 所有后台任务有取消路径。
- 所有状态转换必须在测试中覆盖。
- PartialResponse 永远不是 success。
- 协议变化只禁用 WebGateway，不影响 Official API。
- 保存配置失败不能改变现有生效配置。

### 代码规范

- 遵循当前 Rust 2021、thiserror、serde camelCase 风格。
- 遵循现有 AppResult 和 Tauri Command 返回结构。
- 不使用 `unwrap()`/`expect()` 处理生产路径。
- 测试代码可以使用 `expect()` 并说明断言含义。
- 不进行与本功能无关的 UI、CSS、命名或依赖重构。
- 不逐行复制 ProxyAgent 的 GPL/来源不明实现。

### 产品限制

- WebGateway 必须标记为实验性。
- 不宣称 Qwen 网页接口稳定或官方支持。
- 不实现绕过验证码、风控、限流或账号限制。
- 不自动创建账号。
- 不静默切换到付费 API。
- 在公开发布前由项目所有者确认 Qwen 服务条款和授权风险。

## Validation

### Rust 单元测试

必须覆盖：

#### TranslationBackend

- 默认旧配置路由 OfficialApi。
- WebGateway 配置路由 Qwen。
- LoginRequired 映射正确。
- Official API 错误不改变 QwenSession。
- WebGateway ProtocolMismatch 不影响 OfficialApi 后续请求。
- 新请求 abort 旧 WebGateway 流。

#### CredentialStore

- 明文 UTF-8 round-trip。
- 文件内容与 ticket 一致。
- 空文件、非 UTF-8 或超限内容读取失败。
- 覆盖写入后读取到新凭证。
- logout 删除凭证。

#### QwenSession

- LoggedOut → LoggingIn → Ready。
- LoggedOut → LoggingIn → 用户关闭 → LoggedOut。
- Expired → LoggingIn → 用户关闭 → Expired。
- 同时两次 begin login 只创建一个窗口。
- 401/403：Ready → Expired。
- Cancelled 不改变 phase。
- 启动时可从明文文件恢复 Ready。

#### QwenSseDecoder

- chunk 切在任意 UTF-8/SSE 边界。
- 单 chunk 多事件。
- CRLF。
- 多 data 行。
- reasoning 增量。
- answer 累积转 delta。
- 完整结束。
- error event。
- EOF 前无 complete。
- 累积文本回退或改写触发 ProtocolMismatch。
- 畸形 JSON 不 panic。

#### QwenWebAdapter

- Cookie Header 只包含必要 ticket。
- 每次 session_id/req_id 不同。
- Device ID 不是固定全局常量。
- timeout。
- 401/403。
- 429 有限重试。
- 5xx 有限重试。
- abort 后不继续解析。
- PartialResponse 不返回 success。
- 不产生伪造 usage。

### 前端测试/检查

- Official API 设置 UI 行为不变。
- WebGateway 模式隐藏 API Key。
- 登录轮询仅在需要时运行。
- 页面卸载停止轮询。
- LoginRequired 可引导到设置页。
- 保存配置不会触发登录。
- logout 提示清晰。

### 手工验证

1. 从旧版 config 启动，默认仍走 Official API。
2. 切换到 Qwen WebGateway，不登录触发翻译，应返回 LoginRequired。
3. 设置页点击登录，完成真实 Qwen 登录。
4. 检查：
   - `easyT_Data/web_gateway/qwen/profile` 已创建。
   - `credentials.bin` 已创建。
   - `config.json` 不含 ticket/Cookie。
5. Ctrl+T 翻译短文本。
   - `saveHistory=false` 时网页历史不应出现该请求。
   - `saveHistory=true` 时网页历史应出现对应会话。
6. 翻译过程中再次 Ctrl+T，旧请求不能覆盖新请求。
7. 人为断网，确认显示 Network/Timeout。
8. 使用无效 ticket，确认 Session 变为 Expired。
9. 关闭登录窗口，确认窗口真正销毁而非隐藏。
10. 切回 Official API，确认无需重启即可翻译。
11. 退出应用，确认没有残留 qwen-login 窗口或 watcher。
12. 检查日志没有 Cookie、ticket 和正文。

### 构建命令

```powershell
npm run typecheck
npm run build
cd src-tauri
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo check --release
```

如时间允许，再执行完整：

```powershell
cargo build --release
```

预期结果：

- 所有命令通过。
- 现有测试继续通过。
- 新增测试覆盖关键状态和错误。
- release 不包含 Chromium/Playwright。
- 未登录时不创建 Qwen WebView2 进程。

## Notes for DeepSeek

- 这是实现指导，不授权改变产品范围。不要增加其他 Web Provider。
- 不要让 `translate()` 打开登录窗口。
- 不要实现自动付费回退。
- 不要把 WebGateway 逻辑放进 `commands/translate.rs`、React store 或 `translationCoordinator.ts`。
- 不要为第一家 Web Provider 建立复杂公共 trait；Qwen 的私有实现保持在 Qwen 模块内。
- 不要复用主窗口 WebView profile 作为 Qwen 登录 profile。
- 不要将 `qwen-login` 加入 `default.json` capability。
- 不要假设 `tongyi_sso_ticket` 永久有效。
- 不要把“本地有凭证”当作“连接测试成功”。
- 不要返回截断翻译作为成功。
- 在确定真实登录重定向域名之前，不要放宽 navigation allowlist；这是安全决策，若无法确认应停止该部分并请求用户提供一次真实登录重定向记录。
- 在确定 Qwen 当前私有请求字段前，先用测试账号抓取并脱敏验证，不要盲目照搬历史模型名或固定 Header。
- Qwen 条款目前对自动交互和自动提取存在限制。技术实现不等于获得发布授权；公开发布前需要项目所有者单独确认。
