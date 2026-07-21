# Halo Studio Phase 2E 项目级真实 MCP 写入实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 MCP 预览从 `.halo-studio` 预览文件推进到项目级真实配置目标：Claude Code `.mcp.json`、Codex `.codex/config.toml`、OpenCode `opencode.json`、Pi `.pi/mcp.json`，并继续复用真实写入确认守卫。

**Architecture:** `src/main/mcp/projectTargets.ts` 负责根据 Agent 和 workspace root 生成项目级目标路径，renderer 不直接拼接真实路径；`writeGuard.ts` 继续做项目内路径、安全目录和确认短语校验；MCP 面板从 `planProjectMcpWrite()` 获取真实写入预案，再通过 `applyConfirmedWrite()` 执行。第一版写入生成的完整 MCP 配置文件，后续 Phase 2F 再做 JSON/TOML 结构化 merge。

**Tech Stack:** TypeScript、Vitest、Electron IPC、React、Node path。

---

## 文件结构

- Create: `D:\Halo Studio\src\main\mcp\projectTargets.ts`：项目级 MCP 目标路径和写入计划。
- Create: `D:\Halo Studio\src\tests\projectMcpTargets.test.ts`：四个 Agent 的目标路径测试。
- Modify: `D:\Halo Studio\src\shared\api.ts`：增加 `planProjectMcpWrite()`。
- Modify: `D:\Halo Studio\src\main\ipc.ts`：注册项目级 MCP 写入计划 IPC。
- Modify: `D:\Halo Studio\src\main\preload.ts`：暴露项目级 MCP 写入计划 API。
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`：真实写入预案改为项目级目标。
- Modify: `D:\Halo Studio\README.md`：说明项目级 MCP 写入确认能力。

---

### Task 1: 项目级 MCP 目标路径服务

**Files:**

- Create: `D:\Halo Studio\src\main\mcp\projectTargets.ts`
- Create: `D:\Halo Studio\src\tests\projectMcpTargets.test.ts`

- [ ] **Step 1: 写失败测试**

测试 `createProjectMcpWritePlan()`：

- Claude Code 目标为 `<workspace>/.mcp.json`
- Codex CLI 目标为 `<workspace>/.codex/config.toml`
- OpenCode 目标为 `<workspace>/opencode.json`
- Pi 目标为 `<workspace>/.pi/mcp.json`

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/tests/projectMcpTargets.test.ts`

Expected: FAIL，错误包含 `Cannot find module '../main/mcp/projectTargets'`。

- [ ] **Step 3: 实现服务**

`createProjectMcpWritePlan(workspaceRoot, preview)` 内部调用 `planRealConfigWrite()`，`nextContent` 使用 preview content，`reason` 使用 `${preview.agentName} 项目 MCP 配置`。

- [ ] **Step 4: 运行测试确认通过**

Run: `npm test -- src/tests/projectMcpTargets.test.ts`

Expected: PASS。

- [ ] **Step 5: 提交服务层**

```bash
git add src/main/mcp/projectTargets.ts src/tests/projectMcpTargets.test.ts
git commit -m "功能：添加项目级 MCP 写入目标"
```

---

### Task 2: IPC 与 MCP 面板接入

**Files:**

- Modify: `D:\Halo Studio\src\shared\api.ts`
- Modify: `D:\Halo Studio\src\main\ipc.ts`
- Modify: `D:\Halo Studio\src\main\preload.ts`
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`

- [ ] **Step 1: 扩展 API**

`HaloApi.mcp` 增加：

```ts
planProjectMcpWrite(workspaceRoot: string, preview: McpConfigPreview): Promise<RealConfigWritePlan>;
```

- [ ] **Step 2: 注册 IPC**

主进程注册 `mcp:planProjectWrite`，调用 `createProjectMcpWritePlan()`。

- [ ] **Step 3: 更新 MCP 面板**

把当前 `.halo-studio` 真实写入预案替换为项目级目标。保留确认短语和风险提示。UI 文案标明“当前写入完整生成文件，结构化合并将在下一阶段加入”。

- [ ] **Step 4: 构建验证**

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 5: 提交 UI 接入**

```bash
git add src/shared/api.ts src/main/ipc.ts src/main/preload.ts src/renderer/components/McpPreviewPanel.tsx
git commit -m "界面：接入项目级 MCP 写入预案"
```

---

### Task 3: README 和最终验证

**Files:**

- Modify: `D:\Halo Studio\README.md`

- [ ] **Step 1: 更新 README**

当前阶段列表增加：

```md
- 项目级 MCP 写入预案：`.mcp.json`、`.codex/config.toml`、`opencode.json`、`.pi/mcp.json`
```

- [ ] **Step 2: 最终验证**

Run: `npm test`

Expected: PASS。

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 3: 提交 README**

```bash
git add README.md
git commit -m "文档：补充项目级 MCP 写入说明"
```

## 自检清单

- 项目级真实写入目标都位于 workspace root 内。
- 写入仍需要确认短语。
- 写入仍复用备份、diff、原子写入和回滚。
- 本阶段不做结构化 merge，README 和 UI 都说明这一点。
