import { EventEmitter } from "node:events";
import { mkdir, mkdtemp, readFile, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { createElectronSecretProtector } from "../src/main/electronSecretProtector.js";
import { startDesktopMain } from "../src/main/main.js";
import { createDesktopServices, createRuntimeBinding } from "../src/main/services.js";
import { createSecureWindow, type BrowserWindowPort } from "../src/main/window.js";
import { createHaloApi, installHaloPreload } from "../src/preload/preload.js";

class FakeWebContents extends EventEmitter {
  openHandler: ((details: { url: string }) => { action: "deny" | "allow" }) | undefined;

  setWindowOpenHandler(handler: (details: { url: string }) => { action: "deny" | "allow" }): void {
    this.openHandler = handler;
  }
}

class FakeBrowserWindow implements BrowserWindowPort {
  static instances: FakeBrowserWindow[] = [];
  readonly webContents = new FakeWebContents();
  readonly options: Record<string, unknown>;
  loadedFile: string | undefined;
  loadedUrl: string | undefined;

  constructor(options: Record<string, unknown>) {
    this.options = options;
    FakeBrowserWindow.instances.push(this);
  }

  async loadFile(path: string): Promise<void> {
    this.loadedFile = path;
  }

  async loadURL(url: string): Promise<void> {
    this.loadedUrl = url;
  }
}

describe("secure Electron window", () => {
  it("keeps the production bootstrap local and forbids renderer loopback transport", async () => {
    const [mainSource, rendererHtml] = await Promise.all([
      readFile(new URL("../src/main/main.ts", import.meta.url), "utf8"),
      readFile(new URL("../index.html", import.meta.url), "utf8"),
    ]);

    expect(mainSource).not.toContain("HALO_DESKTOP_DEV_URL");
    expect(mainSource).not.toContain("remote-debugging");
    expect(mainSource).not.toContain("shell.openExternal");
    expect(rendererHtml).toContain("connect-src 'self'");
    expect(rendererHtml).not.toContain("http://127.0.0.1");
  });

  it("enforces isolated sandboxed web preferences without Node or security bypasses", async () => {
    FakeBrowserWindow.instances.length = 0;
    const window = await createSecureWindow({
      BrowserWindow: FakeBrowserWindow,
      preloadPath: "D:\\app\\preload.js",
      rendererPath: "D:\\app\\index.html",
    }) as FakeBrowserWindow;

    expect(window.options).toMatchObject({
      webPreferences: {
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: true,
        webSecurity: true,
        devTools: false,
        preload: "D:\\app\\preload.js",
      },
    });
    expect(JSON.stringify(window.options)).not.toContain("remote-debugging");
  });

  it("loads a local renderer in production and allows an explicit loopback development URL only in development", async () => {
    const production = await createSecureWindow({
      BrowserWindow: FakeBrowserWindow,
      preloadPath: "D:\\app\\preload.js",
      rendererPath: "D:\\app\\index.html",
    }) as FakeBrowserWindow;
    expect(production.loadedFile).toBe("D:\\app\\index.html");
    expect(production.loadedUrl).toBeUndefined();

    const development = await createSecureWindow({
      BrowserWindow: FakeBrowserWindow,
      preloadPath: "D:\\app\\preload.js",
      rendererPath: "D:\\app\\index.html",
      developmentUrl: "http://127.0.0.1:5173",
    }) as FakeBrowserWindow;
    expect(development.loadedUrl).toBe("http://127.0.0.1:5173");
    expect(development.loadedFile).toBeUndefined();
  });

  it("rejects navigation, redirects, window.open, and legacy new-window attempts", async () => {
    const window = await createSecureWindow({
      BrowserWindow: FakeBrowserWindow,
      preloadPath: "D:\\app\\preload.js",
      rendererPath: "D:\\app\\index.html",
    }) as FakeBrowserWindow;
    for (const event of ["will-navigate", "will-redirect", "new-window"]) {
      let prevented = false;
      window.webContents.emit(event, { preventDefault: () => { prevented = true; } }, "https://example.invalid");
      expect(prevented, event).toBe(true);
    }
    expect(window.webContents.openHandler?.({ url: "https://example.invalid" })).toEqual({ action: "deny" });
  });
});

describe("Electron safeStorage secret protector", () => {
  it("round-trips arbitrary Buffer data through an explicitly versioned string boundary", () => {
    const encrypted = Buffer.from([9, 8, 7, 6]);
    let plaintextBoundary = "";
    const protector = createElectronSecretProtector({
      isEncryptionAvailable: () => true,
      encryptString: (value) => {
        plaintextBoundary = value;
        return encrypted;
      },
      decryptString: (value) => {
        expect(value).toEqual(encrypted);
        return plaintextBoundary;
      },
    });
    const secret = Buffer.from([0, 255, 1, 254, 2]);

    const ciphertext = protector.protect(secret);
    expect(ciphertext).toEqual(encrypted);
    expect(ciphertext).not.toBe(encrypted);
    expect(plaintextBoundary).toBe("halo-secret-v1:AP8B/gI=");
    expect(protector.unprotect(ciphertext)).toEqual(secret);
  });

  it("reports availability and maps unavailable or malformed safeStorage results to stable errors", () => {
    const canary = "safe-storage-canary-secret";
    const unavailable = createElectronSecretProtector({
      isEncryptionAvailable: () => false,
      encryptString: () => { throw new Error(canary); },
      decryptString: () => canary,
    });
    expect(unavailable.isAvailable()).toBe(false);
    for (const operation of [
      () => unavailable.protect(Buffer.from(canary)),
      () => unavailable.unprotect(Buffer.from(canary)),
    ]) {
      expect(operation).toThrowError(expect.objectContaining({
        code: "AuthenticationFailed",
        message: "Credential protection is unavailable.",
      }));
      try { operation(); } catch (error) {
        expect(JSON.stringify(error)).not.toContain(canary);
      }
    }

    const malformed = createElectronSecretProtector({
      isEncryptionAvailable: () => true,
      encryptString: () => Buffer.alloc(0),
      decryptString: () => "not-a-halo-secret",
    });
    expect(() => malformed.protect(Buffer.from("secret"))).toThrowError(expect.objectContaining({ code: "AuthenticationFailed" }));
    expect(() => malformed.unprotect(Buffer.from("ciphertext"))).toThrowError(expect.objectContaining({ code: "AuthenticationFailed" }));
  });
});

describe("preload capability boundary", () => {
  it("installs the frozen API under only window.halo", () => {
    const exposed: Array<{ key: string; api: unknown }> = [];
    const api = installHaloPreload({
      exposeInMainWorld(key, value) {
        exposed.push({ key, api: value });
      },
    }, async () => ({ ok: true, data: null }));

    expect(exposed).toEqual([{ key: "halo", api }]);
    expect(Object.isFrozen(api)).toBe(true);
    expect(Object.isFrozen(api.workspace)).toBe(true);
  });

  it("exposes only the twelve named domain methods backed by fixed IPC channels", async () => {
    const calls: Array<{ channel: string; request: unknown }> = [];
    const api = createHaloApi(async (channel, request) => {
      calls.push({ channel, request });
      if (channel === "workspace.pick") {
        return { ok: true, data: null };
      }
      if (channel === "storage.health") {
        return { ok: true, data: { mode: "read-write", schemaVersion: 1, diagnostics: [] } };
      }
      return { ok: false, error: { code: "RuntimeUnavailable", message: "Runtime is unavailable.", retryable: false } };
    });

    expect(Object.keys(api).sort()).toEqual(["config", "runtime", "storage", "workspace"]);
    expect(Object.keys(api.workspace).sort()).toEqual(["open", "pick", "setTrust", "snapshot"]);
    expect(Object.keys(api.runtime).sort()).toEqual(["probe", "snapshot", "start", "stop"]);
    expect(Object.keys(api.config).sort()).toEqual(["commit", "preview", "rollback"]);
    expect(Object.keys(api.storage)).toEqual(["health"]);
    expect("invoke" in api).toBe(false);
    expect("ipcRenderer" in api).toBe(false);
    expect("fs" in api).toBe(false);
    expect("shell" in api).toBe(false);

    await api.workspace.pick({});
    await api.storage.health({});
    expect(calls).toEqual([
      { channel: "workspace.pick", request: {} },
      { channel: "storage.health", request: {} },
    ]);
  });

  it("validates preload requests before invoke and validates responses before returning", async () => {
    let calls = 0;
    const api = createHaloApi(async () => {
      calls += 1;
      return { ok: true, data: { password: "renderer-canary-secret" } };
    });

    await expect(api.workspace.open({ rootPath: "C:\\arbitrary" } as never)).rejects.toMatchObject({ code: "ProtocolViolation" });
    expect(calls).toBe(0);
    await expect(api.storage.health({})).rejects.toMatchObject({ code: "ProtocolViolation" });
    expect(calls).toBe(1);
  });
});

describe("workspace lifecycle security", () => {
  it("fails closed when trust changes after the original root resolves to a different workspace", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-workspace-revalidation-"));
    const rootPath = join(root, "selected-root");
    const replacementPath = join(root, "replacement-root");
    await Promise.all([mkdir(rootPath), mkdir(replacementPath)]);
    const workspaceId = "a".repeat(64);
    const original = { id: workspaceId, rootPath, realPath: rootPath, trustState: "trusted" as const };
    const replacement = { id: "b".repeat(64), rootPath, realPath: replacementPath, trustState: "trusted" as const };
    let opens = 0;
    const runtime = {
      state: "unavailable" as const,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { throw new Error("start should not run"); },
      async stop() { runtime.stopCalls += 1; },
    };
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async () => {
          opens += 1;
          return opens === 1 ? original : replacement;
        },
        createOpenCodeRuntime: () => runtime,
      } as never);

      const candidate = await services.handlers["workspace.pick"]({});
      const workspace = await services.handlers["workspace.open"]({ selectionId: candidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: workspace.id });

      await expect(services.handlers["workspace.trust"]({ workspaceId: workspace.id, trustState: "trusted" })).rejects.toMatchObject({
        code: "ProtocolViolation",
        message: "Workspace is unavailable.",
      });
      expect(runtime.stopCalls).toBe(1);
      await expect(services.handlers["runtime.snapshot"]({ workspaceId: workspace.id })).rejects.toMatchObject({ code: "ProtocolViolation" });
    } finally {
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it("fails closed before runtime start when the original root resolves to a different workspace", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-runtime-revalidation-"));
    const rootPath = join(root, "selected-root");
    const replacementPath = join(root, "replacement-root");
    await Promise.all([mkdir(rootPath), mkdir(replacementPath)]);
    const workspaceId = "c".repeat(64);
    const original = { id: workspaceId, rootPath, realPath: rootPath, trustState: "trusted" as const };
    const replacement = { id: "d".repeat(64), rootPath, realPath: replacementPath, trustState: "trusted" as const };
    let opens = 0;
    const runtime = {
      state: "unavailable" as "unavailable" | "healthy" | "stopped",
      startCalls: 0,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { runtime.startCalls += 1; runtime.state = "healthy"; },
      async stop() { runtime.stopCalls += 1; runtime.state = "stopped"; },
    };
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async () => {
          opens += 1;
          return opens === 1 ? original : replacement;
        },
        createOpenCodeRuntime: () => runtime,
      } as never);

      const candidate = await services.handlers["workspace.pick"]({});
      const workspace = await services.handlers["workspace.open"]({ selectionId: candidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: workspace.id });

      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "opencode" })).rejects.toMatchObject({
        code: "ProtocolViolation",
        message: "Workspace is unavailable.",
      });
      expect(runtime.startCalls).toBe(0);
      expect(runtime.stopCalls).toBe(1);
    } finally {
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it("fails closed when a workspace directory is replaced at the same canonical path", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-workspace-identity-"));
    const rootPath = join(root, "selected-root");
    await mkdir(rootPath);
    const workspace = { id: "f".repeat(64), rootPath, realPath: rootPath, trustState: "trusted" as const };
    let identityReads = 0;
    const runtime = {
      state: "unavailable" as const,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { throw new Error("start should not run"); },
      async stop() { runtime.stopCalls += 1; },
    };
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async () => workspace,
        workspaceIdentity: async () => {
          identityReads += 1;
          return identityReads === 1 ? { device: 1n, inode: 101n } : { device: 1n, inode: 202n };
        },
        createOpenCodeRuntime: () => runtime,
      } as never);

      const candidate = await services.handlers["workspace.pick"]({});
      const opened = await services.handlers["workspace.open"]({ selectionId: candidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: opened.id });

      await expect(services.handlers["workspace.trust"]({ workspaceId: opened.id, trustState: "trusted" })).rejects.toMatchObject({
        code: "ProtocolViolation",
        message: "Workspace is unavailable.",
      });
      expect(runtime.stopCalls).toBe(1);
      await expect(services.handlers["runtime.snapshot"]({ workspaceId: opened.id })).rejects.toMatchObject({ code: "ProtocolViolation" });
    } finally {
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it("fails closed before runtime start when a workspace directory is replaced at the same canonical path", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-runtime-identity-"));
    const rootPath = join(root, "selected-root");
    await mkdir(rootPath);
    const workspace = { id: "d".repeat(64), rootPath, realPath: rootPath, trustState: "trusted" as const };
    let identityReads = 0;
    const runtime = {
      state: "unavailable" as "unavailable" | "healthy" | "stopped",
      startCalls: 0,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { runtime.startCalls += 1; runtime.state = "healthy"; },
      async stop() { runtime.stopCalls += 1; runtime.state = "stopped"; },
    };
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async () => workspace,
        workspaceIdentity: async () => {
          identityReads += 1;
          return identityReads === 1 ? { device: 1n, inode: 101n } : { device: 1n, inode: 202n };
        },
        createOpenCodeRuntime: () => runtime,
      } as never);

      const candidate = await services.handlers["workspace.pick"]({});
      const opened = await services.handlers["workspace.open"]({ selectionId: candidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: opened.id });

      await expect(services.handlers["runtime.start"]({ workspaceId: opened.id, agentKind: "opencode" })).rejects.toMatchObject({
        code: "ProtocolViolation",
        message: "Workspace is unavailable.",
      });
      expect(runtime.stopCalls).toBe(1);
      expect(runtime.startCalls).toBe(0);
      await expect(services.handlers["runtime.start"]({ workspaceId: opened.id, agentKind: "opencode" })).rejects.toMatchObject({ code: "ProtocolViolation" });
      expect(runtime.startCalls).toBe(0);
    } finally {
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it("retires an old runtime before re-opening a replaced directory with the same workspace id", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-workspace-reopen-identity-"));
    const rootPath = join(root, "selected-root");
    await mkdir(rootPath);
    const workspace = { id: "9".repeat(64), rootPath, realPath: rootPath, trustState: "trusted" as const };
    let identityReads = 0;
    const oldRuntime = {
      state: "unavailable" as "unavailable" | "healthy" | "stopped",
      startCalls: 0,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { oldRuntime.startCalls += 1; oldRuntime.state = "healthy"; },
      async stop() { oldRuntime.stopCalls += 1; oldRuntime.state = "stopped"; },
    };
    const replacementRuntime = {
      state: "unavailable" as "unavailable" | "healthy" | "stopped",
      startCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { replacementRuntime.startCalls += 1; replacementRuntime.state = "healthy"; },
      async stop() { replacementRuntime.state = "stopped"; },
    };
    let runtimeCreations = 0;
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async () => workspace,
        workspaceIdentity: async () => {
          identityReads += 1;
          return identityReads === 1 ? { device: 1n, inode: 101n } : { device: 1n, inode: 202n };
        },
        createOpenCodeRuntime: () => {
          runtimeCreations += 1;
          return runtimeCreations === 1 ? oldRuntime : replacementRuntime;
        },
      } as never);

      const firstCandidate = await services.handlers["workspace.pick"]({});
      const opened = await services.handlers["workspace.open"]({ selectionId: firstCandidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: opened.id });

      const secondCandidate = await services.handlers["workspace.pick"]({});
      await services.handlers["workspace.open"]({ selectionId: secondCandidate!.selectionId });
      expect(oldRuntime.stopCalls).toBe(1);

      await services.handlers["runtime.start"]({ workspaceId: opened.id, agentKind: "opencode" });
      expect(oldRuntime.startCalls).toBe(0);
      expect(replacementRuntime.startCalls).toBe(1);
    } finally {
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it("retires an existing runtime when a same-id re-open cannot read directory identity", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-workspace-unreadable-identity-"));
    const rootPath = join(root, "selected-root");
    await mkdir(rootPath);
    const workspace = { id: "8".repeat(64), rootPath, realPath: rootPath, trustState: "trusted" as const };
    let identityReads = 0;
    const runtime = {
      state: "unavailable" as const,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { throw new Error("start should not run"); },
      async stop() { runtime.stopCalls += 1; },
    };
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async () => workspace,
        workspaceIdentity: async () => {
          identityReads += 1;
          if (identityReads === 1) return { device: 1n, inode: 101n };
          throw new Error("identity unavailable");
        },
        createOpenCodeRuntime: () => runtime,
      } as never);

      const firstCandidate = await services.handlers["workspace.pick"]({});
      const opened = await services.handlers["workspace.open"]({ selectionId: firstCandidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: opened.id });

      const secondCandidate = await services.handlers["workspace.pick"]({});
      await expect(services.handlers["workspace.open"]({ selectionId: secondCandidate!.selectionId })).rejects.toMatchObject({
        code: "ProtocolViolation",
        message: "Workspace is unavailable.",
      });
      expect(runtime.stopCalls).toBe(1);
      await expect(services.handlers["runtime.snapshot"]({ workspaceId: opened.id })).rejects.toMatchObject({ code: "ProtocolViolation" });
    } finally {
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it("keeps a failed runtime shutdown reachable without revoking trust or permitting a same-id re-open", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-workspace-stop-failure-"));
    const rootPath = join(root, "selected-root");
    await mkdir(rootPath);
    const workspace = { id: "0".repeat(64), rootPath, realPath: rootPath, trustState: "trusted" as const };
    const runtime = {
      state: "unavailable" as const,
      startCalls: 0,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { runtime.startCalls += 1; },
      async stop() { runtime.stopCalls += 1; throw new Error("stop failed"); },
    };
    const observedTrustStates: Array<"trusted" | "untrusted"> = [];
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async (_rootPath: string, trustStore: { listDecisions(): Promise<readonly { realPath: string; state: "trusted" | "untrusted" }[]> }) => {
          const decision = (await trustStore.listDecisions()).find(({ realPath }) => realPath === workspace.realPath);
          const trustState = decision?.state ?? "trusted";
          observedTrustStates.push(trustState);
          return { ...workspace, trustState };
        },
        createOpenCodeRuntime: () => runtime,
      } as never);

      const candidate = await services.handlers["workspace.pick"]({});
      const opened = await services.handlers["workspace.open"]({ selectionId: candidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: opened.id });

      await expect(services.handlers["workspace.trust"]({ workspaceId: opened.id, trustState: "untrusted" })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
      expect(observedTrustStates).toEqual(["trusted", "trusted"]);
      await expect(services.handlers["runtime.start"]({ workspaceId: opened.id, agentKind: "opencode" })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
      const retryCandidate = await services.handlers["workspace.pick"]({});
      await expect(services.handlers["workspace.open"]({ selectionId: retryCandidate!.selectionId })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
      expect(runtime.startCalls).toBe(0);

      await services.dispose();
      services = undefined;
      expect(runtime.stopCalls).toBe(2);
    } finally {
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });

  it("serializes trust revocation ahead of a concurrent runtime start", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-workspace-serialization-"));
    const rootPath = join(root, "selected-root");
    await mkdir(rootPath);
    const workspaceId = "e".repeat(64);
    const workspace = { id: workspaceId, rootPath, realPath: rootPath, trustState: "trusted" as const };
    let releaseStop!: () => void;
    let signalStop!: () => void;
    const stopEntered = new Promise<void>((resolve) => { signalStop = resolve; });
    const stopGate = new Promise<void>((resolve) => { releaseStop = resolve; });
    const runtime = {
      state: "unavailable" as "unavailable" | "healthy" | "stopped",
      startCalls: 0,
      stopCalls: 0,
      async detect() { return { executable: "C:\\bundle\\opencode.exe", version: "1.18.4" as const }; },
      async start() { runtime.startCalls += 1; runtime.state = "healthy"; },
      async stop() {
        runtime.stopCalls += 1;
        signalStop();
        await stopGate;
        runtime.state = "stopped";
      },
    };
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [rootPath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value) => Buffer.from(value, "utf8"),
          decryptString: (value) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "x" },
        openWorkspace: async (_rootPath: string, trustStore: { listDecisions(): Promise<readonly { realPath: string; state: "trusted" | "untrusted" }[]> }) => {
          const decision = (await trustStore.listDecisions()).find(({ realPath }) => realPath === workspace.realPath);
          return { ...workspace, trustState: decision?.state ?? "trusted" };
        },
        createOpenCodeRuntime: () => runtime,
      } as never);

      const candidate = await services.handlers["workspace.pick"]({});
      const opened = await services.handlers["workspace.open"]({ selectionId: candidate!.selectionId });
      await services.handlers["runtime.snapshot"]({ workspaceId: opened.id });

      const revoking = services.handlers["workspace.trust"]({ workspaceId: opened.id, trustState: "untrusted" });
      const stopStarted = await Promise.race([
        stopEntered.then(() => true),
        new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 250)),
      ]);
      expect(stopStarted).toBe(true);

      const starting = services.handlers["runtime.start"]({ workspaceId: opened.id, agentKind: "opencode" });
      await new Promise<void>((resolve) => setImmediate(resolve));
      expect(runtime.startCalls).toBe(0);

      releaseStop();
      await expect(revoking).resolves.toMatchObject({ trustState: "untrusted" });
      await expect(starting).rejects.toMatchObject({ code: "WorkspaceUntrusted" });
      expect(runtime.startCalls).toBe(0);
    } finally {
      releaseStop?.();
      await services?.dispose();
      await rm(root, { recursive: true, force: true });
    }
  });
});

describe("desktop service composition", () => {
  it("keeps Pi discovery independent from unavailable launch configuration", async () => {
    const source = await readFile(new URL("../src/main/services.ts", import.meta.url), "utf8");

    expect(source).toContain("detectPi");
    expect(source).not.toContain("createPiRuntime");
    expect(source).not.toMatch(/\bmodel\s*:/u);
    expect(source).not.toMatch(/\bthinking\s*:/u);
    expect(source).not.toContain('"unconfigured"');
  });

  it("preserves detected runtime identity without exposing transport credentials", () => {
    const binding = createRuntimeBinding("opencode", "installed", {
      source: "bundled",
      executable: "C:\\app\\opencode.exe",
      version: "1.18.4",
    });

    expect(binding).toMatchObject({
      agentKind: "opencode",
      source: "bundled",
      executable: "C:\\app\\opencode.exe",
      version: "1.18.4",
      health: "installed",
    });
    expect(JSON.stringify(binding)).not.toContain("password");
    expect(JSON.stringify(binding)).not.toContain("Authorization");
  });

  it("creates storage, encrypted credential, and per-runtime roots below userData and composes real services", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-desktop-services-"));
    const userDataPath = join(root, "user-data");
    const workspacePath = join(root, "workspace");
    const emptyPath = join(root, "empty-path");
    await Promise.all([mkdir(workspacePath), mkdir(emptyPath)]);
    const xor = (value: Buffer): Buffer => Buffer.from(value, (byte) => byte ^ 0xa5);
    const services = await createDesktopServices({
      userDataPath,
      picker: {
        showOpenDialog: async () => ({ canceled: false, filePaths: [workspacePath] }),
      },
      safeStorage: {
        isEncryptionAvailable: () => true,
        encryptString: (value) => xor(Buffer.from(value, "utf8")),
        decryptString: (value) => xor(value).toString("utf8"),
      },
      hostEnvironment: { PATH: emptyPath },
    });

    for (const path of [
      services.paths.storageDirectory,
      services.paths.credentialDirectory,
      services.paths.piRuntimeDirectory,
      services.paths.openCodeRuntimeDirectory,
    ]) {
      await expect(stat(path)).resolves.toMatchObject({});
      expect(path.startsWith(userDataPath)).toBe(true);
    }
    await expect(stat(services.paths.databasePath)).resolves.toMatchObject({});
    await expect(services.handlers["storage.health"]({})).resolves.toEqual({
      mode: "read-write",
      schemaVersion: 1,
      diagnostics: [],
    });

    const candidate = await services.handlers["workspace.pick"]({});
    expect(candidate).toMatchObject({ displayPath: workspacePath });
    const workspace = await services.handlers["workspace.open"]({ selectionId: candidate!.selectionId });
    expect(workspace).toMatchObject({ realPath: workspacePath, trustState: "untrusted" });
    const initialBindings = await services.handlers["runtime.snapshot"]({ workspaceId: workspace.id });
    expect(initialBindings).toHaveLength(2);
    expect(JSON.stringify(initialBindings)).not.toMatch(/unconfigured|\bmodel\b|\bthinking\b/u);
    const probedBindings = await services.handlers["runtime.probe"]({ workspaceId: workspace.id });
    expect(probedBindings.find(({ agentKind }) => agentKind === "pi")).toMatchObject({
      agentKind: "pi",
      source: "managed",
      health: "unavailable",
    });
    expect(JSON.stringify(probedBindings)).not.toMatch(/unconfigured|\bmodel\b|\bthinking\b/u);
    await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).rejects.toMatchObject({ code: "WorkspaceUntrusted" });
    const stoppedPi = await services.handlers["runtime.stop"]({ workspaceId: workspace.id, agentKind: "pi" });
    expect(stoppedPi).toMatchObject({ agentKind: "pi", health: "stopped" });
    expect(JSON.stringify(stoppedPi)).not.toMatch(/unconfigured|\bmodel\b|\bthinking\b/u);
    await services.handlers["workspace.trust"]({ workspaceId: workspace.id, trustState: "trusted" });
    const resetBindings = await services.handlers["runtime.snapshot"]({ workspaceId: workspace.id });
    expect(resetBindings.find(({ agentKind }) => agentKind === "pi")?.health).toBe("unavailable");
    await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    await expect(services.handlers["config.preview"]({
      targetId: "unregistered-target",
      operations: [{ op: "set", path: ["secret"], value: "composition-canary" }],
    })).rejects.toMatchObject({ code: "RuntimeUnavailable" });

    await services.dispose();
    const databaseBytes = await readFile(services.paths.databasePath);
    expect(databaseBytes.includes(Buffer.from("composition-canary"))).toBe(false);
    await rm(root, { recursive: true, force: true });
  });
});

describe("desktop app lifecycle", () => {
  it("initializes services and IPC once, then recreates only the window on activate", async () => {
    const listeners = new Map<string, Array<(...args: unknown[]) => void>>();
    let windowCount = 0;
    let serviceCreations = 0;
    let registrations = 0;
    const app = {
      whenReady: async () => undefined,
      getPath: () => "D:\\user-data",
      on(event: string, listener: (...args: unknown[]) => void) {
        listeners.set(event, [...(listeners.get(event) ?? []), listener]);
      },
      quit: () => undefined,
    };
    const lifecycle = startDesktopMain({
      app,
      platform: "win32",
      getWindowCount: () => windowCount,
      createServices: async () => {
        serviceCreations += 1;
        return { handlers: {}, dispose: async () => undefined } as never;
      },
      registerIpc: () => {
        registrations += 1;
        return () => undefined;
      },
      createWindow: async () => {
        windowCount += 1;
        return {} as never;
      },
    });
    await lifecycle.ready;
    expect({ serviceCreations, registrations, windowCount }).toEqual({ serviceCreations: 1, registrations: 1, windowCount: 1 });

    windowCount = 0;
    listeners.get("activate")?.forEach((listener) => listener());
    await lifecycle.idle();
    expect({ serviceCreations, registrations, windowCount }).toEqual({ serviceCreations: 1, registrations: 1, windowCount: 1 });
  });

  it("quits on window-all-closed except on macOS and disposes once before quitting", async () => {
    for (const [platform, expectedWindowQuit] of [["win32", 1], ["darwin", 0]] as const) {
      const listeners = new Map<string, Array<(...args: unknown[]) => void>>();
      let quits = 0;
      let disposals = 0;
      const app = {
        whenReady: async () => undefined,
        getPath: () => "D:\\user-data",
        on(event: string, listener: (...args: unknown[]) => void) {
          listeners.set(event, [...(listeners.get(event) ?? []), listener]);
        },
        quit: () => { quits += 1; },
      };
      const lifecycle = startDesktopMain({
        app,
        platform,
        getWindowCount: () => 0,
        createServices: async () => ({ handlers: {}, dispose: async () => { disposals += 1; } }) as never,
        registerIpc: () => () => undefined,
        createWindow: async () => ({} as never),
      });
      await lifecycle.ready;
      listeners.get("window-all-closed")?.forEach((listener) => listener());
      expect(quits).toBe(expectedWindowQuit);

      let prevented = 0;
      listeners.get("before-quit")?.forEach((listener) => listener({ preventDefault: () => { prevented += 1; } }));
      await lifecycle.idle();
      expect(disposals).toBe(1);
      expect(prevented).toBe(1);
      expect(quits).toBe(expectedWindowQuit + 1);
    }
  });
});
