import { describe, expect, it } from "vitest";
import { ProtocolViolationError } from "./errors.js";
import {
  createOpenCodeSessionAdapter,
  type OpenCodeSessionResponse,
  type OpenCodeSessionTransport,
} from "./session.js";
import type { OpenCodeSseConnection, SseSignal } from "./sse.js";

function response(value: unknown, status = 200, headers?: HeadersInit): OpenCodeSessionResponse {
  const body = status === 204 ? null : JSON.stringify(value);
  return {
    response: new Response(body, { status, headers }),
    close: () => undefined,
  };
}

function transport(overrides: Partial<OpenCodeSessionTransport> = {}): OpenCodeSessionTransport {
  const connection: OpenCodeSseConnection = {
    done: Promise.resolve(),
    close: async () => undefined,
  };
  return {
    list: async () => response([]),
    create: async () => response({ id: "created" }),
    get: async (sessionId) => response({ id: sessionId }),
    history: async () => response([]),
    startPrompt: async () => response(undefined, 204),
    abort: async () => response(true),
    subscribe: async () => connection,
    ...overrides,
  };
}

describe("OpenCode session adapter", () => {
  it("projects summaries and text history without native payload fields", async () => {
    const nativeCanary = "C:\\private\\workspace-canary";
    const adapter = createOpenCodeSessionAdapter(transport({
      list: async () => response([
        {
          id: "s1",
          title: "Review changes",
          time: { updated: 0 },
          directory: nativeCanary,
          provider: "provider-canary",
          model: "model-canary",
          auth: "auth-canary",
        },
        { id: "\u0000invalid" },
      ]),
      get: async (sessionId) => response({
        id: sessionId,
        title: "Review changes",
        time: { updated: 0 },
        directory: nativeCanary,
      }),
      history: async () => response([
        {
          info: { id: "m1", sessionID: "s1", role: "user", provider: "provider-canary" },
          parts: [
            { type: "text", sessionID: "s1", messageID: "m1", text: "Hello", metadata: { secret: "metadata-canary" } },
            { type: "tool", sessionID: "s1", messageID: "m1", command: "shell-canary" },
            { type: "file", sessionID: "s1", messageID: "m1", path: "attachment-canary" },
          ],
        },
        {
          info: { id: "m2", sessionID: "s1", role: "assistant", model: "model-canary" },
          parts: [
            { type: "reasoning", sessionID: "s1", messageID: "m2", text: "reasoning-canary" },
            { type: "text", sessionID: "s1", messageID: "m2", text: "World" },
          ],
        },
        {
          info: { id: "m3", sessionID: "s1", role: "assistant" },
          parts: [{ type: "tool", sessionID: "s1", messageID: "m3", command: "tool-only-canary" }],
        },
      ]),
    }));

    const summaries = await adapter.list();
    const history = await adapter.history("s1");

    expect(summaries).toEqual([{
      sessionId: "s1",
      title: "Review changes",
      updatedAt: "1970-01-01T00:00:00.000Z",
      active: false,
    }]);
    expect(history).toEqual({
      session: {
        sessionId: "s1",
        title: "Review changes",
        updatedAt: "1970-01-01T00:00:00.000Z",
        active: false,
      },
      messages: [
        { sessionId: "s1", ordinal: 0, role: "user", text: "Hello" },
        { sessionId: "s1", ordinal: 1, role: "assistant", text: "World" },
      ],
    });
    const serialised = JSON.stringify({ summaries, history, adapter });
    for (const forbidden of [nativeCanary, "provider-canary", "model-canary", "auth-canary", "metadata-canary", "shell-canary", "attachment-canary", "reasoning-canary", "tool-only-canary"]) {
      expect(serialised).not.toContain(forbidden);
    }
  });

  it("limits prompt and abort calls to a session id and text", async () => {
    const prompts: Array<readonly [string, string]> = [];
    const aborts: string[] = [];
    const adapter = createOpenCodeSessionAdapter(transport({
      startPrompt: async (sessionId, text) => {
        prompts.push([sessionId, text]);
        return response(undefined, 204);
      },
      abort: async (sessionId) => {
        aborts.push(sessionId);
        return response(true);
      },
    }));

    await adapter.startPrompt("s1", "Write the summary.");
    await adapter.abort("s1");

    expect(prompts).toEqual([["s1", "Write the summary."]]);
    expect(aborts).toEqual(["s1"]);
    await expect(adapter.startPrompt("s1", "   ")).rejects.toMatchObject({ code: "ProtocolViolation" });
    await expect(adapter.abort("\u0000invalid")).rejects.toMatchObject({ code: "ProtocolViolation" });
  });

  it("maps response and subscription failures without preserving upstream data", async () => {
    const authentication = createOpenCodeSessionAdapter(transport({
      list: async () => response("response-body-canary", 401),
    }));
    let authenticationError: unknown;
    try { await authentication.list(); } catch (error) { authenticationError = error; }
    expect(authenticationError).toMatchObject({ code: "AuthenticationFailed" });
    expect(String(authenticationError)).not.toContain("response-body-canary");

    const malformed = createOpenCodeSessionAdapter(transport({
      list: async () => ({
        response: new Response("invalid-json-canary", { status: 200 }),
        close: () => undefined,
      }),
    }));
    let malformedError: unknown;
    try { await malformed.list(); } catch (error) { malformedError = error; }
    expect(malformedError).toMatchObject({ code: "ProtocolViolation" });
    expect(String(malformedError)).not.toContain("invalid-json-canary");

    const oversized = createOpenCodeSessionAdapter(transport({
      list: async () => response([], 200, { "content-length": String((2 * 1024 * 1024) + 1) }),
    }));
    await expect(oversized.list()).rejects.toMatchObject({ code: "ProtocolViolation" });

    const disconnected = createOpenCodeSessionAdapter(transport({
      subscribe: async () => { throw new Error("subscribe-canary"); },
    }));
    let disconnectedError: unknown;
    try { await disconnected.subscribe(() => undefined); } catch (error) { disconnectedError = error; }
    expect(disconnectedError).toMatchObject({ code: "TransportDisconnected" });
    expect(String(disconnectedError)).not.toContain("subscribe-canary");
  });

  it("forwards only the safe SSE projection and hides signal payloads", async () => {
    let publish: ((signal: SseSignal) => void) | undefined;
    let closed = false;
    const adapter = createOpenCodeSessionAdapter(transport({
      subscribe: async (onSignal) => {
        publish = onSignal;
        return {
          done: Promise.resolve(),
          close: async () => { closed = true; },
        };
      },
    }));
    const observed: unknown[] = [];
    const subscription = await adapter.subscribe((event) => observed.push(event));

    publish?.({ type: "connected", data: { password: "signal-canary" } });
    publish?.({
      type: "event",
      event: { type: "message-part-updated", sessionId: "s1", messageId: "m1", text: "safe text", delta: "safe" },
    });
    publish?.({ type: "protocol-violation", error: new ProtocolViolationError() });
    await subscription.unsubscribe();

    expect(observed).toEqual([
      { type: "connected" },
      { type: "message-part-updated", sessionId: "s1", messageId: "m1", text: "safe text", delta: "safe" },
      { type: "transport-error" },
    ]);
    expect(JSON.stringify({ observed, adapter })).not.toContain("signal-canary");
    expect(closed).toBe(true);
  });
});
