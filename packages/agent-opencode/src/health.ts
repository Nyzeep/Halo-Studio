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
  const remaining = (): number => Math.max(0, totalTimeoutMs - (now() - started));

  while (remaining() > 0) {
    attempts += 1;
    const controller = new AbortController();
    let response: Response | undefined;
    try {
      const operation = (async (): Promise<HealthResult | "retry"> => {
        response = await fetcher(endpoint(options.baseUrl), {
          method: "GET",
          headers: { authorization: basicAuthHeader(options.credentials), accept: "application/json" },
          signal: controller.signal,
        });
        if (response.status === 401) {
          cancelResponseBody(response);
          throw new AuthenticationFailedError();
        }
        if (response.status >= 500 || response.status === 408 || response.status === 429) {
          cancelResponseBody(response);
          return "retry";
        }
        if (response.status !== 200) {
          cancelResponseBody(response);
          throw new RuntimeUnavailableError();
        }
        let body: unknown;
        try { body = await response.json(); } catch { throw new ProtocolViolationError(); }
        if (!isRecord(body) || typeof body.version !== "string") throw new ProtocolViolationError();
        if (body.version !== OPENCODE_VERSION) throw new VersionMismatchError();
        return { version: OPENCODE_VERSION };
      })();
      operation.catch(() => undefined);
      const result = await withTimeout(operation, Math.max(1, remaining()), () => {
        controller.abort();
        cancelResponseBody(response);
      });
      if (result !== "retry") return result;
    } catch (error) {
      if (error instanceof OpenCodeError && error.code !== "RuntimeUnavailable") throw error;
      if (remaining() <= 0) throw new RuntimeUnavailableError();
    }
    const delayBudget = remaining();
    if (delayBudget <= 0) break;
    try {
      const delay = Promise.resolve().then(() => sleep(Math.min(retryDelayMs, delayBudget)));
      delay.catch(() => undefined);
      await withTimeout(delay, Math.max(1, delayBudget));
    } catch {
      throw new RuntimeUnavailableError();
    }
    if (attempts > 10_000) break;
  }
  throw new RuntimeUnavailableError();
}

function cancelResponseBody(response: Response | undefined): void {
  if (!response?.body) return;
  try {
    const cancellation = response.body.cancel();
    cancellation.catch(() => undefined);
  } catch { /* Cancellation is best effort after abort. */ }
}

async function withTimeout<T>(promise: Promise<T>, timeoutMs: number, onTimeout?: () => void): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => {
      onTimeout?.();
      reject(new RuntimeUnavailableError());
    }, Math.max(0, timeoutMs));
  });
  try { return await Promise.race([promise, timeout]); }
  finally { if (timer !== undefined) clearTimeout(timer); }
}

export { AuthenticationFailedError, ProtocolViolationError, RuntimeUnavailableError, VersionMismatchError };
export const checkOpenCodeHealth = checkHealth;
export const waitForHealthy = checkHealth;
