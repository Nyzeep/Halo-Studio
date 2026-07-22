# Halo Studio 阶段工作总览与交接记录

日期：2026-07-22
当前推荐接续分支：`codex/native-phase-2`
当前推荐工作区：`D:\Halo Studio\.worktrees\native-phase-2`
项目根目录：`D:\Halo Studio`

## 1. 当前结论

Halo Studio 已从早期 Electron/React/Web UI 路线，正式转向 **Windows 优先的原生桌面多 Agent 工作台** 路线。当前推荐技术方向为：

- UI：`PySide6 + QML`
- Runtime：Rust workspace
- IPC：stdio JSONL sidecar
- 目标：纯本地桌面壳，不依赖浏览器 UI、不继续扩展 Web App

截至本记录，已完成：

1. 中文架构重构文档。
2. 原生桌面 Phase 1 纵切片。
3. 原生桌面启动稳定性修复。
4. 项目根 `.venv` 和 `Myprompt/` 忽略规则。
5. Rust Runtime / EventBus / Snapshot / JSONL IPC sidecar 的 Phase 2 纵切片。
6. Python `IpcClient` 与 controller 的 demo/ipc 安全接缝。
7. 最新分支已合入 `origin/main` 的本地忽略规则。

当前桌面 UI 仍默认使用 demo runtime。真实 CLI、PTY、MCP 写入、配置写入、权限审批仍未接入原生路线，后续应在 Phase 3/4 继续。

## 2. 用户关键约束

- 软件形态：本地桌面壳。
- 平台优先级：先做 Windows。
- 技术边界：不要 Web 端，不要 Electron/React/Vue/浏览器 UI 作为目标路线。
- UI 风格：尽量继承当前轻量毛玻璃/微光风格；不能继承时参考 Codex、Google Gemini、Cursor、CC Switch、Raycast、VS Code 的信息组织和交互节奏。
- Agent 范围：Codex CLI、Claude Code、OpenCode、Pi Agent，未来可扩展 Gemini CLI、本地 Agent、自定义 Agent。
- 交互原则：用户与 Agent 的对话进入可视化界面，不以终端作为主交互。
- 终端角色：仅作为 Debug 抽屉或底层 transport。
- 命令体验：`/` 命令补全、参数补全、最近使用、收藏、模糊搜索。
- 性能要求：移除重动画、高 blur、高 shadow、粒子效果；保证 UI 流畅。
- 开发环境：Python 调试与测试使用项目根 `.venv`。
- 本地提示文件：`mission.md`、`request.md` 放在 `Myprompt/`，该目录不提交。
- 提交语言：中文 commit message。

## 3. 已完成任务按阶段归档

### 3.1 早期 Electron/React 路线

这一阶段完成过可运行的 Electron 桌面壳、Agent 探测、终端会话、MCP 配置预览、安全配置写入、备份历史、真实写入确认守卫、项目级 MCP 写入预案等能力。

这些能力现在的状态是：**作为迁移参考保留，不再作为目标产品路线继续扩展**。原因是用户后续明确要求纯原生桌面应用，不要 Web/Electron/React/Vue/浏览器 UI。

可迁移价值：

- Agent 探测逻辑。
- 配置 diff、write guard、backup/rollback。
- MCP preview、project MCP target。
- 部分测试思路与安全边界。

不再推荐继续投入：

- React renderer UI。
- Electron main/preload/IPC。
- Vite/Vitest/jsdom 作为目标 UI 测试链。
- xterm 作为主交互界面。

### 3.2 原生重构设计

已完成中文设计文档：

- `docs/architecture/2026-07-22-native-agent-workspace-rebuild.md`

主要结论：

- 从终端中心转为 Agent 中心。
- 目标架构为 `PySide6/QML UI + Rust Runtime Sidecar`。
- Event Bus、Command Dispatcher、Scheduler、Agent Runtime、Transport Adapter、Plugin Layer 分层。
- 旧 Electron/React 代码冻结为迁移参考。
- 真实 shell、配置写入、MCP 写入必须安全默认关闭。

### 3.3 Phase 1：原生桌面壳纵切片

已完成内容：

- 新建 `apps/desktop` PySide6/QML 应用骨架。
- 新建 `crates/halo-protocol`、`crates/halo-core`。
- 实现三栏工作台：
  - `AgentSidebar`
  - `WorkflowTimeline`
  - `InspectorPanel`
  - `CommandComposer`
- 实现 `/` 命令补全、参数补全、当前 Agent 排序。
- 实现 fake multi-agent runtime，可生成 4/16/32 Agent 的确定性事件流。
- 加入四个内置 Agent manifest：
  - Claude Code
  - Codex CLI
  - OpenCode
  - Pi
- QML 禁止高开销动画模式：
  - `ParticleSystem`
  - `ShaderEffect`
  - `DropShadow`
  - `FastBlur`
  - 持续坐标动画
- 增加 Python/Rust 单元测试。

### 3.4 Phase 1 启动修复

已修复：

- QML controller 生命周期过早释放导致窗口空白或无响应。
- QML 启动时 controller 未注入的空值保护。
- Qt Quick Controls 样式加载问题，默认设置 `QT_QUICK_CONTROLS_STYLE=Basic`。
- Inspector debug drawer 默认折叠。

### 3.5 本地开发约束整理

已完成：

- 在项目根创建 `.venv` 并用于后续 Python 调试与测试。
- 将 `mission.md`、`request.md` 移动到 `Myprompt/`。
- `Myprompt/`、`.venv/`、`.halo-user-data/`、`.worktrees/`、缓存、日志、IDE 配置加入 `.gitignore`。
- 最新 Phase 2 分支已合入 `origin/main` 的忽略规则。

### 3.6 Phase 2：Rust Runtime / IPC 纵切片

已完成内容：

- 扩展 `halo-protocol`：
  - `RuntimeEvent`
  - `RunState`
  - `RunSnapshot`
  - `RuntimeCommand`
- 新增 `halo-core::EventBus`：
  - 按 `run_id` 保存事件。
  - 每个 run 独立校验 seq。
  - ring buffer 限制 snapshot 内事件数量。
  - 拒绝首次乱序事件，且不会污染 snapshot。
  - 拒绝同一 run 的 `agent_id` 漂移。
- 新增 `halo-ipc`：
  - std-only JSONL command/event/snapshot/error 编解码。
  - 不引入 Rust 外部依赖。
  - 覆盖字符串转义、中文、引号、反斜杠、换行、未知命令。
- 新增 `halo-runtime`：
  - stdio JSONL sidecar。
  - 支持 `createRun`、`getSnapshot`、`shutdown`。
  - 使用 fake runtime 输出 ordered events。
  - 重复 run id 返回明确 error。
  - 不接真实 CLI、不启 PTY、不写配置。
- 新增 Python `IpcClient`：
  - import 和 `__init__` 不启动 sidecar。
  - 只有显式 `start_sidecar()` 才启动进程。
  - sidecar stderr 使用 `DEVNULL`，避免 pipe backpressure。
  - stdout reader 使用后台线程和 queue，避免 `readline()` 阻塞 UI 调用路径。
  - malformed JSON 转换为 error event。
  - 保留 `cached_events()` 供 controller 非阻塞读取。
- controller runtime seam：
  - `runtime_mode="demo" | "ipc"`。
  - 默认仍为 `demo`。
  - QML controller 工厂支持传入 `runtime_mode` 和 `ipc_client`。
  - `HALO_RUNTIME_MODE` 环境变量可读取，但非法值回退 demo。
- QML 小修：
  - `CommandComposer` 同时防护 `undefined` 和 `null` controller。
  - agent ready 数量从硬编码改为 `agents.length`。

## 4. 提交记录

### 4.1 原生桌面重构主线

| SHA | 提交信息 | 状态 |
| --- | --- | --- |
| `26e214e` | 文档：制定原生多 Agent 工作台重构方案 | 原生路线设计基线 |
| `95946f4` | 工程：忽略本地工作区目录 | 防止 `.worktrees/` 入库 |
| `9aa1e4d` | 阶段一：建立原生 Agent 工作台纵切片 | Phase 1 主提交 |
| `8ee9385` | 修复：稳定原生桌面启动入口 | Phase 1 启动修复 |
| `712b1cb` | 工程：忽略本地虚拟环境和提示文件 | main 上的本地开发忽略规则 |
| `774b490` | 阶段二：接入原生运行时事件总线 | Phase 2 主提交 |
| `bf5176b` | 合并：同步本地忽略规则 | 将 `origin/main` 忽略规则合入 Phase 2 分支 |

### 4.2 旧 Electron/React 路线历史记录

| SHA | 提交信息 | 当前定位 |
| --- | --- | --- |
| `43a0fa6` | 工程：初始化桌面应用并添加 Agent 检测 | 迁移参考 |
| `f176538` | 功能：添加桌面工作台和终端会话 | 迁移参考 |
| `393bfe0` | 文档：添加 MCP 注册中心实现计划 | 迁移参考 |
| `dd8eae5` | 功能：添加 MCP 配置预览生成器 | 可迁移逻辑 |
| `0c0b35b` | 界面：接入 MCP 配置预览面板 | 旧 UI，弃用 |
| `9c3969e` | 文档：添加安全配置写入实现计划 | 可迁移设计 |
| `328076f` | 功能：添加安全配置写入服务 | 可迁移逻辑 |
| `d34023e` | 功能：添加配置备份历史读取 | 可迁移逻辑 |
| `99aaa68` | 功能：添加真实写入确认守卫 | 可迁移安全逻辑 |
| `05dc96e` | 功能：添加项目级 MCP 写入目标 | 可迁移逻辑 |
| `72fd359` | 文档：补充项目级 MCP 写入说明 | 可迁移文档参考 |
| `942c8fe` | 文档：完善项目 README 说明 | 已被原生路线 README 部分替换 |
| `76098eb` | 修复：恢复桌面启动并修正新版 UI 阻塞 | 旧路线修复记录 |
| `2f9aefa` | 修复：解决启动黑屏和桌面渲染崩溃 | 旧路线修复记录 |
| `5863f65` | 修复：恢复桌面桥接并兜底 Agent 探测失败 | 旧路线修复记录 |

## 5. 当前验证记录

最近一次 Phase 2 验证命令：

```powershell
cargo test --workspace
..\..\.venv\Scripts\python.exe -m unittest discover -s apps/desktop/tests -v
```

结果：

- Rust workspace 测试通过。
- Python desktop 测试通过。
- 最新桌面壳可从 `.venv` 启动，stdout/stderr 启动日志为空。

启动命令：

```powershell
cd "D:\Halo Studio\.worktrees\native-phase-2"
..\..\.venv\Scripts\python.exe -m halo_desktop.main
```

## 6. 失效、弃用与清理分析

### 6.1 已明确弃用但暂不删除的 tracked 文件

以下文件属于旧 Electron/React/Web 路线，与当前纯原生目标冲突。它们应在 Phase 6 统一移动到 `legacy-electron/` 或删除，但本次不直接删除，原因是：

1. 其中仍包含可迁移逻辑。
2. 主目录 `main` 上存在用户未提交 UI 改动。
3. 当前 Phase 2 分支仍需要保留旧代码作为迁移参考和回归参考。

| 路径 | 原因 | 建议处理阶段 |
| --- | --- | --- |
| `src/main/**` | Electron 主进程、IPC、PTY、配置逻辑；目标路线不再使用 Electron，但内部业务逻辑可迁移 | Phase 4/6 |
| `src/renderer/**` | React/Web UI，与纯原生桌面目标冲突 | Phase 6 |
| `src/shared/**` | TypeScript 共享类型，可作为协议迁移参考，但最终由 Rust/Python 协议替代 | Phase 3/4 |
| `src/tests/**` | Vitest/jsdom/Electron 测试链，原生路线只保留行为思想 | Phase 6 |
| `index.html` | Vite Web 入口，目标路线弃用 | Phase 6 |
| `package.json`、`package-lock.json` | 旧 Electron/Web 工具链，迁移期暂留 | Phase 6 |
| `vite.config.ts`、`vitest.config.ts`、`tailwind.config.ts`、`postcss.config.js` | 旧 Web 构建与测试配置 | Phase 6 |
| `tsconfig.json`、`tsconfig.node.json` | 旧 TypeScript 工具链配置 | Phase 6 |

### 6.2 本地用户文件，不应提交或删除

| 路径 | 状态 | 处理 |
| --- | --- | --- |
| `D:\Halo Studio\Myprompt\request.md` | 本地提示源文件，已归纳到新需求文档 | 保留，忽略 |
| `D:\Halo Studio\Myprompt\mission.md` | 当前为空文件 | 保留，忽略 |
| `D:\Halo Studio\.venv\` | 项目根 Python 虚拟环境 | 保留，忽略 |
| `D:\Halo Studio\.worktrees\` | 独立开发工作区 | 保留，忽略 |
| `D:\Halo Studio\.env*` | 本地环境配置 | 保留，忽略 |
| `D:\Halo Studio` 主目录中的未提交 UI 改动 | 用户正在编辑的内容 | 不触碰 |

### 6.3 可以清理的无用生成物

以下是明确生成物，可安全删除；本次收尾已在 `D:\Halo Studio\.worktrees\native-phase-2` 中实际删除这些文件或目录：

| 路径 | 原因 |
| --- | --- |
| `.halo-desktop.out.log` | 试运行日志，已忽略 |
| `.halo-desktop.err.log` | 试运行日志，已忽略 |
| `apps/desktop/**/__pycache__/` | Python 测试/运行缓存 |
| `target/` | Rust 构建产物，可由 `cargo` 重建 |

## 7. 下一轮对话建议入口

建议下一轮从以下目标开始：

1. 决定是否将 `codex/native-phase-2` 合并到 `main`。
2. 开始 Phase 3：真实 Agent Adapter 与 Debug Terminal。
3. 为 Codex CLI、Claude Code、OpenCode、Pi 分别设计 adapter contract。
4. 增加真实 CLI 输出解析到 `RuntimeEvent` 的 parser。
5. 加入 Agent run 提交路径：`CommandComposer -> Controller -> IpcClient -> halo-runtime`。
6. 继续保持 UI 默认不阻塞、真实 shell 默认不启用、配置写入默认关闭。

推荐下一轮第一条指令：

```text
请基于 docs/2026-07-22-project-handoff-summary.md 和 docs/2026-07-22-mission-request-consolidated.md，继续 Phase 3：真实 Agent Adapter 与 Debug Terminal 的实施计划。
```
