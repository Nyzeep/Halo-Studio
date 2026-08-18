# Halo Studio 产品架构

> 权威范围：本文描述 Halo Studio 当前产品结构与接缝。领域词汇以根 `CONTEXT.md` 为准；已生效决策以 `docs/adr/` 为准；本文件不重开既有 ADR。

## 1. 产品形态

Halo Studio 是面向本地开发者的原生开发工作台。唯一正式产品入口是受跟踪的 `product/Halo Studio`（Tauri 桌面产品）；根目录不再提供可运行的产品入口。

当前 P0 执行链：

```text
Halo Studio（Tauri 桌面界面）
  └─ Halo Workbench Runtime（Tauri seam 上的深模块）
       └─ Pi RPC 执行适配（halo-pi-rpc-adapter）
            └─ 受控 Pi 子进程：pi --mode rpc
                 └─ stdin/stdout 严格 LF 分隔的 JSONL command/response/event 流
```

P0 只放行该执行链；历史 OpenCode Server、上游（原 BitFun）内置 Code Agent、ACP、旧 Sidecar 与前端直连均不进入当前发布门槛（ADR-0017、ADR-0072）。

## 2. Tauri 接缝

桌面端由两个 crate 组成：

| crate | 路径 | 职责 |
| --- | --- | --- |
| `halo-tauri-desktop` | `product/Halo Studio/src/apps/halo-desktop` | Tauri 外壳与正式入口（bin `halo-studio`），持有 Halo 存储作用域与 Workbench Runtime 装配 |
| `halo-desktop` | `product/Halo Studio/src/apps/desktop` | 桌面库（`halo_desktop_lib`），承载 Tauri command、桌面宿主适配与工作台装配 |

接缝规则：

- UI 组件不直接调用 Tauri API；一律经过 `src/web-ui/src/infrastructure` 的 adapter 层与 Workbench Runtime 的 typed 接口。
- 每个桌面 Tauri command 必须在 `src/apps/desktop/src/api/remote_workspace_policy.rs` 声明远程工作区策略（契约测试强制）。
- `halo-scope.json` 与 `scripts/halo-scope.mjs` 是 Halo 产品范围守卫：只放行本地编码工作台模块（local-workspaces、coding-sessions、file-explorer、editor、git、terminal），其余模块/路由被排除。

## 3. Workbench Runtime

Halo Workbench Runtime 是位于 Tauri seam 的深模块（ADR-0065），是唯一管理编码会话投影、受管任务、交付证据与人工决策的运行时。它与前端通过 `halo-workbench://event`、`halo_workbench_runtime_snapshot`、`halo_workbench_runtime_submit_intent` 等稳定契约通信，不依赖原始 Pi 会话标识或工具输出。

运行时职责：

- 受管任务会话的创建、续问、终止与中断语义；
- 任务基线、文件写入租约、交付证据版本与新鲜度；
- 凭据引用与脱敏投影；
- 标准/受管双模式分流。

## 4. Pi RPC P0 执行链

Pi RPC 执行适配（`halo-pi-rpc-adapter`）在深模块内受控启动本机 `pi --mode rpc`：

- 启动前先做版本探测与能力档案检查（`--mode rpc`、JSONL framing、`prompt`/`follow_up`/`abort`/`get_state`/`get_entries`、事件、extension UI、取消与清理语义）；
- 子进程环境不继承密钥；凭据通过操作系统凭据存储的引用在启动瞬间解析；
- 协议/传输失败一律 fail-closed；abort 有宽限期与强制回收语义；
- 原生结果规范化为 Halo 运行事实，原始 Session/Message 标识、完整会话与扩展内部状态不构成 Halo Interface。

详细语义见 `docs/development/pi-rpc-adapter.md` 与 `halo-pi-rpc-adapter` 的契约测试。

## 5. 模块划分

`product/Halo Studio` 是 Rust workspace + React 前端。依赖自底向上：

| 层 | 路径 | 拥有 |
| --- | --- | --- |
| 接口与入口 | `src/apps/*`、`src/web-ui`、`src/mobile-web`、`Halo-Installer`、`tests/e2e`、`src/crates/interfaces` | 产品宿主、命令、UI 入口、协议接口、跨端测试 |
| 产品装配 | `src/crates/assembly` | 兼容导出、产品能力选择、product-full 装配、适配器/服务注册 |
| 适配器 | `src/crates/adapters` | AI/传输/WebDriver 协议适配、外部 AI 工作源适配（OpenCode/Claude Code/Codex）、外部 Provider 翻译 |
| 服务 | `src/crates/services` | 可复用的 OS/文件系统/终端/MCP/远程/Git/进程/LSP 插件注册/会话持久化等实现 |
| 执行原语 | `src/crates/execution` | 可移植 Agent、harness、流、DeepReview、插件运行时客户端、tool 契约与执行 |
| 稳定契约 | `src/crates/contracts` | 共享 DTO、事件形态、运行时端口、产品领域契约/策略 |

边界规则：接口层只暴露选定的产品行为；装配层只接线不实现；适配器只翻译协议；服务层实现可复用能力；契约层不向上依赖。各目录最近的 `AGENTS.md` 是更具体的所有权文档。

## 6. 标准/受管双模式

- **标准编码模式**：继承上游产品的本地编码工作流（历史对照），可重开会话历史，不产生 Halo Studio 受管交付结论；可在未信任 Git 工作区中按上游产品的本地权限模型运行。
- **受管交付模式**：要求受信任工作区，任务创建时记录任务基线，产生可审查交付证据，并以本地开发者的人工决策结束。

两种模式共享同一个安全模型与凭据引用服务（ADR-0064），但受管模式不会加载第三方 MCP/Skills/插件/自定义 Agent/Pi TUI（ADR-0011、ADR-0031、ADR-0032、ADR-0033）。

## 7. 数据与安全边界

- 运行时数据：`HALO_USER_ROOT`/`HALO_HOME` 显式作用域（`src/apps/halo-desktop/src/main.rs`），默认用户根为 `%APPDATA%\Halo Studio`，home 为 `~/.halo-studio`；工作区数据目录为 `.halo-studio/`。
- 诊断日志：统一脱敏，不记录密钥、模型请求/响应明文（ADR-0009、ADR-0042、ADR-0043）。
- 凭据：只保存操作系统凭据存储的引用；系统存储不可用时失败关闭（ADR-0008）。
- 上游/历史：上游（原 BitFun）源码与证据只作为历史对照，标注为“历史记录/上游对照（已归档）”，不参与当前产品决策。
