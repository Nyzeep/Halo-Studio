# Halo Studio

Halo Studio 是一个 Windows 优先的本地桌面多 Agent 开发工作台。它的目标是把 Claude Code、Codex CLI、OpenCode 和 Pi 这些常用命令行 Agent 统一放进一个漂亮、可切换、可配置、可扩展的桌面壳里，让开发者可以在同一个界面中启动会话、查看状态、预览配置、写入项目级 MCP 配置，并逐步扩展到多 Agent 协作调用。

当前项目仍处于早期开发阶段，但已经具备可运行的 Electron 桌面壳、Agent 检测、PTY 终端会话、MCP 配置预览、安全写入服务、备份回滚和项目级 MCP 写入预案。

## 设计目标

- 本地优先：先服务 Windows 桌面开发场景，核心功能不依赖云端服务。
- 多 Agent 一体化：统一管理 Claude Code、Codex CLI、OpenCode、Pi 的启动、配置和后续协作。
- 配置安全：涉及真实配置写入时，必须经过项目路径校验、危险目录拦截和确认短语。
- MCP 友好：提供跨 Agent 的 MCP 配置预览、项目级配置目标和后续结构化合并能力。
- 现代 UI：避免只做命令包装器，目标是形成一个适合长时间开发使用的工作台界面。
- 渐进落地：先完成可运行 MVP，再逐步接入配置合并、Agent 间调用、项目管理和插件生态。

## 当前功能

### 桌面工作台

- 基于 Electron、React、TypeScript 构建。
- Windows 本地桌面壳，开发模式下可直接启动桌面窗口。
- 左侧 Agent 启动栏，中间终端工作区，右侧状态和 MCP 配置面板。
- 当前默认工作区为 `D:\Halo Studio`，后续会扩展为可选择项目目录。

### Agent 集成

当前内置四类 Agent：

| Agent | 命令 | 当前能力 |
| --- | --- | --- |
| Claude Code | `claude` | 检测、终端启动、MCP 配置预览、项目级 `.mcp.json` 写入预案 |
| Codex CLI | `codex` | 检测、终端启动、MCP 配置预览、项目级 `.codex/config.toml` 写入预案 |
| OpenCode | `opencode` | 检测、终端启动、MCP 配置预览、项目级 `opencode.json` 写入预案 |
| Pi | `pi` | 检测、终端启动、MCP 配置预览、项目级 `.pi/mcp.json` 写入预案 |

Agent 检测会读取命令是否存在和版本信息。如果本机尚未安装某个 CLI，界面会显示缺失状态，但不会阻塞其他 Agent 使用。

### 终端会话

- 使用 `node-pty` 承载真实 CLI 会话。
- 使用 `xterm.js` 渲染终端。
- 支持启动多个 Agent 会话。
- 支持会话标签、活动会话切换、终端输入输出和窗口尺寸同步。

### MCP 配置预览

当前内置了一个示例 MCP 服务：

```txt
@modelcontextprotocol/server-filesystem
```

界面会为四个 Agent 生成对应配置片段：

- Claude Code：JSON，`mcpServers`
- Codex CLI：TOML，`[mcp_servers.<name>]`
- OpenCode：JSON，`mcp`
- Pi：JSON，`mcpServers`

这些预览用于确认生成内容是否符合目标 Agent 的配置格式。

### 配置写入服务

项目已经实现一套通用配置写入服务：

- 写入前读取旧内容。
- 自动生成统一 diff。
- 自动创建备份文件。
- 通过临时文件进行原子替换。
- 支持按备份回滚。
- 支持列出历史备份。

演示写入默认写到 Electron 用户数据目录下的 `preview-configs`，不会改动真实 Agent 配置。

### 项目级真实写入守卫

真实写入目前只允许写入当前工作区内的目标文件，并且会执行以下检查：

- 目标路径必须位于 workspace root 内。
- 禁止写入 `.git`、`node_modules`、`dist` 等危险目录。
- 写入前必须输入确认短语，格式为 `APPLY <文件名>`。
- 写入仍然复用 diff、备份、原子替换和回滚能力。

当前项目级 MCP 写入目标为：

| Agent | 项目级目标文件 |
| --- | --- |
| Claude Code | `.mcp.json` |
| Codex CLI | `.codex/config.toml` |
| OpenCode | `opencode.json` |
| Pi | `.pi/mcp.json` |

注意：当前真实写入会写入完整生成文件。也就是说，如果目标文件已经存在，现阶段不会做 JSON/TOML 结构化合并。结构化合并会在后续阶段实现。

## 技术架构

```txt
Halo Studio
├─ Electron Main Process
│  ├─ Agent Registry
│  ├─ PTY Session Manager
│  ├─ MCP Preview Service
│  ├─ Project MCP Target Service
│  ├─ Config Write Service
│  └─ IPC Handlers
├─ Electron Preload
│  └─ window.halo 安全桥接 API
├─ React Renderer
│  ├─ Agent Rail
│  ├─ Session Tabs
│  ├─ Terminal Pane
│  ├─ Inspector Panel
│  ├─ Utility Strip
│  └─ MCP Preview Panel
└─ Shared Types
   ├─ Agent 类型
   ├─ MCP 类型
   └─ 配置写入类型
```

核心目录：

| 路径 | 说明 |
| --- | --- |
| `src/main` | Electron 主进程、IPC、PTY、Agent 检测、配置写入 |
| `src/main/agents` | 四个 Agent 的适配器和检测逻辑 |
| `src/main/config` | diff、备份、原子写入、回滚、真实写入守卫 |
| `src/main/mcp` | MCP 配置预览和项目级目标生成 |
| `src/main/pty` | 终端会话管理 |
| `src/main/preload.ts` | Renderer 可访问的安全 API |
| `src/renderer` | React UI |
| `src/shared` | 主进程和渲染进程共享类型 |
| `src/tests` | Vitest 测试 |
| `docs/superpowers` | 阶段设计文档和实现计划 |

## 本地开发

### 环境要求

- Windows 10 或 Windows 11
- Node.js 20 或更高版本
- npm
- Git
- 可选：本机安装 `claude`、`codex`、`opencode`、`pi`

### 安装依赖

```bash
npm install
```

### 启动桌面应用

```bash
npm run dev:electron
```

该命令会同时启动 Vite、TypeScript 主进程监听和 Electron 桌面窗口。

### 运行测试

```bash
npm test
```

当前测试覆盖：

- Agent Registry
- MCP 配置预览
- 配置写入服务
- 真实写入守卫
- 项目级 MCP 写入目标

### 构建

```bash
npm run build
```

构建会执行：

- Renderer TypeScript 检查
- Main Process TypeScript 检查
- Vite 生产构建

当前还没有加入安装包打包脚本。后续可以接入 `electron-builder` 或 `electron-forge` 生成 Windows 安装包。

## 使用说明

### 启动 Agent 会话

1. 打开 Halo Studio。
2. 左侧会显示 Claude Code、Codex CLI、OpenCode、Pi 的检测结果。
3. 点击某个 Agent。
4. 中间终端区域会启动对应 CLI。
5. 可以通过会话标签切换不同 Agent 会话。

### 查看 MCP 配置预览

1. 打开右侧 MCP 预览区域。
2. 在 Agent 按钮中选择目标 Agent。
3. 查看对应 JSON、JSONC 或 TOML 配置内容。
4. 可以先使用演示写入，确认 diff 和备份流程。

### 写入项目级 MCP 配置

1. 在 MCP 面板中选择目标 Agent。
2. 查看“项目级真实写入预案”中的目标路径。
3. 确认目标位于当前工作区内。
4. 输入界面提示的确认短语，例如 `APPLY .mcp.json`。
5. 点击确认写入。

写入完成后会生成备份，并显示 diff。若需要恢复，可以通过回滚入口或备份历史恢复。

## 安全策略

Halo Studio 当前把真实配置写入作为高风险操作处理。

已经实现的保护：

- Renderer 不直接拼接真实 Agent 配置路径。
- 项目级路径由主进程服务生成。
- 写入目标必须位于 workspace root 内。
- 危险目录会被拦截。
- 用户必须输入确认短语。
- 每次写入都会备份旧内容。
- 写入使用临时文件替换，减少半写入风险。
- 支持回滚到备份。

尚未完成的保护：

- 尚未实现已有 JSON/TOML 配置的结构化合并。
- 尚未实现用户选择 workspace root。
- 尚未实现全局 Agent 配置写入。
- 尚未实现写入前的可视化结构化差异视图。

## 开发阶段

### 已完成

- Phase 0/1：Electron + React + TypeScript 桌面壳，多 Agent 工作台 UI，PTY 终端会话。
- Phase 2A：MCP 统一模型和四个 Agent 的配置预览。
- Phase 2B：安全配置写入服务，支持 diff、备份、原子写入和回滚。
- Phase 2C：配置备份历史和历史恢复入口。
- Phase 2D：真实写入确认守卫，限制项目内路径并拦截危险目录。
- Phase 2E：项目级 MCP 写入目标，支持 `.mcp.json`、`.codex/config.toml`、`opencode.json`、`.pi/mcp.json`。

### 下一步建议

1. Phase 2F：实现 JSON/TOML 结构化合并，避免覆盖已有配置。
2. Phase 3A：加入工作区选择和最近项目列表，替换硬编码 `D:\Halo Studio`。
3. Phase 3B：完善 Agent 配置文件管理，支持读取、编辑、备份和恢复。
4. Phase 3C：加入 MCP Server 注册表，支持常用 MCP 模板一键添加。
5. Phase 4A：实现 Agent 间调用和任务编排。
6. Phase 4B：加入会话日志、上下文快照和任务恢复。
7. Phase 5A：接入 Windows 安装包和自动更新。

## 当前限制

- 当前主要验证 Windows 环境。
- 工作区路径仍有硬编码，后续需要改为用户可选。
- MCP 示例服务目前是固定示例，后续会做可编辑注册表。
- 真实写入目前是完整文件写入，不做结构化 merge。
- 终端会话管理仍是 MVP，还没有持久化会话历史。
- 尚未加入 Windows 安装包构建。

## 参考项目

本项目的产品方向参考了以下开源项目和工具生态：

- OpenCode: `anomalyco/opencode`
- Pi: `earendil-works/pi`
- Pi Web: `agegr/pi-web`
- cc-switch: `farion1231/cc-switch`
- Claude Code、Codex CLI、OpenCode、Pi 的 MCP 和配置体系

## 贡献约定

- 文档和提交信息优先使用中文。
- 涉及真实写入、配置迁移、Agent 调用等高风险功能时，需要优先补测试。
- 不直接改动用户全局配置，除非有清晰的预览、确认、备份和回滚能力。
- UI 应保持桌面工作台风格，避免做成营销落地页。

## License

当前仓库尚未声明开源许可证。正式发布前需要补充许可证文件。
