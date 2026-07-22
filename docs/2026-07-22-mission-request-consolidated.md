# Halo Studio 统一使命与需求说明

日期：2026-07-22
来源：`D:\Halo Studio\Myprompt\request.md` 与 `D:\Halo Studio\Myprompt\mission.md`

说明：`mission.md` 当前为空文件；本文件主要依据 `request.md` 与前序对话中用户明确补充的要求整理。

## 1. 产品使命

Halo Studio 的使命是成为一个 **工业级、多 Agent、高性能、可扩展、AI Native 的本地桌面开发工作台**。

它不是简单的 CLI 启动器，也不是多个终端窗口的包装器。系统应围绕 Agent 的协同开发过程来组织信息，让用户看到 Agent 的思考、工具调用、文件操作、Shell、Diff、进度、Token、错误与最终总结。

核心体验目标：

> 用户关注 Agent 在做什么，而不是终端输出了什么。

## 2. 产品定位

Halo Studio 是一个 Windows 优先的原生桌面壳软件，用于统一管理和使用多种本地 AI Agent CLI。

首批目标 Agent：

- Codex CLI
- Claude Code
- OpenCode
- Pi Agent

未来扩展 Agent：

- Gemini CLI
- 本地自定义 Agent
- 其他插件化 Agent

## 3. 硬性技术约束

必须满足：

- 本地桌面应用。
- Windows 优先。
- 默认离线可开发、可测试。
- UI 与 runtime 解耦。
- Agent 通信事件化。
- Python 调试和测试使用项目根 `.venv`。
- 提交信息使用中文。

禁止作为目标路线：

- Electron
- React
- Vue
- Web App
- 浏览器 UI
- 任何依赖浏览器渲染的方案

允许技术：

- Python
- PySide6 / PyQt6
- QML
- Rust
- Tokio
- stdio JSONL / Named Pipe / MessagePack IPC

Tauri 只有在不依赖 Web UI 的情况下才可讨论；当前不推荐采用。

## 4. 目标架构

推荐架构：

```text
PySide6/QML UI
↓
Python ViewModel / Controller
↓
IPC Client
↓
Rust Runtime Sidecar
↓
Event Bus
↓
Command Dispatcher
↓
Scheduler
↓
Agent Runtime Actor
↓
Transport Adapter
↓
Agent CLI / Local Agent / Tool
```

核心原则：

- UI 只负责展示和输入。
- Python 层负责 ViewModel、插件协调、轻量业务逻辑。
- Rust 层负责高并发 runtime、调度、事件总线、PTY/stdio transport、parser、diff、文件监听、日志与高性能数据结构。
- 所有 Agent 输出先解析成 `RuntimeEvent`，再进入 UI。
- 原始终端输出只进入 Debug Terminal，不作为主交互。

## 5. Agent 一等公民模型

每个 Agent 都应拥有：

- 独立 Workspace
- 独立生命周期
- 独立状态管理
- 独立上下文
- 独立任务队列
- 独立可视化工作流
- 独立消息流
- 独立 Tool Call 展示
- 独立配置与权限策略

每个 Agent Run 应具备：

- `run_id`
- `agent_id`
- `workspace_id`
- `state`
- `seq`
- `created_at`
- `updated_at`
- `token_usage`
- `event snapshot`
- `ring buffer events`
- `debug raw output`

## 6. 用户交互要求

用户与 Agent 的对话不应是纯终端交互，而应呈现为结构化界面：

```text
User Message
↓
Assistant Response
↓
Thinking
↓
Tool Call
↓
Shell / File Read / Patch
↓
Diff Preview
↓
Summary
```

UI 应展示：

- Thinking
- Tool Call
- 文件读取
- Shell 命令
- stdout/stderr 摘要
- Patch
- Diff
- Token
- Progress
- 当前状态
- 耗时
- 错误信息
- 权限审批

终端仅作为 Debug 抽屉：

- 默认折叠。
- 用于查看原始 stdout/stderr。
- 不污染主消息流。

## 7. 命令补全要求

输入 `/` 触发命令补全。

基础命令：

- `/claude`
- `/codex`
- `/gemini`
- `/opencode`
- `/pi`
- `/help`
- `/build`
- `/test`
- `/review`
- `/run`
- `/git`
- `/terminal`
- `/clear`
- `/settings`

参数补全示例：

- `--continue`
- `--resume`
- `--permission`
- `--model`
- `--sandbox`

交互要求：

- `Tab` 补全。
- `Enter` 执行或确认。
- `↑/↓` 切换候选。
- `Esc` 关闭。
- 支持模糊搜索。
- 支持最近使用。
- 支持收藏命令。
- 当前 Agent 命令优先排序。

## 8. UI 与视觉要求

整体风格：

- 现代。
- 科技感。
- 轻量。
- 毛玻璃。
- 微光。
- 柔和渐变。
- 舒适阅读。
- 信息密度合理。

可参考：

- Codex
- Google Gemini
- Cursor
- CC Switch
- Raycast
- VS Code

注意：参考信息层级、交互节奏和布局逻辑，不直接复制 UI。

布局要求：

- 左侧：Agent / Workspace / Session 导航。
- 中间：Agent 聊天流与工作流 Timeline。
- 底部：Command Composer。
- 右侧：Inspector，展示状态、任务队列、MCP、文件变更、Token、耗时、错误。
- Debug Terminal 默认折叠。

视觉规范：

- 字号统一。
- 字体统一。
- Padding、Margin、Panel 宽度、Toolbar 高度统一。
- 面板圆角克制。
- 不使用过度卡片嵌套。
- 不使用大面积空白。
- 不让按钮拥挤。
- 不让文本重叠或溢出。

## 9. 性能要求

当前问题：

- 动画导致 CPU/GPU 占用偏高。
- 页面刷新频繁。
- Blur、Shadow、粒子、持续动效开销大。
- 多 Agent 时 UI 卡顿。

目标：

- Idle CPU 小于 3%。
- UI 保持 60 FPS 或设备允许范围内最优帧率。
- 多 Agent 同时运行仍保持流畅。
- 内存稳定，不因长日志或多任务无限增长。

要求：

- 建立统一 Animation Scheduler。
- 页面不可见时停止动画。
- 空闲状态暂停动画。
- 所有动画统一 tick。
- 避免多个高频 Timer。
- 避免频繁 repaint。
- 避免大量 Graphics Effect。
- 避免重复布局计算。
- 减少 Python 主线程压力。
- 使用虚拟列表与事件批处理。

禁止或严格限制：

- 粒子背景。
- 大面积 blur。
- 大面积 shadow。
- 永久 pulse。
- 大面积 glow。
- 多层持续 transform 动画。

## 10. 插件系统要求

插件系统应支持：

- 新 Agent。
- 新命令。
- 新 Sidebar。
- 新 Tool。
- 新 Workspace。
- 新模型。
- 新 Provider。

插件原则：

- 通过 manifest 声明能力。
- 不修改核心代码即可扩展。
- 第三方插件默认禁用危险权限。
- shell、file write、network、mcp 写入必须显式授权。
- 插件能力必须可被 UI 展示、审计和关闭。

## 11. MCP 与配置写入要求

目标能力：

- 支持 Codex CLI、Claude Code、OpenCode、Pi Agent 对应配置文件。
- 支持类似 CC Switch 的 provider/config 切换体验。
- 支持 MCP 工具配置读取、预览、写入。
- 支持项目级和用户级配置目标。

安全要求：

- 写入前必须生成 diff。
- 写入前必须明确确认。
- 写入必须备份。
- 写入应原子化。
- 支持回滚。
- 回滚也必须经过 path guard。
- JSON/TOML 必须 parse round-trip。
- Windows 路径 guard 需要覆盖大小写、symlink、junction、UNC、ADS 等绕过风险。

当前状态：

- 旧 Electron/React 路线已有部分 MCP/config 能力，可作为迁移参考。
- 原生路线 Phase 1/2 尚未接入真实配置写入。
- 后续 Phase 4 应迁移这些能力。

## 12. 并发与调度要求

系统应支持多个 Agent 同时运行：

- 完全异步。
- 互不阻塞。
- UI 不阻塞。
- 支持取消。
- 支持暂停。
- 支持恢复。
- 支持队列调度。

推荐模型：

- Actor per Agent Run。
- 全局并发限制。
- Agent 级并发限制。
- Workspace 级并发限制。
- Weighted Fair Queue。
- Event Bus 批量推送。
- Snapshot + ring buffer 防止内存无限增长。
- 长日志落盘，UI 只保留可见窗口附近数据。

## 13. 安全要求

安全默认关闭：

- 不默认执行真实 shell。
- 不默认写入文件。
- 不默认传递完整环境变量。
- 不默认允许插件危险能力。

必须具备：

- 命令白名单/策略层。
- cwd guard。
- args/env 校验。
- 敏感环境变量过滤。
- 权限审批事件。
- 操作审计日志。
- 失败可恢复。

## 14. 已完成阶段

### Phase 0：原生路线设计

已完成：

- 中文原生多 Agent 工作台重构设计。
- 明确放弃继续扩展 Electron/React 目标路线。
- 建立 PySide6/QML + Rust sidecar 的推荐架构。

### Phase 1：原生桌面壳 MVP

已完成：

- PySide6/QML 应用骨架。
- 三栏 Agent 工作台。
- `/` 命令补全。
- fake multi-agent runtime。
- 内置 Agent manifest。
- QML 静态性能约束。
- Python/Rust 测试。

### Phase 2：Rust Runtime 与 IPC

已完成：

- Rust `EventBus`。
- `RunSnapshot` ring buffer。
- std-only JSONL IPC codec。
- `halo-runtime` sidecar。
- Python `IpcClient`。
- controller demo/ipc seam。
- sidecar 不阻塞 UI 的基础防护。

## 15. 后续阶段建议

### Phase 3：真实 Agent Adapter 与 Debug Terminal

目标：

- 设计统一 Agent Adapter contract。
- 接入 Codex CLI、Claude Code、OpenCode、Pi 的启动策略。
- 先做 PTY/stdin/stdout transport。
- 将原始输出解析成 `RuntimeEvent`。
- 原始输出进入 Debug Terminal。
- `CommandComposer -> Controller -> IpcClient -> halo-runtime` 形成真实提交链路。

### Phase 4：配置与 MCP 安全迁移

目标：

- 迁移旧路线 config write guard。
- 迁移 backup/rollback。
- 迁移 MCP preview/project target。
- 实现原生 UI 的配置中心。
- 写入默认关闭，必须审批。

### Phase 5：性能与压力测试

目标：

- 多 Agent 压测。
- 长时间运行内存稳定性。
- Idle CPU 测量。
- 动画暂停机制。
- UI 虚拟列表与事件批处理。

### Phase 6：旧代码清退与打包

目标：

- 将旧 Electron/React 代码迁移到 `legacy-electron/` 或删除。
- 删除 Web 构建链。
- 完成 Windows 打包。
- 更新 README、Architecture、API、Migration Guide、Developer Guide。

## 16. 验收标准

长期验收标准：

- Windows 原生桌面应用可稳定启动。
- 不依赖 Web/Electron/浏览器 UI。
- UI 默认轻量、低开销。
- 多 Agent 并发时 UI 不阻塞。
- 每个 Agent 工作流可视化。
- `/` 命令补全可用。
- Debug Terminal 仅作为辅助。
- 真实 CLI 启动经过安全策略。
- 配置/MCP 写入有 diff、备份、确认、回滚。
- 插件权限可审计。
- 所有核心模块有测试。

短期下一步验收标准：

- Phase 3 至少接入一个真实 Agent adapter 的 dry-run 或 fake-process 测试。
- 命令提交链路从 UI/controller 进入 runtime。
- runtime 事件可以刷新到 UI 时间线。
- 真实 stdout/stderr 不阻塞 UI。
- 未安装真实 CLI 时 UI 仍可启动并给出明确状态。
