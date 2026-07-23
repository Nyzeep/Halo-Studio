import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";
import { createServerCredentials } from "./auth.js";
import { OpenCodeRuntime, type OpenCodeProcess, type SpawnPort } from "./runtime.js";
import * as runtimeModule from "./runtime.js";

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

const testArtifact = (executable = "opencode.exe") => async () => ({ executable, version: "1.18.4" as const });

describe("OpenCode runtime", () => {
  it("does not let an arbitrary executable bypass the bundled artifact resolver", async () => {
    let spawnedExecutable = "";
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      executable: "C:\\malicious\\opencode.exe",
      resolveArtifact: testArtifact("C:\\fixture\\node_modules\\opencode-ai\\bin\\opencode.exe"),
      cwd: "C:\\workspace",
      hostEnvironment: { PATH: "C:\\malicious" },
      trust: "trusted",
      reservePort: async () => 43120,
      spawn: async (executable, args, options) => {
        spawnedExecutable = executable;
        const child = new FakeProcess(args, options.env);
        processes.push(child);
        return child;
      },
      checkHealth: async () => ({ version: "1.18.4" }),
    } as never);
    await runtime.start();
    expect(spawnedExecutable).toBe("C:\\fixture\\node_modules\\opencode-ai\\bin\\opencode.exe");
    expect(spawnedExecutable).not.toContain("malicious");
    processes[0]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
  });

  it("uses fixed serve args, private credentials, and reaches healthy", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact("C:\\bundle\\opencode.exe"),
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
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 40000 + calls,
      spawn: async () => { calls += 1; const error = Object.assign(new Error("busy"), { code: "EADDRINUSE" }); throw error; },
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(calls).toBe(3);
  });

  it("retries when a spawned child later reports a structured port conflict", async () => {
    let calls = 0;
    let healthCalls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 41000 + calls,
      spawn: async (_executable, args, options) => {
        calls += 1;
        const child = new FakeProcess(args, options.env);
        if (calls < 3) {
          child.resolveWait({ code: 1, signal: null });
          return Object.assign(child, { startup: Promise.resolve("port-conflict" as const) });
        }
        return child;
      },
      checkHealth: async () => { healthCalls += 1; return { version: "1.18.4" }; },
    });
    await runtime.start();
    expect(calls).toBe(3);
    expect(healthCalls).toBe(1);
    expect(runtime.state).toBe("healthy");
  });

  it("bounds a startup spawn that never resolves", async () => {
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43126,
      readinessTimeoutMs: 5,
      spawn: async () => new Promise<never>(() => undefined),
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(runtime.state).toBe("crashed");
  });

  it("clears credentials and maps reserve-port rejection after credential creation", async () => {
    const credentials = { username: "opencode" as const, password: "reserve-canary" };
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(),
      credentialsFactory: () => credentials,
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => { throw new Error(credentials.password); },
    } as never);
    let failure: unknown;
    try { await runtime.start(); } catch (error) { failure = error; }
    expect(failure).toMatchObject({ code: "RuntimeUnavailable" });
    expect(String(failure)).not.toContain("reserve-canary");
    expect(credentials.password).toBe("");
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "RuntimeUnavailable" } });
  });

  it("clears credentials and maps synchronous environment construction failure", async () => {
    const credentials = { username: "opencode" as const, password: "environment-canary" };
    const hostEnvironment = Object.defineProperty({}, "PATH", {
      enumerable: true,
      get: () => { throw new Error(credentials.password); },
    });
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(),
      credentialsFactory: () => credentials,
      cwd: "C:\\workspace", hostEnvironment, trust: "trusted",
    } as never);
    let failure: unknown;
    try { await runtime.start(); } catch (error) { failure = error; }
    expect(failure).toMatchObject({ code: "RuntimeUnavailable" });
    expect(String(failure)).not.toContain("environment-canary");
    expect(credentials.password).toBe("");
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "RuntimeUnavailable" } });
  });

  it("hard-kills and awaits a child after a failed health handshake", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43127,
      spawn: processFactory({ processes, waitForever: true }),
      checkHealth: async () => { throw Object.assign(new Error("health timeout"), { code: "RuntimeUnavailable" }); },
      stopTimeoutMs: 5,
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(processes[0]?.killed).toContain("SIGKILL");
    expect(runtime.state).toBe("crashed");
  });

  it("bounds a pending stdin end while tearing down a failed start", async () => {
    const child = new FakeProcess([], {}, true);
    Object.defineProperty(child, "stdin", { value: { end: () => new Promise<never>(() => undefined) } });
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43130,
      spawn: async () => child,
      checkHealth: async () => { throw new Error("health failure"); },
      stopTimeoutMs: 5,
    });
    const result = await Promise.race([
      runtime.start().then(() => "resolved", (error: unknown) => (error as { code?: string }).code),
      new Promise<string>((resolve) => setTimeout(() => resolve("test-harness-timeout"), 50)),
    ]);
    expect(result).toBe("RuntimeUnavailable");
    expect(child.killed).toContain("SIGKILL");
  });

  it("transitions to crashed on an unexpected ready-process exit", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43124, spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }) });
    await runtime.start();
    processes[0]?.process.emit("exit", 1, null);
    expect(runtime.state).toBe("crashed");
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "TransportDisconnected" } });
    await runtime.stop();
    expect(runtime.snapshot()).toEqual({ state: "stopped" });
  });

  it("hard-kills a process that ignores graceful stop after six seconds", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43125, spawn: processFactory({ processes, waitForever: true }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
    await runtime.start();
    await runtime.stop();
    expect(processes[0]?.killed).toContain("SIGKILL");
    expect(runtime.state).toBe("stopped");
    expect(runtime.snapshot()).toEqual({ state: "stopped" });
  });

  it("forces termination when process wait throws synchronously", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43131, spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
    await runtime.start();
    processes[0]!.wait = () => { throw new Error("raw wait failure"); };
    await expect(runtime.stop()).resolves.toBeUndefined();
    expect(processes[0]?.killed).toContain("SIGKILL");
    expect(runtime.state).toBe("stopped");
  });

  it("maps default child-process errors without exposing stderr or paths", async () => {
    const createFactory = (runtimeModule as unknown as { createNodeProcessFactory?: (spawn: (...args: unknown[]) => unknown) => typeof processFactory }).createNodeProcessFactory;
    expect(createFactory).toBeTypeOf("function");
    if (!createFactory) return;
    const child = new EventEmitter() as EventEmitter & {
      stdin: { end: () => undefined };
      stdout: EventEmitter;
      stderr: EventEmitter;
      kill: () => boolean;
    };
    child.stdin = { end: () => undefined };
    child.stdout = new EventEmitter();
    child.stderr = new EventEmitter();
    child.kill = () => true;
    const spawn = () => {
      queueMicrotask(() => {
        child.stderr.emit("data", "EACCES C:\\private\\secret-path");
        child.emit("error", new Error("ENOENT C:\\private\\secret-path"));
      });
      return child;
    };
    const runtime = new OpenCodeRuntime({
      resolveArtifact: async () => ({ executable: "C:\\bundle\\bin\\opencode.exe", version: "1.18.4" }),
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43132,
      spawn: createFactory(spawn),
      stopTimeoutMs: 5,
    } as never);
    let failure: unknown;
    try { await runtime.start(); } catch (error) { failure = error; }
    expect(failure).toMatchObject({ code: "RuntimeUnavailable" });
    expect(String(failure)).not.toContain("secret-path");
    expect(child.listenerCount("error")).toBe(0);
  });

  it("keeps credentials out of public errors", () => {
    const credentials = createServerCredentials();
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", checkHealth: async () => { throw new Error(credentials.password); } });
    expect(JSON.stringify(runtime.snapshot())).not.toContain(credentials.password);
  });

  it("clears the credential object after stop and unexpected crash", async () => {
    const stoppedProcesses: FakeProcess[] = [];
    let stoppedCredentials: { password: string } | undefined;
    const stopped = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43128,
      spawn: processFactory({ processes: stoppedProcesses }),
      checkHealth: async ({ credentials }) => { stoppedCredentials = credentials; return { version: "1.18.4" }; },
    });
    await stopped.start();
    stoppedProcesses[0]?.resolveWait({ code: 0, signal: null });
    await stopped.stop();
    expect(stoppedCredentials?.password).toBe("");

    const crashedProcesses: FakeProcess[] = [];
    let crashedCredentials: { password: string } | undefined;
    const crashed = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43129,
      spawn: processFactory({ processes: crashedProcesses }),
      checkHealth: async ({ credentials }) => { crashedCredentials = credentials; return { version: "1.18.4" }; },
    });
    await crashed.start();
    crashedProcesses[0]?.process.emit("exit", 1, null);
    expect(crashedCredentials?.password).toBe("");
  });
});
