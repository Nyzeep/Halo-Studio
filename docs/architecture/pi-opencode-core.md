# Pi RPC 核心架构

本文描述 Halo Studio P0 的实际架构边界。它与[迁移规格](../requirements/bitfun-tauri-product-migration/00-bitfun-tauri-product-migration-spec.md)、[ADR-0072](../adr/0072-use-pi-rpc-as-the-p0-managed-execution-adapter.md)和根目录 `CONTEXT.md` 一起构成活动设计来源。实现状态以代码与测试为准；本文不把计划中的界面或协议描述为可用功能。旧 OpenCode 方案只在 ADR-0071 和历史需求中作为比较材料保留。

## 产品边界

Halo Studio P0 只受管本机 Pi RPC。它是安全的配置/运行时工作台，不是完整 IDE：它不提供文件编辑、任意命令执行或嵌入式终端。工作台采用类似 VS Code 的信息布局，但不会因此继承一个完整编辑器的能力集合。

一个工作区是应用唯一的项目上下文。当前服务在任意时刻只保留一个活动工作区；打开另一个工作区、撤销信任或退出应用时，旧工作区的运行时会被停止并清理。未受信任工作区可以被打开和查看状态，但不能启动 Pi RPC 或加载项目 extension。

## 进程与模块边界

| 层级 | 拥有的职责 | 明确不拥有的职责 |
| --- | --- | --- |
| Tauri WebView | React 工作台、工作区/运行时状态展示、经 Tauri command/event 发送的固定请求 | Node API、文件系统、凭据、子进程、直接访问 Pi stdin/stdout |
| Tauri command/event bridge | 固定业务 command/event API；请求与响应的 schema 校验 | 任意 invoke、凭据读取、运行时控制以外的宿主能力 |
| Tauri host / Rust Main | 工作区、信任、进程生命周期、用户数据、凭据库、Main-only 启动解析、command/event 注册 | 将敏感值或底层进程句柄暴露给 WebView |
| `pi-rpc-adapter` (`bitfun-pi-rpc-adapter`) | Pi 可执行文件探测、RPC 子进程、LF JSONL transport、`get_state` readiness、停止与故障语义 | 从 WebView 取得模型或 Provider 凭据；加载未审计 extension |
| `core` / `storage` / `config` | 路径与信任、环境白名单、迁移与凭据保护、配置事务基础 | 绕过 Tauri host 的安全边界 |

Tauri WebView 使用生产配置的隔离、导航和权限策略，只加载正式 Halo Web UI；开发态 WebView/调试入口不得被当作生产路径。桌面烟测必须在交互式、非受限 Windows 宿主运行，不能用弱化宿主安全边界的参数伪造通过。

## Tauri 构建边界

Tauri 构建、Rust contract tests、Web UI type-check 和打包 smoke 必须从 `product/Halo Studio` 执行。Pi 不作为 npm/Cargo vendor 依赖；运行时只解析用户本机安装，并由工单 07 的能力档案决定是否可用。`D:\pi-main` 不进入构建、测试、暂存或发布输入。

## 固定 IPC 契约

Tauri command/event bridge 只暴露这些分组方法，所有输入、输出和错误信封都由共享 contracts 校验：

| 域 | 方法 | R1 当前行为 |
| --- | --- | --- |
| 工作区 | `pick`、`open`、`snapshot`、`setTrust` | 由 Tauri host 校验真实路径、目录类型、可读性和信任状态。 |
| 运行时 | `probe`、`start`、`stop`、`snapshot` | P0 只作用于 Pi RPC；启动选项和凭据不由 WebView 传入。 |
| 会话/任务 | `session.snapshot`、`session.send`、`task.create`、`task.resolve` | 只返回脱敏、有限的 Halo 状态和结构化事件。 |
| 配置/存储 | `preview`、`commit`、`rollback`、`health` | 原生配置事务和本地用户数据由 host 管理；不可用时失败关闭。 |

Bridge 不支持任意 channel、任意对象调用或任意进程启动。WebView 不保有 host 服务对象，也不会直接连接 Pi 子进程。

## 工作区生命周期

1. 用户通过 Tauri host 拥有的目录选择器选择目录。
2. `openWorkspace` 解析输入路径，取得真实路径，确认该路径是可读目录，并据此生成稳定工作区标识与初始信任状态。
3. Workbench Runtime 记录目录设备/文件标识；执行运行时操作前重新打开并比对身份，发现替换、删除或重定向时会使工作区失效。
4. 运行时探测允许在未受信任状态下执行；启动会先要求 `trusted`。
5. 打开另一工作区、改变信任状态或释放桌面服务时，会串行化生命周期操作并尝试停止该工作区中的 Pi RPC。

失败停止的运行时会被保留为不可用，避免后续操作假定它已经安全退出。公开状态是 Workbench Runtime 计算的快照，不是静态“在线”标识。

## Pi 受管运行时

Pi 的检测和启动均由 Tauri host 内的 Workbench Runtime 完成：

- 检测会返回受管运行时的可用性、来源、可执行文件和版本等非敏感元数据。版本探测是凭据盲的：`pi --version` 不接收 Provider 值，也不会读取凭据库。
- 仅当凭据盲的版本探测成功、且即将创建确认的 `--mode rpc` Pi 子进程时，host-only `PiLaunchResolver` 才生成模型、thinking、Provider 环境和允许的 Provider 键集合。该对象不属于 IPC 契约。
- 默认解析器把模型/思考级别/Provider 环境键/凭据引用作为非敏感选择器，并仅在上述 RPC 子进程创建前一次性从受保护的用户凭据库读取实际 Provider 值；运行时和服务缓存不会保留该启动对象或 Provider 值。
- Pi 通过 `pi --mode rpc` 启动；stdin/stdout 只接受严格 LF (`\n`) JSONL。客户端可剥离输入尾部 CR，但不能使用会把 U+2028/U+2029 当换行的通用行读取器。只有 `get_state` 成功后才进入 `ready`。EOF、无效 JSON、未知 response、非单调事件或非正常退出都会作为生命周期故障处理。
- 公开 `RuntimeBinding` 不含模型、thinking、Provider 环境或凭据值。

当前 UI 尚未提供 Pi 启动选择器、凭据输入或凭据管理页面；它只在受信任工作区中发出固定的 Pi 启动、停止和重试请求。若 host 侧选择器或受保护凭据缺失，Pi 启动应返回失败而不是猜测默认值。

### Pi RPC 命令与事件边界

P0 Adapter 只把以下 Pi RPC 能力当作协议输入：

- Commands: `prompt`、`follow_up`、`abort`、`get_state`、`get_entries`。
- Events: `message_update`、`tool_execution_start`、`tool_execution_update`、`tool_execution_end`、`agent_settled`，以及用于错误和生命周期的已验证事件。
- `agent_end` 只代表一次低层 Agent run 结束，不能当作任务结算；`agent_settled` 才代表没有自动 retry、compaction retry 或排队 continuation。
- 每个 command response 只在内部按可选 `id` 关联；事件由 Halo 重新编号。WebView 只收到本地 task/session 关联标识和脱敏摘要。
- `get_entries` 只用于内部有界的状态核对；原始 session JSONL、entry tree、session ID 和 entry ID 不得进入持久化或证据。

Pi 的默认行为没有 Halo 权限弹窗，Pi 项目信任也不是沙箱。权限门控必须由 Halo 第一方 extension 和 Runtime 完成，不能把 Pi 的默认 allow 行为当成产品安全边界。

## 第一方 Pi extension 边界

P0 只显式加载 Halo 第一方、固定版本、来源、依赖、权限和许可证均已审计的 extension。启动使用 `--no-extensions` 禁止发现式加载，再通过显式 `-e` 路径加载该 extension；项目 `.pi/extensions`、用户全局 extension、Pi package 和 Provider extension 不进入受管会话。

extension 用 `tool_call` 在工具执行前检查工具名、参数和当前任务上下文。需要用户决定时，它通过 RPC mode 的 `extension_ui_request` 发出 typed request，Runtime 只转发脱敏摘要并等待匹配 `extension_ui_response`。决议绑定单个任务和单个脱敏 `toolCallId`，只允许一次 allow/deny；deny、超时、协议错误、ID 不匹配或 extension_error 都必须阻止工具并使任务进入明确失败/等待状态。extension 不能通过 Pi 的项目 trust 或原生 UI 绕过 Halo 决议。

## 数据、凭据和环境

桌面服务在平台用户数据目录下创建自己的目录：

- `storage/halo-studio.sqlite3`：应用元数据与迁移状态；迁移失败时以只读恢复方式打开。
- `credentials/`：凭据引用对应的加密文件。保护能力不可用时，读写都会失败关闭。
- `runtime/pi/`：Pi 的受管宿主目录；受管任务另有隔离且可清理的 session/config 目录。

`buildRuntimeEnvironment` 只复制经过审查的基础环境项和经允许列表批准的 Provider 环境键；不继承完整宿主环境。桌面服务还把运行时的 `HOME`/`USERPROFILE` 指向各自的受管目录。Provider 明文不得写入数据库、配置备份、审计、日志、IPC 或 UI 状态。

配置事务包实现了目标注册、预览、指纹冲突检测、备份与回滚的基础设施，但此能力尚未由桌面服务启用。Pi 的 `models.json`、支持的 `settings.json` 和认证存储是 Pi 的原生事实来源；Halo 只保存非敏感选择和 `credential_ref`，不把 `auth.json` 当作 Halo 凭据权威，也不自动改写 Pi 全局配置。

## 当前 UI 的真实能力

WebView 提供紧凑的工作台框架和无障碍标记。当前可观察能力为：选择目录、显示信任状态、刷新 Pi RPC 的 Runtime 状态；在受信任工作区以固定 Tauri command 启动 Pi、在 Pi 启动中或就绪后停止它、在 Pi 崩溃或不可用后停止再重试；以及显示公开错误信息。Pi 面板不会展示或接收模型、thinking、Provider、凭据、session ID 或原始工具输出。

“工作区”“搜索”“历史”“设置”“配置域”和底部“输出/问题”目前仅是工作台导航或状态容器。配置域只明确提示配置写入尚未开放；这些区域不提供文件树、文件读取/写入、搜索、历史、配置写入、终端或命令执行能力。

## R2：结构化会话与原生命令目录

在 R1 的运行时边界之上，当前分支增加了一个受限的 R2 垂直切片。它不是完整聊天产品、IDE 或终端，而是把已经启动的受管运行时投影为可查看、可发送和可中止的原生会话。

- `session.snapshot`、`session.create`、`session.select`、`session.history`、`session.send`、`session.abort` 与 `command.list` 是唯一会话 IPC；所有输入和输出均由共享契约校验。
- `session.event` 是唯一的 Tauri host 到 WebView 推送通道。Pi JSONL 事件先在 Runtime 脱敏并通过 schema 校验，再由安全 bridge 转发；WebView 不持有原始 JSONL、认证信息、session/entry 标识、原始工具输出或订阅句柄。
- Pi 只投影当前受管 RPC 会话：新建会话不接受父会话、路径或导出参数；历史仅包含有界的 user/assistant/system 文本；命令目录仅包含 Pi 真正报告的 slash 命令元数据。
- Pi 标准会话和受管会话都由 Runtime 管理各自的 RPC 子进程和 session/config 目录；停止、工作区切换、撤销信任或应用退出时先发送 `abort`，再按宽限期关闭 stdin 并回收子进程。
- 在未受信任工作区、未启动运行时或不健康运行时中，会话 API 不会隐式启动进程或读取原生会话。受信任但尚未启动任何受管运行时的会话快照为空。
- 输入以普通原生提示发送。用户输入 `/` 时，界面只筛选当前受管应用真实公开的命令并将其插入输入框；Halo 不提供单独的命令执行 API，也不把命令目录伪装成跨应用能力。

该切片明确不包含任意 Shell、PTY、嵌入式终端、文件读取/编辑、Diff 定位、凭据输入、模型或 Provider 配置输入、未经审计的 extension、完整会话归档以及把 Pi TUI 作为执行接口。Pi 的工具执行发生在受控子进程中，但 Halo 仍必须通过第一方 extension 在执行前完成权限门控；Halo 不声称 Pi 天然安全或提供通用沙箱。

## 验证接口

Tauri host 的 Workbench Runtime service composition 是最高层测试接缝。集成测试通过它注入受控的 Pi 子进程工厂，验证真实 JSONL 协议、信任门槛、状态、停止、extension 决议和临时目录清理，而不把测试夹具作为产品回退路径。

命令、覆盖范围和尚未具备的验收项见[核心重构验证指南](../testing/core-rebuild-verification.md)。
