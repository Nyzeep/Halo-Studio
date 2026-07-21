# Halo Studio 设计文档

日期：2026-07-21

## 项目目标

Halo Studio 是一个 Windows 优先的本地桌面开发工作台，用来统一管理和使用多个 AI 编程 Agent。第一版目标是把 OpenCode、Pi、Codex CLI、Claude Code 集成到一个漂亮、稳定、实用的桌面壳里，让用户可以在一个软件中启动、切换、配置、对比和转交任务，而不需要手动记住四套 CLI 命令、配置路径、MCP 格式、账号凭据和环境变量。

Halo Studio 不应该重写这些官方 CLI。它真正有价值的地方，是围绕官方 CLI 建立一层可靠的编排能力：现代化 UI、稳定终端托管、统一配置中心、安全 Profile 切换、MCP 管理，以及后续的 Agent 互调和任务交接。

## 参考输入

设计参考了以下项目和官方文档：

- OpenCode 仓库与文档：`https://github.com/anomalyco/opencode`、`https://dev.opencode.ai/docs/config`、`https://dev.opencode.ai/docs/mcp-servers`
- Pi 仓库与文档：`https://github.com/earendil-works/pi`、`https://pi.dev/docs/latest/rpc`
- Pi Web 仓库：`https://github.com/agegr/pi-web`
- cc-switch 仓库：`https://github.com/farion1231/cc-switch`
- Codex CLI 配置与 MCP 文档：`https://developers.openai.com/codex/config-reference`、`https://developers.openai.com/codex/mcp`
- Claude Code 设置与 MCP 文档：`https://docs.anthropic.com/en/docs/claude-code/settings`、`https://docs.anthropic.com/en/docs/claude-code/mcp`

这些外部项目和文档在开发时需要定期复查，因为各厂商 CLI 和配置格式都有更新风险。

## 产品原则

1. 官方 CLI 是执行来源，Halo Studio 负责统一入口和编排。
2. UI 不直接写死厂商逻辑，所有厂商能力通过 Agent Adapter 暴露。
3. 配置写入必须有预览、备份、校验和回滚，不能粗暴覆盖。
4. 第一阶段先保证终端模式可靠，再做更高级的原生聊天或 RPC 集成。
5. 数据本地优先，敏感凭据使用 Windows 原生安全能力保护。
6. UI 做成开发者每天会用的工作台，不做营销式落地页。
7. 吸收 Pi Web 的实用工作流：会话浏览、项目文件预览、模型配置、技能开关、Git worktree 切换和结构化消息展示，都应进入 Halo 的长期产品能力。

## 推荐技术架构

第一版推荐使用 Electron、React、TypeScript 和 Node.js。

选择 Electron 的原因很直接：Halo Studio 的核心能力是启动本地 CLI、托管伪终端、读取和写入本地配置、传递 Ctrl+C、处理窗口 resize、管理环境变量、托盘和 Windows 安装包。这些能力在 Electron + Node 生态里最成熟，尤其适合配合 `node-pty` 和 `xterm.js` 做真实终端集成。

Tauri 可以作为后续优化方向。如果未来重点变成更小体积和更低内存占用，可以再评估迁移或做 Tauri 版本。但第一版不推荐从 Tauri 开始，因为多 CLI 伪终端和 Node 工具链集成会更折腾。

## 运行时分层

### 桌面壳

桌面壳负责窗口生命周期、菜单、托盘、文件选择、开机行为、更新流程，以及所有需要本地权限的服务。React 渲染层不能直接访问文件系统和进程能力，必须通过受控 IPC 调用主进程服务。

第一版职责：

- 创建主窗口。
- 管理 Windows 托盘快捷操作。
- 启动和停止 Agent 会话。
- 暴露安全 IPC 接口，用于配置读取、配置写入和会话管理。
- 把 Halo Studio 自己的设置保存在应用数据目录中。

### 前端 UI

UI 使用 React、Vite、TypeScript、Tailwind CSS、Radix primitives、shadcn 风格组件和 lucide 图标。

应用启动后直接进入工作台，不做介绍页。

主要区域：

- 左侧栏：工作区选择、Agent 列表、Profile 切换、快速启动。
- 中间区：多标签终端、未来的原生聊天标签、配置 diff 预览。
- 右侧面板：当前会话上下文、配置状态、MCP 服务器、常用指令。
- 底部状态栏：当前 Agent、模型、工作区、Profile、运行状态。

视觉风格应该接近现代 IDE 伴侣：信息密度高、层次清楚、适合长时间编码、少装饰、易扫描。

### Agent Adapter 层

每个厂商一个 Adapter，UI 只和 Adapter Registry 通信，不直接依赖某个 CLI 的实现细节。

Adapter 负责：

- 检测 CLI 是否安装。
- 读取版本和能力信息。
- 提供启动命令模板。
- 启动终端会话。
- 定位已知配置文件。
- 读取和校验当前配置。
- 生成厂商专属配置补丁。
- 报告支持的集成模式。

第一版 Adapter：

- `claude-code`
- `codex-cli`
- `opencode`
- `pi`

Adapter 接口需要支持以下模式：

- `terminal`：在 PTY 中运行官方 CLI。
- `rpc`：当厂商提供结构化本地协议时使用。
- `mcp`：暴露或消费本地 MCP 工具。
- `config-only`：即使没有运行会话，也允许管理配置。

### PTY 会话管理器

PTY 会话管理器负责托管真实终端会话。Windows 下使用 ConPTY 能力，通过 `node-pty` 把终端输入输出连接到前端 `xterm.js`。

核心要求：

- 创建、resize、聚焦、停止、重启会话。
- 保存会话元数据。
- 支持 Ctrl+C 和进程终止。
- 每个工作区支持多个终端标签。
- 支持按 Profile 注入启动环境变量。
- 会话日志默认关闭，用户选择后才保留。

终端模式是兼容性底线。即使未来某个 Agent 的原生 API 集成失败，用户仍然可以在 Halo 的终端面板里正常使用官方 CLI。

### 配置服务

Halo Studio 保存一份自己的标准化配置模型，再把它编译成各厂商自己的配置文件。

配置服务禁止无脑覆盖厂商文件。每次写入都必须经过以下流程：

1. 发现目标文件。
2. 使用结构化 parser 读取当前文件。
3. 生成最小补丁。
4. 校验生成结果。
5. 在 UI 中展示 diff 预览。
6. 写入带时间戳的备份。
7. 原子写入目标文件。
8. 重新读取文件确认写入结果。
9. 写入失败或校验失败时提供回滚。

支持的格式：

- JSON
- JSONC
- TOML
- 环境变量模板

如果厂商配置文件里有 Halo 不认识的字段，必须尽量保留。

### 凭据服务

敏感凭据不能保存在 Halo 的 SQLite 数据库或普通 JSON 设置文件里。Windows 第一版应使用 Windows Credential Manager 或 DPAPI 支持的安全存储库。

凭据服务保存：

- API Key
- Provider Token
- Profile 专属密钥
- 可选环境变量密钥

UI 必须清楚区分普通配置和敏感凭据。密钥默认掩码展示，只有用户明确操作时才允许临时显示。日志、配置导出和错误信息默认不能包含密钥。

### MCP 注册中心

Halo Studio 提供统一 MCP 注册中心，再把统一模型写入每个 Agent 的原生配置格式。

标准 MCP Server 模型：

- `id`
- `displayName`
- `transport`：`stdio`、`sse`、`http`
- `command`
- `args`
- `env`
- `url`
- `headers`
- `enabled`
- `scopes`
- `targetAgents`

厂商映射：

- Claude Code：支持项目 `.mcp.json`，以及必要时的 user/local scope 配置。
- Codex CLI：写入 `~/.codex/config.toml` 或项目 `.codex/config.toml` 中的 `[mcp_servers.<name>]`。
- OpenCode：写入 OpenCode JSON/JSONC 配置中的 `mcp` 对象，并尊重全局和项目配置分层。
- Pi：优先使用 Pi 文档里提供的 MCP 文件位置和 adapter 流程。如果 Pi 的 MCP 支持变化，优先调用官方命令或文档方式，而不是猜测文件格式。

UI 需要显示每个 MCP Server 启用了哪些 Agent，以及 Halo 是否能验证它。

### Halo Broker

Halo Broker 是后续阶段的核心差异化能力。它是一个本地服务，可以暴露 MCP Server 和可选 HTTP/IPC API，让一个 Agent 通过 Halo 调用另一个 Agent。

示例工具：

- `ask_codex`
- `ask_claude`
- `ask_opencode`
- `ask_pi`
- `handoff_task`
- `summarize_session`
- `list_active_agents`

Broker 默认只传递摘要上下文，不直接传递完整终端日志。完整 transcript 共享必须由用户明确确认。

## 厂商集成说明

### OpenCode

OpenCode 有自己的配置、Agent 和 MCP 概念，不能只当成普通 shell 命令。Halo 需要检测 OpenCode 配置文件，尊重配置分层，并避免删除未知字段。

第一版：

- 用终端模式启动 OpenCode。
- 读取和 patch OpenCode 的项目/全局配置。
- 管理 OpenCode MCP 条目。
- 提供常用 OpenCode 指令预设。

后续：

- 如果 OpenCode 的 Agent 模式稳定，暴露到 Halo UI。
- 如果 OpenCode 提供可靠本地 API，增加更深的会话元数据展示。

### Pi

Pi 是较适合早期做原生 Adapter 的 Agent，因为它的文档描述了基于 JSONL 的 RPC mode。第一版仍然先支持终端模式，后续再加入 RPC 原生聊天标签。

Pi Web 提供了一个值得参考的产品形态：读取本地 Pi 会话文件，以 Web 工作区形式展示会话、结构化 Markdown、工具调用、项目文件、模型配置、技能管理和 Git worktree。Halo Studio 应把这些能力抽象为跨 Agent 的通用工作台能力，而不只服务 Pi。

第一版：

- 用终端模式启动 Pi。
- 检测 Pi 安装状态和版本。
- 提供 Pi 指令预设。
- 通过 Pi 文档路径或命令管理 MCP 配置。
- 在 UI 中预留 Pi 会话档案、模型配置和技能面板入口。

后续：

- 增加 RPC 驱动的原生聊天界面。
- 渲染结构化状态、工具调用和消息事件。
- 读取 `~/.pi/agent/sessions` 下的 Pi JSONL 会话，支持按项目浏览、继续会话、从历史消息 fork 会话。

### Codex CLI

Codex CLI 第一版以终端模式和配置管理为主。早期重点是安全管理 `config.toml` 中的 MCP 和 Profile 相关配置。

第一版：

- 用终端模式启动 Codex CLI。
- 读取和 patch `~/.codex/config.toml`，以及存在时的项目 `.codex/config.toml`。
- 管理 MCP Server 条目。
- 在官方支持范围内暴露模型、sandbox、approval、workspace 等常用预设。

后续：

- 如果 Codex MCP server mode 足够稳定，再接入跨 Agent 编排。

### Claude Code

Claude Code 的配置位置和 scope 较多，必须谨慎处理。Halo 需要把账号/Profile 切换当成敏感功能，从第一版就加入备份和回滚。

第一版：

- 用终端模式启动 Claude Code。
- 管理常用指令和 MCP 条目。
- 读取和 patch 项目 `.mcp.json` 与 `.claude/settings*.json`。
- 支持带备份的 Profile 切换。

后续：

- 增加更完整的 cc-switch 风格 Profile 快照。
- 导入已有 Claude Code provider、commands、skills。

## 数据存储

Halo Studio 使用 SQLite 保存本地应用状态。

建议表：

- `workspaces`：已知项目根目录和展示信息。
- `agents`：检测到的 Agent 安装位置和版本。
- `profiles`：命名运行/配置 Profile。
- `profile_agents`：每个 Profile 下的 Agent 设置。
- `mcp_servers`：标准化 MCP 注册表。
- `sessions`：终端或原生会话记录。
- `config_snapshots`：配置备份元数据和恢复指针。
- `command_presets`：可复用 prompt 和启动命令。
- `audit_events`：配置写入、恢复和敏感操作审计，不记录密钥。

大型日志和备份文件不要塞进 SQLite。SQLite 只保存路径和元数据。

## UI 信息架构

### 主工作台

默认界面直接打开开发工作台，不做落地页。

主要区域：

- 左侧栏：工作区选择、Agent 列表、Profile、快速启动。
- 中间标签区：终端会话、未来原生聊天、会话档案、项目文件预览、diff 预览。
- 右侧检查器：当前会话详情、MCP 状态、配置面板、指令预设、模型/技能摘要。
- 命令面板：搜索动作、启动命令、切换 Profile、添加 MCP Server。

参考 Pi Web，Halo 的工作台后续应包含：

- 会话档案：按项目浏览历史会话，显示摘要、时间、Agent、模型和上下文状态。
- 项目浏览器：安全地浏览工作区文件，预览源码、Markdown、图片、PDF 和 diff。
- 结构化消息视图：把工具调用、思考状态、命令输出、错误和最终回答拆成清晰块。
- 会话分叉：从历史消息创建新路线，用于尝试不同实现方向。
- Git worktree 切换：在多分支开发时让新会话跟随选中的 worktree。
- 模型与技能管理：集中配置模型、测试可用性、启用/禁用技能或指令集。

### 配置中心

配置中心包含：

- Profiles
- Agents
- MCP
- Credentials
- Backups
- Diagnostics

每次配置写入都需要展示：

- 目标 Agent
- 目标文件
- scope
- 生成 diff
- 备份路径
- 校验状态

### 视觉风格

UI 应该现代、漂亮，但偏实用和工作流导向。避免大面积 hero、营销卡片和纯装饰布局。使用紧凑面板、清晰分隔线、工具图标、模式标签、启用状态开关，以及 Agent/Profile 菜单。

建议配色：

- 中性深色背景
- 白色和 zinc 色文本
- cyan 表示活跃运行状态
- amber 表示待应用配置
- green 表示校验通过
- red 表示危险或失败操作
- violet 只作为少量次级强调色

整体不能做成单一紫色或单一深蓝灰主题。

## MVP 范围

### Phase 0：技术验证

目标：证明桌面壳可以在 Windows 上稳定托管 CLI Agent。

交付物：

- Electron 应用骨架。
- PTY 终端面板。
- 四个 CLI 的安装检测。
- 手动启动可用 CLI。
- Ctrl+C、resize、重启、停止行为。

验收标准：

- 至少一个 Agent 可以在 app 内交互运行。
- 未安装 Agent 有清晰提示。
- PTY 行为足够稳定，可以进入日常测试。

### Phase 1：工作台 MVP

目标：让 Halo 作为多 Agent 启动器已经可日常使用。

交付物：

- 工作区选择。
- Agent 切换。
- 多标签终端会话。
- 指令预设。
- 本地 SQLite 应用状态。
- 基础设置页。

验收标准：

- 用户可以打开项目、启动 Agent、切换会话并复用 prompt。

### Phase 2：配置与 MCP 中心

目标：统一多 Agent 使用中最麻烦的配置部分。

交付物：

- 标准化配置模型。
- JSON/JSONC/TOML 解析和 patch。
- MCP 注册中心 UI。
- 各厂商 MCP 写入器。
- diff 预览。
- 备份和回滚。
- 凭据安全存储。

验收标准：

- 用户可以添加一个 MCP Server，并启用到支持的 Agent，无需手动编辑配置文件。

### Phase 3：Profile 切换

目标：提供类似 cc-switch、但覆盖多个 Agent 的 Profile 能力。

交付物：

- 命名 Profile。
- 每个 Agent 的启动环境变量和配置 patch。
- 由安全存储保护的环境变量密钥。
- 托盘或命令面板快速切换。
- 配置快照历史。

验收标准：

- 用户可以在至少两个 Profile 之间切换，并能安全回滚。

### Phase 4：Broker 与原生集成

目标：让 Halo 不只是终端复用器，而成为多 Agent 编排层。

交付物：

- 本地 Halo Broker MCP Server。
- Agent 任务交接工具。
- Pi RPC 原生聊天 Adapter。
- 会话摘要。
- 跨 Agent 任务交接 UI。

验收标准：

- 一个 Agent 可以通过 Halo 控制的工具，把摘要上下文交接给另一个 Agent。

## 测试策略

开发应优先测试风险最高的本地服务，再打磨 UI。

早期必须覆盖：

- 配置 parser 能保留未知字段。
- TOML writer 能生成预期 Codex MCP 配置。
- JSON/JSONC writer 能生成预期 OpenCode 和 Claude MCP 配置。
- 写入前会创建备份。
- 回滚能恢复旧内容。
- Adapter Registry 在 CLI 缺失时不会崩溃。
- PTY 会话生命周期能处理启动、停止和重启状态。

UI 测试覆盖：

- Agent 检测状态。
- 会话标签创建。
- MCP 表单校验。
- diff 预览。
- 凭据掩码展示。

Windows 手工验证必须覆盖 PTY 行为、Ctrl+C、终端 resize 和真实 CLI 交互。

## 安全与保护

敏感操作包括：

- 写入用户级配置文件。
- 切换凭据。
- 导出 Profile。
- 运行任意 Agent 命令。
- 把 Broker 工具暴露给 Agent。

安全要求：

- renderer 日志不出现密钥。
- SQLite 不保存密钥。
- 配置写入前必须备份。
- 第一版 Broker 不提供任意 shell 执行工具。
- 跨 Agent 交接默认只共享摘要上下文。
- 完整 transcript 共享必须明确确认。
- 配置导出默认不包含密钥。

## 待确认问题

1. 第一版只支持 PowerShell 环境，还是同时支持 Git Bash 和 WSL。
2. Profile 切换是立即 patch 厂商文件，还是只在启动会话时应用 overlay。
3. 第一版只做 dark mode，还是同时做 light mode。
4. 第一版是否制作安装包，还是先用开发模式运行。

推荐先这样决定：

- 第一版优先支持 PowerShell。
- 项目配置文件只有在用户确认 diff 后才 patch。
- 第一版只做 dark mode。
- 在 PTY 和配置流稳定前，先使用开发模式运行。

## 实施建议

从最小可用纵切开始：

1. 搭建 Electron + React + TypeScript 项目。
2. 建立桌面工作台 UI 骨架。
3. 增加 PTY 终端托管。
4. 建立 Adapter Registry 和模拟检测测试。
5. 先实现一个 CLI 的真实检测和启动。
6. 扩展到四个 CLI。
7. 接入 SQLite。
8. 增加标准化 MCP 注册表。
9. 先完成一个 MCP writer，推荐从 Codex 开始，因为 TOML 输出明确、便于测试。
10. 再增加 Claude、OpenCode 和 Pi 的 MCP writer。

这条路径能尽早产出可运行软件，同时把更复杂的配置服务和 Broker 功能保护在可测试的服务边界后面。
