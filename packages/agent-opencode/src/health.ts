import { basicAuthHeader, type ServerCredentials } from "./auth.js";
import {
  AuthenticationFailedError,
  OpenCodeError,
  ProtocolViolationError,
  RuntimeUnavailableError,
  VersionMismatchError,
} from "./errors.js";

export const OPENCODE_VERSION = "1.18.4" as const;

export interface HealthResult { readonly version: typeof OPENCODE_VERSION; }

export type HealthFetch = (url: string, init?: RequestInit) => Promise<Response>;

export interface HealthOptions {
  readonly baseUrl: string;
  readonly credentials: ServerCredentials;
  readonly fetch?: HealthFetch;
  readonly totalTimeoutMs?: number;
  readonly retryDelayMs?: number;
  readonly now?: () => number;
  readonly sleep?: (ms: number) => Promise<void>;
}

export class OpenCodeHealthError extends OpenCodeError {
  constructor(code: "RuntimeUnavailable" | "AuthenticationFailed" | "VersionMismatch" | "ProtocolViolation") {
    super(code);
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function endpoint(baseUrl: string): string {
  return `${baseUrl.replace(/\/$/u, "")}/global/health`;
}

export async function checkHealth(options: HealthOptions): Promise<HealthResult> {
  const fetcher = options.fetch ?? ((url, init) => fetch(url, init));
  const totalTimeoutMs = Math.max(1, options.totalTimeoutMs ?? 10_000);
  const retryDelayMs = Math.max(0, options.retryDelayMs ?? 100);
  const now = options.now ?? (() => Date.now());
  const sleep = options.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)));
  const started = now();
  let attempts = 0;

  while (now() - started <= totalTimeoutMs) {
    attempts += 1;
    try {
      const response = await withTimeout(fetcher(endpoint(options.baseUrl), {
        method: "GET",
        headers: { authorization: basicAuthHeader(options.credentials), accept: "application/json" },
      }), Math.max(1, totalTimeoutMs - (now() - started)));
      if (response.status === 401) throw new AuthenticationFailedError();
      if (response.status >= 500 || response.status === 408 || response.status === 429) {
        if (now() - started >= totalTimeoutMs) throw new RuntimeUnavailableError();
      } else if (response.status !== 200) {
        throw new RuntimeUnavailableError();
      } else {
        let body: unknown;
        try { body = await response.json(); } catch { throw new ProtocolViolationError(); }
        if (!isRecord(body) || typeof body.version !== "string") throw new ProtocolViolationError();
        if (body.version !== OPENCODE_VERSION) throw new VersionMismatchError();
        return { version: OPENCODE_VERSION };
      }
    } catch (error) {
      if (error instanceof OpenCodeError && error.code !== "RuntimeUnavailable") throw error;
      if (now() - started >= totalTimeoutMs) throw new RuntimeUnavailableError();
    }
    const elapsed = now() - started;
    const remaining = totalTimeoutMs - elapsed;
    if (remaining <= 0) break;
    await sleep(Math.min(retryDelayMs, remaining));
    if (attempts > 10_000) break;
  }
  throw new RuntimeUnavailableError();
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new RuntimeUnavailableError()), Math.max(0, timeoutMs));
  });
  try { return await Promise.race([promise, timeout]); }
  finally { if (timer !== undefined) clearTimeout(timer); }
}

export { AuthenticationFailedError, ProtocolViolationError, RuntimeUnavailableError, VersionMismatchError };
export const checkOpenCodeHealth = checkHealth;
export const waitForHealthy = checkHealth;
