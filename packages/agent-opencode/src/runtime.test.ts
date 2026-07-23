import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";
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
  disposed = false;
  waitPromise: Promise<{ code: number | null; signal: NodeJS.Signals | null }>;
  resolveWait!: (value: { code: number | null; signal: NodeJS.Signals | null }) => void;

  constructor(args: readonly string[], env: Readonly<Record<string, string>>, waitForever = false) {
    this.args = args;
    this.env = env;
    this.waitPromise = waitForever ? new Promise(() => undefined) : new Promise((resolve) => { this.resolveWait = resolve; });
  }

  wait() { return this.waitPromise; }
  kill(signal?: NodeJS.Signals) { this.killed.push(signal ?? "SIGTERM"); return true; }
  dispose() { this.disposed = true; }
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
  it("detects and caches the bundled artifact without downgrading a healthy runtime", async () => {
    let resolutions = 0;
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: async () => { resolutions += 1; return { executable: "C:\\bundle\\bin\\opencode.exe", version: "1.18.4" }; },
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43110,
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await expect(runtime.detect()).resolves.toMatchObject({ version: "1.18.4" });
    expect(runtime.snapshot()).toEqual({ state: "installed", version: "1.18.4" });
    await runtime.start();
    expect(resolutions).toBe(1);
    await expect(runtime.detect()).resolves.toMatchObject({ version: "1.18.4" });
    expect(runtime.state).toBe("healthy");
    processes[0]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
  });

  it("serializes concurrent start calls and spawns only once", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43111,
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    const results = await Promise.allSettled([runtime.start(), runtime.start()]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((result) => result.status === "rejected")[0]).toMatchObject({ reason: { code: "RuntimeUnavailable" } });
    expect(processes).toHaveLength(1);
    processes[0]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
  });

  it("connects SSE with runtime-owned credentials and closes it before stop clears secrets", async () => {
    const processes: FakeProcess[] = [];
    let authorization = "";
    let requestSignal: AbortSignal | undefined;
    let cancelCalls = 0;
    const body = new ReadableStream<Uint8Array>({ cancel() { cancelCalls += 1; } });
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43115,
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await runtime.start();
    const connection = await runtime.connectSse({
      onSignal: () => undefined,
      fetch: async (_url, init) => {
        authorization = String(new Headers(init?.headers).get("authorization"));
        requestSignal = init?.signal ?? undefined;
        return new Response(body, { status: 200, headers: { "content-type": "text/event-stream" } });
      },
    });
    expect(authorization).toMatch(/^Basic [A-Za-z0-9+/]+={0,2}$/u);
    expect(authorization).not.toContain("opencode:");
    expect(runtime.snapshot()).not.toMatchObject({ password: expect.anything() });

    processes[0]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
    await connection.done;
    expect(requestSignal?.aborted).toBe(true);
    expect(cancelCalls).toBe(1);
  });

  it("closes connected SSE when the managed process exits unexpectedly", async () => {
    const processes: FakeProcess[] = [];
    let requestSignal: AbortSignal | undefined;
    let cancelCalls = 0;
    const body = new ReadableStream<Uint8Array>({ cancel() { cancelCalls += 1; } });
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43116,
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await runtime.start();
    const connection = await runtime.connectSse({
      onSignal: () => undefined,
      fetch: async (_url, init) => {
        requestSignal = init?.signal ?? undefined;
        return new Response(body, { status: 200 });
      },
    });
    processes[0]?.process.emit("exit", 1, null);
    await connection.done;
    expect(runtime.state).toBe("crashed");
    expect(requestSignal?.aborted).toBe(true);
    expect(cancelCalls).toBe(1);
  });

  it("unbinds a completed SSE connection from later runtime shutdown", async () => {
    const processes: FakeProcess[] = [];
    let requestSignal: AbortSignal | undefined;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43118,
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await runtime.start();
    const connection = await runtime.connectSse({
      onSignal: () => undefined,
      fetch: async (_url, init) => {
        requestSignal = init?.signal ?? undefined;
        return new Response(new ReadableStream<Uint8Array>({ start(controller) { controller.close(); } }), { status: 200 });
      },
    });
    await connection.done;
    processes[0]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
    expect(requestSignal?.aborted).toBe(false);
  });

  it("aborts an in-flight SSE handshake when stop runs concurrently", async () => {
    const processes: FakeProcess[] = [];
    let requestSignal: AbortSignal | undefined;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43117,
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await runtime.start();
    const connecting = runtime.connectSse({
      onSignal: () => undefined,
      fetch: async (_url, init) => new Promise<Response>((_resolve, reject) => {
        requestSignal = init?.signal ?? undefined;
        init?.signal?.addEventListener("abort", () => reject(new Error("aborted")), { once: true });
      }),
    });
    await new Promise<void>((resolve) => setImmediate(resolve));
    processes[0]?.resolveWait({ code: 0, signal: null });
    const stopping = runtime.stop();
    const [connectionResult, stopResult] = await Promise.allSettled([connecting, stopping]);
    expect(connectionResult).toMatchObject({ status: "rejected", reason: { code: "TransportDisconnected" } });
    expect(stopResult).toMatchObject({ status: "fulfilled" });
    expect(requestSignal?.aborted).toBe(true);
    expect(runtime.state).toBe("stopped");
  });

  it("clears crash credentials only after connected SSE close completes", async () => {
    const credentials = { username: "opencode" as const, password: "crash-order-canary" };
    const processes: FakeProcess[] = [];
    let resolveCancel!: () => void;
    const body = new ReadableStream<Uint8Array>({
      cancel: () => new Promise<void>((resolve) => { resolveCancel = resolve; }),
    });
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), credentialsFactory: () => credentials,
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43119,
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await runtime.start();
    await runtime.connectSse({
      onSignal: () => undefined,
      fetch: async () => new Response(body, { status: 200 }),
    });
    processes[0]?.process.emit("exit", 1, null);
    await Promise.resolve();
    expect(credentials.password).toBe("crash-order-canary");
    resolveCancel();
    await new Promise<void>((resolve) => setImmediate(resolve));
    expect(credentials.password).toBe("");
  });

  it("serializes detect and stop without leaving installed state behind", async () => {
    let resolveDetection!: (artifact: { executable: string; version: "1.18.4" }) => void;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: async () => new Promise((resolve) => { resolveDetection = resolve; }),
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
    });
    const detection = runtime.detect();
    const stopping = runtime.stop();
    await Promise.resolve();
    resolveDetection({ executable: "C:\\bundle\\bin\\opencode.exe", version: "1.18.4" });
    await expect(detection).resolves.toMatchObject({ version: "1.18.4" });
    await expect(stopping).resolves.toBeUndefined();
    expect(runtime.snapshot()).toEqual({ state: "stopped" });
  });

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

  it("does not infer a port conflict from an arbitrary exception message", async () => {
    let calls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43113,
      spawn: async () => { calls += 1; throw new Error("diagnostic text mentions EADDRINUSE out of context"); },
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(calls).toBe(1);
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

  it("cleans up a child that fulfills after the spawn timeout", async () => {
    let lateChild: FakeProcess | undefined;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43112,
      readinessTimeoutMs: 5,
      stopTimeoutMs: 5,
      spawn: async (_executable, args, options) => {
        await new Promise((resolve) => setTimeout(resolve, 25));
        const child = new FakeProcess(args, options.env);
        child.kill = (signal?: NodeJS.Signals) => {
          child.killed.push(signal ?? "SIGTERM");
          child.resolveWait({ code: null, signal: signal ?? "SIGTERM" });
          return true;
        };
        lateChild = child;
        return child;
      },
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    await new Promise((resolve) => setTimeout(resolve, 45));
    expect(lateChild?.killed).toContain("SIGKILL");
    expect(lateChild?.disposed).toBe(true);
    expect(lateChild?.env.OPENCODE_SERVER_PASSWORD).toBeUndefined();
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

  it("does not report stopped when SIGKILL succeeds but the child never exits", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43125, spawn: processFactory({ processes, waitForever: true }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
    await runtime.start();
    await expect(runtime.stop()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(processes[0]?.killed).toContain("SIGKILL");
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "RuntimeUnavailable" } });
  });

  it("reports stopped only after observing a delayed post-kill exit", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43133, spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
    await runtime.start();
    const child = processes[0]!;
    child.kill = (signal?: NodeJS.Signals) => {
      child.killed.push(signal ?? "SIGTERM");
      setTimeout(() => child.resolveWait({ code: null, signal: signal ?? "SIGTERM" }), 2);
      return true;
    };
    await expect(runtime.stop()).resolves.toBeUndefined();
    expect(child.killed).toContain("SIGKILL");
    expect(runtime.snapshot()).toEqual({ state: "stopped" });
  });

  it("forces termination when process wait throws synchronously", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", reservePort: async () => 43131, spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
    await runtime.start();
    processes[0]!.wait = () => { throw new Error("raw wait failure"); };
    await expect(runtime.stop()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(processes[0]?.killed).toContain("SIGKILL");
    expect(runtime.state).toBe("crashed");
  });

  it("drains default-adapter stdout for the process lifetime and removes the listener", () => {
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
    const factory = runtimeModule.createNodeProcessFactory(() => child);
    const port = factory("C:\\bundle\\bin\\opencode.exe", [], { cwd: "C:\\workspace", env: {} }) as OpenCodeProcess;
    expect(child.stdout.listenerCount("data")).toBe(1);
    for (let index = 0; index < 1_000; index += 1) child.stdout.emit("data", Buffer.alloc(1_024));
    port.dispose?.();
    expect(child.stdout.listenerCount("data")).toBe(0);
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

  it("classifies only controlled listen failures as stderr port conflicts", async () => {
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
    const factory = runtimeModule.createNodeProcessFactory(() => child);
    const port = factory("C:\\bundle\\bin\\opencode.exe", [], { cwd: "C:\\workspace", env: {} }) as OpenCodeProcess;
    child.stderr.emit("data", "diagnostic field contains EADDRINUSE but is not a listen failure\n");
    child.emit("exit", 1, null);
    await expect(port.startup).resolves.toBe("exited");
    port.dispose?.();
  });

  it("classifies a listen EADDRINUSE stderr line as a port conflict", async () => {
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
    const factory = runtimeModule.createNodeProcessFactory(() => child);
    const port = factory("C:\\bundle\\bin\\opencode.exe", [], { cwd: "C:\\workspace", env: {} }) as OpenCodeProcess;
    child.stderr.emit("data", "Error: listen EADDRINUSE: address already in use 127.0.0.1\n");
    await expect(port.startup).resolves.toBe("port-conflict");
    port.dispose?.();
  });

  it("keeps credentials out of a real startup error and clears the source object", async () => {
    const credentials = { username: "opencode" as const, password: "startup-error-canary" };
    const child = new FakeProcess([], {}, true);
    child.kill = (signal?: NodeJS.Signals) => {
      child.killed.push(signal ?? "SIGTERM");
      child.resolveWait({ code: null, signal: signal ?? "SIGTERM" });
      return true;
    };
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), credentialsFactory: () => credentials,
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      reservePort: async () => 43114, spawn: async () => child,
      checkHealth: async () => { throw new Error(credentials.password); }, stopTimeoutMs: 5,
    });
    let failure: unknown;
    try { await runtime.start(); } catch (error) { failure = error; }
    expect(failure).toMatchObject({ code: "RuntimeUnavailable" });
    expect(String(failure)).not.toContain("startup-error-canary");
    expect(JSON.stringify(runtime.snapshot())).not.toContain("startup-error-canary");
    expect(credentials.password).toBe("");
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
    await new Promise<void>((resolve) => setImmediate(resolve));
    expect(crashedCredentials?.password).toBe("");
  });
});
