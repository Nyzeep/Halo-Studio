# Halo Studio

Halo Studio 是一个 Windows 优先的本地多 Agent 开发工作台，目标是统一管理 OpenCode、Pi、Codex CLI 和 Claude Code。

## 本地开发

安装依赖：

```bash
npm install
```

启动桌面应用：

```bash
npm run dev:electron
```

运行测试：

```bash
npm test
```

构建：

```bash
npm run build
```

## 当前阶段

当前实现已推进到 Phase 2E：

- Windows Electron 桌面壳
- Agent 检测
- PTY 终端会话
- 多 Agent 工作台 UI
- 会话档案、项目文件、模型配置、技能管理和 Worktree 入口
- MCP 配置预览和项目级真实写入预案
- 配置写入演示：diff、备份、原子写入和回滚
- 配置备份历史列表和历史恢复入口
- 真实写入确认守卫：项目目录内写入、危险路径拦截和确认短语
- 项目级 MCP 写入目标：`.mcp.json`、`.codex/config.toml`、`opencode.json`、`.pi/mcp.json`
- 当前真实写入会生成完整配置文件；结构化合并将在后续阶段加入
