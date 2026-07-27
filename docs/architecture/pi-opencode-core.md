# Pi/OpenCode 核心架构

本文描述 Halo Studio R1 的实际架构边界。它与[产品需求](../requirements/2026-07-24-halo-studio-pi-opencode-product-requirements.md)、[架构 ADR](../adr/0001-pi-opencode-managed-workbench-boundary.md)和根目录 `CONTEXT.md` 一起构成活动设计来源。实现状态以代码与测试为准；本文不把计划中的界面或协议描述为可用功能。

## 产品边界

Halo Studio 只受管 Pi 与 OpenCode。R1 是安全的配置/运行时工作台，不是完整 IDE：它不提供文件编辑、结构化对话、命令执行或嵌入式终端。工作台采用类似 VS Code 的信息布局，但不会因此继承一个完整编辑器的能力集合。

一个工作区是应用唯一的项目上下文。当前服务在任意时刻只保留一个活动工作区；打开另一个工作区、撤销信任或退出应用时，旧工作区的运行时会被停止并清理。未受信任工作区可以被打开和查看状态，但不能启动 Pi 或 OpenCode。

## 进程与模块边界

| 层级 | 拥有的职责 | 明确不拥有的职责 |
| --- | --- | --- |
| Renderer | React 工作台、工作区/运行时状态展示、经桥接发送的固定请求 | Node API、文件系统、凭据、子进程、直接访问回环服务 |
| Preload | `window.halo` 的固定业务 API；请求与响应的 schema 校验 | 任意 IPC、凭据读取、运行时控制以外的 Node 能力 |
| Electron Main | 工作区、信任、进程生命周期、SQLite、凭据库、Main-only 启动解析、IPC 处理 | 将敏感值或底层端口暴露给 Renderer |
| `agent-pi` | Pi 检测、JSONL transport、`get_state` 就绪、停止与故障语义 | 从 Renderer 取得模型或 Provider 凭据 |
| `agent-opencode` | 锁定运行时工件、受管子进程、回环监听、认证、健康/版本检查、停止 | 把服务端认证或端口交给 Renderer |
| `core` / `storage` / `config` | 路径与信任、环境白名单、迁移与凭据保护、配置事务基础 | 绕过 Main 的安全边界 |

桌面窗口启用 `contextIsolation`、`sandbox` 和 `webSecurity`，关闭 Node 集成与开发者工具，并拒绝导航、重定向和新窗口。生产窗口加载本地 Renderer 文件；测试中可以显式传入受限的 loopback 开发 URL，但生产启动流程不配置开发 URL。开发态烟测即使使用 headless/GPU 兼容开关也保留 Renderer sandbox；当前受限 Codex 环境会阻断 Chromium sandboxed Renderer，因此正式烟测必须在交互式、非受限 Windows 宿主运行，且不得用 `--no-sandbox` 弱化该边界。

## 原生运行时 ABI

`better-sqlite3` 需要分别匹配宿主 Node 与 Electron 的 ABI。`scripts/prepare-native-runtime.mjs node` 在测试和 `windows-smoke.mjs` 前恢复当前 Node ABI；`scripts/prepare-native-runtime.mjs electron` 在桌面 `dev`、`build` 和 `smoke:dev` 前准备 Electron ABI。脚本把已构建副本缓存到 `.halo-runtime/native-build-cache/`，该目录受 Git 忽略，仅是本机构建缓存，不属于应用数据、发布物或版本控制内容。

## 固定 IPC 契约

Preload 只暴露这些分组方法，所有输入、输出和错误信封都由 `@halo-studio/contracts` 校验：

| 域 | 方法 | R1 当前行为 |
| --- | --- | --- |
| 工作区 | `pick`、`open`、`snapshot`、`setTrust` | 已接入 Main 服务；目录经真实路径、目录类型、可读性和信任状态检查。 |
| 运行时 | `probe`、`start`、`stop`、`snapshot` | 已接入 Main 服务；只能作用于 Pi 或 OpenCode。当前 Pi 界面只通过这组固定请求执行启动、停止和重试，不传入启动配置或凭据。 |
| 配置 | `preview`、`commit`、`rollback` | 契约与底层事务包存在，但桌面服务当前统一返回不可用；尚未形成用户可用的配置写入能力。 |
| 存储 | `health` | 已返回 SQLite 模式、schema 版本和有限诊断信息。 |

IPC 不支持任意通道、任意对象调用或任意进程启动。Renderer 不保有 Main 服务对象，也不会直接连接 Pi 或 OpenCode。

## 工作区生命周期

1. 用户通过 Main 拥有的目录选择器选择目录。
2. `openWorkspace` 解析输入路径，取得真实路径，确认该路径是可读目录，并据此生成稳定工作区标识与初始信任状态。
3. `createDesktopServices` 记录目录设备/文件标识；执行运行时操作前重新打开并比对身份，发现替换、删除或重定向时会使工作区失效。
4. 运行时探测允许在未受信任状态下执行；启动会先要求 `trusted`。
5. 打开另一工作区、改变信任状态或释放桌面服务时，会串行化生命周期操作并尝试停止该工作区中的 Pi 与 OpenCode。

失败停止的运行时会被保留为不可用，避免后续操作假定它已经安全退出。公开状态是 Main 服务计算的快照，不是静态“在线”标识。

## Pi 受管运行时

Pi 的检测和启动均由 Main 侧创建的运行时完成：

- 检测会返回受管运行时的可用性、来源、可执行文件和版本等非敏感元数据。版本探测是凭据盲的：`pi --version` 不接收 Provider 值，也不会读取凭据库。
- 仅当凭据盲的版本探测成功、且即将创建确认的 `--mode rpc` Pi 子进程时，Main-only `PiLaunchResolver` 才生成模型、thinking、Provider 环境和允许的 Provider 键集合。该对象不属于 IPC 契约。
- 默认解析器把模型/思考级别/Provider 环境键/凭据引用作为非敏感选择器，并仅在上述 RPC 子进程创建前一次性从 Electron 保护的凭据库读取实际 Provider 值；运行时和服务缓存不会保留该启动对象或 Provider 值。
- Pi 通过 JSONL RPC 启动；只有 `get_state` 成功后才进入 `ready`。EOF、无效协议和非正常退出都会作为生命周期故障处理。
- 公开 `RuntimeBinding` 不含模型、thinking、Provider 环境或凭据值。

当前 UI 尚未提供 Pi 启动选择器、凭据输入或凭据管理页面；它只在受信任工作区中发出固定的 Pi 启动、停止和重试请求。若 Main 侧选择器或受保护凭据缺失，Pi 启动应返回失败而不是猜测默认值。

## OpenCode 受管运行时

OpenCode 由 `@halo-studio/agent-opencode` 创建，使用锁定的 `opencode-ai` 依赖（当前锁定版本 `1.18.4`）作为运行时工件来源。生命周期如下：

1. Main 为受信任工作区创建受管实例，并建立经过白名单筛选的环境。
2. 每次启动都创建新的服务端认证信息，子进程只报告回环监听端口。
3. 运行时对本地服务执行认证健康检查和精确版本握手；两者成功后才报告 `healthy`。
4. 退出、健康检查失败或版本不匹配会进入相应的非健康状态；停止会清除进程内认证信息。

认证信息与端口不属于 Renderer 状态或 IPC 输出。当前 UI 可以在工作区受信任后请求启动 OpenCode；停止能力已在 Main IPC 中，但尚未在当前界面提供按钮。

## 数据、凭据和环境

桌面服务在 Electron `userData` 下创建自己的目录：

- `storage/halo-studio.sqlite3`：应用元数据与迁移状态；迁移失败时以只读恢复方式打开。
- `credentials/`：凭据引用对应的加密文件。保护能力不可用时，读写都会失败关闭。
- `runtime/pi/` 与 `runtime/opencode/`：各运行时的受管宿主目录。

`buildRuntimeEnvironment` 只复制经过审查的基础环境项和经允许列表批准的 Provider 环境键；不继承完整宿主环境。桌面服务还把运行时的 `HOME`/`USERPROFILE` 指向各自的受管目录。Provider 明文不得写入数据库、配置备份、审计、日志、IPC 或 UI 状态。

配置事务包实现了目标注册、预览、指纹冲突检测、备份与回滚的基础设施，但此能力尚未由桌面服务启用。原生配置文件仍是配置内容的事实来源；在桌面层正式开放前，不应宣称 Halo Studio 已能编辑或提交 Pi/OpenCode 配置。

## 当前 UI 的真实能力

Renderer 提供紧凑的工作台框架和无障碍标记。当前可观察能力为：选择目录、显示信任状态、刷新 Pi/OpenCode 的 Main 状态；在受信任工作区以固定 IPC 启动 Pi、在 Pi 启动中或就绪后停止它、在 Pi 崩溃或不可用后停止再重试；启动 OpenCode；以及显示公开错误信息。Pi 面板不会展示或接收模型、thinking、Provider 或凭据值。

“工作区”“搜索”“历史”“设置”“配置域”和底部“输出/问题”目前仅是工作台导航或状态容器。配置域只明确提示配置写入尚未开放；这些区域不提供文件树、文件读取/写入、搜索、历史、配置写入、终端或命令执行能力。

## R2：结构化会话与原生命令目录

在 R1 的运行时边界之上，当前分支增加了一个受限的 R2 垂直切片。它不是完整聊天产品、IDE 或终端，而是把已经启动的受管运行时投影为可查看、可发送和可中止的原生会话。

- `session.snapshot`、`session.create`、`session.select`、`session.history`、`session.send`、`session.abort` 与 `command.list` 是唯一会话 IPC；所有输入和输出均由共享契约校验。
- `session.event` 是唯一的 Main 到 Renderer 推送通道。Pi JSONL 与 OpenCode SSE 事件会先在 Main 脱敏并通过 schema 校验，再由安全窗口转发；Renderer 不持有端口、认证信息、原始日志或订阅句柄。
- Pi 只投影当前受管 RPC 会话：新建会话不接受父会话、路径或导出参数；历史仅包含有界的 user/assistant/system 文本；命令目录仅包含 Pi 真正报告的 slash 命令元数据。
- OpenCode 只通过健康的受管 sidecar 取得受限 HTTP/SSE 会话适配器。Basic Auth 和 loopback 地址继续只在 Main 内存中存在；运行时停止、工作区切换、撤销信任或应用退出时会关闭 SSE 订阅。
- 在未受信任工作区、未启动运行时或不健康运行时中，会话 API 不会隐式启动进程或读取原生会话。受信任但尚未启动任何受管运行时的会话快照为空。
- 输入以普通原生提示发送。用户输入 `/` 时，界面只筛选当前受管应用真实公开的命令并将其插入输入框；Halo 不提供单独的命令执行 API，也不把命令目录伪装成跨应用能力。

该切片明确不包含任意 Shell、PTY、嵌入式终端、文件读取/编辑、Diff 定位、凭据输入、模型或 Provider 配置输入、权限批准代理以及完整会话归档。Pi/OpenCode 的原生权限与文件写入语义仍由其各自运行时负责，Halo 不代理这些写入。

## 验证接口

`createDesktopServices` 是 Main 服务组合的最高层测试接缝。集成测试通过它注入受控的 Pi/OpenCode 子进程工厂，验证真实子进程协议、信任门槛、状态、停止和临时目录清理，而不把测试夹具作为产品回退路径。

命令、覆盖范围和尚未具备的验收项见[核心重构验证指南](../testing/core-rebuild-verification.md)。
