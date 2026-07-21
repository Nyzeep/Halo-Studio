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

当前实现聚焦 Phase 0/1：

- Windows Electron 桌面壳
- Agent 检测
- PTY 终端会话
- 多 Agent 工作台 UI
- 会话档案、项目文件、模型配置、技能管理和 Worktree 入口
- MCP 配置预览，不写入真实配置文件
- 配置写入演示：diff、备份、原子写入和回滚
