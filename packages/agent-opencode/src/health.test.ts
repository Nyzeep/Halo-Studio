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

  it("bounds a health request that never resolves", async () => {
    await expect(checkHealth({
      baseUrl: "http://127.0.0.1:43123",
      credentials,
      totalTimeoutMs: 5,
      retryDelayMs: 0,
      fetch: async () => new Promise<Response>(() => undefined),
    })).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });

  it("never exposes raw response details in stable errors", () => {
    const error = new OpenCodeHealthError("RuntimeUnavailable");
    expect(error.message).not.toContain(credentials.password);
    expect(error.message).not.toContain("127.0.0.1");
  });
});
