# Halo Studio Phase 2B 安全配置写入实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为配置中心加入可测试的安全写入链路：diff 预览、备份、原子写入和回滚，并在 UI 中提供只写入 Halo 演示目录的安全试运行按钮。

**Architecture:** `src/main/config` 提供主进程文件写入服务，负责读取旧内容、生成 diff、写备份、原子替换和回滚；`src/shared/config.ts` 定义 IPC 类型；renderer 只通过 IPC 触发写入演示，不能直接写文件。Phase 2B 不写真实厂商配置文件，只把 MCP 预览内容写入 Electron `userData/preview-configs` 下的演示文件。

**Tech Stack:** TypeScript、Node fs/promises、Electron app data path、Vitest、React、Tailwind CSS。

---

## 文件结构

- Create: `D:\Halo Studio\src\shared\config.ts`：配置写入请求、结果和回滚类型。
- Create: `D:\Halo Studio\src\main\config\diff.ts`：轻量 unified diff 生成器。
- Create: `D:\Halo Studio\src\main\config\configFileService.ts`：安全写入和回滚服务。
- Create: `D:\Halo Studio\src\tests\configFileService.test.ts`：备份、原子写入、回滚测试。
- Modify: `D:\Halo Studio\src\shared\api.ts`：加入 `config.applyDemoWrite` 和 `config.rollbackWrite`。
- Modify: `D:\Halo Studio\src\main\ipc.ts`：注册配置写入 IPC。
- Modify: `D:\Halo Studio\src\main\preload.ts`：暴露配置写入 API。
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`：增加“写入演示文件”和“回滚”按钮。
- Modify: `D:\Halo Studio\README.md`：说明当前写入只进入 Halo 演示目录。

---

### Task 1: 配置写入服务测试

**Files:**

- Create: `D:\Halo Studio\src\tests\configFileService.test.ts`

- [ ] **Step 1: 写失败测试**

`D:\Halo Studio\src\tests\configFileService.test.ts`：

```ts
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { applyConfigWrite, rollbackConfigWrite } from "../main/config/configFileService";

let tempDir: string;

beforeEach(async () => {
  tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "halo-config-test-"));
});

afterEach(async () => {
  await fs.rm(tempDir, { recursive: true, force: true });
});

describe("config file service", () => {
  it("creates a backup, writes atomically, and returns a diff", async () => {
    const targetPath = path.join(tempDir, "config.toml");
    await fs.writeFile(targetPath, "model = \"old\"\n", "utf8");

    const result = await applyConfigWrite({
      targetPath,
      nextContent: "model = \"new\"\n",
      reason: "测试写入"
    });

    await expect(fs.readFile(targetPath, "utf8")).resolves.toBe("model = \"new\"\n");
    await expect(fs.readFile(result.backupPath, "utf8")).resolves.toBe("model = \"old\"\n");
    expect(result.diff).toContain("-model = \"old\"");
    expect(result.diff).toContain("+model = \"new\"");
  });

  it("rolls back from a backup", async () => {
    const targetPath = path.join(tempDir, "config.json");
    await fs.writeFile(targetPath, "{\"value\":1}\n", "utf8");

    const result = await applyConfigWrite({
      targetPath,
      nextContent: "{\"value\":2}\n",
      reason: "测试回滚"
    });

    const rollback = await rollbackConfigWrite({
      targetPath,
      backupPath: result.backupPath
    });

    await expect(fs.readFile(targetPath, "utf8")).resolves.toBe("{\"value\":1}\n");
    expect(rollback.restored).toBe(true);
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/tests/configFileService.test.ts`

Expected: FAIL，错误包含 `Cannot find module '../main/config/configFileService'`。

---

### Task 2: 安全写入服务实现

**Files:**

- Create: `D:\Halo Studio\src\shared\config.ts`
- Create: `D:\Halo Studio\src\main\config\diff.ts`
- Create: `D:\Halo Studio\src\main\config\configFileService.ts`

- [ ] **Step 1: 写共享类型**

`D:\Halo Studio\src\shared\config.ts`：

```ts
export interface ConfigWriteRequest {
  targetPath: string;
  nextContent: string;
  reason: string;
}

export interface ConfigWriteResult {
  targetPath: string;
  backupPath: string;
  diff: string;
  wroteAt: string;
}

export interface ConfigRollbackRequest {
  targetPath: string;
  backupPath: string;
}

export interface ConfigRollbackResult {
  targetPath: string;
  backupPath: string;
  restored: boolean;
  restoredAt: string;
}
```

- [ ] **Step 2: 写 diff 生成器**

`D:\Halo Studio\src\main\config\diff.ts`：

```ts
export function createUnifiedDiff(oldContent: string, nextContent: string, label = "config") {
  const oldLines = oldContent.split(/\r?\n/);
  const nextLines = nextContent.split(/\r?\n/);
  const maxLength = Math.max(oldLines.length, nextLines.length);
  const lines = [`--- ${label}:current`, `+++ ${label}:next`];

  for (let index = 0; index < maxLength; index += 1) {
    const oldLine = oldLines[index];
    const nextLine = nextLines[index];

    if (oldLine === nextLine) {
      if (oldLine !== undefined && oldLine.length > 0) {
        lines.push(` ${oldLine}`);
      }
      continue;
    }

    if (oldLine !== undefined && oldLine.length > 0) {
      lines.push(`-${oldLine}`);
    }
    if (nextLine !== undefined && nextLine.length > 0) {
      lines.push(`+${nextLine}`);
    }
  }

  return `${lines.join("\n")}\n`;
}
```

- [ ] **Step 3: 写安全写入和回滚服务**

`D:\Halo Studio\src\main\config\configFileService.ts`：

```ts
import fs from "node:fs/promises";
import path from "node:path";
import type {
  ConfigRollbackRequest,
  ConfigRollbackResult,
  ConfigWriteRequest,
  ConfigWriteResult
} from "../../shared/config.js";
import { createUnifiedDiff } from "./diff.js";

export async function applyConfigWrite(request: ConfigWriteRequest): Promise<ConfigWriteResult> {
  const targetPath = path.resolve(request.targetPath);
  const targetDir = path.dirname(targetPath);
  await fs.mkdir(targetDir, { recursive: true });

  const currentContent = await readExistingFile(targetPath);
  const backupDir = path.join(targetDir, ".halo-backups");
  await fs.mkdir(backupDir, { recursive: true });

  const stamp = createStamp();
  const backupPath = path.join(backupDir, `${path.basename(targetPath)}.${stamp}.bak`);
  await fs.writeFile(backupPath, currentContent, "utf8");

  const tempPath = path.join(targetDir, `.${path.basename(targetPath)}.${stamp}.tmp`);
  await fs.writeFile(tempPath, request.nextContent, "utf8");
  await fs.rename(tempPath, targetPath);

  return {
    targetPath,
    backupPath,
    diff: createUnifiedDiff(currentContent, request.nextContent, request.reason),
    wroteAt: new Date().toISOString()
  };
}

export async function rollbackConfigWrite(request: ConfigRollbackRequest): Promise<ConfigRollbackResult> {
  const targetPath = path.resolve(request.targetPath);
  const backupPath = path.resolve(request.backupPath);
  const backupContent = await fs.readFile(backupPath, "utf8");

  const stamp = createStamp();
  const tempPath = path.join(path.dirname(targetPath), `.${path.basename(targetPath)}.${stamp}.rollback.tmp`);
  await fs.writeFile(tempPath, backupContent, "utf8");
  await fs.rename(tempPath, targetPath);

  return {
    targetPath,
    backupPath,
    restored: true,
    restoredAt: new Date().toISOString()
  };
}

async function readExistingFile(targetPath: string) {
  try {
    return await fs.readFile(targetPath, "utf8");
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return "";
    }
    throw error;
  }
}

function createStamp() {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `npm test -- src/tests/configFileService.test.ts`

Expected: PASS，输出包含 `2 passed`。

- [ ] **Step 5: 提交服务层**

```bash
git add src/shared/config.ts src/main/config src/tests/configFileService.test.ts
git commit -m "功能：添加安全配置写入服务"
```

---

### Task 3: IPC 与 MCP 面板写入演示

**Files:**

- Modify: `D:\Halo Studio\src\shared\api.ts`
- Modify: `D:\Halo Studio\src\main\ipc.ts`
- Modify: `D:\Halo Studio\src\main\preload.ts`
- Modify: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`

- [ ] **Step 1: 扩展 API**

在 `HaloApi` 中增加：

```ts
config: {
  applyDemoWrite(request: ConfigWriteRequest): Promise<ConfigWriteResult>;
  rollbackWrite(request: ConfigRollbackRequest): Promise<ConfigRollbackResult>;
};
```

- [ ] **Step 2: 注册 IPC**

主进程用 `app.getPath("userData")` 创建演示目录，把 renderer 传来的 `targetPath` 只作为文件名使用，最终写到 `preview-configs` 目录。

- [ ] **Step 3: 更新 MCP 面板**

在选中预览下方添加：

- “写入演示文件”按钮。
- 最近写入路径。
- diff 结果。
- “回滚演示文件”按钮。

- [ ] **Step 4: 构建验证**

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 5: 提交 UI 接入**

```bash
git add src/shared/api.ts src/main/ipc.ts src/main/preload.ts src/renderer/components/McpPreviewPanel.tsx
git commit -m "界面：添加 MCP 预览安全写入演示"
```

---

### Task 4: README 和最终验证

**Files:**

- Modify: `D:\Halo Studio\README.md`

- [ ] **Step 1: 更新 README**

在当前阶段列表中增加：

```md
- 配置写入演示：diff、备份、原子写入和回滚
```

- [ ] **Step 2: 最终验证**

Run: `npm test`

Expected: PASS。

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 3: 提交 README**

```bash
git add README.md
git commit -m "文档：补充安全配置写入说明"
```

## 自检清单

- 写入服务有备份、diff、原子 rename 和回滚。
- 测试只使用临时目录，不写真实用户配置。
- UI 演示写入只进入 Electron app data 的 `preview-configs` 目录。
- renderer 不直接访问文件系统。
- 本阶段不开放真实厂商配置写入。
