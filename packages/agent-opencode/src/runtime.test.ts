import { EventEmitter } from "node:events";
import { describe, expect, it, vi } from "vitest";
import { createNodeProcessFactory, OpenCodeRuntime, type NodeChildPort, type OpenCodeProcess, type SpawnPort } from "./runtime.js";
import * as runtimeModule from "./runtime.js";

class FakeProcess implements OpenCodeProcess {
  readonly stdout = new EventEmitter();
  readonly stderr = new EventEmitter();
  readonly process = new EventEmitter();
  readonly stdin = { end: () => undefined };
  readonly listeningAddress: Promise<number | undefined>;
  readonly args: readonly string[];
  readonly env: Readonly<Record<string, string>>;
  killed: string[] = [];
  disposed = false;
  waitPromise: Promise<{ code: number | null; signal: NodeJS.Signals | null }>;
  resolveWait!: (value: { code: number | null; signal: NodeJS.Signals | null }) => void;
  #resolveListeningAddress!: (port: number | undefined) => void;

  constructor(args: readonly string[], env: Readonly<Record<string, string>>, waitForever = false, announceListening = true) {
    this.args = args;
    this.env = env;
    this.waitPromise = waitForever ? new Promise(() => undefined) : new Promise((resolve) => { this.resolveWait = resolve; });
    this.listeningAddress = new Promise((resolve) => { this.#resolveListeningAddress = resolve; });
    if (announceListening) queueMicrotask(() => this.reportListening());
  }

  reportListening(address = "http://127.0.0.1:43123"): void {
    this.stdout.emit("data", `opencode server listening on ${address}\r\n`);
    const match = /^http:\/\/127\.0\.0\.1:([1-9]\d{0,4})$/u.exec(address);
    this.#resolveListeningAddress(match === null ? undefined : Number(match[1]));
  }
  wait() { return this.waitPromise; }
  kill(signal?: NodeJS.Signals) { this.killed.push(signal ?? "SIGTERM"); return true; }
  dispose() { this.disposed = true; }
}

function processFactory(options: { processes: FakeProcess[]; waitForever?: boolean }) {
  return (_exe: string, args: readonly string[], spawn: SpawnPort) => {
    const child = new FakeProcess(args, spawn.env, options.waitForever);
    options.processes.push(child);
    return child;
  };
}

const testArtifact = (executable = "opencode.exe") => async () => ({ executable, version: "1.18.4" as const });

function createFakeNodeChild(): NodeChildPort {
  const child = new EventEmitter() as EventEmitter & {
    stdin: { end: () => unknown };
    stdout: EventEmitter;
    stderr: EventEmitter;
    kill: (signal?: NodeJS.Signals) => boolean;
  };
  child.stdin = {
    end: () => {
      queueMicrotask(() => child.emit("exit", 0, null));
    },
  };
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.kill = (signal?: NodeJS.Signals) => {
    queueMicrotask(() => child.emit("exit", null, signal ?? "SIGKILL"));
    return true;
  };
  return child;
}

describe("OpenCode runtime", () => {
  it("detects and caches the bundled artifact without downgrading a healthy runtime", async () => {
    let resolutions = 0;
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: async () => { resolutions += 1; return { executable: "C:\\bundle\\bin\\opencode.exe", version: "1.18.4" }; },
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
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

  it("restarts after a managed stop releases the owned child", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });

    await runtime.start();
    processes[0]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
    expect(runtime.state).toBe("stopped");

    await runtime.start();
    expect(processes).toHaveLength(2);
    processes[1]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
  });

  it("restarts after an unexpected exit releases the owned child", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });

    await runtime.start();
    processes[0]?.process.emit("exit", 1, null);
    expect(runtime.state).toBe("crashed");

    await runtime.start();
    expect(processes).toHaveLength(2);
    processes[1]?.resolveWait({ code: 0, signal: null });
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

  it("waits for crash SSE cleanup before recreating runtime credentials", async () => {
    const firstCredentials = { username: "opencode" as const, password: "first-crash-canary" };
    const secondCredentials = { username: "opencode" as const, password: "second-crash-canary" };
    const credentials = [firstCredentials, secondCredentials];
    const processes: FakeProcess[] = [];
    let credentialCalls = 0;
    let resolveCancel!: () => void;
    const body = new ReadableStream<Uint8Array>({
      cancel: () => new Promise<void>((resolve) => { resolveCancel = resolve; }),
    });
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(),
      credentialsFactory: () => credentials[credentialCalls++]!,
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await runtime.start();
    await runtime.connectSse({
      onSignal: () => undefined,
      fetch: async () => new Response(body, { status: 200 }),
    });

    processes[0]?.process.emit("exit", 1, null);
    const restarting = runtime.start();
    await Promise.resolve();
    expect(credentialCalls).toBe(1);
    expect(processes).toHaveLength(1);

    resolveCancel();
    await restarting;
    expect(credentialCalls).toBe(2);
    expect(secondCredentials.password).toBe("second-crash-canary");
    processes[1]?.resolveWait({ code: 0, signal: null });
    await runtime.stop();
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
      spawn: (executable, args, options) => {
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
    });
    await runtime.start();
    expect(runtime.state).toBe("healthy");
    expect(processes[0]?.args).toEqual(["serve", "--hostname", "127.0.0.1", "--port", "0"]);
    expect(processes[0]?.env.OPENCODE_SERVER_USERNAME).toBe("opencode");
    expect(runtime.snapshot()).not.toMatchObject({ password: expect.anything() });
    expect(JSON.stringify(runtime.snapshot())).not.toContain(processes[0]?.env.OPENCODE_SERVER_PASSWORD ?? "impossible");
  });

  it("waits for the managed child to report its loopback address before sending health credentials", async () => {
    const child = createFakeNodeChild();
    let spawnedArgs: readonly string[] = [];
    const healthBaseUrls: string[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact("C:\\bundle\\opencode.exe"),
      cwd: "C:\\workspace",
      hostEnvironment: { PATH: "C:\\Windows" },
      trust: "trusted",
      spawn: createNodeProcessFactory((_executable, args) => {
        spawnedArgs = args;
        return child;
      }),
      checkHealth: async ({ baseUrl }) => {
        healthBaseUrls.push(baseUrl);
        return { version: "1.18.4" };
      },
    });

    const starting = runtime.start();
    await new Promise<void>((resolve) => setImmediate(resolve));
    expect(spawnedArgs).toEqual(["serve", "--hostname", "127.0.0.1", "--port", "0"]);
    expect(healthBaseUrls).toEqual([]);

    child.stdout.emit("data", "unrelated startup output\nopencode server listening on http://127.0.0.");
    child.stdout.emit("data", "1:43199\r\n");
    await starting;

    expect(healthBaseUrls).toEqual(["http://127.0.0.1:43199"]);
    await runtime.stop();
  });

  it("observes a real child exit queued by a synchronous factory before health begins", async () => {
    const child = new FakeProcess([], {}, false, false);
    child.kill = (signal?: NodeJS.Signals) => {
      child.killed.push(signal ?? "SIGKILL");
      child.resolveWait({ code: null, signal: signal ?? "SIGKILL" });
      return true;
    };
    let healthCalls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      stopTimeoutMs: 5,
      spawn: () => {
        queueMicrotask(() => child.process.emit("exit", 1, null));
        queueMicrotask(() => child.reportListening());
        return child;
      },
      checkHealth: async () => { healthCalls += 1; return { version: "1.18.4" }; },
    });

    try {
      await expect(runtime.start()).rejects.toMatchObject({ code: "TransportDisconnected" });
      expect(healthCalls).toBe(0);
    } finally {
      child.resolveWait({ code: 1, signal: null });
      await runtime.stop().catch(() => undefined);
    }
  });

  it("does not treat an ordinary child error queued by a synchronous factory as an exit", async () => {
    const child = new FakeProcess([], {}, false, false);
    let healthCalls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: () => {
        queueMicrotask(() => child.process.emit("error", new Error("transient child error")));
        queueMicrotask(() => child.reportListening());
        return child;
      },
      checkHealth: async () => { healthCalls += 1; return { version: "1.18.4" }; },
    });

    try {
      await expect(runtime.start()).resolves.toBeUndefined();
      expect(healthCalls).toBe(1);
      expect(child.disposed).toBe(false);
    } finally {
      child.process.emit("exit", 0, null);
      await runtime.stop().catch(() => undefined);
    }
  });

  it("accepts the pinned 1.18.4 startup line when fragmented across CRLF", async () => {
    const child = createFakeNodeChild();
    const factory = runtimeModule.createNodeProcessFactory(() => child);
    const port = factory("C:\\bundle\\bin\\opencode.exe", [], { cwd: "C:\\workspace", env: {} }) as OpenCodeProcess;

    try {
      child.stdout.emit("data", "opencode server listening on http://127.0.0.");
      child.stdout.emit("data", "1:43199\r\n");
      child.emit("exit", 1, null);
      await expect(port.listeningAddress).resolves.toBe(43199);
    } finally {
      port.dispose?.();
    }
  });

  it.each([
    ["the unpinned legacy prefix", "server listening on http://127.0.0.1:43199\n"],
    ["a non-loopback address", "opencode server listening on http://127.0.0.2:43199\n"],
    ["port zero", "opencode server listening on http://127.0.0.1:0\n"],
    ["an out-of-range port", "opencode server listening on http://127.0.0.1:65536\n"],
    ["trailing content", "opencode server listening on http://127.0.0.1:43199 ready\n"],
    ["no listening address", undefined],
  ])("does not send health credentials when stdout reports %s", async (_description, output) => {
    const child = createFakeNodeChild();
    let healthCalls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(),
      cwd: "C:\\workspace",
      hostEnvironment: { PATH: "x" },
      trust: "trusted",
      readinessTimeoutMs: 10,
      stopTimeoutMs: 10,
      spawn: createNodeProcessFactory(() => child),
      checkHealth: async () => {
        healthCalls += 1;
        return { version: "1.18.4" };
      },
    });

    const starting = runtime.start();
    await new Promise<void>((resolve) => setImmediate(resolve));
    if (output !== undefined) child.stdout.emit("data", output);
    await expect(starting).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(healthCalls).toBe(0);
  });

  it("does not send health credentials when the managed child exits after reporting its address", async () => {
    const child = createFakeNodeChild();
    const credentials = { username: "opencode" as const, password: "exit-before-health-canary" };
    let healthCalls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(),
      credentialsFactory: () => credentials,
      cwd: "C:\\workspace",
      hostEnvironment: { PATH: "x" },
      trust: "trusted",
      spawn: createNodeProcessFactory(() => child),
      checkHealth: async () => {
        healthCalls += 1;
        return { version: "1.18.4" };
      },
    });

    const starting = runtime.start();
    await new Promise<void>((resolve) => setImmediate(resolve));
    child.stdout.emit("data", "opencode server listening on http://127.0.0.1:43199\n");
    child.emit("exit", 1, null);

    expect(credentials.password).toBe("");
    await expect(starting).rejects.toMatchObject({ code: "TransportDisconnected" });
    expect(healthCalls).toBe(0);
  });

  it("retries port conflicts at most three times", async () => {
    let calls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: () => { calls += 1; const error = Object.assign(new Error("busy"), { code: "EADDRINUSE" }); throw error; },
      checkHealth: async () => ({ version: "1.18.4" }),
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(calls).toBe(3);
  });

  it("does not infer a port conflict from an arbitrary exception message", async () => {
    let calls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: () => { calls += 1; throw new Error("diagnostic text mentions EADDRINUSE out of context"); },
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(calls).toBe(1);
  });

  it("retries when a spawned child later reports a structured port conflict", async () => {
    let calls = 0;
    let healthCalls = 0;
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: (_executable, args, options) => {
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

  it("clears credentials and maps spawn rejection after credential creation", async () => {
    const credentials = { username: "opencode" as const, password: "spawn-canary" };
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(),
      credentialsFactory: () => credentials,
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: () => { throw new Error(credentials.password); },
    } as never);
    let failure: unknown;
    try { await runtime.start(); } catch (error) { failure = error; }
    expect(failure).toMatchObject({ code: "RuntimeUnavailable" });
    expect(String(failure)).not.toContain("spawn-canary");
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
      spawn: () => child,
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
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }) });
    await runtime.start();
    processes[0]?.process.emit("exit", 1, null);
    expect(runtime.state).toBe("crashed");
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "TransportDisconnected" } });
    await runtime.stop();
    expect(runtime.snapshot()).toEqual({ state: "stopped" });
  });

  it("does not report stopped when SIGKILL succeeds but the child never exits", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", spawn: processFactory({ processes, waitForever: true }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
    await runtime.start();
    await expect(runtime.stop()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(processes[0]?.killed).toContain("SIGKILL");
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "RuntimeUnavailable" } });
  });

  it("retains an unexited child for repeated failed termination attempts", async () => {
    const credentials = { username: "opencode" as const, password: "retained-stop-canary" };
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(), credentialsFactory: () => credentials,
      cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted",
      spawn: processFactory({ processes, waitForever: true }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5,
    });
    await runtime.start();
    const child = processes[0]!;

    await expect(runtime.stop()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(child.killed).toEqual(["SIGKILL"]);
    expect(child.disposed).toBe(false);
    expect(credentials.password).toBe("");

    await expect(runtime.stop()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(child.killed).toEqual(["SIGKILL", "SIGKILL"]);
    expect(child.disposed).toBe(false);
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "RuntimeUnavailable" } });

    child.process.emit("exit", null, "SIGKILL");
    expect(child.disposed).toBe(true);
    expect(child.process.listenerCount("error")).toBe(0);
    expect(child.process.listenerCount("close")).toBe(0);
    expect(runtime.snapshot()).toEqual({ state: "crashed", error: { code: "RuntimeUnavailable" } });
  });

  it("reports stopped only after observing a delayed post-kill exit", async () => {
    const processes: FakeProcess[] = [];
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
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
    const runtime = new OpenCodeRuntime({ resolveArtifact: testArtifact(), cwd: "C:\\workspace", hostEnvironment: { PATH: "x" }, trust: "trusted", spawn: processFactory({ processes }), checkHealth: async () => ({ version: "1.18.4" }), stopTimeoutMs: 5 });
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

  it("bounds default-adapter stdout decoding before appending an overlong line", async () => {
    const child = createFakeNodeChild();
    const factory = runtimeModule.createNodeProcessFactory(() => child);
    const port = factory("C:\\bundle\\bin\\opencode.exe", [], { cwd: "C:\\workspace", env: {} }) as OpenCodeProcess;
    const overlong = Buffer.alloc(8_192, "a");
    const originalToString = overlong.toString.bind(overlong);
    let decodedBeyondLimit = false;
    overlong.toString = (encoding, start, end) => {
      if ((end ?? overlong.length) - (start ?? 0) > 4_096) decodedBeyondLimit = true;
      return originalToString(encoding, start, end);
    };

    child.stdout.emit("data", overlong);
    expect(decodedBeyondLimit).toBe(false);
    child.stdout.emit("data", "\nopencode server listening on http://127.0.0.1:43199\n");
    await expect(port.listeningAddress).resolves.toBe(43199);
    port.dispose?.();
  });

  it("bounds default-adapter stderr decoding before appending an overlong chunk", async () => {
    const child = createFakeNodeChild();
    const factory = runtimeModule.createNodeProcessFactory(() => child);
    const port = factory("C:\\bundle\\bin\\opencode.exe", [], { cwd: "C:\\workspace", env: {} }) as OpenCodeProcess;
    const overlong = Buffer.alloc(8_192, "a");
    const originalToString = overlong.toString.bind(overlong);
    let decodedBeyondLimit = false;
    overlong.toString = (encoding, start, end) => {
      if ((end ?? overlong.length) - (start ?? 0) > 4_096) decodedBeyondLimit = true;
      return originalToString(encoding, start, end);
    };

    try {
      child.stderr.emit("data", "Error: listen EADDRINUSE: address already in use 127.0.0.1\n");
      await expect(port.startup).resolves.toBe("port-conflict");
      child.stderr.emit("data", overlong);
      expect(decodedBeyondLimit).toBe(false);
    } finally {
      port.dispose?.();
    }
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
      spawn: () => child,
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
      spawn: processFactory({ processes: crashedProcesses }),
      checkHealth: async ({ credentials }) => { crashedCredentials = credentials; return { version: "1.18.4" }; },
    });
    await crashed.start();
    crashedProcesses[0]?.process.emit("exit", 1, null);
    await new Promise<void>((resolve) => setImmediate(resolve));
    expect(crashedCredentials?.password).toBe("");
  });

  it("creates a Main-owned session adapter with fixed workspace requests", async () => {
    const processes: FakeProcess[] = [];
    const credentials = { username: "opencode" as const, password: "session-auth-canary" };
    const requests: Array<{ readonly url: string; readonly init: RequestInit | undefined }> = [];
    const runtime = new OpenCodeRuntime({
      resolveArtifact: testArtifact(),
      credentialsFactory: () => credentials,
      cwd: "C:\\workspace",
      hostEnvironment: { PATH: "x" },
      trust: "trusted",
      spawn: processFactory({ processes }),
      checkHealth: async () => ({ version: "1.18.4" }),
    });

    expect(() => runtime.createSessionAdapter()).toThrow(expect.objectContaining({ code: "RuntimeUnavailable" }));

    vi.stubGlobal("fetch", async (input: string | URL | Request, init?: RequestInit) => {
      const url = input instanceof Request ? input.url : String(input);
      requests.push({ url, init });
      const pathname = new URL(url).pathname;
      if (pathname === "/session") {
        return new Response(JSON.stringify([{
          id: "s1",
          title: "Public session",
          time: { updated: 0 },
          directory: "C:\\private\\runtime-path-canary",
          provider: "provider-canary",
          model: "model-canary",
        }]), { status: 200 });
      }
      if (pathname === "/session/s1/prompt_async") return new Response(null, { status: 204 });
      return new Response(null, { status: 404 });
    });

    try {
      await runtime.start();
      const adapter = runtime.createSessionAdapter();
      const sessions = await adapter.list();
      await adapter.startPrompt("s1", "Summarize this change.");

      const listRequest = requests[0];
      expect(listRequest).toBeDefined();
      if (!listRequest) return;
      const listUrl = new URL(listRequest.url);
      expect(listUrl.pathname).toBe("/session");
      expect(listUrl.searchParams.get("directory")).toBe("C:\\workspace");
      const listHeaders = new Headers(listRequest.init?.headers);
      expect(listHeaders.get("authorization")).toBe(`Basic ${Buffer.from("opencode:session-auth-canary").toString("base64")}`);
      expect(listHeaders.get("accept")).toBe("application/json");

      const promptRequest = requests.find((request) => new URL(request.url).pathname === "/session/s1/prompt_async");
      expect(promptRequest).toBeDefined();
      if (!promptRequest) return;
      expect(JSON.parse(String(promptRequest.init?.body))).toEqual({
        parts: [{ type: "text", text: "Summarize this change." }],
      });
      expect(new Headers(promptRequest.init?.headers).get("content-type")).toBe("application/json");

      const serialised = JSON.stringify({ adapter, sessions });
      for (const forbidden of ["session-auth-canary", "runtime-path-canary", "provider-canary", "model-canary", "43123", "C:\\workspace"]) {
        expect(serialised).not.toContain(forbidden);
      }
    } finally {
      vi.unstubAllGlobals();
      processes[0]?.resolveWait({ code: 0, signal: null });
      await runtime.stop();
    }
  });
});
