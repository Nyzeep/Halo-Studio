import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";
import { PiJsonlTransport, ProtocolViolationError, TransportDisconnectedError } from "./jsonlTransport.js";

class FakePort {
  readonly stdin = {
    writes: [] as string[],
    write: (value: string) => { this.stdin.writes.push(value); },
    end: () => undefined,
  };
  readonly stdout = new EventEmitter();
  readonly stderr = new EventEmitter();
  readonly process = new EventEmitter();
  kill = () => undefined;
}

describe("Pi JSONL transport", () => {
  it("decodes UTF-8 fragments and LF/CRLF without splitting U+2028/U+2029 strings", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    const pending = transport.request({ type: "get_state" });
    const id = JSON.parse(port.stdin.writes[0]!).id as string;
    const line = JSON.stringify({ type: "response", id, command: "get_state", success: true, data: { text: "a\u2028b\u2029c" } }) + "\r\n";
    const bytes = Buffer.from(line, "utf8");
    const split = bytes.indexOf(0xe2); // first byte of U+2028
    port.stdout.emit("data", bytes.subarray(0, split + 1));
    port.stdout.emit("data", bytes.subarray(split + 1, split + 2));
    port.stdout.emit("data", bytes.subarray(split + 2));
    await expect(pending).resolves.toMatchObject({ success: true, data: { text: "a\u2028b\u2029c" } });
  });

  it("serializes ordinary commands while steer and abort use an independent channel", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    const first = transport.request({ type: "prompt", message: "one" });
    const second = transport.request({ type: "get_state" });
    const steer = transport.request({ type: "steer", message: "interrupt" });
    const abort = transport.request({ type: "abort" });
    expect(port.stdin.writes).toHaveLength(3);
    await Promise.resolve();
    expect(port.stdin.writes).toHaveLength(3);
    expect((JSON.parse(port.stdin.writes[0]!) as { type: string }).type).toBe("prompt");
    expect(port.stdin.writes.map((line) => (JSON.parse(line) as { type: string }).type)).toContain("steer");
    const firstId = JSON.parse(port.stdin.writes[0]!) as { id: string };
    port.stdout.emit("data", JSON.stringify({ type: "response", id: firstId.id, command: "prompt", success: true }) + "\n");
    await expect(first).resolves.toBeTruthy();
    await Promise.resolve();
    expect(port.stdin.writes).toHaveLength(4);
    const ids = port.stdin.writes.map((line) => JSON.parse(line) as { id: string; type: string });
    for (const command of ids.slice(1, 3)) port.stdout.emit("data", JSON.stringify({ type: "response", id: command.id, command: command.type, success: true }) + "\n");
    await expect(steer).resolves.toBeTruthy();
    await expect(abort).resolves.toBeTruthy();
    const secondId = ids[3]!;
    port.stdout.emit("data", JSON.stringify({ type: "response", id: secondId.id, command: "get_state", success: true }) + "\n");
    await expect(second).resolves.toBeTruthy();
  });

  it("ignores stderr and unknown or duplicate response ids", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    port.stderr.emit("data", "warning that must not enter protocol");
    const pending = transport.request({ type: "get_state" });
    const id = JSON.parse(port.stdin.writes[0]!) as { id: string };
    port.stdout.emit("data", JSON.stringify({ type: "response", id: "unknown", command: "get_state", success: true }) + "\n");
    let settled = false;
    void pending.then(() => { settled = true; });
    await Promise.resolve();
    expect(settled).toBe(false);
    port.stdout.emit("data", JSON.stringify({ type: "response", id: id.id, command: "get_state", success: true }) + "\n");
    await expect(pending).resolves.toBeTruthy();
    port.stdout.emit("data", JSON.stringify({ type: "response", id: id.id, command: "get_state", success: true }) + "\n");
  });

  it("correlates out-of-order independent responses only by unique id", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    const steer = transport.request({ type: "steer", message: "s" });
    const abort = transport.request({ type: "abort" });
    const commands = port.stdin.writes.map((line) => JSON.parse(line) as { id: string; type: string });
    const abortCommand = commands.find((command) => command.type === "abort")!;
    const steerCommand = commands.find((command) => command.type === "steer")!;
    port.stdout.emit("data", JSON.stringify({ type: "response", id: abortCommand.id, command: "abort", success: true }) + "\n");
    await expect(abort).resolves.toMatchObject({ command: "abort" });
    let steerSettled = false;
    void steer.then(() => { steerSettled = true; });
    await Promise.resolve();
    expect(steerSettled).toBe(false);
    port.stdout.emit("data", JSON.stringify({ type: "response", id: steerCommand.id, command: "steer", success: true }) + "\n");
    await expect(steer).resolves.toMatchObject({ command: "steer" });
  });

  it("rejects queued and in-flight requests when disposed", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    const first = transport.request({ type: "prompt", message: "one" });
    const queued = transport.request({ type: "get_state" });
    const concurrent = transport.request({ type: "abort" });
    transport.close();
    await expect(first).rejects.toBeInstanceOf(TransportDisconnectedError);
    await expect(queued).rejects.toBeInstanceOf(TransportDisconnectedError);
    await expect(concurrent).rejects.toBeInstanceOf(TransportDisconnectedError);
  });

  it("rejects invalid outbound commands before writing JSONL", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    await expect(transport.request({ type: "unknown" } as never)).rejects.toBeInstanceOf(ProtocolViolationError);
    expect(port.stdin.writes).toHaveLength(0);
  });

  it("keeps asynchronous events from resolving a pending response", async () => {
    const port = new FakePort();
    const events: unknown[] = [];
    const transport = new PiJsonlTransport(port);
    transport.onEvent((event) => events.push(event));
    const pending = transport.request({ type: "prompt", message: "hello" });
    const id = JSON.parse(port.stdin.writes[0]!).id as string;
    port.stdout.emit("data", JSON.stringify({ type: "agent_start", data: { runId: "r1" } }) + "\n");
    expect(events).toHaveLength(1);
    let settled = false;
    void pending.then(() => { settled = true; });
    await Promise.resolve();
    expect(settled).toBe(false);
    port.stdout.emit("data", JSON.stringify({ type: "response", id, command: "prompt", success: true }) + "\n");
    await expect(pending).resolves.toMatchObject({ command: "prompt" });
  });

  it("rejects all pending requests with stable errors on malformed JSON, timeout, and EOF", async () => {
    const malformedPort = new FakePort();
    const malformed = new PiJsonlTransport(malformedPort);
    const malformedPending = malformed.request({ type: "get_state" });
    malformedPort.stdout.emit("data", "{bad json}\n");
    await expect(malformedPending).rejects.toBeInstanceOf(ProtocolViolationError);

    const timeoutPort = new FakePort();
    const timeout = new PiJsonlTransport(timeoutPort);
    await expect(timeout.request({ type: "get_state" }, { timeoutMs: 5 })).rejects.toBeInstanceOf(TransportDisconnectedError);

    const eofPort = new FakePort();
    const eof = new PiJsonlTransport(eofPort);
    const eofPending = eof.request({ type: "get_state" });
    eofPort.stdout.emit("end");
    await expect(eofPending).rejects.toBeInstanceOf(TransportDisconnectedError);
  });

  it("closes on one timeout and rejects every other pending or queued request without later writes", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    const timedOut = transport.request({ type: "prompt", message: "one" }, { timeoutMs: 5 });
    const queued = transport.request({ type: "get_state" });
    const concurrent = transport.request({ type: "abort" });
    await expect(timedOut).rejects.toBeInstanceOf(TransportDisconnectedError);
    await expect(queued).rejects.toBeInstanceOf(TransportDisconnectedError);
    await expect(concurrent).rejects.toBeInstanceOf(TransportDisconnectedError);
    const writeCount = port.stdin.writes.length;
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(port.stdin.writes).toHaveLength(writeCount);
    await expect(transport.request({ type: "get_state" })).rejects.toBeInstanceOf(TransportDisconnectedError);
  });

  it("rejects all requests on an unexpected process exit", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    const first = transport.request({ type: "prompt", message: "one" });
    const queued = transport.request({ type: "get_state" });
    port.process.emit("exit", 2, null);
    await expect(first).rejects.toBeInstanceOf(TransportDisconnectedError);
    await expect(queued).rejects.toBeInstanceOf(TransportDisconnectedError);
  });

  it("treats stdout close and stderr error as transport disconnects", async () => {
    const stdoutPort = new FakePort();
    const stdoutTransport = new PiJsonlTransport(stdoutPort);
    const stdoutPending = stdoutTransport.request({ type: "get_state" });
    stdoutPort.stdout.emit("close");
    await expect(stdoutPending).rejects.toBeInstanceOf(TransportDisconnectedError);

    const stderrPort = new FakePort();
    const stderrTransport = new PiJsonlTransport(stderrPort);
    const stderrPending = stderrTransport.request({ type: "get_state" });
    stderrPort.stderr.emit("error", new Error("noise"));
    await expect(stderrPending).rejects.toBeInstanceOf(TransportDisconnectedError);
  });

  it("removes stream/process listeners and event subscriptions when closed", async () => {
    const port = new FakePort();
    const transport = new PiJsonlTransport(port);
    const events: unknown[] = [];
    transport.onEvent((event) => events.push(event));
    transport.close();
    expect(port.stdout.listenerCount("data")).toBe(0);
    expect(port.stdout.listenerCount("close")).toBe(0);
    expect(port.stderr.listenerCount("data")).toBe(0);
    expect(port.process.listenerCount("exit")).toBe(0);
    port.stdout.emit("data", JSON.stringify({ type: "agent_start", data: {} }) + "\n");
    expect(events).toHaveLength(0);
  });
});
