import { describe, expect, it } from "vitest";
import { consumeSse, type SseSignal } from "./sse.js";

async function* chunks(values: readonly string[]): AsyncIterable<string> {
  for (const value of values) yield value;
}

describe("OpenCode SSE", () => {
  it("parses connected and heartbeat JSON frames and reports disconnect", async () => {
    const signals: SseSignal[] = [];
    await consumeSse(chunks([
      "event: connected\r\n",
      "data: {\"session\":\"s1\"}\r\n\r\n",
      ": keepalive\n\n",
      "event: heartbeat\n",
      "data: {\"ok\":true}\n\n",
    ]), { onSignal: (signal) => signals.push(signal) });
    expect(signals.map((signal) => signal.type)).toEqual(["connected", "heartbeat", "disconnected"]);
    expect(signals[0]).toMatchObject({ type: "connected", data: { session: "s1" } });
  });

  it("samples and ignores unknown events, while bad JSON becomes ProtocolViolation", async () => {
    const signals: SseSignal[] = [];
    const samples: string[] = [];
    await consumeSse(chunks([
      "event: unknown-event\n",
      "data: {\"secret\":\"canary\"}\n\n",
      "event: connected\n",
      "data: {bad json}\n\n",
    ]), { onSignal: (signal) => signals.push(signal), sampleUnknown: (name) => samples.push(name) });
    expect(samples).toEqual(["unknown-event"]);
    expect(signals.map((signal) => signal.type)).toEqual(["protocol-violation", "disconnected"]);
    expect(signals.map((signal) => JSON.stringify(signal))).not.toContain("canary");
  });

  it("does not synthesize replay headers or require Last-Event-ID", async () => {
    const requests: RequestInit[] = [];
    const signals: SseSignal[] = [];
    await consumeSse(chunks(["event: connected\ndata: {}\n\n"]), {
      onSignal: (signal) => signals.push(signal),
      requestUrl: "http://127.0.0.1:43123/global/event",
      request: (_url, init) => { requests.push(init ?? {}); return Promise.resolve(); },
    });
    expect(requests).toHaveLength(1);
    expect(new Headers(requests[0]?.headers).has("last-event-id")).toBe(false);
    expect(signals.map((signal) => signal.type)).toEqual(["connected", "disconnected"]);
  });

  it("recognizes OpenCode server event payloads and heartbeat comments", async () => {
    const signals: SseSignal[] = [];
    await consumeSse(chunks([
      ": heartbeat\n\n",
      "data: {\"type\":\"server.connected\",\"data\":{}}\n\n",
      "data: {\"type\":\"server.heartbeat\"}\n\n",
    ]), { onSignal: (signal) => signals.push(signal) });
    expect(signals.map((signal) => signal.type)).toEqual(["heartbeat", "connected", "heartbeat", "disconnected"]);
  });

  it("marks malformed JSON in an otherwise unknown event as a protocol violation", async () => {
    const signals: SseSignal[] = [];
    await consumeSse(chunks(["event: future-event\ndata: {broken}\n\n"]), { onSignal: (signal) => signals.push(signal) });
    expect(signals.map((signal) => signal.type)).toEqual(["protocol-violation", "disconnected"]);
  });
});
