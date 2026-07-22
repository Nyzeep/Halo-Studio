import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";
import { createServerCredentials } from "./auth.js";
import { OpenCodeRuntime, type OpenCodeProcess, type SpawnPort } from "./runtime.js";

class FakeProcess implements OpenCodeProcess {
  readonly stdout = new EventEmitter();
  readonly stderr = new EventEmitter();
  readonly process = new EventEmitter();
  readonly stdin = { end: () => undefined };
  readonly args: readonly string[];
  readonly env: Readonly<Record<string, string>>;
  killed: string[] = [];
  waitPromise: Promise<{ code: number | null; signal: NodeJS.Signals | null }>;
  resolveWait!: (value: { code: number | null; signal: NodeJS.Signals | null }) => void;

  constructor(args: readonly string[], env: Readonly<Record<string, string>>, waitForever = false) {
    this.args = args;
    this.env = env;
    this.waitPromise = waitForever ? new Promise(() => undefined) : new Promise((resolve) => { this.resolveWait = resolve; });
  }

  wait() { return this.waitPromise; }
  kill(signal?: NodeJS.Signals) { this.killed.push(signal ?? "SIGTERM"); return true; }
}

function processFactory(options: { processes: FakeProcess[]; waitForever?: boolean }) {
  return async (_exe: string, args: readonly string[], spawn: SpawnPort) => {
    const child = new FakeProcess(args, spawn.env, options.waitForever);
    options.processes.push(child);
    return child;
  };
}

describe("OpenCode runtime", () => {
  it("uses fixed serve args, private credentials, and reaches healthy", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      executable: "C:\\bundle\\opencode.exe",
      cwd: "C:\\workspace",
      hostEnvironment: { PATH: "C:\\Windows", TEMP: "C:\\Temp" },
      trust: "untrusted",
      spawn: processFactory({ processes }),
      checkHealth: async ({ credentials }) => {
        expect(credentials.username).toBe("opencode");
        expect(credentials.password).toBeTruthy();
        return { version: "1.18.4" };
      },
      reservePort: async () => 43123,
    });
    await runtime.start();
    expect(runtime.state).toBe("healthy");
    expect(processes[0]?.args).toEqual(["serve", "--hostname", "127.0.0.1", "--port", "43123"]);
    expect(processes[0]?.env.OPENCODE_SERVER_USERNAME).toBe("opencode");
    expect(runtime.snapshot()).not.toMatchObject({ password: expect.anything() });
    expect(JSON.stringify(runtime.snapshot())).not.toContain(processes[0]?.env.OPENCODE_SERVER_PASSWORD ?? "impossible");
  });

  it("retries port conflicts at most three times", async () => {
    let calls = 0;
    const runtime = new OpenCodeRuntime({
      executable: "opencode.exe", cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 40000 + calls,
      spawn: async () => { calls += 1; const error = Object.assign(new Error("busy"), { code: "EADDRINUSE" }); throw error; },
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(calls).toBe(3);
  });

  it("bounds a startup spawn that never resolves", async () => {
    const runtime = new OpenCodeRuntime({
      executable: "opencode.exe", cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43126,
      readinessTimeoutMs: 5,
      spawn: async () => new Promise<never>(() => undefined),
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(runtime.state).toBe("crashed");
  });

  it("transitions to crashed on an unexpected ready-process exit", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ executable: "opencode.exe", cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43124, spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }) });
    await runtime.start();
    processes[0]?.process.emit("exit", 1, null);
    expect(runtime.state).toBe("crashed");
    expect(runtime.snapshot().error?.code).toBe("TransportDisconnected");
  });

  it("hard-kills a process that ignores graceful stop after six seconds", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ executable: "opencode.exe", cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43125, spawn: processFactory({ processes, waitForever: true }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
    await runtime.start();
    await runtime.stop();
    expect(processes[0]?.killed).toContain("SIGKILL");
    expect(runtime.state).toBe("stopped");
  });

  it("keeps credentials out of public errors", () => {
    const credentials = createServerCredentials();
    const runtime = new OpenCodeRuntime({ executable: "opencode.exe", cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", checkHealth: async () => { throw new Error(credentials.password); } });
    expect(JSON.stringify(runtime.snapshot())).not.toContain(credentials.password);
  });
});
