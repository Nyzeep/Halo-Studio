import type { TrustState } from "@halo-studio/contracts";
import { buildRuntimeEnvironment, mergeRuntimeEnvironment, runtimeTrustPolicy } from "@halo-studio/core";
import { EventEmitter } from "node:events";
import { createRequire } from "node:module";
import { createServer } from "node:net";
import { join } from "node:path";
import { createServerCredentials, serverCredentialEnvironment, type ServerCredentials } from "./auth.js";
import { resolveOpenCodeArtifact } from "./artifact.js";
import { checkHealth, OPENCODE_VERSION, type HealthResult } from "./health.js";
import { OpenCodeError, RuntimeUnavailableError, TransportDisconnectedError } from "./errors.js";

const require = createRequire(import.meta.url);

export interface OpenCodeProcess {
  readonly stdin: { end: () => unknown };
  readonly stdout?: EventEmitter;
  readonly stderr?: EventEmitter;
  readonly process?: EventEmitter;
  wait?: () => Promise<{ readonly code: number | null; readonly signal: NodeJS.Signals | null }>;
  kill?: (signal?: NodeJS.Signals) => boolean | Promise<boolean>;
}

export interface SpawnPort {
  readonly cwd: string;
  readonly env: Readonly<Record<string, string>>;
}

export type ProcessFactory = (executable: string, args: readonly string[], options: SpawnPort) => OpenCodeProcess | Promise<OpenCodeProcess>;

export interface RuntimeSnapshot {
  readonly state: OpenCodeLifecycleState;
  readonly port?: number;
  readonly version?: typeof OPENCODE_VERSION;
  readonly error?: { readonly code: OpenCodeError["code"] };
}

export type OpenCodeLifecycleState = "unavailable" | "installed" | "starting" | "healthy" | "stopping" | "stopped" | "crashed";

export interface RuntimeHealthOptions {
  readonly baseUrl: string;
  readonly credentials: ServerCredentials;
}

export interface OpenCodeRuntimeOptions {
  readonly executable?: string;
  readonly cwd: string;
  readonly trust: TrustState;
  readonly hostEnvironment: Readonly<Record<string, string | undefined>>;
  readonly providerEnvironment?: Readonly<Record<string, string>>;
  readonly allowedProviderKeys?: ReadonlySet<string>;
  readonly spawn?: ProcessFactory;
  readonly reservePort?: () => Promise<number>;
  readonly checkHealth?: (options: RuntimeHealthOptions) => Promise<HealthResult>;
  readonly readinessTimeoutMs?: number;
  readonly stopTimeoutMs?: number;
  readonly onDisconnect?: () => void;
}

async function reserveLoopbackPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen({ host: "127.0.0.1", port: 0 }, () => resolve());
  });
  const address = server.address();
  const port = typeof address === "object" && address !== null ? address.port : 0;
  await new Promise<void>((resolve) => server.close(() => resolve()));
  if (port <= 0) throw new RuntimeUnavailableError();
  return port;
}

function defaultSpawn(executable: string, args: readonly string[], options: SpawnPort): OpenCodeProcess {
  // Dynamic import keeps this package testable without touching a real child process.
  const child = requireChild(executable, args, options);
  return child;
}

function requireChild(executable: string, args: readonly string[], options: SpawnPort): OpenCodeProcess {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { spawn } = require("node:child_process") as typeof import("node:child_process");
  const child = spawn(executable, [...args], { cwd: options.cwd, env: options.env, stdio: ["pipe", "pipe", "pipe"] });
  return {
    stdin: child.stdin,
    stdout: child.stdout,
    stderr: child.stderr,
    process: child,
    wait: () => new Promise((resolve) => child.once("exit", (code: number | null, signal: NodeJS.Signals | null) => resolve({ code, signal }))),
    kill: (signal = "SIGTERM") => child.kill(signal),
  };
}

export class OpenCodeRuntime {
  readonly #options: OpenCodeRuntimeOptions;
  #state: OpenCodeLifecycleState = "unavailable";
  #process: OpenCodeProcess | undefined;
  #port: number | undefined;
  #version: typeof OPENCODE_VERSION | undefined;
  #error: OpenCodeError | undefined;
  #credentials: ServerCredentials | undefined;
  #stopRequested = false;
  #operation: Promise<void> = Promise.resolve();

  constructor(options: OpenCodeRuntimeOptions) {
    this.#options = options;
    if (options.executable !== undefined) this.#state = "installed";
  }

  get state(): OpenCodeLifecycleState { return this.#state; }
  get running(): boolean { return this.#state === "healthy"; }
  snapshot(): RuntimeSnapshot {
    return {
      state: this.#state,
      ...(this.#port === undefined ? {} : { port: this.#port }),
      ...(this.#version === undefined ? {} : { version: this.#version }),
      ...(this.#error === undefined ? {} : { error: { code: this.#error.code } }),
    };
  }

  start(): Promise<void> { return this.#exclusive(() => this.#startInternal()); }

  async #startInternal(): Promise<void> {
    if (this.#state !== "unavailable" && this.#state !== "installed") throw new RuntimeUnavailableError();
    this.#error = undefined;
    let executable = this.#options.executable;
    if (!executable) {
      try { executable = (await resolveOpenCodeArtifact()).executable; } catch (error) { this.#state = "unavailable"; throw error instanceof OpenCodeError ? error : new RuntimeUnavailableError(); }
    }
    this.#state = "installed";
    this.#credentials = createServerCredentials();
    const trust = runtimeTrustPolicy("opencode", this.#options.trust);
    const base = buildRuntimeEnvironment(this.#options.hostEnvironment, this.#options.providerEnvironment ?? {}, this.#options.allowedProviderKeys ?? new Set());
    const argsBase = ["serve", "--hostname", "127.0.0.1"] as const;
    let lastError: unknown;
    for (let attempt = 0; attempt < 3; attempt += 1) {
      const port = await (this.#options.reservePort ?? reserveLoopbackPort)();
      const env = mergeRuntimeEnvironment({
        ...base,
        ...serverCredentialEnvironment(this.#credentials),
        XDG_CONFIG_HOME: join(this.#options.cwd, ".halo", "opencode", "config"),
        XDG_DATA_HOME: join(this.#options.cwd, ".halo", "opencode", "data"),
        XDG_STATE_HOME: join(this.#options.cwd, ".halo", "opencode", "state"),
        OPENCODE_PROFILE: "halo-studio",
      }, trust);
      const args = [...argsBase, "--port", String(port)] as const;
      this.#state = "starting";
      try {
        const readinessTimeoutMs = this.#options.readinessTimeoutMs ?? 10_000;
        const spawned = await this.#race(Promise.resolve((this.#options.spawn ?? defaultSpawn)(executable, args, { cwd: this.#options.cwd, env })), readinessTimeoutMs);
        if (spawned === "timeout") throw new RuntimeUnavailableError();
        const process = spawned;
        this.#process = process;
        this.#port = port;
        this.#stopRequested = false;
        process.process?.once("exit", () => this.#onExit());
        const health = this.#options.checkHealth ?? ((options: RuntimeHealthOptions) => checkHealth({ ...options, totalTimeoutMs: this.#options.readinessTimeoutMs ?? 10_000 }));
        const result = await this.#race(health({ baseUrl: `http://127.0.0.1:${port}`, credentials: this.#credentials }), readinessTimeoutMs);
        if (result === "timeout") throw new RuntimeUnavailableError();
        if (this.#state !== "starting") throw new TransportDisconnectedError();
        this.#version = result.version;
        this.#state = "healthy";
        return;
      } catch (error) {
        lastError = error;
        if (error instanceof Error && (error as Error & { code?: string }).code === "EADDRINUSE") continue;
        await this.#terminateFailedStart();
        this.#state = error instanceof OpenCodeError && error.code === "AuthenticationFailed" ? "crashed" : "crashed";
        this.#error = error instanceof OpenCodeError ? error : new RuntimeUnavailableError();
        throw this.#error;
      }
    }
    this.#state = "crashed";
    this.#error = new RuntimeUnavailableError();
    void lastError;
    throw this.#error;
  }

  stop(): Promise<void> { return this.#exclusive(() => this.#stopInternal()); }

  async #stopInternal(): Promise<void> {
    if (this.#state === "stopped" || this.#state === "unavailable") { this.#state = "stopped"; return; }
    const process = this.#process;
    if (!process) { this.#state = "stopped"; return; }
    this.#stopRequested = true;
    this.#state = "stopping";
    try { await process.stdin.end(); } catch { /* process may already be gone */ }
    const timeoutMs = this.#options.stopTimeoutMs ?? 6_000;
    const wait = process.wait ? process.wait() : Promise.resolve({ code: 0, signal: null } satisfies { code: number | null; signal: NodeJS.Signals | null });
    let first: { readonly code: number | null; readonly signal: NodeJS.Signals | null } | "timeout";
    try { first = await this.#race(wait, timeoutMs); } catch {
      this.#state = "crashed";
      this.#error = new RuntimeUnavailableError();
      throw this.#error;
    }
    if (first === "timeout") {
      let killed: boolean | "timeout" = false;
      try { killed = process.kill ? await this.#race(Promise.resolve(process.kill("SIGKILL")), timeoutMs) : false; } catch { killed = false; }
      if (killed !== true) {
        this.#state = "crashed";
        this.#error = new RuntimeUnavailableError();
        throw this.#error;
      }
    }
    this.#state = "stopped";
  }

  #onExit(): void {
    if (this.#stopRequested || this.#state === "stopping" || this.#state === "stopped") return;
    this.#state = "crashed";
    this.#error = new TransportDisconnectedError();
    this.#options.onDisconnect?.();
  }

  async #terminateFailedStart(): Promise<void> {
    const process = this.#process;
    if (!process) return;
    try { await process.stdin.end(); } catch { /* ignored */ }
    if (process.kill) {
      try { await process.kill("SIGTERM"); } catch { /* ignored */ }
    }
  }

  async #race<T>(promise: Promise<T>, timeoutMs: number): Promise<T | "timeout"> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<"timeout">((resolve) => { timer = setTimeout(() => resolve("timeout"), Math.max(0, timeoutMs)); });
    try { return await Promise.race([promise, timeout]); } finally { if (timer !== undefined) clearTimeout(timer); }
  }

  #exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const previous = this.#operation;
    const current = previous.then(operation, operation);
    this.#operation = current.then(() => undefined, () => undefined);
    return current;
  }
}

export const createOpenCodeRuntime = (options: OpenCodeRuntimeOptions): OpenCodeRuntime => new OpenCodeRuntime(options);
