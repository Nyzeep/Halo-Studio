import { describe, expect, it } from "vitest";

import {
  ipcContracts,
  type DataOf,
  type InputOf,
  type IpcChannel,
} from "@halo-studio/contracts";

import {
  registerIpcHandlers,
  type IpcMainPort,
  type IpcServiceMap,
} from "../src/main/ipc/registerIpc.js";

const workspaceId = "a".repeat(64);
const selectionId = "13ebf428-5647-4a32-ae2e-55304b4e3e9f";

const unsupported = {
  supported: false,
  channel: "unavailable",
  restartRequired: false,
  reason: "Not available in this phase.",
} as const;

const runtimeBinding = {
  agentKind: "pi",
  source: "system",
  health: "detected",
  capabilities: {
    sessions: unsupported,
    streamingMessages: unsupported,
    toolEvents: unsupported,
    permissions: unsupported,
    diff: unsupported,
    commands: unsupported,
    mcp: unsupported,
    skills: unsupported,
    prompts: unsupported,
    extensions: unsupported,
    packages: unsupported,
    models: unsupported,
    usage: unsupported,
  },
} as const;

const fixtures = {
  "workspace.pick": [{}, { selectionId, displayPath: "D:\\Workspace" }],
  "workspace.open": [
    { selectionId },
    { id: workspaceId, rootPath: "D:\\Workspace", realPath: "D:\\Workspace", trustState: "untrusted" },
  ],
  "workspace.snapshot": [
    {},
    [{ id: workspaceId, rootPath: "D:\\Workspace", realPath: "D:\\Workspace", trustState: "untrusted" }],
  ],
  "workspace.trust": [
    { workspaceId, trustState: "trusted" },
    { id: workspaceId, rootPath: "D:\\Workspace", realPath: "D:\\Workspace", trustState: "trusted" },
  ],
  "runtime.probe": [{ workspaceId }, [runtimeBinding]],
  "runtime.start": [{ workspaceId, agentKind: "pi" }, runtimeBinding],
  "runtime.stop": [{ workspaceId, agentKind: "pi" }, { ...runtimeBinding, health: "stopped" }],
  "runtime.snapshot": [{ workspaceId }, [runtimeBinding]],
  "config.preview": [
    { targetId: "target-1", operations: [{ op: "set", path: ["model"], value: "test" }] },
    { previewId: "preview-1", targetId: "target-1", fingerprint: "b".repeat(64), unifiedDiff: "diff", restartRequired: ["pi"] },
  ],
  "config.commit": [
    { previewId: "preview-1" },
    { backupId: "backup-1", targetId: "target-1", fingerprint: "b".repeat(64) },
  ],
  "config.rollback": [
    { backupId: "backup-1" },
    { backupId: "backup-1", targetId: "target-1", fingerprint: "c".repeat(64) },
  ],
  "storage.health": [{}, { mode: "read-write", schemaVersion: 1, diagnostics: [] }],
} as const satisfies {
  readonly [K in IpcChannel]: readonly [InputOf<K>, DataOf<K>];
};

class RecordingIpcMain implements IpcMainPort {
  readonly handlers = new Map<string, (event: unknown, raw: unknown) => Promise<unknown>>();

  handle(channel: string, handler: (event: unknown, raw: unknown) => Promise<unknown>): void {
    if (this.handlers.has(channel)) throw new Error("duplicate handler");
    this.handlers.set(channel, handler);
  }

  removeHandler(channel: string): void {
    this.handlers.delete(channel);
  }

  invoke(channel: IpcChannel, raw: unknown): Promise<unknown> {
    const handler = this.handlers.get(channel);
    if (handler === undefined) throw new Error("missing handler");
    return handler({}, raw);
  }
}

function services(overrides: Partial<IpcServiceMap> = {}): IpcServiceMap {
  const entries = Object.entries(fixtures).map(([channel, [, data]]) => [
    channel,
    async () => data,
  ]);
  return { ...Object.fromEntries(entries), ...overrides } as IpcServiceMap;
}

describe("typed IPC registration", () => {
  it("registers exactly the contract channels and unregisters only those handlers", () => {
    const ipcMain = new RecordingIpcMain();
    const unregister = registerIpcHandlers(ipcMain, services());

    expect([...ipcMain.handlers.keys()].sort()).toEqual(Object.keys(ipcContracts).sort());
    expect(ipcMain.handlers.has("shell.exec")).toBe(false);

    unregister();
    expect(ipcMain.handlers.size).toBe(0);
  });

  it("parses every request before invoking its service and every data result before responding", async () => {
    const ipcMain = new RecordingIpcMain();
    const calls: Array<{ channel: IpcChannel; input: unknown }> = [];
    const serviceMap = services();
    for (const channel of Object.keys(fixtures) as IpcChannel[]) {
      const original = serviceMap[channel] as (input: never) => Promise<unknown>;
      (serviceMap as Record<IpcChannel, (input: never) => Promise<unknown>>)[channel] = async (input) => {
        calls.push({ channel, input });
        return original(input);
      };
    }
    registerIpcHandlers(ipcMain, serviceMap);

    for (const channel of Object.keys(fixtures) as IpcChannel[]) {
      const [request, data] = fixtures[channel];
      await expect(ipcMain.invoke(channel, request)).resolves.toEqual({ ok: true, data });
    }
    expect(calls).toHaveLength(Object.keys(ipcContracts).length);
  });

  it("rejects malformed input before the service runs", async () => {
    let called = false;
    const ipcMain = new RecordingIpcMain();
    registerIpcHandlers(ipcMain, services({
      "workspace.open": async () => {
        called = true;
        return fixtures["workspace.open"][1];
      },
    }));

    await expect(ipcMain.invoke("workspace.open", { rootPath: "C:\\secret\\credential.txt" })).resolves.toEqual({
      ok: false,
      error: {
        code: "ProtocolViolation",
        message: "Invalid IPC request or response.",
        retryable: false,
      },
    });
    expect(called).toBe(false);
  });

  it("rejects malformed service data without returning its secret fields", async () => {
    const canary = "ipc-output-canary-secret";
    const ipcMain = new RecordingIpcMain();
    registerIpcHandlers(ipcMain, services({
      "storage.health": async () => ({
        mode: "read-write",
        schemaVersion: 1,
        diagnostics: [],
        password: canary,
      }) as never,
    }));

    const response = await ipcMain.invoke("storage.health", {});
    expect(response).toEqual({
      ok: false,
      error: {
        code: "ProtocolViolation",
        message: "Invalid IPC request or response.",
        retryable: false,
      },
    });
    expect(JSON.stringify(response)).not.toContain(canary);
  });

  it("maps known app errors to stable public messages", async () => {
    const ipcMain = new RecordingIpcMain();
    registerIpcHandlers(ipcMain, services({
      "runtime.start": async () => {
        throw Object.assign(new Error("C:\\Users\\name\\token.json: server body canary"), {
          code: "WorkspaceUntrusted",
          retryable: false,
        });
      },
    }));

    await expect(ipcMain.invoke("runtime.start", fixtures["runtime.start"][0])).resolves.toEqual({
      ok: false,
      error: {
        code: "WorkspaceUntrusted",
        message: "Workspace trust is required.",
        retryable: false,
        action: "Review workspace trust before starting a runtime.",
      },
    });
  });

  it("maps unknown exceptions to a stable envelope without stack, path, stderr, body, or secret", async () => {
    const canary = "unknown-service-canary";
    const ipcMain = new RecordingIpcMain();
    registerIpcHandlers(ipcMain, services({
      "storage.health": async () => {
        const error = new Error(`C:\\Users\\name\\credentials\\vault: ${canary}`);
        Object.assign(error, { stderr: canary, body: canary, password: canary });
        throw error;
      },
    }));

    const response = await ipcMain.invoke("storage.health", {});
    expect(response).toEqual({
      ok: false,
      error: {
        code: "ProtocolViolation",
        message: "The desktop service could not complete the request.",
        retryable: false,
      },
    });
    const serialized = JSON.stringify(response);
    expect(serialized).not.toContain(canary);
    expect(serialized).not.toContain("C:\\Users");
    expect(serialized).not.toContain("stack");
    expect(serialized).not.toContain("stderr");
    expect(serialized).not.toContain("body");
  });
});
