# Halo Studio Phase 0/1 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建 Halo Studio 的第一条可运行纵切：Windows Electron 桌面壳、现代工作台 UI、PTY 终端托管、四个 Agent 的检测与基础启动入口。

**Architecture:** Electron 主进程负责本地权限、Agent 检测、PTY 会话和安全 IPC；React renderer 负责工作台界面、状态展示和用户交互；shared 层保存跨进程类型。第一轮先保证 terminal mode 可用，同时在 UI 中预留 pi-web 启发的会话档案、项目文件和技能/模型入口。MCP、Profile 和 Broker 在后续计划中实现。

**Tech Stack:** Electron、React、TypeScript、Vite、Tailwind CSS、lucide-react、xterm.js、node-pty、Vitest、Testing Library。

---

## 文件结构

第一轮开发创建这些文件：

- `D:\Halo Studio\package.json`：脚本、依赖、Electron/Vite/Vitest 配置入口。
- `D:\Halo Studio\index.html`：renderer HTML 入口。
- `D:\Halo Studio\tsconfig.json`：TypeScript 基础配置。
- `D:\Halo Studio\tsconfig.node.json`：Electron main/preload 与 Vite 配置的 TypeScript 配置。
- `D:\Halo Studio\vite.config.ts`：React renderer 构建配置。
- `D:\Halo Studio\vitest.config.ts`：测试配置。
- `D:\Halo Studio\tailwind.config.ts`：Tailwind 内容扫描和主题配置。
- `D:\Halo Studio\postcss.config.js`：Tailwind/PostCSS 配置。
- `D:\Halo Studio\src\main\main.ts`：Electron 主进程入口。
- `D:\Halo Studio\src\main\preload.ts`：安全暴露 renderer API。
- `D:\Halo Studio\src\main\ipc.ts`：IPC handler 注册。
- `D:\Halo Studio\src\main\agents\types.ts`：主进程 Agent Adapter 类型。
- `D:\Halo Studio\src\main\agents\registry.ts`：Adapter Registry。
- `D:\Halo Studio\src\main\agents\detect.ts`：跨平台命令检测工具，第一版重点支持 Windows。
- `D:\Halo Studio\src\main\agents\adapters.ts`：四个 Agent 的初始 Adapter。
- `D:\Halo Studio\src\main\pty\ptyManager.ts`：PTY 会话管理。
- `D:\Halo Studio\src\shared\api.ts`：renderer 可用 API 类型。
- `D:\Halo Studio\src\shared\agents.ts`：Agent、workspace、session 的共享类型。
- `D:\Halo Studio\src\renderer\main.tsx`：React 入口。
- `D:\Halo Studio\src\renderer\App.tsx`：工作台主界面。
- `D:\Halo Studio\src\renderer\styles.css`：全局样式和 Tailwind 注入。
- `D:\Halo Studio\src\renderer\components\AgentRail.tsx`：左侧 Agent/Workspace/Profile 区。
- `D:\Halo Studio\src\renderer\components\SessionTabs.tsx`：会话标签栏。
- `D:\Halo Studio\src\renderer\components\TerminalPane.tsx`：xterm.js 终端容器。
- `D:\Halo Studio\src\renderer\components\InspectorPanel.tsx`：右侧状态和配置预览面板。
- `D:\Halo Studio\src\renderer\components\UtilityStrip.tsx`：会话档案、项目文件、技能和模型入口。
- `D:\Halo Studio\src\renderer\components\StatusBar.tsx`：底部状态栏。
- `D:\Halo Studio\src\renderer\hooks\useAgents.ts`：Agent 检测状态 hook。
- `D:\Halo Studio\src\renderer\hooks\useTerminalSession.ts`：终端会话 hook。
- `D:\Halo Studio\src\tests\agents.test.ts`：Agent Registry 与检测逻辑测试。
- `D:\Halo Studio\src\tests\setup.ts`：测试环境配置。

---

### Task 1: 项目脚手架

**Files:**

- Create: `D:\Halo Studio\package.json`
- Create: `D:\Halo Studio\index.html`
- Create: `D:\Halo Studio\tsconfig.json`
- Create: `D:\Halo Studio\tsconfig.node.json`
- Create: `D:\Halo Studio\vite.config.ts`
- Create: `D:\Halo Studio\vitest.config.ts`
- Create: `D:\Halo Studio\tailwind.config.ts`
- Create: `D:\Halo Studio\postcss.config.js`

- [ ] **Step 1: 写入 `package.json`**

```json
{
  "name": "halo-studio",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "main": "dist/main/main.js",
  "scripts": {
    "dev": "vite --host 127.0.0.1",
    "dev:electron": "concurrently -k \"npm:dev\" \"wait-on tcp:5173 && cross-env VITE_DEV_SERVER_URL=http://127.0.0.1:5173 electron .\"",
    "build": "tsc -p tsconfig.json && tsc -p tsconfig.node.json && vite build",
    "test": "vitest run",
    "lint": "eslint . --ext .ts,.tsx",
    "preview": "vite preview --host 127.0.0.1"
  },
  "dependencies": {
    "@vitejs/plugin-react": "^4.3.4",
    "electron": "^33.2.1",
    "lucide-react": "^0.468.0",
    "node-pty": "^1.0.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "xterm": "^5.3.0",
    "xterm-addon-fit": "^0.8.0"
  },
  "devDependencies": {
    "@testing-library/jest-dom": "^6.6.3",
    "@testing-library/react": "^16.1.0",
    "@types/node": "^22.10.2",
    "@types/react": "^18.3.18",
    "@types/react-dom": "^18.3.5",
    "@typescript-eslint/eslint-plugin": "^8.18.2",
    "@typescript-eslint/parser": "^8.18.2",
    "autoprefixer": "^10.4.20",
    "concurrently": "^9.1.0",
    "cross-env": "^7.0.3",
    "eslint": "^9.17.0",
    "eslint-plugin-react-hooks": "^5.1.0",
    "jsdom": "^25.0.1",
    "postcss": "^8.4.49",
    "tailwindcss": "^3.4.17",
    "typescript": "^5.7.2",
    "vite": "^6.0.5",
    "vitest": "^2.1.8",
    "wait-on": "^8.0.1"
  }
}
```

- [ ] **Step 2: 写入基础 TypeScript 和 Vite 配置**

`D:\Halo Studio\tsconfig.json`：

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["DOM", "DOM.Iterable", "ES2022"],
    "allowJs": false,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "forceConsistentCasingInFileNames": true,
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx"
  },
  "include": ["src/renderer", "src/shared", "src/tests", "vitest.config.ts"]
}
```

`D:\Halo Studio\tsconfig.node.json`：

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "outDir": "dist",
    "rootDir": ".",
    "strict": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "types": ["node"]
  },
  "include": ["src/main/**/*.ts", "src/shared/**/*.ts", "vite.config.ts"]
}
```

`D:\Halo Studio\vite.config.ts`：

```ts
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  root: ".",
  build: {
    outDir: "dist/renderer",
    emptyOutDir: false
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true
  }
});
```

`D:\Halo Studio\vitest.config.ts`：

```ts
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["src/tests/setup.ts"],
    include: ["src/tests/**/*.test.ts", "src/tests/**/*.test.tsx"]
  }
});
```

- [ ] **Step 3: 写入 Tailwind 和 HTML 入口**

`D:\Halo Studio\tailwind.config.ts`：

```ts
import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/renderer/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        halo: {
          bg: "#0b0f14",
          panel: "#111820",
          panelSoft: "#151f2a",
          line: "#263241",
          cyan: "#22d3ee",
          amber: "#f59e0b",
          green: "#22c55e",
          red: "#ef4444"
        }
      }
    }
  },
  plugins: []
} satisfies Config;
```

`D:\Halo Studio\postcss.config.js`：

```js
export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {}
  }
};
```

`D:\Halo Studio\index.html`：

```html
<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Halo Studio</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/renderer/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 4: 安装依赖**

Run: `npm install`

Expected: exit code `0`，生成 `D:\Halo Studio\package-lock.json` 和 `D:\Halo Studio\node_modules`。

- [ ] **Step 5: 提交脚手架**

```bash
git add package.json package-lock.json index.html tsconfig.json tsconfig.node.json vite.config.ts vitest.config.ts tailwind.config.ts postcss.config.js
git commit -m "工程：初始化桌面应用脚手架"
```

---

### Task 2: 共享类型与 Agent Adapter Registry

**Files:**

- Create: `D:\Halo Studio\src\shared\agents.ts`
- Create: `D:\Halo Studio\src\main\agents\types.ts`
- Create: `D:\Halo Studio\src\main\agents\detect.ts`
- Create: `D:\Halo Studio\src\main\agents\adapters.ts`
- Create: `D:\Halo Studio\src\main\agents\registry.ts`
- Create: `D:\Halo Studio\src\tests\setup.ts`
- Create: `D:\Halo Studio\src\tests\agents.test.ts`

- [ ] **Step 1: 写失败测试**

`D:\Halo Studio\src\tests\setup.ts`：

```ts
import "@testing-library/jest-dom/vitest";
```

`D:\Halo Studio\src\tests\agents.test.ts`：

```ts
import { describe, expect, it } from "vitest";
import { createAgentRegistry } from "../main/agents/registry";

describe("Agent Registry", () => {
  it("registers the four supported agents", () => {
    const registry = createAgentRegistry();

    expect(registry.list().map((agent) => agent.id)).toEqual([
      "claude-code",
      "codex-cli",
      "opencode",
      "pi"
    ]);
  });

  it("reports a clear missing state when commands are not found", async () => {
    const registry = createAgentRegistry({
      commandExists: async () => false,
      readVersion: async () => null
    });

    const agents = await registry.detectAll();

    expect(agents).toHaveLength(4);
    expect(agents.every((agent) => agent.status === "missing")).toBe(true);
    expect(agents[0]?.installHint).toContain("未检测到");
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/tests/agents.test.ts`

Expected: FAIL，错误包含 `Cannot find module '../main/agents/registry'`。

- [ ] **Step 3: 写共享类型**

`D:\Halo Studio\src\shared\agents.ts`：

```ts
export type AgentId = "claude-code" | "codex-cli" | "opencode" | "pi";

export type AgentStatus = "ready" | "missing" | "error";

export type AgentIntegrationMode = "terminal" | "rpc" | "mcp" | "config-only";

export interface AgentInfo {
  id: AgentId;
  name: string;
  command: string;
  status: AgentStatus;
  version: string | null;
  installHint: string;
  modes: AgentIntegrationMode[];
}

export interface WorkspaceInfo {
  id: string;
  name: string;
  path: string;
}

export interface TerminalSessionInfo {
  id: string;
  agentId: AgentId;
  title: string;
  cwd: string;
  status: "starting" | "running" | "stopped" | "failed";
  createdAt: string;
}
```

- [ ] **Step 4: 写 Agent 检测实现**

`D:\Halo Studio\src\main\agents\types.ts`：

```ts
import type { AgentId, AgentInfo, AgentIntegrationMode } from "../../shared/agents";

export interface CommandProbe {
  commandExists(command: string): Promise<boolean>;
  readVersion(command: string, args: string[]): Promise<string | null>;
}

export interface AgentAdapterDefinition {
  id: AgentId;
  name: string;
  command: string;
  versionArgs: string[];
  installHint: string;
  modes: AgentIntegrationMode[];
}

export interface AgentAdapter {
  definition: AgentAdapterDefinition;
  detect(probe: CommandProbe): Promise<AgentInfo>;
}
```

`D:\Halo Studio\src\main\agents\detect.ts`：

```ts
import { execFile } from "node:child_process";

export async function commandExists(command: string): Promise<boolean> {
  const locator = process.platform === "win32" ? "where.exe" : "which";

  return new Promise((resolve) => {
    execFile(locator, [command], { windowsHide: true }, (error) => {
      resolve(!error);
    });
  });
}

export async function readVersion(command: string, args: string[]): Promise<string | null> {
  return new Promise((resolve) => {
    execFile(command, args, { timeout: 5000, windowsHide: true }, (error, stdout, stderr) => {
      if (error) {
        resolve(null);
        return;
      }

      const output = `${stdout}${stderr}`.trim();
      resolve(output.length > 0 ? output.split(/\r?\n/)[0] ?? output : null);
    });
  });
}
```

`D:\Halo Studio\src\main\agents\adapters.ts`：

```ts
import type { AgentAdapter, AgentAdapterDefinition, CommandProbe } from "./types";

const definitions: AgentAdapterDefinition[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    command: "claude",
    versionArgs: ["--version"],
    installHint: "未检测到 Claude Code，请先安装并确认 claude 命令在 PATH 中。",
    modes: ["terminal", "mcp", "config-only"]
  },
  {
    id: "codex-cli",
    name: "Codex CLI",
    command: "codex",
    versionArgs: ["--version"],
    installHint: "未检测到 Codex CLI，请先安装并确认 codex 命令在 PATH 中。",
    modes: ["terminal", "mcp", "config-only"]
  },
  {
    id: "opencode",
    name: "OpenCode",
    command: "opencode",
    versionArgs: ["--version"],
    installHint: "未检测到 OpenCode，请先安装并确认 opencode 命令在 PATH 中。",
    modes: ["terminal", "mcp", "config-only"]
  },
  {
    id: "pi",
    name: "Pi",
    command: "pi",
    versionArgs: ["--version"],
    installHint: "未检测到 Pi，请先安装并确认 pi 命令在 PATH 中。",
    modes: ["terminal", "rpc", "mcp", "config-only"]
  }
];

export function createAgentAdapters(): AgentAdapter[] {
  return definitions.map((definition) => ({
    definition,
    async detect(probe: CommandProbe) {
      const exists = await probe.commandExists(definition.command);
      if (!exists) {
        return {
          id: definition.id,
          name: definition.name,
          command: definition.command,
          status: "missing",
          version: null,
          installHint: definition.installHint,
          modes: definition.modes
        };
      }

      return {
        id: definition.id,
        name: definition.name,
        command: definition.command,
        status: "ready",
        version: await probe.readVersion(definition.command, definition.versionArgs),
        installHint: "",
        modes: definition.modes
      };
    }
  }));
}
```

`D:\Halo Studio\src\main\agents\registry.ts`：

```ts
import type { AgentInfo } from "../../shared/agents";
import { createAgentAdapters } from "./adapters";
import { commandExists, readVersion } from "./detect";
import type { AgentAdapter, CommandProbe } from "./types";

export class AgentRegistry {
  constructor(
    private readonly adapters: AgentAdapter[],
    private readonly probe: CommandProbe
  ) {}

  list(): AgentInfo[] {
    return this.adapters.map(({ definition }) => ({
      id: definition.id,
      name: definition.name,
      command: definition.command,
      status: "missing",
      version: null,
      installHint: definition.installHint,
      modes: definition.modes
    }));
  }

  async detectAll(): Promise<AgentInfo[]> {
    return Promise.all(this.adapters.map((adapter) => adapter.detect(this.probe)));
  }
}

export function createAgentRegistry(probe: CommandProbe = { commandExists, readVersion }) {
  return new AgentRegistry(createAgentAdapters(), probe);
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `npm test -- src/tests/agents.test.ts`

Expected: PASS，输出包含 `2 passed` 或等价的 Vitest 通过信息。

- [ ] **Step 6: 提交 Agent Registry**

```bash
git add src/shared/agents.ts src/main/agents src/tests vitest.config.ts
git commit -m "功能：添加 Agent 检测注册表"
```

---

### Task 3: Electron 主进程、Preload 和 IPC

**Files:**

- Create: `D:\Halo Studio\src\shared\api.ts`
- Create: `D:\Halo Studio\src\main\ipc.ts`
- Create: `D:\Halo Studio\src\main\main.ts`
- Create: `D:\Halo Studio\src\main\preload.ts`

- [ ] **Step 1: 写 renderer API 类型**

`D:\Halo Studio\src\shared\api.ts`：

```ts
import type { AgentInfo, AgentId, TerminalSessionInfo } from "./agents";

export interface StartSessionRequest {
  agentId: AgentId;
  cwd: string;
}

export interface HaloApi {
  agents: {
    detectAll(): Promise<AgentInfo[]>;
  };
  sessions: {
    start(request: StartSessionRequest): Promise<TerminalSessionInfo>;
    stop(sessionId: string): Promise<void>;
    write(sessionId: string, data: string): Promise<void>;
    resize(sessionId: string, cols: number, rows: number): Promise<void>;
    onData(callback: (event: { sessionId: string; data: string }) => void): () => void;
    onExit(callback: (event: { sessionId: string; exitCode: number | null }) => void): () => void;
  };
}

declare global {
  interface Window {
    halo: HaloApi;
  }
}
```

- [ ] **Step 2: 写 Electron 主进程和 IPC**

`D:\Halo Studio\src\main\ipc.ts`：

```ts
import { BrowserWindow, ipcMain } from "electron";
import { createAgentRegistry } from "./agents/registry";
import { PtyManager } from "./pty/ptyManager";

export function registerIpcHandlers(mainWindow: BrowserWindow) {
  const registry = createAgentRegistry();
  const ptyManager = new PtyManager({
    onData: (sessionId, data) => {
      mainWindow.webContents.send("sessions:data", { sessionId, data });
    },
    onExit: (sessionId, exitCode) => {
      mainWindow.webContents.send("sessions:exit", { sessionId, exitCode });
    }
  });

  ipcMain.handle("agents:detectAll", () => registry.detectAll());
  ipcMain.handle("sessions:start", (_event, request) => ptyManager.start(request));
  ipcMain.handle("sessions:stop", (_event, sessionId: string) => ptyManager.stop(sessionId));
  ipcMain.handle("sessions:write", (_event, sessionId: string, data: string) => ptyManager.write(sessionId, data));
  ipcMain.handle("sessions:resize", (_event, sessionId: string, cols: number, rows: number) =>
    ptyManager.resize(sessionId, cols, rows)
  );
}
```

`D:\Halo Studio\src\main\preload.ts`：

```ts
import { contextBridge, ipcRenderer } from "electron";
import type { HaloApi } from "../shared/api";

const api: HaloApi = {
  agents: {
    detectAll: () => ipcRenderer.invoke("agents:detectAll")
  },
  sessions: {
    start: (request) => ipcRenderer.invoke("sessions:start", request),
    stop: (sessionId) => ipcRenderer.invoke("sessions:stop", sessionId),
    write: (sessionId, data) => ipcRenderer.invoke("sessions:write", sessionId, data),
    resize: (sessionId, cols, rows) => ipcRenderer.invoke("sessions:resize", sessionId, cols, rows),
    onData: (callback) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: { sessionId: string; data: string }) => {
        callback(payload);
      };
      ipcRenderer.on("sessions:data", listener);
      return () => ipcRenderer.off("sessions:data", listener);
    },
    onExit: (callback) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: { sessionId: string; exitCode: number | null }) => {
        callback(payload);
      };
      ipcRenderer.on("sessions:exit", listener);
      return () => ipcRenderer.off("sessions:exit", listener);
    }
  }
};

contextBridge.exposeInMainWorld("halo", api);
```

`D:\Halo Studio\src\main\main.ts`：

```ts
import { app, BrowserWindow } from "electron";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { registerIpcHandlers } from "./ipc";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function createWindow() {
  const mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 1100,
    minHeight: 720,
    backgroundColor: "#0b0f14",
    title: "Halo Studio",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });

  registerIpcHandlers(mainWindow);

  if (process.env.VITE_DEV_SERVER_URL) {
    await mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL);
  } else {
    await mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"));
  }
}

app.whenReady().then(createWindow);

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) {
    void createWindow();
  }
});
```

- [ ] **Step 3: 提交 Electron IPC**

```bash
git add src/shared/api.ts src/main/ipc.ts src/main/main.ts src/main/preload.ts
git commit -m "功能：添加 Electron 主进程和安全 IPC"
```

---

### Task 4: PTY 会话管理

**Files:**

- Create: `D:\Halo Studio\src\main\pty\ptyManager.ts`
- Modify: `D:\Halo Studio\src\main\ipc.ts`
- Modify: `D:\Halo Studio\src\tests\agents.test.ts`

- [ ] **Step 1: 写 PTY Manager 单元测试**

在 `D:\Halo Studio\src\tests\agents.test.ts` 追加：

```ts
import { PtyManager } from "../main/pty/ptyManager";

describe("PTY Manager", () => {
  it("starts with no active sessions", () => {
    const manager = new PtyManager({ onData: () => undefined, onExit: () => undefined });
    expect(manager.list()).toEqual([]);
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/tests/agents.test.ts`

Expected: FAIL，错误包含 `Cannot find module '../main/pty/ptyManager'`。

- [ ] **Step 3: 写 PTY Manager**

`D:\Halo Studio\src\main\pty\ptyManager.ts`：

```ts
import os from "node:os";
import path from "node:path";
import { randomUUID } from "node:crypto";
import pty from "node-pty";
import type { AgentId, TerminalSessionInfo } from "../../shared/agents";
import type { StartSessionRequest } from "../../shared/api";

interface PtyManagerEvents {
  onData(sessionId: string, data: string): void;
  onExit(sessionId: string, exitCode: number | null): void;
}

interface SessionRecord {
  info: TerminalSessionInfo;
  process: pty.IPty;
}

const commandByAgent: Record<AgentId, string> = {
  "claude-code": "claude",
  "codex-cli": "codex",
  opencode: "opencode",
  pi: "pi"
};

export class PtyManager {
  private readonly sessions = new Map<string, SessionRecord>();

  constructor(private readonly events: PtyManagerEvents) {}

  list(): TerminalSessionInfo[] {
    return Array.from(this.sessions.values()).map((session) => session.info);
  }

  async start(request: StartSessionRequest): Promise<TerminalSessionInfo> {
    const sessionId = randomUUID();
    const shell = commandByAgent[request.agentId];
    const cwd = path.resolve(request.cwd || os.homedir());

    const child = pty.spawn(shell, [], {
      name: "xterm-256color",
      cols: 100,
      rows: 32,
      cwd,
      env: { ...process.env }
    });

    const info: TerminalSessionInfo = {
      id: sessionId,
      agentId: request.agentId,
      title: shell,
      cwd,
      status: "running",
      createdAt: new Date().toISOString()
    };

    this.sessions.set(sessionId, { info, process: child });

    child.onData((data) => this.events.onData(sessionId, data));
    child.onExit(({ exitCode }) => {
      this.sessions.delete(sessionId);
      this.events.onExit(sessionId, exitCode);
    });

    return info;
  }

  write(sessionId: string, data: string): void {
    this.sessions.get(sessionId)?.process.write(data);
  }

  resize(sessionId: string, cols: number, rows: number): void {
    this.sessions.get(sessionId)?.process.resize(cols, rows);
  }

  stop(sessionId: string): void {
    this.sessions.get(sessionId)?.process.kill();
    this.sessions.delete(sessionId);
  }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `npm test -- src/tests/agents.test.ts`

Expected: PASS。

- [ ] **Step 5: 提交 PTY Manager**

```bash
git add package.json package-lock.json src/main/pty src/main/ipc.ts src/tests/agents.test.ts
git commit -m "功能：添加 PTY 会话管理"
```

---

### Task 5: Renderer 工作台 UI

**Files:**

- Create: `D:\Halo Studio\src\renderer\main.tsx`
- Create: `D:\Halo Studio\src\renderer\App.tsx`
- Create: `D:\Halo Studio\src\renderer\styles.css`
- Create: `D:\Halo Studio\src\renderer\components\AgentRail.tsx`
- Create: `D:\Halo Studio\src\renderer\components\SessionTabs.tsx`
- Create: `D:\Halo Studio\src\renderer\components\TerminalPane.tsx`
- Create: `D:\Halo Studio\src\renderer\components\InspectorPanel.tsx`
- Create: `D:\Halo Studio\src\renderer\components\UtilityStrip.tsx`
- Create: `D:\Halo Studio\src\renderer\components\StatusBar.tsx`
- Create: `D:\Halo Studio\src\renderer\hooks\useAgents.ts`
- Create: `D:\Halo Studio\src\renderer\hooks\useTerminalSession.ts`

- [ ] **Step 1: 写 React 入口和全局样式**

`D:\Halo Studio\src\renderer\main.tsx`：

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

`D:\Halo Studio\src\renderer\styles.css`：

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

* {
  box-sizing: border-box;
}

html,
body,
#root {
  width: 100%;
  height: 100%;
  margin: 0;
}

body {
  background: #0b0f14;
  color: #f8fafc;
  font-family:
    Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI",
    sans-serif;
}
```

- [ ] **Step 2: 写 hooks**

`D:\Halo Studio\src\renderer\hooks\useAgents.ts`：

```ts
import { useEffect, useState } from "react";
import type { AgentInfo } from "../../shared/agents";

export function useAgents() {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let active = true;
    window.halo.agents.detectAll().then((result) => {
      if (active) {
        setAgents(result);
        setLoading(false);
      }
    });
    return () => {
      active = false;
    };
  }, []);

  return { agents, loading };
}
```

`D:\Halo Studio\src\renderer\hooks\useTerminalSession.ts`：

```ts
import { useEffect, useMemo, useState } from "react";
import type { AgentId, TerminalSessionInfo } from "../../shared/agents";

export function useTerminalSession() {
  const [sessions, setSessions] = useState<TerminalSessionInfo[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);

  useEffect(() => {
    return window.halo.sessions.onExit(({ sessionId }) => {
      setSessions((current) => current.filter((session) => session.id !== sessionId));
      setActiveSessionId((current) => (current === sessionId ? null : current));
    });
  }, []);

  const activeSession = useMemo(
    () => sessions.find((session) => session.id === activeSessionId) ?? null,
    [activeSessionId, sessions]
  );

  async function start(agentId: AgentId, cwd: string) {
    const session = await window.halo.sessions.start({ agentId, cwd });
    setSessions((current) => [...current, session]);
    setActiveSessionId(session.id);
  }

  async function stop(sessionId: string) {
    await window.halo.sessions.stop(sessionId);
    setSessions((current) => current.filter((session) => session.id !== sessionId));
    setActiveSessionId((current) => (current === sessionId ? null : current));
  }

  return {
    sessions,
    activeSession,
    activeSessionId,
    setActiveSessionId,
    start,
    stop
  };
}
```

- [ ] **Step 3: 写 UI 组件**

组件需要呈现真实工作台而不是落地页。左侧 Agent 按 ready/missing 分色，中间显示终端容器，右侧显示当前会话和后续配置中心入口。

`D:\Halo Studio\src\renderer\components\AgentRail.tsx`：

```tsx
import { Bot, FolderOpen, Play, Settings2 } from "lucide-react";
import type { AgentId, AgentInfo } from "../../shared/agents";

interface AgentRailProps {
  agents: AgentInfo[];
  loading: boolean;
  onLaunch(agentId: AgentId): void;
}

export function AgentRail({ agents, loading, onLaunch }: AgentRailProps) {
  return (
    <aside className="flex h-full w-72 flex-col border-r border-halo-line bg-halo-panel">
      <div className="flex h-16 items-center gap-3 border-b border-halo-line px-5">
        <div className="flex h-9 w-9 items-center justify-center rounded bg-halo-cyan/15 text-halo-cyan">
          <Bot size={20} />
        </div>
        <div>
          <div className="text-sm font-semibold">Halo Studio</div>
          <div className="text-xs text-slate-400">多 Agent 工作台</div>
        </div>
      </div>

      <button className="mx-4 mt-4 flex items-center gap-2 rounded border border-halo-line bg-halo-panelSoft px-3 py-2 text-left text-sm text-slate-200">
        <FolderOpen size={16} />
        D:\Halo Studio
      </button>

      <div className="mt-5 px-4 text-xs font-medium uppercase tracking-wide text-slate-500">Agents</div>
      <div className="mt-2 flex-1 space-y-2 px-3">
        {loading ? (
          <div className="rounded border border-halo-line bg-halo-panelSoft px-3 py-3 text-sm text-slate-400">检测中...</div>
        ) : (
          agents.map((agent) => (
            <div key={agent.id} className="rounded border border-halo-line bg-halo-panelSoft p-3">
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-sm font-medium text-slate-100">{agent.name}</div>
                  <div className="mt-1 text-xs text-slate-500">{agent.version ?? agent.command}</div>
                </div>
                <span className={agent.status === "ready" ? "text-halo-green" : "text-halo-amber"}>
                  {agent.status === "ready" ? "Ready" : "Missing"}
                </span>
              </div>
              <button
                className="mt-3 flex w-full items-center justify-center gap-2 rounded bg-halo-cyan px-3 py-2 text-sm font-medium text-slate-950 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
                disabled={agent.status !== "ready"}
                onClick={() => onLaunch(agent.id)}
              >
                <Play size={15} />
                启动
              </button>
            </div>
          ))
        )}
      </div>

      <button className="m-4 flex items-center gap-2 rounded border border-halo-line px-3 py-2 text-sm text-slate-300">
        <Settings2 size={16} />
        配置中心
      </button>
    </aside>
  );
}
```

`D:\Halo Studio\src\renderer\components\SessionTabs.tsx`：

```tsx
import { X } from "lucide-react";
import type { TerminalSessionInfo } from "../../shared/agents";

interface SessionTabsProps {
  sessions: TerminalSessionInfo[];
  activeSessionId: string | null;
  onSelect(sessionId: string): void;
  onClose(sessionId: string): void;
}

export function SessionTabs({ sessions, activeSessionId, onSelect, onClose }: SessionTabsProps) {
  return (
    <div className="flex h-12 items-center gap-2 border-b border-halo-line bg-halo-panel px-3">
      {sessions.length === 0 ? (
        <div className="text-sm text-slate-500">选择左侧 Agent 启动会话</div>
      ) : (
        sessions.map((session) => (
          <button
            key={session.id}
            className={`flex h-8 items-center gap-2 rounded border px-3 text-sm ${
              session.id === activeSessionId
                ? "border-halo-cyan bg-halo-cyan/10 text-halo-cyan"
                : "border-halo-line bg-halo-panelSoft text-slate-300"
            }`}
            onClick={() => onSelect(session.id)}
          >
            {session.title}
            <X
              size={14}
              onClick={(event) => {
                event.stopPropagation();
                onClose(session.id);
              }}
            />
          </button>
        ))
      )}
    </div>
  );
}
```

`D:\Halo Studio\src\renderer\components\TerminalPane.tsx`：

```tsx
import { useEffect, useRef } from "react";
import { Terminal } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import "xterm/css/xterm.css";
import type { TerminalSessionInfo } from "../../shared/agents";

interface TerminalPaneProps {
  session: TerminalSessionInfo | null;
}

export function TerminalPane({ session }: TerminalPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);

  useEffect(() => {
    if (!hostRef.current || !session) {
      return undefined;
    }

    const terminal = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: "Cascadia Mono, Consolas, monospace",
      theme: {
        background: "#0b0f14",
        foreground: "#dbeafe",
        cursor: "#22d3ee"
      }
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(hostRef.current);
    fitAddon.fit();
    terminal.focus();
    terminalRef.current = terminal;

    const disposeData = window.halo.sessions.onData(({ sessionId, data }) => {
      if (sessionId === session.id) {
        terminal.write(data);
      }
    });

    const onData = terminal.onData((data) => {
      void window.halo.sessions.write(session.id, data);
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      void window.halo.sessions.resize(session.id, terminal.cols, terminal.rows);
    });
    resizeObserver.observe(hostRef.current);

    return () => {
      disposeData();
      onData.dispose();
      resizeObserver.disconnect();
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [session]);

  if (!session) {
    return (
      <div className="flex h-full items-center justify-center bg-halo-bg text-sm text-slate-500">
        启动一个 Agent 后，终端会显示在这里。
      </div>
    );
  }

  return <div ref={hostRef} className="h-full w-full overflow-hidden bg-halo-bg p-3" />;
}
```

`D:\Halo Studio\src\renderer\components\InspectorPanel.tsx`：

```tsx
import { Activity, Database, KeyRound, Network } from "lucide-react";
import type { AgentInfo, TerminalSessionInfo } from "../../shared/agents";

interface InspectorPanelProps {
  agents: AgentInfo[];
  activeSession: TerminalSessionInfo | null;
}

export function InspectorPanel({ agents, activeSession }: InspectorPanelProps) {
  const readyCount = agents.filter((agent) => agent.status === "ready").length;

  return (
    <aside className="h-full w-80 border-l border-halo-line bg-halo-panel p-4">
      <section>
        <div className="flex items-center gap-2 text-sm font-semibold text-slate-100">
          <Activity size={16} />
          会话状态
        </div>
        <div className="mt-3 rounded border border-halo-line bg-halo-panelSoft p-3 text-sm text-slate-300">
          {activeSession ? (
            <>
              <div>{activeSession.title}</div>
              <div className="mt-1 text-xs text-slate-500">{activeSession.cwd}</div>
            </>
          ) : (
            "暂无运行会话"
          )}
        </div>
      </section>

      <section className="mt-6 space-y-3">
        <div className="flex items-center gap-2 text-sm font-semibold text-slate-100">
          <Network size={16} />
          MCP
        </div>
        <div className="rounded border border-dashed border-halo-line p-3 text-sm text-slate-500">MCP 注册中心将在下一阶段接入。</div>
      </section>

      <section className="mt-6 grid gap-3">
        <div className="flex items-center gap-2 rounded border border-halo-line bg-halo-panelSoft p-3 text-sm text-slate-300">
          <Database size={16} />
          已检测 Agent：{readyCount}/{agents.length}
        </div>
        <div className="flex items-center gap-2 rounded border border-halo-line bg-halo-panelSoft p-3 text-sm text-slate-300">
          <KeyRound size={16} />
          凭据服务未启用
        </div>
      </section>
    </aside>
  );
}
```

`D:\Halo Studio\src\renderer\components\UtilityStrip.tsx`：

```tsx
import { Boxes, FileSearch, GitBranch, History, SlidersHorizontal } from "lucide-react";

const utilities = [
  { label: "会话档案", description: "按项目浏览历史会话", icon: History },
  { label: "项目文件", description: "预览源码、Markdown 和 diff", icon: FileSearch },
  { label: "模型配置", description: "集中查看 Agent 模型状态", icon: SlidersHorizontal },
  { label: "技能管理", description: "启用指令集和 Agent 技能", icon: Boxes },
  { label: "Worktree", description: "为分支实验预留入口", icon: GitBranch }
];

export function UtilityStrip() {
  return (
    <div className="grid grid-cols-5 gap-2 border-b border-halo-line bg-halo-panel px-3 py-2">
      {utilities.map((item) => (
        <button
          key={item.label}
          className="flex h-14 items-center gap-3 rounded border border-halo-line bg-halo-panelSoft px-3 text-left hover:border-halo-cyan/60"
        >
          <item.icon size={17} className="shrink-0 text-halo-cyan" />
          <span className="min-w-0">
            <span className="block truncate text-sm font-medium text-slate-200">{item.label}</span>
            <span className="block truncate text-xs text-slate-500">{item.description}</span>
          </span>
        </button>
      ))}
    </div>
  );
}
```

`D:\Halo Studio\src\renderer\components\StatusBar.tsx`：

```tsx
import type { TerminalSessionInfo } from "../../shared/agents";

interface StatusBarProps {
  activeSession: TerminalSessionInfo | null;
}

export function StatusBar({ activeSession }: StatusBarProps) {
  return (
    <footer className="flex h-8 items-center justify-between border-t border-halo-line bg-halo-panel px-4 text-xs text-slate-500">
      <span>Halo Studio · Windows Preview</span>
      <span>{activeSession ? `${activeSession.title} · ${activeSession.status}` : "Idle"}</span>
    </footer>
  );
}
```

- [ ] **Step 4: 写 App 组合界面**

`D:\Halo Studio\src\renderer\App.tsx`：

```tsx
import { AgentRail } from "./components/AgentRail";
import { InspectorPanel } from "./components/InspectorPanel";
import { SessionTabs } from "./components/SessionTabs";
import { StatusBar } from "./components/StatusBar";
import { TerminalPane } from "./components/TerminalPane";
import { UtilityStrip } from "./components/UtilityStrip";
import { useAgents } from "./hooks/useAgents";
import { useTerminalSession } from "./hooks/useTerminalSession";

export function App() {
  const { agents, loading } = useAgents();
  const { sessions, activeSession, activeSessionId, setActiveSessionId, start, stop } = useTerminalSession();

  return (
    <div className="flex h-full min-h-0 bg-halo-bg text-slate-100">
      <AgentRail agents={agents} loading={loading} onLaunch={(agentId) => void start(agentId, "D:\\Halo Studio")} />
      <main className="flex min-w-0 flex-1 flex-col">
        <SessionTabs
          sessions={sessions}
          activeSessionId={activeSessionId}
          onSelect={setActiveSessionId}
          onClose={(sessionId) => void stop(sessionId)}
        />
        <UtilityStrip />
        <div className="min-h-0 flex-1">
          <TerminalPane session={activeSession} />
        </div>
        <StatusBar activeSession={activeSession} />
      </main>
      <InspectorPanel agents={agents} activeSession={activeSession} />
    </div>
  );
}
```

- [ ] **Step 5: 构建验证**

Run: `npm run build`

Expected: exit code `0`，生成 `D:\Halo Studio\dist`。

- [ ] **Step 6: 提交工作台 UI**

```bash
git add src/renderer index.html tailwind.config.ts postcss.config.js
git commit -m "界面：添加 Halo Studio 工作台"
```

---

### Task 6: 本地运行与人工验证

**Files:**

- Modify: `D:\Halo Studio\README.md`

- [ ] **Step 1: 更新 README**

`D:\Halo Studio\README.md`：

````md
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
````

- [ ] **Step 2: 启动应用**

Run: `npm run dev:electron`

Expected: Electron 窗口打开，左侧显示四个 Agent 检测状态，中间区域显示空终端提示，右侧显示会话和 MCP 占位状态。

- [ ] **Step 3: 手工验证 Agent 启动**

在 UI 中点击一个本机已安装且状态为 `Ready` 的 Agent。

Expected:

- 中间区域出现终端。
- 终端能显示官方 CLI 输出。
- 键盘输入能送到 CLI。
- 关闭 tab 后进程停止。

- [ ] **Step 4: 最终验证**

Run: `npm test`

Expected: PASS。

Run: `npm run build`

Expected: exit code `0`。

- [ ] **Step 5: 提交 README 和最终验证修正**

```bash
git add README.md
git commit -m "文档：更新本地开发说明"
```

## 自检清单

- Phase 0 的 Electron 壳、PTY、Agent 检测、启动入口都有任务覆盖。
- Phase 1 的工作台、Agent 切换、多标签终端和基础设置入口都有任务覆盖。
- pi-web 启发的会话档案、项目文件、模型配置、技能管理和 Worktree 入口已有 UI 预留，真实数据读取放入后续计划。
- MCP、Profile、Credential、Broker 已明确排到后续阶段，没有混入第一轮实现。
- 所有提交信息使用中文。
- 没有留下占位词或空泛步骤。
