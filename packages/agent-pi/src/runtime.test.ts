import { EventEmitter } from "node:events";
import { describe, expect, it, vi } from "vitest";
import { PiRuntime } from "./runtime.js";
import type { ProcessPort } from "./jsonlTransport.js";
import { detectPi, PiProbeCleanupError, type PiExecutableResolver, type PiHostExecutableName } from "./detect.js";

const testPiDirectory = process.platform === "win32" ? "C:\\host-bin" : "/host-bin";
const testPiExecutableFor = (name: PiHostExecutableName): string => `${testPiDirectory}${process.platform === "win32" ? "\\" : "/"}${name}`;
const testPiExecutable = testPiExecutableFor("pi");
const testPiResolver: PiExecutableResolver = async (name) => [testPiExecutableFor(name)];
const detectPiForTest = (options: Parameters<typeof detectPi>[0]) => detectPi({
  ...options,
  resolveExecutables: testPiResolver,
});

class RuntimePort implements ProcessPort {
  readonly stdin = {
    writes: [] as string[],
    write: (value: string) => {
      this.stdin.writes.push(value);
      const command = JSON.parse(value) as { id: string; type: string };
      queueMicrotask(() => {
        if (command.type === "get_state") {
          this.stdout.emit("data", JSON.stringify({ type: "response", id: command.id, command: "get_state", success: true, data: {} }) + "\n");
        }
      });
    },
    end: () => { this.ended = true; },
  };
  readonly stdout = new EventEmitter();
  readonly stderr = new EventEmitter();
  readonly process = new EventEmitter();
  ended = false;
  kill = () => { this.process.emit("exit", 1, "SIGTERM"); };
  wait = async () => ({ code: 0, signal: null });
}

describe("Pi runtime lifecycle", () => {
  it("uses get_state readiness and only settles a run on agent_settled", async () => {
    const port = new RuntimePort();
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace",
      session: "session",
      model: "openai/gpt-4o",
      thinking: "medium",
      trust: "untrusted",
      hostEnvironment: { PATH: "C:/bin" },
    });
    await runtime.start();
    expect(runtime.state).toBe("ready");
    const prompt = runtime.prompt("hello");
    const promptCommand = JSON.parse(port.stdin.writes.at(-1)!) as { id: string; type: string };
    expect(promptCommand.type).toBe("prompt");
    port.stdout.emit("data", JSON.stringify({ type: "agent_start", data: {} }) + "\n");
    expect(runtime.running).toBe(true);
    port.stdout.emit("data", JSON.stringify({ type: "agent_end", data: { willRetry: true } }) + "\n");
    expect(runtime.running).toBe(true);
    port.stdout.emit("data", JSON.stringify({ type: "agent_settled", data: {} }) + "\n");
    expect(runtime.running).toBe(false);
    const id = promptCommand.id;
    port.stdout.emit("data", JSON.stringify({ type: "response", id, command: "prompt", success: true }) + "\n");
    await expect(prompt).resolves.toMatchObject({ command: "prompt" });
    await runtime.stop();
    expect(runtime.state).toBe("stopped");
    expect(port.ended).toBe(true);
  });

  it("fails closed on an incompatible PATH version and tries pi.exe after pi", async () => {
    const calls: string[] = [];
    let receivedEnv: Readonly<Record<string, string>> | undefined;
    const ports = new Map<string, RuntimePort>();
    const detection = await detectPiForTest({
      hostEnvironment: { PATH: "C:/bin" },
      processFactory: (executable, _args, options) => {
        calls.push(executable);
        receivedEnv = options.env;
        const port = new RuntimePort();
        ports.set(executable, port);
        queueMicrotask(() => {
          const output = executable === testPiExecutable ? "pi 0.80.0\n" : "pi 0.81.1\n";
          port.stdout.emit("data", output);
          port.stdout.emit("end");
          port.stderr.emit("end");
        });
        return port;
      },
    });
    expect(calls).toEqual([testPiExecutableFor("pi"), testPiExecutableFor("pi.exe")]);
    expect(receivedEnv).toEqual({ PATH: "C:/bin" });
    expect(detection).toMatchObject({ status: "detected", source: "system", executable: testPiExecutableFor("pi.exe"), version: "0.81.1" });
  });

  it("keeps provider values out of direct version detection", async () => {
    const canary = "provider-canary-never-for-version";
    const receivedEnvironments: Array<Readonly<Record<string, string>> | undefined> = [];
    const detection = await detectPiForTest({
      hostEnvironment: { PATH: "C:/bin", OPENAI_API_KEY: "host-value-must-not-leak" },
      processFactory: (_executable, _args, options) => {
        receivedEnvironments.push(options.env);
        const port = new RuntimePort();
        queueMicrotask(() => {
          port.stdout.emit("data", "pi 0.81.1\n");
          port.stdout.emit("end");
          port.stderr.emit("end");
        });
        return port;
      },
      // JavaScript callers can still provide unknown properties; discovery
      // must ignore them as well as rejecting them at the TypeScript boundary.
      providerEnvironment: { OPENAI_API_KEY: canary },
      allowedProviderKeys: new Set(["OPENAI_API_KEY"]),
    } as never);

    expect(detection).toMatchObject({ status: "detected", executable: testPiExecutable });
    expect(receivedEnvironments).toEqual([{ PATH: "C:/bin" }]);
    expect(JSON.stringify(receivedEnvironments)).not.toContain(canary);
  });

  it("keeps legacy provider properties out of PiRuntime detection", async () => {
    const canary = "provider-canary-never-for-runtime-detect";
    const receivedEnvironments: Array<Readonly<Record<string, string>> | undefined> = [];
    const runtime = new PiRuntime({
      spawn: (_executable, _args, options) => {
        receivedEnvironments.push(options.env);
        const port = new RuntimePort();
        queueMicrotask(() => {
          port.stdout.emit("data", "pi 0.81.1\n");
          port.stdout.emit("end");
          port.stderr.emit("end");
        });
        return port;
      },
      cwd: "C:/workspace",
      session: "s",
      model: "m",
      thinking: "low",
      trust: "trusted",
      hostEnvironment: { PATH: "C:/bin", OPENAI_API_KEY: "host-provider-value-must-not-leak" },
      resolveExecutables: testPiResolver,
      providerEnvironment: { OPENAI_API_KEY: canary },
      allowedProviderKeys: new Set(["OPENAI_API_KEY"]),
    } as never);

    await expect(runtime.detect()).resolves.toMatchObject({ status: "detected", executable: testPiExecutable });
    expect(receivedEnvironments).toEqual([{ PATH: "C:/bin" }]);
    expect(JSON.stringify(receivedEnvironments)).not.toContain(canary);
  });

  it("waits for stdout close after process exit before parsing the exact version", async () => {
    const calls: string[] = [];
    const detection = await detectPiForTest({
      hostEnvironment: { PATH: "C:/bin" },
      processFactory: (executable) => {
        calls.push(executable);
        const stdout = new EventEmitter();
        const stderr = new EventEmitter();
        const process = new EventEmitter();
        const wait = new Promise<{ code: number; signal: null }>((resolve) => {
          process.once("exit", () => resolve({ code: 0, signal: null }));
        });
        queueMicrotask(() => {
          process.emit("exit", 0, null);
          queueMicrotask(() => {
            stdout.emit("data", "pi 0.81.1\n");
            stdout.emit("end");
            stderr.emit("end");
            process.emit("close");
          });
        });
        return { stdin: { write: () => undefined, end: () => undefined }, stdout, stderr, process, wait: () => wait };
      },
    });
    expect(calls).toEqual([testPiExecutable]);
    expect(detection).toMatchObject({ status: "detected", executable: testPiExecutable, version: "0.81.1" });
  });

  it("fails closed on a nonzero top-level process exit without wait()", async () => {
    const detection = await detectPiForTest({
      hostEnvironment: { PATH: "C:/bin" },
      processFactory: () => {
        const stdout = new EventEmitter();
        const stderr = new EventEmitter();
        const process = new EventEmitter();
        queueMicrotask(() => {
          process.emit("exit", 1, null);
          stdout.emit("data", "pi 0.81.1\n");
          stdout.emit("end");
          stderr.emit("end");
          process.emit("close");
        });
        return { stdin: { write: () => undefined, end: () => undefined }, stdout, stderr, process };
      },
    });
    expect(detection).toMatchObject({ status: "unavailable", source: "managed" });
  });

  it("removes probe listeners when process events are exposed at the port level", async () => {
    const processEvents = new EventEmitter();
    const stdout = new EventEmitter();
    const stderr = new EventEmitter();
    const port: ProcessPort = {
      stdin: { write: () => undefined, end: () => undefined },
      stdout,
      stderr,
      on: processEvents.on.bind(processEvents),
      off: processEvents.off.bind(processEvents),
      removeListener: processEvents.removeListener.bind(processEvents),
    };
    const detection = await detectPiForTest({
      hostEnvironment: { PATH: "C:/bin" },
      processFactory: () => {
        queueMicrotask(() => {
          processEvents.emit("exit", 0, null);
          stdout.emit("data", "pi 0.81.1\n");
          stdout.emit("end");
          stderr.emit("end");
          processEvents.emit("close");
        });
        return port;
      },
    });
    expect(detection).toMatchObject({ status: "detected", executable: testPiExecutable, version: "0.81.1" });
    expect(processEvents.listenerCount("exit")).toBe(0);
    expect(processEvents.listenerCount("close")).toBe(0);
    expect(stdout.listenerCount("data")).toBe(0);
    expect(stdout.listenerCount("end")).toBe(0);
    expect(stderr.listenerCount("end")).toBe(0);
  });

  it("fails closed when a timed-out version probe cannot prove its child exited", async () => {
    const killed: string[] = [];
    await expect(detectPiForTest({
      hostEnvironment: { PATH: "C:/bin" },
      timeoutMs: 5,
      processFactory: (executable) => {
        const port = new RuntimePort();
        port.wait = () => new Promise(() => undefined);
        port.kill = () => { killed.push(executable); };
        return port;
      },
    })).rejects.toBeInstanceOf(PiProbeCleanupError);
    expect(killed).toEqual([testPiExecutable]);
  });

  it("absorbs a rejected stdin end during a timed-out version probe", async () => {
    const unhandled: unknown[] = [];
    const onUnhandled = (reason: unknown): void => { unhandled.push(reason); };
    process.on("unhandledRejection", onUnhandled);
    try {
      await expect(detectPiForTest({
        hostEnvironment: { PATH: "C:/bin" },
        timeoutMs: 1,
        processFactory: () => {
          const port = new RuntimePort();
          port.wait = () => new Promise(() => undefined);
          port.stdin.end = () => Promise.reject(new Error("end failed"));
          return port;
        },
      })).rejects.toBeInstanceOf(PiProbeCleanupError);
      await Promise.resolve();
      expect(unhandled).toHaveLength(0);
    } finally {
      process.off("unhandledRejection", onUnhandled);
    }
  });

  it("confirms a timed-out version probe has exited before trying the next candidate", async () => {
    const calls: string[] = [];
    let firstClosed = false;
    const detection = await detectPiForTest({
      hostEnvironment: { PATH: "C:/bin" },
      timeoutMs: 1,
      processFactory: (executable) => {
        calls.push(executable);
        if (executable === testPiExecutable) {
          const port = new RuntimePort();
          let resolveWait!: (exit: { code: number; signal: null }) => void;
          const wait = new Promise<{ code: number; signal: null }>((resolve) => { resolveWait = resolve; });
          port.wait = () => wait;
          port.kill = () => {
            queueMicrotask(() => {
              firstClosed = true;
              resolveWait({ code: 0, signal: null });
            });
            return true;
          };
          return port;
        }
        expect(firstClosed).toBe(true);
        const port = new RuntimePort();
        queueMicrotask(() => {
          port.stdout.emit("data", "pi 0.81.1\n");
          port.stdout.emit("end");
          port.stderr.emit("end");
        });
        return port;
      },
    });
    expect(calls).toEqual([testPiExecutableFor("pi"), testPiExecutableFor("pi.exe")]);
    expect(detection).toMatchObject({ status: "detected", executable: testPiExecutableFor("pi.exe") });
  });

  it("accepts only the exact Pi version output format", async () => {
    const detection = await detectPiForTest({
      hostEnvironment: { PATH: "C:/bin" },
      processFactory: () => {
        const port = new RuntimePort();
        queueMicrotask(() => {
          port.stdout.emit("data", "garbage 0.81.1\n");
          port.stdout.emit("end");
          port.stderr.emit("end");
        });
        return port;
      },
    });
    expect(detection).toMatchObject({ status: "unavailable", source: "managed" });
  });

  it("rejects direct detection without an explicit runtime environment", async () => {
    await expect(detectPiForTest({ processFactory: () => { throw new Error("must not spawn"); } })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("rejects raw environment injection instead of passing arbitrary keys to the probe", async () => {
    await expect(detectPiForTest({ env: { SECRET: "x" }, processFactory: () => { throw new Error("must not spawn"); } } as never)).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("re-probes supplied detection before starting a process", async () => {
    const probes: string[][] = [];
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      spawn: (_executable, args) => {
        probes.push([...args]);
        const port = new RuntimePort();
        if (args[0] === "--version") {
          queueMicrotask(() => {
            port.stdout.emit("data", "pi 0.80.0\n");
            port.stdout.emit("end");
            port.stderr.emit("end");
            port.process.emit("exit", 0, null);
            port.process.emit("close");
          });
        }
        return port;
      },
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      resolveExecutables: testPiResolver,
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(probes.length).toBe(2);
    expect(probes.every((args) => args[0] === "--version")).toBe(true);
  });

  it("passes trust policy and a whitelisted environment into stable startup arguments", async () => {
    const port = new RuntimePort();
    let spawnArgs: readonly string[] = [];
    let spawnOptions: { cwd?: string; env?: Readonly<Record<string, string>> } = {};
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: (_executable, args, options) => { spawnArgs = args; spawnOptions = options; return port; },
      cwd: "C:/workspace",
      session: "s",
      model: "m",
      thinking: "high",
      trust: "untrusted",
      hostEnvironment: { PATH: "C:/bin", SECRET: "not-allowed" },
      resolveRpcLaunch: async () => ({
        model: "m",
        thinking: "high",
        providerEnvironment: { OPENAI_API_KEY: "key" },
        allowedProviderKeys: new Set(["OPENAI_API_KEY"]),
      }),
    });
    await runtime.start();
    expect(spawnArgs).toEqual(["--mode", "rpc", "--session-id", "s", "--model", "m", "--thinking", "high", "--no-approve", "--no-context-files"]);
    expect(spawnOptions).toMatchObject({ cwd: "C:/workspace", env: { PATH: "C:/bin", OPENAI_API_KEY: "key" } });
    expect(spawnOptions.env).not.toHaveProperty("SECRET");
  });

  it("resolves provider values only for the confirmed RPC spawn", async () => {
    const canary = "provider-canary-rpc-only";
    const spawns: Array<{ readonly executable: string; readonly args: readonly string[]; readonly env: Readonly<Record<string, string>> | undefined }> = [];
    let resolverCalls = 0;
    const runtime = new PiRuntime({
      spawn: (executable, args, options) => {
        spawns.push({ executable, args, env: options.env });
        const port = new RuntimePort();
        if (args[0] === "--version") {
          queueMicrotask(() => {
            port.stdout.emit("data", "pi 0.81.1\n");
            port.stdout.emit("end");
            port.stderr.emit("end");
          });
        }
        return port;
      },
      cwd: "C:/workspace",
      session: "s",
      trust: "trusted",
      hostEnvironment: { PATH: "C:/bin" },
      resolveExecutables: testPiResolver,
      resolveRpcLaunch: async () => {
        resolverCalls += 1;
        return {
          model: "m",
          thinking: "low",
          providerEnvironment: { OPENAI_API_KEY: canary },
          allowedProviderKeys: new Set(["OPENAI_API_KEY"]),
        };
      },
    });

    await runtime.start();

    const versionSpawns = spawns.filter(({ args }) => args[0] === "--version");
    const rpcSpawn = spawns.find(({ args }) => args[0] === "--mode");
    expect(versionSpawns).not.toHaveLength(0);
    expect(versionSpawns.every(({ env }) => env?.OPENAI_API_KEY === undefined)).toBe(true);
    expect(JSON.stringify(versionSpawns)).not.toContain(canary);
    expect(rpcSpawn).toMatchObject({ env: { PATH: "C:/bin", OPENAI_API_KEY: canary } });
    expect(rpcSpawn?.args.slice(0, 8)).toEqual([
      "--mode", "rpc", "--session-id", "s", "--model", "m", "--thinking", "low",
    ]);
    expect(rpcSpawn?.executable).toBe(testPiExecutable);
    expect(resolverCalls).toBe(1);
  });

  it("marks unexpected process exit as crashed and stop closes stdin before waiting", async () => {
    const port = new RuntimePort();
    let waited = false;
    port.wait = async () => { waited = true; return { code: 0, signal: null }; };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    await runtime.start();
    port.process.emit("exit", 2, null);
    expect(runtime.state).toBe("crashed");
    await runtime.stop();
    expect(port.ended).toBe(true);
    expect(waited).toBe(true);
  });

  it("notifies Main only after crash cleanup confirms the child has exited", async () => {
    const port = new RuntimePort();
    let resolveWait!: (exit: { code: number; signal: null }) => void;
    const wait = new Promise<{ code: number; signal: null }>((resolve) => { resolveWait = resolve; });
    port.wait = () => wait;
    let crashes = 0;
    let notifyCrashed!: () => void;
    const crashed = new Promise<void>((resolve) => { notifyCrashed = resolve; });
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      onCrashed: () => { crashes += 1; notifyCrashed(); },
    });
    await runtime.start();
    port.stdout.emit("end");
    await Promise.resolve();
    expect(runtime.state).toBe("crashed");
    expect(crashes).toBe(0);

    resolveWait({ code: 0, signal: null });
    await crashed;
    expect(crashes).toBe(1);
  });

  it("retains failed crash cleanup for an explicit stop retry", async () => {
    const port = new RuntimePort();
    let exited = false;
    port.wait = async () => {
      if (exited) return { code: 0, signal: null };
      return new Promise(() => undefined);
    };
    port.kill = () => false;
    let crashes = 0;
    let notifyFailure!: () => void;
    const cleanupFailed = new Promise<void>((resolve) => { notifyFailure = resolve; });
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      stopTimeoutMs: 1,
      onCrashed: () => { crashes += 1; },
      onCrashCleanupFailed: notifyFailure,
    });
    await runtime.start();
    port.stdout.emit("end");
    await cleanupFailed;
    expect(runtime.state).toBe("crashed");
    expect(crashes).toBe(0);
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });

    exited = true;
    await runtime.stop();
    expect(runtime.state).toBe("stopped");
  });

  it("terminates the process after graceful stop times out", async () => {
    const port = new RuntimePort();
    let killed = false;
    port.wait = () => new Promise((resolve) => { port.process.once("exit", () => resolve({ code: 0, signal: "SIGTERM" })); });
    port.kill = () => { killed = true; port.process.emit("exit", 0, "SIGTERM"); };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      stopTimeoutMs: 1,
    });
    await runtime.start();
    await runtime.stop();
    expect(port.ended).toBe(true);
    expect(killed).toBe(true);
    expect(runtime.state).toBe("stopped");
  });

  it("clears the stdin end timeout after an immediately settled Promise", async () => {
    vi.useFakeTimers();
    try {
      const port = new RuntimePort();
      port.stdin.end = () => Promise.resolve();
      const runtime = new PiRuntime({
        detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
        detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
        spawn: () => port,
        cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
        stopTimeoutMs: 5_000,
      });
      await runtime.start();
      await runtime.stop();
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("kills even when stdin.end never completes", async () => {
    const port = new RuntimePort();
    let killed = false;
    port.stdin.end = () => new Promise(() => undefined);
    port.wait = () => new Promise((resolve) => { port.process.once("exit", () => resolve({ code: 0, signal: "SIGTERM" })); });
    port.kill = () => { killed = true; port.process.emit("exit", 0, "SIGTERM"); };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      stopTimeoutMs: 1,
    });
    await runtime.start();
    await runtime.stop();
    expect(killed).toBe(true);
    expect(runtime.state).toBe("stopped");
  });

  it("does not report stopped when kill fails", async () => {
    const port = new RuntimePort();
    port.wait = () => new Promise(() => undefined);
    port.kill = () => false;
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      stopTimeoutMs: 1,
    });
    await runtime.start();
    await expect(runtime.stop()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(runtime.state).toBe("crashed");
  });

  it("bounds a pending kill during stop and reports crashed", async () => {
    const port = new RuntimePort();
    let killed = false;
    let rejectKill!: (error: Error) => void;
    port.wait = () => new Promise(() => undefined);
    port.kill = () => {
      killed = true;
      return new Promise<boolean>((_resolve, reject) => { rejectKill = reject; });
    };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      stopTimeoutMs: 1,
    });
    await runtime.start();
    const result = await Promise.race([
      runtime.stop().then(() => "resolved", () => "rejected"),
      new Promise<string>((resolve) => setTimeout(() => resolve("timeout"), 100)),
    ]);
    expect(result).toBe("rejected");
    expect(killed).toBe(true);
    expect(runtime.state).toBe("crashed");
    rejectKill(new Error("late kill failure"));
    await Promise.resolve();
  });

  it("attempts to kill when wait rejects during stop", async () => {
    const port = new RuntimePort();
    let killed = false;
    port.wait = () => Promise.reject(new Error("wait failed"));
    port.kill = () => {
      killed = true;
      port.process.emit("exit", 1, "SIGTERM");
      return true;
    };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      stopTimeoutMs: 1,
    });
    await runtime.start();
    await expect(runtime.stop()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(killed).toBe(true);
    expect(runtime.state).toBe("crashed");
  });

  it("fails readiness as crashed when get_state times out and terminates the process", async () => {
    const port = new RuntimePort();
    port.stdin.write = (value: string) => { port.stdin.writes.push(value); };
    let killed = false;
    port.kill = () => { killed = true; };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      readinessTimeoutMs: 1,
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "TransportDisconnected" });
    expect(runtime.state).toBe("crashed");
    expect(port.ended).toBe(true);
    expect(killed).toBe(true);
  });

  it("bounds a pending kill while terminating a failed start", async () => {
    const port = new RuntimePort();
    port.stdin.write = (value: string) => { port.stdin.writes.push(value); };
    port.wait = () => new Promise(() => undefined);
    port.kill = () => new Promise<boolean>(() => undefined);
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      readinessTimeoutMs: 1,
      stopTimeoutMs: 1,
    });
    const result = await Promise.race([
      runtime.start().then(() => "resolved", () => "rejected"),
      new Promise<string>((resolve) => setTimeout(() => resolve("timeout"), 100)),
    ]);
    expect(result).toBe("rejected");
    expect(runtime.state).toBe("crashed");
  });

  it("does not report failed-start cleanup when wait never settles and kill is unavailable", async () => {
    const port = new RuntimePort();
    port.stdin.write = (value: string) => { port.stdin.writes.push(value); };
    port.wait = () => new Promise(() => undefined);
    port.kill = undefined;
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      readinessTimeoutMs: 1,
      stopTimeoutMs: 1,
    });
    const result = await Promise.race([
      runtime.start().then(() => "resolved", () => "rejected"),
      new Promise<string>((resolve) => setTimeout(() => resolve("timeout"), 100)),
    ]);
    expect(result).toBe("rejected");
    expect(runtime.state).toBe("crashed");
  });

  it("rejects a failed get_state response and never becomes ready", async () => {
    const port = new RuntimePort();
    port.stdin.write = (value: string) => {
      port.stdin.writes.push(value);
      const command = JSON.parse(value) as { id: string; type: string };
      if (command.type === "get_state") queueMicrotask(() => port.stdout.emit("data", JSON.stringify({ type: "response", id: command.id, command: "get_state", success: false, error: "unavailable" }) + "\n"));
    };
    let killed = false;
    port.kill = () => { killed = true; };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(runtime.state).toBe("crashed");
    expect(port.ended).toBe(true);
    expect(killed).toBe(true);
    await expect(runtime.prompt("after failure")).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("does not become ready after the process exits with the readiness response", async () => {
    const port = new RuntimePort();
    port.stdin.write = (value: string) => {
      port.stdin.writes.push(value);
      const command = JSON.parse(value) as { id: string; type: string };
      if (command.type === "get_state") queueMicrotask(() => {
        port.stdout.emit("data", JSON.stringify({ type: "response", id: command.id, command: "get_state", success: true, data: {} }) + "\n");
        port.process.emit("exit", 1, null);
        port.process.emit("close");
      });
    };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "TransportDisconnected" });
    expect(runtime.state).toBe("crashed");
    await expect(runtime.prompt("after exit")).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("does not spawn twice and does not downgrade a running state on detect", async () => {
    const port = new RuntimePort();
    let spawns = 0;
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
      detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
      spawn: () => { spawns += 1; return port; },
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    await runtime.start();
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(spawns).toBe(1);
    await expect(runtime.detect()).resolves.toMatchObject({ status: "detected" });
    expect(runtime.state).toBe("ready");
  });

  it("serializes concurrent start calls and waits for start before stop", async () => {
    const port = new RuntimePort();
    let resolveDetect!: (detection: { status: "detected"; source: "system"; executable: string; version: string }) => void;
    let spawns = 0;
    const runtime = new PiRuntime({
      detect: () => new Promise((resolve) => { resolveDetect = resolve; }),
      spawn: () => { spawns += 1; return port; },
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    const first = runtime.start();
    const second = runtime.start();
    const stopping = runtime.stop();
    await Promise.resolve();
    expect(spawns).toBe(0);
    expect(runtime.state).toBe("unavailable");
    resolveDetect({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" });
    await expect(first).resolves.toBeUndefined();
    await expect(second).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    await expect(stopping).resolves.toBeUndefined();
    expect(spawns).toBe(1);
    expect(runtime.state).toBe("stopped");
  });

  it("serializes detect and stop without allowing unavailable to reappear", async () => {
    let resolveDetect!: (detection: { status: "unavailable"; source: "managed"; managedInstall: "available" }) => void;
    const runtime = new PiRuntime({
      detect: () => new Promise((resolve) => { resolveDetect = resolve; }),
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    const detecting = runtime.detect();
    const stopping = runtime.stop();
    await Promise.resolve();
    expect(runtime.state).toBe("unavailable");
    resolveDetect({ status: "unavailable", source: "managed", managedInstall: "available" });
    await expect(detecting).resolves.toMatchObject({ status: "unavailable" });
    await expect(stopping).resolves.toBeUndefined();
    expect(runtime.state).toBe("stopped");
  });
});
