# Halo Studio Pi 与 OpenCode 核心重构设计

**日期：** 2026-07-22

**状态：** 已确认，等待实施计划

**适用范围：** 第一子项目“核心重构”

**目标平台：** Windows 首发，架构兼容 macOS 与 Linux

## 1. 背景与结论

Halo Studio 将从旧的多 Agent 终端壳重构为只服务 Pi 与 OpenCode 的跨平台桌面工作台。产品同时包含两个彼此独立但共享工作区上下文的领域：

- 开发域：项目、文件、Monaco 编辑器、Diff、Agent 会话和按需调试终端。
- 配置域：Provider、Profile、OpenCode MCP、Pi Skills、Prompts、Extensions、Packages、安全写入、备份与恢复。

本次不保留旧实现兼容层，不迁移旧数据，也不建立 `legacy` 目录。第一子项目只建立可运行、安全、可测试的新核心；完整编辑器、Agent 会话与配置管理在后续子项目交付。

## 2. 已确认的产品原则

1. 只支持 Pi 与 OpenCode，不再保留其他 Agent 的注册、品牌、协议、文档或测试。
2. Pi 使用官方 JSONL RPC；OpenCode 使用锁定版本的本地 Server、SDK 与 SSE。
3. 不伪造跨 Agent 能力：Pi 没有原生 MCP，OpenCode MCP 与 Pi 资源分别建模。
4. GUI 保留受管 Agent 的原生命令语义；不支持的 TUI 专属命令不得伪装成可用。
5. 调试终端是开发与故障诊断工具，不是产品主界面，也不能绕过 Agent 权限边界。
6. 原生配置文件仍是事实来源；SQLite 只保存 Halo 自有元数据、索引、审计和同步状态。
7. 凭据默认不进入 SQLite、日志、Diff、备份或云同步。
8. 所有写入必须先预览 Diff，并经过路径守卫、冲突检查、备份、原子替换和可验证回滚。
9. Windows 首发不得以牺牲 macOS/Linux 架构边界为代价。
10. 设计、计划、提交信息和项目活动文档使用中文。

## 3. 项目拆分

完整产品拆成四个依次实施的子项目，每个子项目分别经过规格、计划、实施和验收。

### 3.1 第一子项目：核心重构

- 仓库卫生与旧代码清退。
- 新 Electron 工程骨架与包边界。
- 只包含 Pi/OpenCode 的能力模型与运行时接口。
- Workspace、项目路径和信任模型。
- 类型化 IPC、Schema 校验和 Renderer 隔离。
- SQLite migration、凭据保险库和安全写入基线。
- VS Code 风格双域外壳与真实运行时状态。

### 3.2 第二子项目：双域工作台

- Pi RPC 完整会话接入。
- OpenCode Server/SDK/SSE 完整会话接入。
- 文件树、Monaco、Diff、消息、工具调用与权限交互。
- `/` 命令目录、命令路由和调试终端。

### 3.3 第三子项目：完整配置与流量层

- Provider、Profile 与凭据管理。
- OpenCode MCP。
- Pi Skills、Prompts、Extensions 与 Packages。
- 代理、故障转移策略和用量统计。

### 3.4 第四子项目：同步与发布

- 端到端加密云同步。
- 托盘、更新和国际化。
- macOS/Linux 适配与发布。

本规格仅授权实施第一子项目。后续内容只作为接口约束，不是当前阶段的交付范围。

## 4. 参考项目边界

`用于参考的几个项目的代码/` 是只读资料区：

- 不修改。
- 不在其中安装依赖或执行构建。
- 不复制其 Git 历史。
- 不进入 Halo Studio 的暂存区、提交或发布包。
- 引用代码、资源或图标前必须核对许可证并记录第三方声明。

审计绑定的关键参考版本：

- Pi：`@earendil-works/pi-coding-agent 0.81.1`。
- OpenCode/Desktop/JS SDK：`1.18.4`，参考提交 `712b1cbe715a428b876a26ea5a4d07c6cb092d8a`。

实现不得假设后续版本保持协议兼容。所有适配器都必须经过版本探测和兼容性策略。

## 5. 技术架构

采用 Electron + React + Vite + TypeScript + Monaco + SQLite。暂不引入 Rust；只有性能数据证明 TypeScript/Node 实现无法满足要求时，才允许将明确的热点下沉。

```text
React Renderer
    ↓ 类型化 IPC
Electron Main
    ├─ Workspace Service
    ├─ Trust Service
    ├─ Runtime Manager
    ├─ Agent Gateway
    │   ├─ Pi Adapter
    │   └─ OpenCode Adapter
    ├─ Config Service
    ├─ Credential Vault
    ├─ Storage Service
    └─ Audit / Logging
```

### 5.1 进程职责

**Renderer：**

- 只负责界面、交互和瞬时视图状态。
- 不直接访问 Node.js、文件系统、数据库、子进程或系统凭据。
- 不持有 OpenCode 本地认证密码或解密后的长期凭据。

**Preload：**

- 只暴露按业务域划分的最小 API。
- 不暴露通用 `invoke(channel, payload)`、文件路径任意读写或 Shell 执行能力。
- 请求与响应都使用共享 Schema 校验。

**Main：**

- 独占 Workspace、子进程、Server、文件系统、SQLite、凭据和系统集成。
- 校验所有来自 Renderer 的输入，不能把 TypeScript 类型视为运行时安全边界。
- 将 Pi/OpenCode 原生事件转换成 Halo 领域事件。

### 5.2 建议目录

```text
apps/
  desktop/
    src/main/
    src/preload/
    src/renderer/

packages/
  contracts/
  core/
  agent-pi/
  agent-opencode/
  config/
  storage/
  editor/
  ui/

docs/
  architecture/
  superpowers/specs/
  superpowers/plans/
```

根目录使用 npm workspaces。每个包必须有单一职责、显式公共入口和独立测试；禁止 Renderer 通过深层路径导入 Main 实现。

## 6. 核心领域模型

品牌枚举只存在于适配器选择边界。业务层使用能力声明，不使用分散的品牌条件判断。

```text
Workspace
  id, rootPath, realPath, trustState

RuntimeBinding
  agentKind, source, executable/version, health, capabilities

AgentSession
  workspaceId, agentKind, nativeSessionId, state, recoveryCursor

Profile
  agentKind, provider/model references, proxy policy, credential references

ConfigTarget
  scope, owner, path, format, source, writable
```

首批能力键至少包括：

- `sessions`
- `streamingMessages`
- `toolEvents`
- `permissions`
- `diff`
- `commands`
- `mcp`
- `skills`
- `prompts`
- `extensions`
- `packages`
- `models`
- `usage`

能力模型必须表达“是否支持、通过何种通道支持、是否需要重启”，不能只返回布尔值。

## 7. 运行时策略

### 7.1 Pi

- 默认探测系统安装的 `pi`/`pi.exe`。
- 系统版本缺失或不兼容时，Runtime Manager 预留安装到应用数据目录的隔离受管版本接口。
- 第一阶段只实现探测、版本信息、进程生命周期接口和最小 readiness probe，不交付完整聊天。
- readiness 使用带超时的 `get_state`，因为 RPC 没有独立 ready 消息。
- 进程必须显式传入 `cwd`、会话、模型、thinking 和信任选项，不依赖交互默认值。
- 请求使用唯一 `id` 关联；适配器必须容忍乱序响应。
- 冲突操作串行化；`abort` 与 `steer` 保留独立并发通道。
- 普通 Agent 运行以 `agent_start` 开始，以 `agent_settled` 结束。`prompt success` 和 `agent_end` 都不是完成信号。

### 7.2 OpenCode

- 默认使用应用内置且与 SDK 精确匹配的 Server，不依赖 PATH 中的外部可执行文件。
- 外部安装只用于诊断与后续显式连接功能；未通过白名单版本验证时不得自动连接。
- Server 通过 Electron `utilityProcess` 或等价隔离进程运行。
- 只绑定 loopback，启动时生成随机 Basic Auth 凭据。
- 健康检查必须验证 `/global/health` 和精确版本，再发布“可用”。
- 端口冲突有限重试；停止时先优雅关闭，超时后强制终止。
- 第一阶段实现生命周期、认证、版本握手和最小事件连接，不交付完整聊天。
- 后续会话为每个 Workspace 目录创建独立 SDK client，并通过一条 `/global/event` SSE 接收事件。
- SSE 不提供可靠重放；断线后必须重新获取会话、状态、消息和权限快照。
- 未知事件忽略并记录采样日志，不得导致全局崩溃。

### 7.3 环境变量

- Agent 进程使用显式白名单构造环境，禁止无条件继承完整 `process.env`。
- 允许 PATH、HOME/USERPROFILE、临时目录、Locale、代理和经 Profile 明确授权的 Provider 变量。
- Halo 解密的凭据只在最小生命周期内注入目标进程，不写日志、不回传 Renderer。
- OpenCode Profile 的 XDG 目录必须在导入 Server 模块前设置。

## 8. Workspace 与信任模型

Workspace 是全应用唯一的项目上下文来源。设置页、编辑器、Agent、配置和终端不得分别维护路径副本。

打开 Workspace 时按以下顺序处理：

1. 将用户路径解析为绝对路径。
2. 获取规范化真实路径并识别 symlink/junction。
3. 验证目录存在且可访问。
4. 查询 Halo Trust Store 中最近的有效决策。
5. 未信任时只开放普通文件浏览，不启动项目资源加载。
6. 用户确认后才创建 Agent Runtime。

未信任项目：

- Pi 使用拒绝项目配置的启动策略，并关闭上下文文件自动加载。
- OpenCode 设置 `OPENCODE_DISABLE_PROJECT_CONFIG=1`，不加载项目插件或 MCP。
- 调试终端只有用户显式打开后才启动。

信任仅表示允许读取项目配置和资源，不代表系统沙箱。界面和文档不得做超出事实的安全承诺。

## 9. 配置、安全写入与回滚

### 9.1 事实来源

- Pi：`~/.pi/agent/` 与项目 `.pi/` 下的原生配置和资源。
- OpenCode：原生全局与项目 JSON/JSONC 配置、Auth API 和 MCP Auth 存储。
- Halo：Profile 映射、Workspace 索引、信任、审计、备份元数据和同步元数据。

不得把 Pi MCP 当作配置目标。不得默认把 Provider API Key 写入 OpenCode JSONC。

### 9.2 写入事务

每次配置写入执行：

```text
读取原文件与指纹
  → 结构化解析
  → 应用最小变更
  → 生成真正的 unified diff
  → 用户确认
  → 重新比较指纹
  → 写入同目录临时文件并刷盘
  → 原子替换
  → 验证可重新解析
  → 记录备份和审计
```

- JSONC 必须保留注释、排版和未知字段。
- 文件被外部修改时停止写入，重新生成 Diff。
- 回滚必须重新经过路径守卫、指纹检查和原子替换。
- 备份与审计不得包含可恢复的明文凭据。
- Pi 活跃 RPC 没有通用 reload；相关配置保存后标记需要重启。

### 9.3 路径守卫

- 目标和备份都使用真实路径比较。
- 防止 `..`、大小写差异、symlink、junction 和替换竞态逃逸。
- 只允许声明过的全局配置根、应用数据根或当前 Workspace 范围。
- Renderer 不能提交任意可写绝对路径，必须引用 Main 颁发的目标标识。

## 10. 存储与凭据

使用 `better-sqlite3` 及显式 migration，封装在 `packages/storage`，避免业务层依赖具体驱动。

第一阶段表只包含：

- `schema_migrations`
- `workspaces`
- `runtime_bindings`
- `profiles`
- `credential_refs`
- `config_backups`
- `audit_events`

SQLite 不保存：

- API Key、OAuth Token、OpenCode Basic Auth 密码。
- 调试终端完整输出。
- 未经用户选择的原生会话全文。
- 云同步恢复密钥。

`CredentialVault` 提供 `store/get/delete/isAvailable` 接口。Windows 首发使用操作系统保护能力；macOS/Linux 使用各自系统密钥服务。若系统保护不可用或退化到不安全明文模式，必须失败关闭，不能静默写入磁盘。

Migration 只前进、不自动删除数据。失败时应用进入只读恢复模式，并给出导出诊断和重试入口。

## 11. IPC 与事件契约

共享契约使用 Zod Schema，同时生成 TypeScript 类型。IPC 按域命名：

- `workspace.*`
- `runtime.*`
- `agent.*`
- `config.*`
- `storage.*`
- `terminal.*`

禁止通用 Shell、任意文件读写和通用数据库查询通道。

Agent Gateway 输出统一事件外壳：

```text
eventId
workspaceId
agentKind
sessionId?
sequence
timestamp
payload
```

`payload` 保留可识别的原生语义。统一事件只解决消费方式，不得抹平 Pi 与 OpenCode 的状态机差异。

Renderer 使用快照加增量事件：

- 启动和重连先读取快照。
- 增量按 session/part/tool 标识合并。
- 大量文本 delta 在一帧内批处理，防止频繁重渲染。
- 重连前状态显示为 stale，不把旧快照标记成实时状态。

## 12. GUI 命令与调试终端边界

完整功能在第二子项目实现，第一子项目必须预留以下契约。

### 12.1 命令

`CommandDescriptor` 至少包含：

- 原生命令名称与参数提示。
- 所属 Agent 和来源。
- 执行通道。
- 是否允许在运行中执行。
- 是否会修改全局默认值。
- 是否仅 TUI 可用。

Composer 输入 `/` 后根据当前 Agent 动态加载目录：

- Pi 的结构化 RPC 能力映射成命令；Extension 命令保留原始文本并通过原生 prompt 通道处理。
- OpenCode 只暴露 Server/SDK 当前版本真实支持的命令。
- 仅 TUI 可用的命令明确标记，并提供转到调试终端的入口。
- Agent 切换后清空旧命令目录，禁止跨 Agent 错发。

### 12.2 调试终端

- 使用独立 `DebugTerminalService`，不复用 Agent Transport。
- 默认在开发模式或高级设置中启用。
- 工作目录来自当前 Workspace。
- 不注入 Halo 保存的凭据或 OpenCode 本地认证信息。
- 输出默认不进入 SQLite、备份或云同步。
- 终端进程崩溃只影响对应实例。

## 13. UI 与编辑器设计

采用 VS Code 式信息架构，但不复制完整产品功能。

### 13.1 固定结构

- 顶部：标题栏与命令中心。
- 左侧 Activity Bar：资源、搜索、Agent、配置、历史和设置。
- 左侧 Side Bar：当前域的文件树或配置导航。
- 中心：Monaco、Diff 或配置编辑器。
- 右侧 Auxiliary Bar：当前 Agent 会话。
- 底部 Panel：变更、按需调试终端和日志。
- 底部 Status Bar：信任、Git、运行时、光标和语言状态。

开发域和配置域共享 Workspace、Agent 状态和命令中心，不通过两个独立窗口复制状态。

### 13.2 Monaco 保留能力

后续编辑器包含：

- 多标签、脏文件和保存。
- 常用语言语法高亮。
- 查找替换、跳转行、括号匹配、格式化入口。
- 快捷键、撤销重做和基础命令面板。
- 双栏 Diff 与 Agent 修改定位。
- 大文件限制和二进制文件保护。

明确排除：扩展市场、调试器、Notebook、远程开发和完整 LSP/SCM 生态。

### 13.3 视觉约束

- 安静、紧凑、适合长时间工作的中性界面。
- 主要区域使用直角或小圆角，避免营销式卡片堆叠。
- 使用 Codicons/Lucide 等许可兼容图标，不复制产品商标。
- 图标按钮提供可访问名称和必要 Tooltip。
- 面板支持拖动、隐藏与合理最小尺寸，文本不能重叠或溢出。

## 14. 错误模型与恢复

统一 `AppError` 至少覆盖：

- `RuntimeUnavailable`
- `VersionMismatch`
- `AuthenticationFailed`
- `PermissionRequired`
- `WorkspaceUntrusted`
- `TransportDisconnected`
- `ProtocolViolation`
- `ConfigConflict`
- `UnsafePath`
- `MigrationFailed`

错误在发生位置显示，并带可执行的下一步。禁止使用全局弹窗替代状态设计。

- Pi 退出后会话标记为断开；可以根据原生 session file 重启恢复，但不自动重发未确认请求。
- OpenCode SSE 重连后重新拉取快照；Sidecar 异常退出时会话进入断开状态。
- 无法确认是否已提交的 Prompt 一律不自动重放。
- 配置冲突要求重新读取并生成 Diff。
- 调试终端故障与 Agent Runtime 隔离。
- 所有日志经过敏感字段过滤和大小限制。

第一阶段不实现跨供应商自动故障转移。Pi 和 OpenCode 当前协议都缺少足够稳定的跨模型幂等重放保证。

## 15. 测试策略

实施使用测试驱动开发。生产代码不得存在测试 Mock 自动兜底。

### 15.1 单元测试

- Schema、能力声明和事件转换。
- 规范路径、symlink/junction、Workspace 边界。
- 环境变量白名单和日志脱敏。
- unified diff、指纹冲突、原子写入与回滚。
- 命令路由和错误映射。

### 15.2 第一阶段 Pi 契约测试

- UTF-8 分片、LF/CRLF、`U+2028/U+2029` 和无效 JSON。
- 乱序响应、stderr 噪声、超时、EOF 和异常退出。
- `get_state` readiness、版本探测和受管版本接口。
- 冲突命令串行化以及进程停止。

### 15.3 第一阶段 OpenCode 契约测试

- ready、error、启动悬挂、端口冲突和意外退出。
- Basic Auth、健康 401/500/200 和版本不匹配。
- 最小 SSE connected/heartbeat、未知事件和断线状态。
- 优雅停止超时后的强制终止。

### 15.4 第一阶段集成与端到端测试

- 临时 Workspace、临时 XDG/Agent 根和临时 SQLite。
- Electron IPC、Renderer 隔离和凭据不进入 Renderer/日志。
- 双域切换、运行时状态、项目信任和配置预览。
- Windows 空格/CJK 路径、`pi.exe --mode rpc`、OpenCode Sidecar、打包启动。

### 15.5 后续阶段继承的契约门禁

- Pi 的 `agent_end` 后重试、最终 `agent_settled`、队列、取消、会话恢复和动态命令目录。
- OpenCode 的完整 SSE 增量、断线快照校准、消息 Part、权限列表与回复。
- Monaco 脏文件、Diff、`/` 命令菜单和调试终端生命周期。

这些测试在对应子项目实施时启用，不作为第一子项目的完成条件。

验收以关键行为覆盖为准，不用覆盖率数字代替协议、安全和恢复测试。

## 16. 第一子项目清理范围

直接删除：

- 旧 Agent Adapter、类型、品牌、命令、UI 和测试夹具。
- 旧 MCP preview、项目目标和 Pi 伪 MCP 假设。
- Mock PTY、静态假历史和宇宙主题。
- 旧 Web fallback、无效 Server、无关 metadata。
- 不再适用的旧架构规格、实施计划和测试。

需要重写：

- README 和活动架构文档。
- Electron bootstrap、preload、IPC 和共享契约。
- Agent registry、探测和生命周期。
- 配置、Diff、路径守卫和回滚。
- Renderer 外壳与测试。

可以保留思想但必须重新实现：

- Agent Registry 与 Command Probe 的职责分离。
- 写入前 Diff、确认、备份、原子替换和回滚。
- 单个运行时探测失败不拖垮全局 UI。

不迁移旧数据，不把旧代码移动到 `legacy`。

## 17. 仓库卫生与 Git 流程

### 17.1 忽略与行尾

`.gitignore` 必须包含：

- `用于参考的几个项目的代码/`
- `.superpowers/`
- 构建产物、日志、本地 SQLite、运行时缓存和临时备份。

新增 `.gitattributes` 固定源码与文档行尾，消除 Windows `core.autocrlf` 引起的索引噪声。

### 17.2 提交

提交信息使用中文且单一职责，例如：

- `文档: 确立 Pi 与 OpenCode 核心重构规格`
- `维护: 隔离只读参考项目并统一行尾`
- `重构: 清退旧 Agent 实现`
- `功能: 建立双 Agent 能力模型`
- `测试: 增加 Pi RPC 传输契约测试`

规格、计划、清理、实现和测试分别提交。

### 17.3 远端清理

首次推送新开发流程前：

1. 将 `origin` 设置为 `git@github.com:Nyzeep/Halo-Studio.git`。
2. 联网获取远端真实分支列表。
3. 验证目标仓库与默认分支为 `main`。
4. 删除远端除 `main` 外的全部旧分支。
5. 再次验证远端只剩 `main`。
6. 不强推、不改写 `main` 已发布历史。

旧本地原型分支绑定现有 Worktree，并包含未合并提交。它们不用于新开发，也不整体合并；待新核心验证完成后再精确移除对应 Worktree 和本地分支。

### 17.4 新分支模型

```text
main
  └─ develop
       ├─ feature/<功能>
       ├─ fix/<问题>
       └─ refactor/<范围>
```

- `main` 始终可发布。
- `develop` 承担阶段集成。
- 子代理使用独立短期分支或 Worktree。
- 子项目测试和审查通过后合入 `develop`。
- 阶段验收后由 `develop` 合入 `main` 并打版本标签。

## 18. 第一子项目验收标准

1. 应用可以在 Windows 启动并显示新的双域外壳。
2. Agent 模型只包含 Pi/OpenCode，业务层通过能力声明工作。
3. Workspace 是唯一项目路径来源，运行时代码没有固定本机绝对路径。
4. 未信任项目不会加载项目 Agent 配置、插件或资源。
5. Pi 可以完成系统运行时探测和 `get_state` readiness。
6. OpenCode 内置 Server 可以完成 loopback 启动、认证、健康检查、版本握手和关闭。
7. Renderer 不能直接访问 Node、文件系统、SQLite、子进程或凭据。
8. IPC 输入输出都经过运行时 Schema 校验。
9. SQLite migration、CredentialVault、Diff、原子写入、备份和安全回滚基线通过测试。
10. 旧 Web fallback、Mock Agent、伪 MCP、宇宙主题和静态假数据全部移除。
11. 参考目录未修改、未构建、未被 Git 跟踪，也不进入发布包。
12. 旧桌面测试全部被新架构测试替换，完整测试、类型检查和构建通过。

## 19. 明确不在第一子项目中的内容

- 完整 Monaco 编辑和文件操作。
- 完整 Pi/OpenCode 对话、工具流、权限交互与会话管理。
- `/` 命令执行和嵌入式调试终端实现。
- Provider/Profile 完整 UI。
- OpenCode MCP 和 Pi 资源管理。
- 代理、跨供应商故障转移和用量统计。
- 云同步、恢复密钥与多端冲突处理。
- 托盘、自动更新、国际化和 macOS/Linux 发布。

这些能力必须沿用本规格的边界，但分别进入后续规格与实施计划。

## 20. 主要风险与缓解

| 风险 | 缓解 |
|---|---|
| OpenCode SDK 与 Server 快速漂移 | 内置精确版本、健康握手、契约测试和外部版本白名单 |
| Pi RPC 响应乱序或进程异常 | 请求 ID、冲突操作串行化、readiness probe、会话恢复但不自动重放 |
| 项目配置在未信任状态被自动加载 | Halo Trust Store、启动前环境开关、进程级隔离 |
| JSONC 写入破坏注释或外部修改 | AST patch、文件指纹、Diff、原子替换和解析验证 |
| 凭据通过日志、IPC 或备份泄露 | Main 独占、系统保险库、日志脱敏、明文回退失败关闭 |
| 重构范围失控 | 四子项目拆分，第一子项目按第 18 节验收，不提前实现后续功能 |
| 参考代码被误提交 | 根目录忽略、提交前 `git ls-files` 与暂存区检查 |

本设计是第一子项目实施计划的唯一范围依据。任何扩大范围的改动必须先更新规格并重新确认。
