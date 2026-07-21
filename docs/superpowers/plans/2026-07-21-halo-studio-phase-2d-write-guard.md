# Halo Studio Phase 2D 真实写入确认守卫实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在开放真实配置写入前，加入目标路径安全判断、风险等级、确认短语和确认后写入接口，确保真实写入只能发生在明确允许的项目目录内。

**Architecture:** 主进程新增 `config/writeGuard.ts`，集中处理路径规范化、项目根目录约束、危险路径拦截和确认短语生成；`configFileService` 仍负责实际备份/写入/回滚；IPC 新增 `planRealWrite` 和 `applyConfirmedWrite`。Renderer 只展示计划和确认状态，不直接访问文件系统。

**Tech Stack:** TypeScript、Node path、Vitest、Electron IPC、React、Tailwind CSS。

---

## 文件结构

- Create: `D:\Halo Studio\src\main\config\writeGuard.ts`：真实写入路径守卫和确认短语校验。
- Create: `D:\Halo Studio\src\tests\writeGuard.test.ts`：项目内允许、项目外拦截、确认短语测试。
- Modify: `D:\Halo Studio\src\shared\config.ts`：增加真实写入计划和确认写入类型。
- Modify: `D:\Halo Studio\src\shared\api.ts`：增加 `planRealWrite` 和 `applyConfirmedWrite`。
- Modify: `D:\Halo Studio\src\main\ipc.ts`：注册真实写入守卫 IPC。
- Modify: `D:\Halo Studio\src\main\preload.ts`：暴露真实写入守卫 API。
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`：显示真实写入计划、确认短语输入和按钮。
- Modify: `D:\Halo Studio\README.md`：说明真实写入守卫仍限制在项目目录内。

---

### Task 1: 写入守卫服务

**Files:**

- Create: `D:\Halo Studio\src\main\config\writeGuard.ts`
- Create: `D:\Halo Studio\src\tests\writeGuard.test.ts`
- Modify: `D:\Halo Studio\src\shared\config.ts`

- [ ] **Step 1: 写失败测试**

`writeGuard.test.ts` 应覆盖：

- `planRealConfigWrite()` 允许目标位于 workspace root 内。
- 目标位于 workspace root 外时返回 `allowed: false`。
- `applyConfirmedConfigWrite()` 在确认短语错误时拒绝写入。

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/tests/writeGuard.test.ts`

Expected: FAIL，错误包含 `Cannot find module '../main/config/writeGuard'`。

- [ ] **Step 3: 实现共享类型和守卫服务**

增加类型：

```ts
export type ConfigWriteRisk = "low" | "blocked";

export interface RealConfigWritePlanRequest {
  workspaceRoot: string;
  targetPath: string;
  nextContent: string;
  reason: string;
}

export interface RealConfigWritePlan {
  workspaceRoot: string;
  targetPath: string;
  normalizedTargetPath: string;
  nextContent: string;
  reason: string;
  allowed: boolean;
  risk: ConfigWriteRisk;
  confirmationPhrase: string;
  warnings: string[];
}

export interface ConfirmedConfigWriteRequest extends RealConfigWritePlanRequest {
  confirmation: string;
}
```

实现规则：

- 目标路径必须位于 `workspaceRoot` 内。
- `node_modules`、`.git`、`dist` 内的目标直接 blocked。
- 确认短语格式为 `APPLY <basename>`。
- 确认失败时抛出错误，不写文件。

- [ ] **Step 4: 运行测试确认通过**

Run: `npm test -- src/tests/writeGuard.test.ts`

Expected: PASS。

- [ ] **Step 5: 提交服务层**

```bash
git add src/shared/config.ts src/main/config/writeGuard.ts src/tests/writeGuard.test.ts
git commit -m "功能：添加真实写入确认守卫"
```

---

### Task 2: IPC 与 MCP 面板确认流程

**Files:**

- Modify: `D:\Halo Studio\src\shared\api.ts`
- Modify: `D:\Halo Studio\src\main\ipc.ts`
- Modify: `D:\Halo Studio\src\main\preload.ts`
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`

- [ ] **Step 1: 扩展 API**

`HaloApi.config` 增加：

```ts
planRealWrite(request: RealConfigWritePlanRequest): Promise<RealConfigWritePlan>;
applyConfirmedWrite(request: ConfirmedConfigWriteRequest): Promise<ConfigWriteResult>;
```

- [ ] **Step 2: 注册 IPC**

注册 `config:planRealWrite` 和 `config:applyConfirmedWrite`，都调用 `writeGuard.ts`。

- [ ] **Step 3: 更新 MCP 面板**

在 MCP 面板显示“项目级真实写入预案”，默认目标为当前工作区下 `.halo-studio/<agent>-mcp-preview.<ext>`；显示风险、警告、确认短语输入；只有确认正确且计划 allowed 时启用写入按钮。

- [ ] **Step 4: 构建验证**

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 5: 提交 UI 接入**

```bash
git add src/shared/api.ts src/main/ipc.ts src/main/preload.ts src/renderer/components/McpPreviewPanel.tsx
git commit -m "界面：添加真实写入确认流程"
```

---

### Task 3: README 和最终验证

**Files:**

- Modify: `D:\Halo Studio\README.md`

- [ ] **Step 1: 更新 README**

增加：

```md
- 真实写入确认守卫：项目目录内写入、危险路径拦截和确认短语
```

- [ ] **Step 2: 最终验证**

Run: `npm test`

Expected: PASS。

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 3: 提交 README**

```bash
git add README.md
git commit -m "文档：补充真实写入守卫说明"
```

## 自检清单

- 真实写入默认只允许 workspace root 内。
- `.git`、`node_modules`、`dist` 被拦截。
- 确认短语错误时不会写文件。
- 真实写入仍复用备份、diff、原子写入和回滚服务。
