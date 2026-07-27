import { spawn } from "node:child_process";
import { mkdtemp, mkdir, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import {
  createNodeProcessFactory,
  createOpenCodeRuntime,
  type NodeChildPort,
} from "@halo-studio/agent-opencode";
import {
  createPiRuntime,
  detectPi,
  nodeProcessFactory,
} from "@halo-studio/agent-pi";

import { createDesktopServices } from "../src/main/services.js";

const fakePiPath = fileURLToPath(new URL("./fixtures/fake-pi.mjs", import.meta.url));
const fakeOpenCodePath = fileURLToPath(new URL("./fixtures/fake-opencode.mjs", import.meta.url));

const fakePiProcessFactory: typeof nodeProcessFactory = (_executable, args, options) => {
  return nodeProcessFactory(process.execPath, [fakePiPath, ...args], options);
};

const fakeOpenCodeProcessFactory = createNodeProcessFactory((_executable, args, options) => {
  return spawn(process.execPath, [fakeOpenCodePath, ...args], {
    cwd: options.cwd,
    env: options.env,
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  }) as unknown as NodeChildPort;
});

const safeStorage = {
  isEncryptionAvailable: () => true,
  encryptString: (value: string) => Buffer.from(value, "utf8"),
  decryptString: (value: Buffer) => value.toString("utf8"),
};

describe("workspace runtime integration", () => {
  it("uses real child-process runtime protocols only after the workspace is trusted", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo 集成 runtime "));
    const workspacePath = join(root, "项目 空格");
    const userDataPath = join(root, "用户 数据");
    await mkdir(workspacePath, { recursive: true });
    let services: Awaited<ReturnType<typeof createDesktopServices>> | undefined;
    const sessionEvents: unknown[] = [];
    let launchCalls = 0;
    const piSpawnArgs: string[][] = [];
    const trackedPiProcessFactory: typeof nodeProcessFactory = (executable, args, options) => {
      piSpawnArgs.push([...args]);
      return fakePiProcessFactory(executable, args, options);
    };

    try {
      services = await createDesktopServices({
        userDataPath,
        picker: {
          showOpenDialog: async () => ({ canceled: false, filePaths: [workspacePath] }),
        },
        safeStorage,
        hostEnvironment: { PATH: process.env.PATH ?? "" },
        detectPi: (options) => detectPi({
          ...(options ?? {}),
          processFactory: trackedPiProcessFactory,
          resolveExecutables: async () => [join(root, "host-bin", "pi")],
        }),
        createPiRuntime: (options) => createPiRuntime({
          ...options,
          spawn: trackedPiProcessFactory,
        }),
        createOpenCodeRuntime: (options) => createOpenCodeRuntime({
          ...options,
          spawn: fakeOpenCodeProcessFactory,
        }),
        resolvePiLaunch: async () => {
          launchCalls += 1;
          return {
            model: "test-model",
            thinking: "medium",
            providerEnvironment: {},
            allowedProviderKeys: new Set<string>(),
          };
        },
      });

      const selection = await services.handlers["workspace.pick"]({});
      const workspace = await services.handlers["workspace.open"]({ selectionId: selection!.selectionId });
      const unsubscribeSessionEvents = services.subscribeSessionEvents((event) => { sessionEvents.push(event); });
      const untrustedProbe = await services.handlers["runtime.probe"]({ workspaceId: workspace.id });
      expect(untrustedProbe.find(({ agentKind }) => agentKind === "pi")).toMatchObject({
        agentKind: "pi",
        health: "detected",
        version: "0.81.1",
      });
      expect(piSpawnArgs).toEqual([["--version"]]);
      expect(launchCalls).toBe(0);

      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" })).rejects.toMatchObject({
        code: "WorkspaceUntrusted",
      });
      await expect(services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "opencode" })).rejects.toMatchObject({
        code: "WorkspaceUntrusted",
      });
      await expect(services.handlers["session.snapshot"]({ workspaceId: workspace.id })).rejects.toMatchObject({
        code: "WorkspaceUntrusted",
      });
      expect(launchCalls).toBe(0);

      await services.handlers["workspace.trust"]({ workspaceId: workspace.id, trustState: "trusted" });
      await expect(services.handlers["session.snapshot"]({ workspaceId: workspace.id })).resolves.toEqual([]);
      const detected = await services.handlers["runtime.probe"]({ workspaceId: workspace.id });
      expect(detected.find(({ agentKind }) => agentKind === "pi")).toMatchObject({
        agentKind: "pi",
        health: "detected",
        version: "0.81.1",
      });
      expect(piSpawnArgs.some(([first]) => first === "--version")).toBe(true);
      const pi = await services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" });
      const openCode = await services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "opencode" });
      expect(pi).toMatchObject({ agentKind: "pi", health: "ready", version: "0.81.1" });
      expect(openCode).toMatchObject({ agentKind: "opencode", health: "healthy", version: "1.18.4" });
      expect(launchCalls).toBe(1);

      const sessions = await services.handlers["session.snapshot"]({ workspaceId: workspace.id });
      const piSession = sessions.find(({ agentKind }) => agentKind === "pi");
      expect(piSession).toMatchObject({ agentKind: "pi", sessionId: "fake-pi-session-1" });
      await expect(services.handlers["command.list"]({ workspaceId: workspace.id, agentKind: "pi" })).resolves.toEqual([
        expect.objectContaining({ name: "/compact", agentKind: "pi", channel: "rpc" }),
      ]);
      await services.handlers["session.send"]({
        workspaceId: workspace.id,
        agentKind: "pi",
        sessionId: piSession!.sessionId,
        message: "inspect the fake workspace",
        clientRequestId: "11111111-1111-4111-8111-111111111111",
      });
      await expect(services.handlers["session.history"]({
        workspaceId: workspace.id,
        agentKind: "pi",
        sessionId: piSession!.sessionId,
      })).resolves.toMatchObject({
        messages: [
          { role: "user", text: "inspect the fake workspace" },
          { role: "assistant", text: "Fake Pi accepted the prompt." },
        ],
      });
      expect(sessionEvents).toEqual(expect.arrayContaining([
        expect.objectContaining({ agentKind: "pi", payload: expect.objectContaining({ type: "agent_start" }) }),
      ]));

      const openCodeSession = await services.handlers["session.create"]({ workspaceId: workspace.id, agentKind: "opencode" });
      await services.handlers["session.send"]({
        workspaceId: workspace.id,
        agentKind: "opencode",
        sessionId: openCodeSession.sessionId,
        message: "review this change",
        clientRequestId: "22222222-2222-4222-8222-222222222222",
      });
      await expect(services.handlers["session.history"]({
        workspaceId: workspace.id,
        agentKind: "opencode",
        sessionId: openCodeSession.sessionId,
      })).resolves.toMatchObject({
        messages: [{ role: "user", text: "review this change" }],
      });
      await expect(services.handlers["command.list"]({ workspaceId: workspace.id, agentKind: "opencode" })).resolves.toEqual([]);
      unsubscribeSessionEvents();

      await expect(services.handlers["runtime.stop"]({ workspaceId: workspace.id, agentKind: "pi" })).resolves.toMatchObject({
        agentKind: "pi",
        health: "stopped",
      });
      await expect(services.handlers["runtime.stop"]({ workspaceId: workspace.id, agentKind: "opencode" })).resolves.toMatchObject({
        agentKind: "opencode",
        health: "stopped",
      });

      await services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "pi" });
      await services.handlers["runtime.start"]({ workspaceId: workspace.id, agentKind: "opencode" });
      await services.dispose();
      services = undefined;
      await rm(root, { recursive: true, force: true });
      await expect(stat(root)).rejects.toThrow();
    } finally {
      await services?.dispose().catch(() => undefined);
      await rm(root, { recursive: true, force: true });
    }
  });
});
