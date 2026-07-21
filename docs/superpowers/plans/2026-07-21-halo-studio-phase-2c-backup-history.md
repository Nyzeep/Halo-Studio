# Halo Studio Phase 2C 备份历史与目标确认实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在安全配置写入链路上增加备份历史列表和更清晰的目标确认信息，让用户能看到每次演示写入的目标、备份文件和恢复入口。

**Architecture:** 主进程 `configFileService` 增加 `listConfigBackups()`，只读取目标文件旁边的 `.halo-backups` 目录；IPC 增加 demo-safe 的 `listDemoBackups()`，仍由主进程把前端目标映射到 Electron `userData/preview-configs`；MCP 面板展示当前预览对应的安全演示目标和备份历史。

**Tech Stack:** TypeScript、Node fs/promises、Electron IPC、React、Vitest。

---

## 文件结构

- Modify: `D:\Halo Studio\src\shared\config.ts`：增加备份历史类型。
- Modify: `D:\Halo Studio\src\main\config\configFileService.ts`：增加备份列表读取。
- Modify: `D:\Halo Studio\src\tests\configFileService.test.ts`：增加备份历史排序测试。
- Modify: `D:\Halo Studio\src\shared\api.ts`：增加 `listDemoBackups()` API。
- Modify: `D:\Halo Studio\src\main\ipc.ts`：复用 demo 路径映射并注册备份历史 IPC。
- Modify: `D:\Halo Studio\src\main\preload.ts`：暴露备份历史 API。
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`：展示安全演示目标、备份历史和从历史恢复。
- Modify: `D:\Halo Studio\README.md`：说明备份历史能力。

---

### Task 1: 服务层备份历史

**Files:**

- Modify: `D:\Halo Studio\src\shared\config.ts`
- Modify: `D:\Halo Studio\src\main\config\configFileService.ts`
- Modify: `D:\Halo Studio\src\tests\configFileService.test.ts`

- [ ] **Step 1: 写失败测试**

在 `configFileService.test.ts` 增加测试：连续写入两次同一目标文件后，`listConfigBackups(targetPath)` 返回两个备份，并且按 `createdAt` 倒序排列。

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/tests/configFileService.test.ts`

Expected: FAIL，错误包含 `listConfigBackups` 未导出。

- [ ] **Step 3: 实现类型和服务**

在 `shared/config.ts` 增加：

```ts
export interface ConfigBackupEntry {
  targetPath: string;
  backupPath: string;
  size: number;
  createdAt: string;
}
```

在 `configFileService.ts` 增加 `listConfigBackups(targetPath: string): Promise<ConfigBackupEntry[]>`，读取 `.halo-backups` 目录，筛选当前文件名前缀的 `.bak` 文件，按 `createdAt` 倒序返回。

- [ ] **Step 4: 运行测试确认通过**

Run: `npm test -- src/tests/configFileService.test.ts`

Expected: PASS。

- [ ] **Step 5: 提交服务层**

```bash
git add src/shared/config.ts src/main/config/configFileService.ts src/tests/configFileService.test.ts
git commit -m "功能：添加配置备份历史读取"
```

---

### Task 2: IPC 和 UI 备份历史

**Files:**

- Modify: `D:\Halo Studio\src\shared\api.ts`
- Modify: `D:\Halo Studio\src\main\ipc.ts`
- Modify: `D:\Halo Studio\src\main\preload.ts`
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`

- [ ] **Step 1: 扩展 API**

`HaloApi.config` 增加：

```ts
listDemoBackups(targetPath: string): Promise<ConfigBackupEntry[]>;
```

- [ ] **Step 2: 注册 IPC**

把 demo 目标路径映射抽成 `resolveDemoTargetPath(userDataPath, requestedTargetPath)`，`applyDemoWrite` 和 `listDemoBackups` 都使用它。

- [ ] **Step 3: 更新 MCP 面板**

显示当前“安全演示目标”路径；写入后刷新备份历史；历史列表里每条记录显示时间、大小和恢复按钮。

- [ ] **Step 4: 构建验证**

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 5: 提交 UI 接入**

```bash
git add src/shared/api.ts src/main/ipc.ts src/main/preload.ts src/renderer/components/McpPreviewPanel.tsx
git commit -m "界面：显示配置备份历史"
```

---

### Task 3: README 和最终验证

**Files:**

- Modify: `D:\Halo Studio\README.md`

- [ ] **Step 1: 更新 README**

当前阶段列表增加：

```md
- 配置备份历史列表和历史恢复入口
```

- [ ] **Step 2: 最终验证**

Run: `npm test`

Expected: PASS。

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 3: 提交 README**

```bash
git add README.md
git commit -m "文档：补充配置备份历史说明"
```

## 自检清单

- 备份历史只读 `.halo-backups`。
- UI 仍只操作 demo-safe 路径，不开放真实配置文件写入。
- 备份历史恢复复用已有 rollback IPC。
- 测试覆盖多次写入后的备份列表。
