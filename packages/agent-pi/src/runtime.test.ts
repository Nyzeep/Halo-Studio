import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";
import { PiRuntime } from "./runtime.js";
import type { ProcessPort } from "./jsonlTransport.js";
import { detectPi } from "./detect.js";

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
      detection: { status: "detected", source: "system", executable: "pi", version: "0.81.1" },
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
    const ports = new Map<string, RuntimePort>();
    const detection = await detectPi({
      processFactory: (executable) => {
        calls.push(executable);
        const port = new RuntimePort();
        ports.set(executable, port);
        queueMicrotask(() => {
          const output = executable === "pi" ? "pi 0.80.0\n" : "pi 0.81.1\n";
          port.stdout.emit("data", output);
          port.stdout.emit("end");
          port.stderr.emit("end");
        });
        return port;
      },
    });
    expect(calls).toEqual(["pi", "pi.exe"]);
    expect(detection).toMatchObject({ status: "detected", source: "system", executable: "pi.exe", version: "0.81.1" });
  });

  it("waits for stdout close after process exit before parsing the exact version", async () => {
    const calls: string[] = [];
    const detection = await detectPi({
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
    expect(calls).toEqual(["pi"]);
    expect(detection).toMatchObject({ status: "detected", executable: "pi", version: "0.81.1" });
  });

  it("fails closed on a nonzero top-level process exit without wait()", async () => {
    const detection = await detectPi({
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

  it("passes trust policy and a whitelisted environment into stable startup arguments", async () => {
    const port = new RuntimePort();
    let spawnArgs: readonly string[] = [];
    let spawnOptions: { cwd?: string; env?: Readonly<Record<string, string>> } = {};
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: "pi", version: "0.81.1" },
      spawn: (_executable, args, options) => { spawnArgs = args; spawnOptions = options; return port; },
      cwd: "C:/workspace",
      session: "s",
      model: "m",
      thinking: "high",
      trust: "untrusted",
      hostEnvironment: { PATH: "C:/bin", SECRET: "not-allowed" },
      providerEnvironment: { OPENAI_API_KEY: "key" },
      allowedProviderKeys: new Set(["OPENAI_API_KEY"]),
    });
    await runtime.start();
    expect(spawnArgs).toEqual(["--mode", "rpc", "--session", "s", "--model", "m", "--thinking", "high", "--no-approve", "--no-context-files"]);
    expect(spawnOptions).toMatchObject({ cwd: "C:/workspace", env: { PATH: "C:/bin", OPENAI_API_KEY: "key" } });
    expect(spawnOptions.env).not.toHaveProperty("SECRET");
  });

  it("marks unexpected process exit as crashed and stop closes stdin before waiting", async () => {
    const port = new RuntimePort();
    let waited = false;
    port.wait = async () => { waited = true; return { code: 0, signal: null }; };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: "pi", version: "0.81.1" },
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

  it("terminates the process after graceful stop times out", async () => {
    const port = new RuntimePort();
    let killed = false;
    port.wait = () => new Promise(() => undefined);
    port.kill = () => { killed = true; };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: "pi", version: "0.81.1" },
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

  it("fails readiness as crashed when get_state times out and terminates the process", async () => {
    const port = new RuntimePort();
    port.stdin.write = (value: string) => { port.stdin.writes.push(value); };
    let killed = false;
    port.kill = () => { killed = true; };
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: "pi", version: "0.81.1" },
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
      readinessTimeoutMs: 1,
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "TransportDisconnected" });
    expect(runtime.state).toBe("crashed");
    expect(port.ended).toBe(true);
    expect(killed).toBe(true);
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
      detection: { status: "detected", source: "system", executable: "pi", version: "0.81.1" },
      spawn: () => port,
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(runtime.state).toBe("crashed");
    expect(port.ended).toBe(true);
    expect(killed).toBe(true);
    await expect(runtime.prompt("after failure")).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("does not spawn twice and does not downgrade a running state on detect", async () => {
    const port = new RuntimePort();
    let spawns = 0;
    const runtime = new PiRuntime({
      detection: { status: "detected", source: "system", executable: "pi", version: "0.81.1" },
      spawn: () => { spawns += 1; return port; },
      detect: async () => ({ status: "unavailable", source: "managed", managedInstall: "available" }),
      cwd: "C:/workspace", session: "s", model: "m", thinking: "low", trust: "trusted", hostEnvironment: { PATH: "C:/bin" },
    });
    await runtime.start();
    await expect(runtime.start()).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(spawns).toBe(1);
    await expect(runtime.detect()).resolves.toMatchObject({ status: "detected" });
    expect(runtime.state).toBe("ready");
  });
});
