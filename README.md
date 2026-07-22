# Halo Studio

Halo Studio 的目标是成为一个 Windows 优先的 AI Native Desktop Workspace：把 Claude Code、Codex CLI、OpenCode、Pi Agent 等本地 CLI Agent 统一放进一个原生桌面工作台中，让用户看到 Agent 的思考、工具调用、Shell、Diff、进度、Token 与结果摘要，而不是只面对终端字节流。

当前仓库正在从旧的 Electron/React/Web UI 路线迁移到原生桌面路线。Phase 1 先交付可运行、可测试的纵切片：原生桌面壳、三栏 Agent 工作区、`/` 命令补全、fake multi-agent runtime、内置 Agent manifest 和基础并发测试。

## 当前方向

- 原生桌面优先：Windows 先行，目标 UI 为 PySide6/QML。
- Runtime 分层：高并发 Agent runtime 逐步迁移到 Rust。
- Agent 是一等公民：每个 Agent 拥有独立状态、消息流、任务队列和工作流事件。
- 终端退到 Debug 角色：真实 CLI 输出会被解析成工作流事件，原始终端只作为调试抽屉。
- 少即是多：删除高成本动画、粒子背景、Web fallback 和低价值面板。
- 安全默认关闭：Phase 1 manifest 声明能力但不执行真实 shell 或文件写入。

## Phase 1 目录

```text
apps/
  desktop/
    halo_desktop/
      main.py
      app_controller.py
      completion.py
      demo_runtime.py
      plugin_registry.py
      qml/
crates/
  halo-protocol/
  halo-core/
plugins/
  agents/
    claude-code/
    codex-cli/
    opencode/
    pi/
docs/
  architecture/
  superpowers/plans/
```

## Windows 开发环境

建议环境：

- Windows 10 或 Windows 11
- Python 3.13+
- Rust 1.95+
- Git
- 可选：Claude Code、Codex CLI、OpenCode、Pi Agent 的本地 CLI

安装桌面依赖：

```powershell
cd "D:\Halo Studio"
python -m venv .venv
.\.venv\Scripts\Activate.ps1
python -m pip install -r apps\desktop\requirements.txt
python -m pip install -e apps\desktop
```

启动原生桌面壳：

```powershell
python -m halo_desktop.main
```

如果你暂时不想安装 editable 包，也可以这样启动：

```powershell
cd apps\desktop
python -m halo_desktop.main
```

如果没有安装 PySide6，入口会给出安装提示；核心 Python 测试不依赖 PySide6。

## 测试

Rust runtime 与协议测试：

```powershell
cargo test --workspace
```

Python 桌面后端、命令补全、manifest 读取、QML 静态约束测试：

```powershell
python -m unittest discover -s apps/desktop/tests -v
```

Phase 1 验收重点：

- fake runtime 支持 4/16/32 个 Agent 的确定性事件生成。
- slash completion 支持 `/codex`、`/claude`、`/opencode`、`/pi`、`/test`、`/review` 等命令。
- QML 主界面包含 AgentSidebar、WorkflowTimeline、InspectorPanel、CommandComposer。
- QML 不使用粒子、ShaderEffect、DropShadow、FastBlur 或持续坐标动画。
- 新增代码不引入新的 Electron、React、Vue、WebView 或浏览器 UI。

## 内置 Agent Manifest

Phase 1 已准备四个内置 Agent profile：

| Agent | 命令 | 默认权限 |
| --- | --- | --- |
| Claude Code | `claude` | shell/file_write 默认关闭 |
| Codex CLI | `codex` | shell/file_write 默认关闭 |
| OpenCode | `opencode` | shell/file_write 默认关闭 |
| Pi Agent | `pi` | shell/file_write 默认关闭 |

这些 manifest 目前只用于 UI、命令补全、能力声明和后续 runtime 接入。真实进程启动、配置写入、MCP 写入和权限审批会在后续阶段接入。

## 旧 Electron 状态

旧的 Electron/React 代码仍保留在仓库中，用作迁移参考和行为回归参考，但它不再是目标产品路线。后续阶段会逐步将可复用的 Agent 检测、配置写入、MCP preview、备份回滚等能力迁移到原生桌面架构，再清退旧 Web/Electron 入口。

## 文档

- 架构设计：`docs/architecture/2026-07-22-native-agent-workspace-rebuild.md`
- Phase 1 实施计划：`docs/superpowers/plans/2026-07-22-native-agent-workspace-phase-1.md`

## 当前限制

- Phase 1 使用 fake runtime，不直接调用真实 CLI。
- QML UI 是原生桌面壳纵切片，不是完整产品功能。
- 配置写入、MCP 写入、真实 shell、插件执行默认不启用。
- Windows 打包脚本尚未接入。
- PySide6 需要本机安装后才能实际打开桌面窗口。

## 提交约定

本项目提交信息优先使用中文。涉及真实配置、shell、文件写入、Agent 调度和并发 runtime 的变更，需要先补测试，再实现。
