import { describe, expect, it } from "vitest";
import { connectOpenCodeSse, consumeSse, type SseSignal } from "./sse.js";

const credentials = { username: "opencode", password: "sse-canary" } as const;

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

  it("uses one authenticated response body without synthesizing Last-Event-ID", async () => {
    const requests: Array<{ readonly url: string; readonly init: RequestInit }> = [];
    const signals: SseSignal[] = [];
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new TextEncoder().encode("event: connected\ndata: {\"source\":\"response-body\"}\n\n"));
        controller.enqueue(new TextEncoder().encode(": heartbeat\n\n"));
        controller.close();
      },
    });
    const connection = await connectOpenCodeSse({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      onSignal: (signal) => signals.push(signal),
      fetch: async (url, init) => {
        requests.push({ url, init: init ?? {} });
        return new Response(body, { status: 200, headers: { "content-type": "text/event-stream" } });
      },
    });
    await connection.done;
    expect(requests).toHaveLength(1);
    expect(requests[0]?.url).toBe("http://127.0.0.1:43123/global/event");
    const headers = new Headers(requests[0]?.init.headers);
    expect(headers.get("authorization")).toBe(`Basic ${Buffer.from("opencode:sse-canary").toString("base64")}`);
    expect(headers.get("accept")).toBe("text/event-stream");
    expect(headers.has("last-event-id")).toBe(false);
    expect(signals.map((signal) => signal.type)).toEqual(["connected", "heartbeat", "disconnected"]);
    expect(signals[0]).toMatchObject({ data: { source: "response-body" } });
  });

  it("validates HTTP status without exposing response bodies or URLs", async () => {
    let failure: unknown;
    try {
      await connectOpenCodeSse({
        baseUrl: "http://127.0.0.1:43123/private-path-canary",
        credentials,
        onSignal: () => undefined,
        fetch: async () => new Response("response-body-canary", { status: 500 }),
      });
    } catch (error) { failure = error; }
    expect(failure).toMatchObject({ code: "TransportDisconnected" });
    expect(String(failure)).not.toContain("response-body-canary");
    expect(String(failure)).not.toContain("private-path-canary");

    await expect(connectOpenCodeSse({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      onSignal: () => undefined,
      fetch: async () => new Response(null, { status: 401 }),
    })).rejects.toMatchObject({ code: "AuthenticationFailed" });
  });

  it("aborts and cancels a hanging response body on close", async () => {
    let requestSignal: AbortSignal | undefined;
    let cancelCalls = 0;
    const signals: SseSignal[] = [];
    const body = new ReadableStream<Uint8Array>({
      cancel() { cancelCalls += 1; },
    });
    const connection = await connectOpenCodeSse({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      onSignal: (signal) => signals.push(signal),
      fetch: async (_url, init) => {
        requestSignal = init?.signal ?? undefined;
        return new Response(body, { status: 200 });
      },
    });
    await connection.close();
    await connection.done;
    expect(requestSignal?.aborted).toBe(true);
    expect(cancelCalls).toBe(1);
    expect(signals.map((signal) => signal.type)).toEqual(["disconnected"]);
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

  it("bounds unknown-event samples per stream", async () => {
    const samples: string[] = [];
    await consumeSse(chunks(Array.from({ length: 25 }, (_, index) => `event: future-${index}\ndata: {}\n\n`)), {
      onSignal: () => undefined,
      sampleUnknown: (event) => samples.push(event),
    });
    expect(samples).toHaveLength(10);
    expect(samples).toEqual(Array.from({ length: 10 }, (_, index) => `future-${index}`));
  });
});
