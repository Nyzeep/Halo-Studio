import { describe, expect, it } from "vitest";
import { checkHealth, OpenCodeHealthError } from "./health.js";

const credentials = { username: "opencode", password: "health-canary" } as const;

function response(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("OpenCode health handshake", () => {
  it("sends Basic Auth and accepts only the pinned version", async () => {
    let seenAuth = "";
    const result = await checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 100,
      retryDelayMs: 0,
      fetch: async (_url, init) => {
        seenAuth = String(new Headers(init?.headers).get("authorization"));
        return response(200, { version: "1.18.4" });
      },
    });
    expect(result).toEqual({ version: "1.18.4" });
    expect(seenAuth).toBe(`Basic ${Buffer.from("opencode:health-canary").toString("base64")}`);
  });

  it("maps 401 to AuthenticationFailed without exposing the password", async () => {
    await expect(checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 50,
      retryDelayMs: 0,
      fetch: async () => response(401, { password: credentials.password }),
    })).rejects.toMatchObject({ code: "AuthenticationFailed" });
    try {
      await checkHealth({ baseUrl: "http://127.0.0.1:43123", credentials, totalTimeoutMs: 50, retryDelayMs: 0, fetch: async () => response(401, { password: credentials.password }) });
    } catch (error) {
      expect(String(error)).not.toContain(credentials.password);
      expect((error as Error & { body?: unknown }).body).toBeUndefined();
    }
  });

  it("maps a different version immediately and bounds transient failures", async () => {
    await expect(checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 30,
      retryDelayMs: 0,
      fetch: async () => response(200, { version: "0.0.0" }),
    })).rejects.toMatchObject({ code: "VersionMismatch" });

    let attempts = 0;
    await expect(checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 10,
      retryDelayMs: 0,
      fetch: async () => { attempts += 1; return response(500, { password: credentials.password }); },
    })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
    expect(attempts).toBeGreaterThan(0);
    expect(attempts).toBeLessThan(100);
  });

  it("cancels response bodies that are not parsed before retry or rejection", async () => {
    let transientCancellations = 0;
    let attempts = 0;
    const result = await checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 100,
      retryDelayMs: 0,
      fetch: async () => {
        attempts += 1;
        if (attempts === 1) {
          return {
            status: 500,
            body: { cancel: async () => { transientCancellations += 1; } },
          } as unknown as Response;
        }
        return response(200, { version: "1.18.4" });
      },
    });
    expect(result).toEqual({ version: "1.18.4" });
    expect(transientCancellations).toBe(1);

    let authenticationCancellations = 0;
    await expect(checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 50,
      fetch: async () => ({
        status: 401,
        body: { cancel: async () => { authenticationCancellations += 1; } },
      }) as unknown as Response,
    })).rejects.toMatchObject({ code: "AuthenticationFailed" });
    expect(authenticationCancellations).toBe(1);
  });

  it("bounds a health request that never resolves", async () => {
    await expect(checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 5,
      retryDelayMs: 0,
      fetch: async () => new Promise<Response>(() => undefined),
    })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("includes response body parsing in the total timeout", async () => {
    const pending = checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 5,
      retryDelayMs: 0,
      fetch: async () => ({
        status: 200,
        json: async () => new Promise<never>(() => undefined),
      }) as Response,
    }).then(
      () => "resolved",
      (error: unknown) => (error as { code?: string }).code,
    );
    const result = await Promise.race([
      pending,
      new Promise<string>((resolve) => setTimeout(() => resolve("test-harness-timeout"), 50)),
    ]);
    expect(result).toBe("RuntimeUnavailable");
  });

  it("aborts the request and cancels the response body when the total deadline expires", async () => {
    let requestSignal: AbortSignal | undefined;
    let cancelCalls = 0;
    let rejectBody!: (error: Error) => void;
    const bodyParsing = new Promise<never>((_resolve, reject) => { rejectBody = reject; });
    const unhandled: unknown[] = [];
    const onUnhandled = (error: unknown): void => { unhandled.push(error); };
    process.on("unhandledRejection", onUnhandled);
    try {
      await expect(checkHealth({
        baseUrl: "http://127.0.0.1:43123",
        credentials,
        totalTimeoutMs: 5,
        retryDelayMs: 0,
        fetch: async (_url, init) => {
          requestSignal = init?.signal ?? undefined;
          return {
            status: 200,
            body: { cancel: async () => { cancelCalls += 1; } },
            json: async () => bodyParsing,
          } as unknown as Response;
        },
      })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
      expect(requestSignal?.aborted).toBe(true);
      expect(cancelCalls).toBe(1);
      rejectBody(new Error("late-body-canary"));
      await new Promise<void>((resolve) => setImmediate(resolve));
      expect(unhandled).toEqual([]);
    } finally {
      process.off("unhandledRejection", onUnhandled);
    }
  });

  it("applies the same total deadline to retry sleep", async () => {
    const pending = checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 5,
      retryDelayMs: 5,
      fetch: async () => response(500, {}),
      sleep: async () => new Promise<void>(() => undefined),
    }).then(
      () => "resolved",
      (error: unknown) => (error as { code?: string }).code,
    );
    const result = await Promise.race([
      pending,
      new Promise<string>((resolve) => setTimeout(() => resolve("test-harness-timeout"), 50)),
    ]);
    expect(result).toBe("RuntimeUnavailable");
  });

  it("never exposes raw response details in stable errors", () => {
    const error = new OpenCodeHealthError("RuntimeUnavailable");
    expect(error.message).not.toContain(credentials.password);
    expect(error.message).not.toContain("127.0.0.1");
  });
});
