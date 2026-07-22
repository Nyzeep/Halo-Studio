# Halo Studio

Halo Studio 正在重构为只面向 Pi 与 OpenCode 的精简桌面工作台。项目以跨平台 Electron 应用为目标，Windows 为首发平台。

## 当前状态

仓库目前处于第一阶段的中间基线：旧实现已经清退，npm workspaces、仓库卫生规则和后续实现边界已经建立。`apps/*` 与 `packages/*` 工作区将在后续任务中逐步加入，当前基线本身不提供可运行的产品界面。

第一阶段只覆盖以下范围：

- 新 Electron/TypeScript 工程骨架与明确的包边界。
- Pi JSONL RPC 和 OpenCode 本地 Server 的运行时探测、版本校验与生命周期接口。
- Workspace、路径规范化、信任状态和最小权限边界。
- 类型化 IPC、SQLite migration、凭据保护和安全配置写入基线。
- 反映真实运行时状态的最小桌面外壳。

完整编辑器、完整聊天与工具交互、`/` 命令执行、调试终端、MCP 与资源管理界面以及云同步尚未交付。

## 环境

- Node.js 20.18 或更高版本
- npm 10.8 或更高版本

安装锁定依赖：

```powershell
npm ci
```

运行测试：

```powershell
npm test
```

执行构建：

```powershell
npm run build
```

执行仓库检查、类型检查、测试和构建：

```powershell
npm run verify
```

## 参考资料边界

`用于参考的几个项目的代码/` 是只读参考资料目录。不得在其中修改文件、安装依赖或执行构建；该目录也不得被 Git 跟踪、提交或包含在发布产物中。引用其中的代码或资源前必须单独核对许可证。

## 设计与计划

- 核心重构规格：`docs/superpowers/specs/2026-07-22-pi-opencode-core-rebuild-design.md`
- 当前实施计划：`docs/superpowers/plans/2026-07-22-pi-opencode-core-rebuild.md`
