import { EventEmitter } from "node:events";
import { readFile, readdir, mkdtemp, mkdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import {
  createPiRuntime,
  detectPi,
  nodeProcessFactory,
  type ProcessPort,
} from "@halo-studio/agent-pi";
import type { IpcChannel } from "@halo-studio/contracts";
import { FileCredentialVault } from "@halo-studio/storage";

import {
  createElectronSecretProtector,
} from "../src/main/electronSecretProtector.js";
import {
  registerIpcHandlers,
  type IpcMainPort,
} from "../src/main/ipc/registerIpc.js";
import { createDesktopServices } from "../src/main/services.js";

const fakePiPath = fileURLToPath(new URL("./fixtures/fake-pi.mjs", import.meta.url));

const fakePiProcessFactory: typeof nodeProcessFactory = (_executable, args, options) => {
  return nodeProcessFactory(process.execPath, [fakePiPath, ...args], options);
};

class RecordingIpcMain implements IpcMainPort {
  readonly #handlers = new Map<string, (event: unknown, raw: unknown) => Promise<unknown>>();

  handle(channel: string, handler: (event: unknown, raw: unknown) => Promise<unknown>): void {
    this.#handlers.set(channel, handler);
  }

  removeHandler(channel: string): void {
    this.#handlers.delete(channel);
  }

  invoke(channel: IpcChannel, raw: unknown): Promise<unknown> {
    const handler = this.#handlers.get(channel);
    if (handler === undefined) throw new Error(`Missing IPC handler: ${channel}`);
    return handler({}, raw);
  }
}

function xor(value: Buffer): Buffer {
  return Buffer.from(value.map((byte) => byte ^ 0xa5));
}

function assertNoCanary(value: unknown, canary: string): void {
  expect(JSON.stringify(value) ?? "").not.toContain(canary);
}

describe("credential boundary integration", () => {
  it("keeps a Main-only Pi provider credential out of storage and public runtime surfaces", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-credential-boundary-"));
    const userDataPath = join(root, "user-data");
    const credentialDirectory = join(userDataPath, "credentials");
    const workspacePath = join(root, "workspace");
    const credentialReference = "pi-provider/default";
    const canary = "pi-provider-credential-canary-9e0e6679";
    const safeStorage = {
      isEncryptionAvailable: () => true,
      encryptString: (value: string) => xor(Buffer.from(value, "utf8")),
      decryptString: (value: Buffer) => xor(value).toString("utf8"),
    };
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;
    let unregister: (() => void) | undefined;

    try {
      await mkdir(workspacePath, { recursive: true });
      const vault = new FileCredentialVault(
        credentialDirectory,
        createElectronSecretProtector(safeStorage),
      );
      await vault.store(credentialReference, canary);

      const vaultFiles = await readdir(credentialDirectory);
      expect(vaultFiles).toHaveLength(1);
      for (const vaultFile of vaultFiles) {
        const ciphertext = await readFile(join(credentialDirectory, vaultFile));
        expect(ciphertext.includes(Buffer.from(canary, "utf8"))).toBe(false);
      }

      services = await createDesktopServices({
        userDataPath,
        picker: {
          showOpenDialog: async () => ({ canceled: false, filePaths: [workspacePath] }),
        },
        safeStorage,
        hostEnvironment: {
          PATH: process.env.PATH ?? "",
          HALO_PI_MODEL: "test-model",
          HALO_PI_THINKING: "medium",
          HALO_PI_PROVIDER_ENV_KEY: "OPENAI_API_KEY",
          HALO_PI_CREDENTIAL_REFERENCE: credentialReference,
        },
        detectPi: (options) => detectPi({
          ...(options ?? {}),
          processFactory: fakePiProcessFactory,
          resolveExecutables: async () => [join(root, "host-bin", "pi")],
        }),
        createPiRuntime: (options) => createPiRuntime({
          ...options,
          spawn: fakePiProcessFactory,
        }),
      });

      const ipcMain = new RecordingIpcMain();
      unregister = registerIpcHandlers(ipcMain, services.handlers);
      const selection = await services.handlers["workspace.pick"]({});
      const workspace = await services.handlers["workspace.open"]({
        selectionId: selection!.selectionId,
      });

      const probed = await ipcMain.invoke("runtime.probe", { workspaceId: workspace.id });
      expect(probed).toMatchObject({
        ok: true,
        data: expect.arrayContaining([
          expect.objectContaining({ agentKind: "opencode" }),
        ]),
      });
      assertNoCanary(probed, canary);

      const untrustedStart = await ipcMain.invoke("runtime.start", {
        workspaceId: workspace.id,
        agentKind: "pi",
      });
      expect(untrustedStart).toMatchObject({
        ok: false,
        error: { code: "WorkspaceUntrusted" },
      });
      assertNoCanary(untrustedStart, canary);

      const trusted = await ipcMain.invoke("workspace.trust", {
        workspaceId: workspace.id,
        trustState: "trusted",
      });
      assertNoCanary(trusted, canary);

      const started = await ipcMain.invoke("runtime.start", {
        workspaceId: workspace.id,
        agentKind: "pi",
      });
      expect(started).toMatchObject({
        ok: true,
        data: { agentKind: "pi", health: "ready" },
      });
      assertNoCanary(started, canary);

      const bindings = await services.handlers["runtime.snapshot"]({ workspaceId: workspace.id });
      expect(bindings.find(({ agentKind }) => agentKind === "pi")).toMatchObject({
        agentKind: "pi",
        health: "ready",
      });
      assertNoCanary(bindings, canary);

      const duplicateStart = await ipcMain.invoke("runtime.start", {
        workspaceId: workspace.id,
        agentKind: "pi",
      });
      expect(duplicateStart).toMatchObject({
        ok: false,
        error: { code: "RuntimeUnavailable" },
      });
      assertNoCanary(duplicateStart, canary);

      const stopped = await ipcMain.invoke("runtime.stop", {
        workspaceId: workspace.id,
        agentKind: "pi",
      });
      expect(stopped).toMatchObject({
        ok: true,
        data: { agentKind: "pi", health: "stopped" },
      });
      assertNoCanary(stopped, canary);

      const databasePath = services.paths.databasePath;
      await services.dispose();
      services = undefined;

      const databaseBytes = await readFile(databasePath);
      expect(databaseBytes.includes(Buffer.from(canary, "utf8"))).toBe(false);
      for (const vaultFile of await readdir(credentialDirectory)) {
        const ciphertext = await readFile(join(credentialDirectory, vaultFile));
        expect(ciphertext.includes(Buffer.from(canary, "utf8"))).toBe(false);
      }
    } finally {
      unregister?.();
      await services?.dispose().catch(() => undefined);
      await rm(root, { recursive: true, force: true });
    }
  });

  it("keeps probes credential-blind and recreates Pi after a readiness EOF", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-pi-crash-retry-"));
    const userDataPath = join(root, "user-data");
    const workspacePath = join(root, "workspace");
    const canary = "pi-rpc-only-crash-retry-canary";
    const spawns: Array<{ readonly args: readonly string[]; readonly env: Readonly<Record<string, string>> | undefined }> = [];
    let rpcAttempts = 0;
    let launchCalls = 0;
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    type TestPort = ProcessPort & {
      readonly stdout: EventEmitter;
      readonly stderr: EventEmitter;
    };
    let readyRpcPort: TestPort | undefined;
    const createPort = (onWrite?: (value: string, port: TestPort) => void): TestPort => {
      const stdout = new EventEmitter();
      const stderr = new EventEmitter();
      const child = new EventEmitter();
      const port: TestPort = {
        stdin: {
          write: (value) => onWrite?.(value, port),
          end: () => undefined,
        },
        stdout,
        stderr,
        process: child,
        wait: async () => ({ code: 0, signal: null }),
        kill: () => {
          child.emit("exit", 0, "SIGTERM");
          return true;
        },
      };
      return port;
    };

    const spawnPi: typeof nodeProcessFactory = (_executable, args, options) => {
      spawns.push({ args, env: options.env });
      if (args[0] === "--version") {
        const port = createPort();
        queueMicrotask(() => {
          port.stdout.emit("data", "pi 0.81.1\n");
          port.stdout.emit("end");
          port.stderr.emit("end");
        });
        return port;
      }
      rpcAttempts += 1;
      const port = createPort((value, currentPort) => {
        const command = JSON.parse(value) as { id: string; type: string };
        if (command.type !== "get_state") return;
        queueMicrotask(() => {
          if (rpcAttempts === 1) {
            // Simulate a child that closes its JSONL stream during readiness.
            currentPort.stdout.emit("end");
            return;
          }
          currentPort.stdout.emit("data", JSON.stringify({
            type: "response",
            id: command.id,
            command: "get_state",
            success: true,
            data: {},
          }) + "\n");
        });
      });
      if (rpcAttempts > 1) readyRpcPort = port;
      return port;
    };

    try {
      await mkdir(workspacePath, { recursive: true });
      services = await createDesktopServices({
        userDataPath,
        picker: {
          showOpenDialog: async () => ({ canceled: false, filePaths: [workspacePath] }),
        },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value: string) => Buffer.from(value, "utf8"),
          decryptString: (value: Buffer) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "C:/bin" },
        detectPi: (options) => detectPi({
          ...(options ?? {}),
          processFactory: spawnPi,
          resolveExecutables: async () => ["C:/bin/pi"],
        }),
        createPiRuntime: (options) => {
          expect(options).not.toHaveProperty("providerEnvironment");
          expect(options).not.toHaveProperty("allowedProviderKeys");
          return createPiRuntime({ ...options, spawn: spawnPi });
        },
        resolvePiLaunch: async () => {
          launchCalls += 1;
          return {
            model: "test-model",
            thinking: "medium",
            providerEnvironment: { OPENAI_API_KEY: canary },
            allowedProviderKeys: new Set(["OPENAI_API_KEY"]),
          };
        },
      });

      const selection = await services.handlers["workspace.pick"]({});
      const workspace = await services.handlers["workspace.open"]({ selectionId: selection!.selectionId });
      await services.handlers["workspace.trust"]({ workspaceId: workspace.id, trustState: "trusted" });

      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).rejects.toMatchObject({
        code: "TransportDisconnected",
      });
      await expect(services.handlers["runtime.snapshot"]({ workspaceId: workspace.id })).resolves.toEqual(expect.arrayContaining([
        expect.objectContaining({ agentKind: "pi", health: "crashed" }),
      ]));

      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).resolves.toMatchObject({
        agentKind: "pi",
        health: "ready",
      });
      expect(rpcAttempts).toBe(2);
      expect(launchCalls).toBe(2);

      // Once readiness has succeeded, an EOF is still terminal. The Main
      // cache must evict that instance so a fresh third start can be made.
      expect(readyRpcPort).toBeDefined();
      readyRpcPort!.stdout.emit("end");
      await Promise.resolve();
      await expect(services.handlers["runtime.snapshot"]({ workspaceId: workspace.id })).resolves.toEqual(expect.arrayContaining([
        expect.objectContaining({ agentKind: "pi", health: "crashed" }),
      ]));
      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).resolves.toMatchObject({
        agentKind: "pi",
        health: "ready",
      });
      expect(rpcAttempts).toBe(3);
      expect(launchCalls).toBe(3);

      const versionSpawns = spawns.filter(({ args }) => args[0] === "--version");
      const rpcSpawns = spawns.filter(({ args }) => args[0] === "--mode");
      expect(versionSpawns).not.toHaveLength(0);
      expect(versionSpawns.every(({ env }) => env?.OPENAI_API_KEY === undefined)).toBe(true);
      expect(JSON.stringify(versionSpawns)).not.toContain(canary);
      expect(rpcSpawns).toHaveLength(3);
      expect(rpcSpawns.every(({ env }) => env?.OPENAI_API_KEY === canary)).toBe(true);
    } finally {
      await services?.dispose().catch(() => undefined);
      await rm(root, { recursive: true, force: true });
    }
  });

  it("keeps a crashed Pi managed until cleanup confirms exit or stop retries it", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-pi-retained-cleanup-"));
    const workspacePath = join(root, "workspace");
    type RuntimeRecord = {
      state: "unavailable" | "ready" | "crashed" | "stopped";
      readonly onCrashed: () => void;
      readonly onCrashCleanupFailed: () => void;
    };
    const runtimes: RuntimeRecord[] = [];
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      await mkdir(workspacePath, { recursive: true });
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [workspacePath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value: string) => Buffer.from(value, "utf8"),
          decryptString: (value: Buffer) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "C:/bin" },
        createPiRuntime: (options) => {
          const record: RuntimeRecord = {
            state: "unavailable",
            onCrashed: options.onCrashed ?? (() => undefined),
            onCrashCleanupFailed: options.onCrashCleanupFailed ?? (() => undefined),
          };
          const runtime = {
            get state() { return record.state; },
            async detect() {
              return { status: "detected" as const, source: "system" as const, executable: "pi", version: "0.81.1" };
            },
            async start() {
              if (record.state === "crashed") throw new Error("cleanup is pending");
              record.state = "ready";
            },
            async stop() { record.state = "stopped"; },
          };
          runtimes.push(record);
          return runtime;
        },
      });

      const selection = await services.handlers["workspace.pick"]({});
      const workspace = await services.handlers["workspace.open"]({ selectionId: selection!.selectionId });
      await services.handlers["workspace.trust"]({ workspaceId: workspace.id, trustState: "trusted" });

      await services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" });
      expect(runtimes).toHaveLength(1);
      runtimes[0]!.state = "crashed";

      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).rejects.toThrow("cleanup is pending");
      expect(runtimes).toHaveLength(1);
      await expect(services.handlers["runtime.snapshot"]({ workspaceId: workspace.id })).resolves.toEqual(expect.arrayContaining([
        expect.objectContaining({ agentKind: "pi", health: "crashed" }),
      ]));

      runtimes[0]!.onCrashed();
      await services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" });
      expect(runtimes).toHaveLength(2);

      runtimes[1]!.state = "crashed";
      runtimes[1]!.onCrashCleanupFailed();
      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).rejects.toMatchObject({
        code: "RuntimeUnavailable",
      });
      expect(runtimes).toHaveLength(2);

      await expect(services.handlers["runtime.stop"]({ workspaceId: workspace.id, agentKind: "pi" })).resolves.toMatchObject({
        agentKind: "pi",
        health: "stopped",
      });
      await services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" });
      expect(runtimes).toHaveLength(3);
    } finally {
      await services?.dispose().catch(() => undefined);
      await rm(root, { recursive: true, force: true });
    }
  });

  it("revalidates workspace identity before resolving a Pi launch credential", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-pi-launch-identity-"));
    const workspacePath = join(root, "workspace");
    const workspace = {
      id: "a".repeat(64),
      rootPath: workspacePath,
      realPath: workspacePath,
      trustState: "trusted" as const,
    };
    let identityChanged = false;
    let launchResolverCalls = 0;
    let rpcStarts = 0;
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;

    try {
      await mkdir(workspacePath, { recursive: true });
      services = await createDesktopServices({
        userDataPath: join(root, "user-data"),
        picker: { showOpenDialog: async () => ({ canceled: false, filePaths: [workspacePath] }) },
        safeStorage: {
          isEncryptionAvailable: () => true,
          encryptString: (value: string) => Buffer.from(value, "utf8"),
          decryptString: (value: Buffer) => value.toString("utf8"),
        },
        hostEnvironment: { PATH: "C:/bin" },
        openWorkspace: async () => workspace,
        workspaceIdentity: async () => ({ device: 1n, inode: identityChanged ? 2n : 1n }),
        createPiRuntime: (options) => {
          let state: "unavailable" | "detected" | "starting" | "ready" | "stopped" = "unavailable";
          return {
            get state() { return state; },
            async detect() {
              state = "detected";
              identityChanged = true;
              return { status: "detected" as const, source: "system" as const, executable: "pi", version: "0.81.1" };
            },
            async start() {
              state = "starting";
              await options.resolveRpcLaunch?.();
              rpcStarts += 1;
              state = "ready";
            },
            async stop() { state = "stopped"; },
          };
        },
        resolvePiLaunch: async () => {
          launchResolverCalls += 1;
          return {
            model: "test-model",
            thinking: "medium",
            providerEnvironment: { OPENAI_API_KEY: "must-not-be-read" },
            allowedProviderKeys: new Set(["OPENAI_API_KEY"]),
          };
        },
      });

      const selection = await services.handlers["workspace.pick"]({});
      const opened = await services.handlers["workspace.open"]({ selectionId: selection!.selectionId });
      await expect(services.handlers["runtime.start"]({ workspaceId: opened.id, agentKind: "pi" })).rejects.toMatchObject({
        code: "RuntimeUnavailable",
      });
      expect(launchResolverCalls).toBe(0);
      expect(rpcStarts).toBe(0);
    } finally {
      await services?.dispose().catch(() => undefined);
      await rm(root, { recursive: true, force: true });
    }
  });
});
