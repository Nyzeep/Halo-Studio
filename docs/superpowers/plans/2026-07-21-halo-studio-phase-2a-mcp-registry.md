# Halo Studio Phase 2A MCP 注册中心实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 添加安全的 MCP 注册中心基础能力：标准 MCP Server 模型、四个 Agent 的配置预览生成、IPC 接口、右侧配置面板 UI。

**Architecture:** `src/shared/mcp.ts` 定义跨进程数据结构；`src/main/mcp/configPreview.ts` 只生成配置预览，不写真实用户文件；Electron IPC 暴露 `mcp.previewConfig()`；React 右侧面板展示示例 MCP Server、目标 Agent 和生成的配置片段。真实备份、原子写入和回滚放入 Phase 2B。

**Tech Stack:** TypeScript、Vitest、Electron IPC、React、Tailwind CSS、lucide-react。

---

## 文件结构

- Create: `D:\Halo Studio\src\shared\mcp.ts`：标准 MCP Server 类型、目标 Agent 类型、预览结果类型。
- Create: `D:\Halo Studio\src\main\mcp\configPreview.ts`：四个 Agent 的配置预览生成器。
- Create: `D:\Halo Studio\src\tests\mcpPreview.test.ts`：MCP 预览生成测试。
- Modify: `D:\Halo Studio\src\shared\api.ts`：增加 MCP 预览 API。
- Modify: `D:\Halo Studio\src\main\ipc.ts`：注册 MCP IPC handler。
- Modify: `D:\Halo Studio\src\main\preload.ts`：暴露 MCP API 给 renderer。
- Create: `D:\Halo Studio\src\renderer\hooks\useMcpPreview.ts`：获取 MCP 配置预览。
- Create: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`：MCP 预览 UI。
- Modify: `D:\Halo Studio\src\renderer\components\InspectorPanel.tsx`：把 MCP 占位区替换为真实预览面板。
- Modify: `D:\Halo Studio\README.md`：补充当前 MCP 预览能力说明。

---

### Task 1: MCP 类型与预览测试

**Files:**

- Create: `D:\Halo Studio\src\shared\mcp.ts`
- Create: `D:\Halo Studio\src\tests\mcpPreview.test.ts`

- [ ] **Step 1: 写失败测试**

`D:\Halo Studio\src\tests\mcpPreview.test.ts`：

```ts
import { describe, expect, it } from "vitest";
import { createMcpConfigPreviews } from "../main/mcp/configPreview";
import type { McpServerConfig } from "../shared/mcp";

const filesystemServer: McpServerConfig = {
  id: "filesystem",
  displayName: "Filesystem",
  transport: "stdio",
  command: "npx",
  args: ["-y", "@modelcontextprotocol/server-filesystem", "D:\\Halo Studio"],
  env: {
    HALO_SCOPE: "workspace"
  },
  enabled: true,
  targetAgents: ["claude-code", "codex-cli", "opencode", "pi"]
};

describe("MCP config preview", () => {
  it("generates a Codex TOML preview", () => {
    const previews = createMcpConfigPreviews(filesystemServer);
    const codex = previews.find((preview) => preview.agentId === "codex-cli");

    expect(codex?.targetPath).toBe("~/.codex/config.toml");
    expect(codex?.language).toBe("toml");
    expect(codex?.content).toContain("[mcp_servers.filesystem]");
    expect(codex?.content).toContain('command = "npx"');
  });

  it("generates JSON previews for Claude, OpenCode, and Pi", () => {
    const previews = createMcpConfigPreviews(filesystemServer);

    expect(previews).toHaveLength(4);
    expect(previews.find((preview) => preview.agentId === "claude-code")?.content).toContain("\"filesystem\"");
    expect(previews.find((preview) => preview.agentId === "opencode")?.content).toContain("\"mcp\"");
    expect(previews.find((preview) => preview.agentId === "pi")?.targetPath).toBe("~/.pi/mcp.json");
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/tests/mcpPreview.test.ts`

Expected: FAIL，错误包含 `Cannot find module '../main/mcp/configPreview'`。

---

### Task 2: MCP 配置预览生成器

**Files:**

- Create: `D:\Halo Studio\src\shared\mcp.ts`
- Create: `D:\Halo Studio\src\main\mcp\configPreview.ts`
- Modify: `D:\Halo Studio\src\tests\mcpPreview.test.ts`

- [ ] **Step 1: 写 MCP 共享类型**

`D:\Halo Studio\src\shared\mcp.ts`：

```ts
import type { AgentId } from "./agents.js";

export type McpTransport = "stdio" | "sse" | "http";

export interface McpServerConfig {
  id: string;
  displayName: string;
  transport: McpTransport;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  url?: string;
  headers?: Record<string, string>;
  enabled: boolean;
  targetAgents: AgentId[];
}

export interface McpConfigPreview {
  agentId: AgentId;
  agentName: string;
  targetPath: string;
  language: "json" | "jsonc" | "toml";
  content: string;
}
```

- [ ] **Step 2: 写四个 Agent 的预览生成器**

`D:\Halo Studio\src\main\mcp\configPreview.ts`：

```ts
import type { AgentId } from "../../shared/agents.js";
import type { McpConfigPreview, McpServerConfig } from "../../shared/mcp.js";

const agentNames: Record<AgentId, string> = {
  "claude-code": "Claude Code",
  "codex-cli": "Codex CLI",
  opencode: "OpenCode",
  pi: "Pi"
};

export function createMcpConfigPreviews(server: McpServerConfig): McpConfigPreview[] {
  return server.targetAgents.map((agentId) => {
    switch (agentId) {
      case "codex-cli":
        return createCodexPreview(server);
      case "claude-code":
        return createClaudePreview(server);
      case "opencode":
        return createOpenCodePreview(server);
      case "pi":
        return createPiPreview(server);
    }
  });
}

function createCodexPreview(server: McpServerConfig): McpConfigPreview {
  const lines = [
    `[mcp_servers.${server.id}]`,
    `command = ${toTomlString(server.command ?? "")}`,
    `args = ${toTomlArray(server.args ?? [])}`
  ];

  if (server.env && Object.keys(server.env).length > 0) {
    lines.push(`[mcp_servers.${server.id}.env]`);
    for (const [key, value] of Object.entries(server.env)) {
      lines.push(`${key} = ${toTomlString(value)}`);
    }
  }

  return {
    agentId: "codex-cli",
    agentName: agentNames["codex-cli"],
    targetPath: "~/.codex/config.toml",
    language: "toml",
    content: lines.join("\n")
  };
}

function createClaudePreview(server: McpServerConfig): McpConfigPreview {
  return {
    agentId: "claude-code",
    agentName: agentNames["claude-code"],
    targetPath: ".mcp.json",
    language: "json",
    content: stringifyJson({
      mcpServers: {
        [server.id]: createJsonServerConfig(server)
      }
    })
  };
}

function createOpenCodePreview(server: McpServerConfig): McpConfigPreview {
  return {
    agentId: "opencode",
    agentName: agentNames.opencode,
    targetPath: "opencode.json",
    language: "jsonc",
    content: stringifyJson({
      mcp: {
        [server.id]: createJsonServerConfig(server)
      }
    })
  };
}

function createPiPreview(server: McpServerConfig): McpConfigPreview {
  return {
    agentId: "pi",
    agentName: agentNames.pi,
    targetPath: "~/.pi/mcp.json",
    language: "json",
    content: stringifyJson({
      mcpServers: {
        [server.id]: createJsonServerConfig(server)
      }
    })
  };
}

function createJsonServerConfig(server: McpServerConfig) {
  if (server.transport === "stdio") {
    return {
      command: server.command ?? "",
      args: server.args ?? [],
      env: server.env ?? {}
    };
  }

  return {
    type: server.transport,
    url: server.url ?? "",
    headers: server.headers ?? {}
  };
}

function stringifyJson(value: unknown) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function toTomlArray(values: string[]) {
  return `[${values.map(toTomlString).join(", ")}]`;
}

function toTomlString(value: string) {
  return JSON.stringify(value);
}
```

- [ ] **Step 3: 运行测试确认通过**

Run: `npm test -- src/tests/mcpPreview.test.ts`

Expected: PASS，输出包含 `2 passed`。

- [ ] **Step 4: 提交 MCP 预览生成器**

```bash
git add src/shared/mcp.ts src/main/mcp/configPreview.ts src/tests/mcpPreview.test.ts
git commit -m "功能：添加 MCP 配置预览生成器"
```

---

### Task 3: IPC 和前端 MCP 面板

**Files:**

- Modify: `D:\Halo Studio\src\shared\api.ts`
- Modify: `D:\Halo Studio\src\main\ipc.ts`
- Modify: `D:\Halo Studio\src\main\preload.ts`
- Create: `D:\Halo Studio\src\renderer\hooks\useMcpPreview.ts`
- Create: `D:\Halo Studio\src\renderer\components\McpPreviewPanel.tsx`
- Modify: `D:\Halo Studio\src\renderer\components\InspectorPanel.tsx`

- [ ] **Step 1: 扩展 API**

在 `HaloApi` 中增加：

```ts
mcp: {
  previewConfig(server: McpServerConfig): Promise<McpConfigPreview[]>;
};
```

- [ ] **Step 2: 注册 IPC**

在 `registerIpcHandlers` 中增加：

```ts
ipcMain.handle("mcp:previewConfig", (_event, server) => createMcpConfigPreviews(server));
```

- [ ] **Step 3: 暴露 preload API**

在 `api` 中增加：

```ts
mcp: {
  previewConfig: (server) => ipcRenderer.invoke("mcp:previewConfig", server)
}
```

- [ ] **Step 4: 实现 `useMcpPreview` 和 `McpPreviewPanel`**

面板默认使用 filesystem MCP 示例，不写真实文件，只展示每个 Agent 会生成什么配置。

- [ ] **Step 5: 构建验证**

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 6: 提交 UI 接入**

```bash
git add src/shared/api.ts src/main/ipc.ts src/main/preload.ts src/renderer/hooks/useMcpPreview.ts src/renderer/components/McpPreviewPanel.tsx src/renderer/components/InspectorPanel.tsx
git commit -m "界面：接入 MCP 配置预览面板"
```

---

### Task 4: README 和最终验证

**Files:**

- Modify: `D:\Halo Studio\README.md`

- [ ] **Step 1: 更新 README**

在当前阶段列表中增加：

```md
- MCP 配置预览，不写入真实配置文件
```

- [ ] **Step 2: 最终验证**

Run: `npm test`

Expected: PASS。

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 3: 提交 README**

```bash
git add README.md
git commit -m "文档：补充 MCP 预览说明"
```

## 自检清单

- MCP 只做预览，不写真实配置文件。
- Codex、Claude Code、OpenCode、Pi 都有预览输出。
- 前端从 IPC 获取预览，不在 renderer 里复制生成逻辑。
- 测试覆盖 TOML 和 JSON/JSONC 预览。
- 中文提交信息贯穿本阶段。
