import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";
import { ProtocolViolationError, RuntimeUnavailableError } from "./errors.js";
import type { ProcessPort } from "./jsonlTransport.js";
import { PiRuntime } from "./runtime.js";
import { piCommandSchema, piSessionCommandSchema } from "./schemas.js";

type RpcCommand = { readonly id: string; readonly type: string };
type RpcResponse = Record<string, unknown>;

const testPiExecutable = process.platform === "win32" ? "C:\\host-bin\\pi.exe" : "/host-bin/pi";

class SessionPort implements ProcessPort {
  readonly stdin = {
    writes: [] as string[],
    write: (value: string) => {
      this.stdin.writes.push(value);
      const command = JSON.parse(value) as RpcCommand;
      const response = this.respond(command);
      if (response !== undefined) {
        queueMicrotask(() => this.stdout.emit("data", `${JSON.stringify(response)}\n`));
      }
    },
    end: () => { this.ended = true; },
  };
  readonly stdout = new EventEmitter();
  readonly stderr = new EventEmitter();
  readonly process = new EventEmitter();
  ended = false;

  constructor(readonly respond: (command: RpcCommand) => RpcResponse | undefined) {}

  kill = () => { this.process.emit("exit", 1, "SIGTERM"); };
  wait = async () => ({ code: 0, signal: null });
}

function response(command: RpcCommand, data: unknown): RpcResponse {
  return { type: "response", id: command.id, command: command.type, success: true, data };
}

function createRuntime(port: SessionPort): PiRuntime {
  return new PiRuntime({
    detection: { status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" },
    detect: async () => ({ status: "detected", source: "system", executable: testPiExecutable, version: "0.81.1" }),
    spawn: () => port,
    cwd: process.platform === "win32" ? "C:\\workspace" : "/workspace",
    session: "launch-session",
    model: "provider/model",
    thinking: "medium",
    trust: "trusted",
    hostEnvironment: { PATH: process.platform === "win32" ? "C:\\host-bin" : "/host-bin" },
  });
}

const sessionState = {
  sessionId: "native-session",
  sessionName: "Current task",
  isStreaming: false,
  isCompacting: false,
  messageCount: 4,
  pendingMessageCount: 0,
  sessionFile: "C:\\private\\session.jsonl",
  model: { provider: "private", id: "model" },
};

describe("Pi session adapter", () => {
  it("uses only the session whitelist and returns bounded, projected native data", async () => {
    const port = new SessionPort((command) => {
      switch (command.type) {
        case "get_state":
          return response(command, sessionState);
        case "new_session":
          return response(command, { cancelled: false, parentSession: "must-not-cross-boundary" });
        case "get_messages":
          return response(command, {
            messages: [
              { role: "user", content: [{ type: "text", text: "Review this change" }, { type: "image", data: "not-rendered" }] },
              { role: "assistant", content: [{ type: "thinking", thinking: "private" }, { type: "text", text: "I will review it." }] },
              { role: "toolResult", content: [{ type: "text", text: "tool output must not cross" }] },
            ],
          });
        case "get_commands":
          return response(command, {
            commands: [
              { name: "review", description: "Review the current change", source: "extension", sourceInfo: { path: "C:\\private" } },
              { name: "skill:testing", source: "skill", sourceInfo: { path: "C:\\private" } },
            ],
          });
        default:
          return undefined;
      }
    });
    const runtime = createRuntime(port);

    await runtime.start();
    await expect(runtime.getSessionState()).resolves.toEqual({
      sessionId: "native-session",
      sessionName: "Current task",
      isStreaming: false,
      isCompacting: false,
      messageCount: 4,
      pendingMessageCount: 0,
    });
    await expect(runtime.newSession()).resolves.toEqual({ cancelled: false });
    await expect(runtime.getMessages()).resolves.toEqual([
      { role: "user", text: "Review this change" },
      { role: "assistant", text: "I will review it." },
    ]);
    await expect(runtime.getCommands()).resolves.toEqual([
      { name: "review", description: "Review the current change", source: "extension" },
      { name: "skill:testing", source: "skill" },
    ]);

    const commands = port.stdin.writes.map((line) => JSON.parse(line) as RpcCommand);
    expect(commands.map((command) => command.type)).toEqual([
      "get_state",
      "get_state",
      "new_session",
      "get_messages",
      "get_commands",
    ]);
    expect(Object.keys(commands[2]!).sort()).toEqual(["id", "type"]);
    expect((runtime as unknown as { switchSession?: unknown }).switchSession).toBeUndefined();
    expect((runtime as unknown as { bash?: unknown }).bash).toBeUndefined();
    expect((runtime as unknown as { exportSession?: unknown }).exportSession).toBeUndefined();

    await runtime.stop();
  });

  it("requires a ready runtime before reading a session", async () => {
    const port = new SessionPort(() => undefined);
    const runtime = createRuntime(port);

    await expect(runtime.getSessionState()).rejects.toBeInstanceOf(RuntimeUnavailableError);
    expect(port.stdin.writes).toEqual([]);
  });

  it("fails closed when Pi returns a mismatched or malformed session response", async () => {
    let calls = 0;
    const port = new SessionPort((command) => {
      calls += 1;
      if (calls === 1) return response(command, sessionState);
      return {
        type: "response",
        id: command.id,
        command: "bash",
        success: true,
        data: { command: "Get-ChildItem" },
      };
    });
    const runtime = createRuntime(port);

    await runtime.start();
    await expect(runtime.getCommands()).rejects.toBeInstanceOf(ProtocolViolationError);
    expect(runtime.state).toBe("crashed");
    await runtime.stop();
  });

  it("fails closed when a command catalogue contains an invalid native name", async () => {
    let calls = 0;
    const port = new SessionPort((command) => {
      calls += 1;
      if (calls === 1) return response(command, sessionState);
      return response(command, { commands: [{ name: "/not-allowed", source: "extension" }] });
    });
    const runtime = createRuntime(port);

    await runtime.start();
    await expect(runtime.getCommands()).rejects.toBeInstanceOf(ProtocolViolationError);
    expect(runtime.state).toBe("crashed");
    await runtime.stop();
  });

  it("rejects paths and arbitrary native RPC commands at the transport boundary", () => {
    expect(piSessionCommandSchema.safeParse({ type: "new_session", parentSession: "C:\\private\\session.jsonl" }).success).toBe(false);
    expect(piCommandSchema.safeParse({ type: "switch_session", sessionPath: "C:\\private\\session.jsonl" }).success).toBe(false);
    expect(piCommandSchema.safeParse({ type: "bash", command: "Get-ChildItem" }).success).toBe(false);
    expect(piCommandSchema.safeParse({ type: "export_html", outputPath: "C:\\private\\session.html" }).success).toBe(false);
  });
});
