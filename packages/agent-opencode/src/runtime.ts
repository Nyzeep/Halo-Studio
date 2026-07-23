import type { TrustState } from "@halo-studio/contracts";
import { buildRuntimeEnvironment, mergeRuntimeEnvironment, runtimeTrustPolicy } from "@halo-studio/core";
import { spawn as spawnChild } from "node:child_process";
import { EventEmitter } from "node:events";
import { createServer } from "node:net";
import { join } from "node:path";
import {
  clearServerCredentials,
  createServerCredentials,
  serverCredentialEnvironment,
  type ServerCredentials,
} from "./auth.js";
import { resolveOpenCodeArtifact, type OpenCodeArtifact } from "./artifact.js";
import { checkHealth, OPENCODE_VERSION, type HealthResult } from "./health.js";
import {
  OpenCodeError,
  RuntimeUnavailableError,
  TransportDisconnectedError,
  VersionMismatchError,
} from "./errors.js";

export type ProcessStartupFailure = "port-conflict" | "spawn-error" | "exited";

export interface OpenCodeProcess {
  readonly stdin: { end: () => unknown };
  readonly stdout?: EventEmitter;
  readonly stderr?: EventEmitter;
  readonly process?: EventEmitter;
  readonly startup?: Promise<ProcessStartupFailure>;
  wait?: () => Promise<{ readonly code: number | null; readonly signal: NodeJS.Signals | null }>;
  kill?: (signal?: NodeJS.Signals) => boolean | Promise<boolean>;
  dispose?: () => void;
}

export interface SpawnPort {
  readonly cwd: string;
  readonly env: Readonly<Record<string, string>>;
}

export type ProcessFactory = (
  executable: string,
  args: readonly string[],
  options: SpawnPort,
) => OpenCodeProcess | Promise<OpenCodeProcess>;

export interface NodeChildPort extends EventEmitter {
  readonly stdin: { end: () => unknown };
  readonly stdout: EventEmitter;
  readonly stderr: EventEmitter;
  kill(signal?: NodeJS.Signals): boolean;
}

export type NodeSpawn = (
  executable: string,
  args: readonly string[],
  options: { readonly cwd: string; readonly env: Readonly<Record<string, string>>; readonly stdio: readonly ["pipe", "pipe", "pipe"] },
) => NodeChildPort;

const PORT_CONFLICT_PATTERN = /\bEADDRINUSE\b|address already in use|addr(?:ess)? in use/iu;
const MAX_STARTUP_STDERR = 4_096;

function defaultNodeSpawn(executable: string, args: readonly string[], options: Parameters<NodeSpawn>[2]): NodeChildPort {
  return spawnChild(executable, [...args], {
    cwd: options.cwd,
    env: options.env,
    stdio: ["pipe", "pipe", "pipe"],
  }) as unknown as NodeChildPort;
}

export function createNodeProcessFactory(spawn: NodeSpawn = defaultNodeSpawn): ProcessFactory {
  return (executable, args, options) => {
    const child = spawn(executable, args, { ...options, stdio: ["pipe", "pipe", "pipe"] });
    let stderr = "";
    let startupSettled = false;
    let disposed = false;
    let resolveStartup!: (failure: ProcessStartupFailure) => void;
    const startup = new Promise<ProcessStartupFailure>((resolve) => { resolveStartup = resolve; });

    const settleStartup = (failure: ProcessStartupFailure): void => {
      if (startupSettled) return;
      startupSettled = true;
      resolveStartup(failure);
    };
    const onStderr = (chunk: unknown): void => {
      if (stderr.length >= MAX_STARTUP_STDERR) return;
      try { stderr = `${stderr}${String(chunk)}`.slice(0, MAX_STARTUP_STDERR); }
      catch { stderr = ""; }
      if (PORT_CONFLICT_PATTERN.test(stderr)) settleStartup("port-conflict");
    };
    const onError = (): void => settleStartup("spawn-error");
    const onStartupExit = (): void => settleStartup(PORT_CONFLICT_PATTERN.test(stderr) ? "port-conflict" : "exited");

    child.stderr.on("data", onStderr);
    child.on("error", onError);
    child.once("exit", onStartupExit);
    let resolveExit!: (exit: { readonly code: number | null; readonly signal: NodeJS.Signals | null }) => void;
    const exit = new Promise<{ readonly code: number | null; readonly signal: NodeJS.Signals | null }>((resolve) => {
      resolveExit = resolve;
    });
    const onWaitExit = (code: number | null, signal: NodeJS.Signals | null): void => resolveExit({ code, signal });
    child.once("exit", onWaitExit);

    const dispose = (): void => {
      if (disposed) return;
      disposed = true;
      stderr = "";
      child.stderr.off("data", onStderr);
      child.off("error", onError);
      child.off("exit", onStartupExit);
      child.off("exit", onWaitExit);
    };

    return {
      stdin: child.stdin,
      stdout: child.stdout,
      stderr: child.stderr,
      process: child,
      startup,
      wait: () => exit,
      kill: (signal = "SIGTERM") => child.kill(signal),
      dispose,
    };
  };
}

export const nodeProcessFactory = createNodeProcessFactory();

export interface RuntimeSnapshot {
  readonly state: OpenCodeLifecycleState;
  readonly port?: number;
  readonly version?: typeof OPENCODE_VERSION;
  readonly error?: { readonly code: OpenCodeError["code"] };
}

export type OpenCodeLifecycleState =
  | "unavailable"
  | "installed"
  | "starting"
  | "healthy"
  | "stopping"
  | "stopped"
  | "crashed";

export interface RuntimeHealthOptions {
  readonly baseUrl: string;
  readonly credentials: ServerCredentials;
}

export interface OpenCodeRuntimeOptions {
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
  /** Source-module seam for tests; production callers use the bundled resolver. */
  readonly resolveArtifact?: () => Promise<OpenCodeArtifact>;
}

export type OpenCodeRuntimePublicOptions = Omit<OpenCodeRuntimeOptions, "resolveArtifact">;

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

function isPortConflict(error: unknown): boolean {
  if (!(error instanceof Error)) return false;
  const code = (error as Error & { readonly code?: unknown }).code;
  return code === "EADDRINUSE" || PORT_CONFLICT_PATTERN.test(error.message);
}

type Settlement<T> =
  | { readonly status: "fulfilled"; readonly value: T }
  | { readonly status: "rejected" }
  | { readonly status: "timeout" };

export class OpenCodeRuntime {
  readonly #options: OpenCodeRuntimeOptions;
  #state: OpenCodeLifecycleState = "unavailable";
  #process: OpenCodeProcess | undefined;
  #port: number | undefined;
  #version: typeof OPENCODE_VERSION | undefined;
  #error: OpenCodeError | undefined;
  #credentials: ServerCredentials | undefined;
  #spawnEnvironment: Record<string, string> | undefined;
  #stopRequested = false;
  #processExited = false;
  #exitListener: (() => void) | undefined;
  #errorListener: (() => void) | undefined;
  #operation: Promise<void> = Promise.resolve();

  constructor(options: OpenCodeRuntimeOptions) { this.#options = options; }

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
    let artifact: OpenCodeArtifact;
    try { artifact = await (this.#options.resolveArtifact ?? resolveOpenCodeArtifact)(); }
    catch (error) {
      this.#state = "unavailable";
      throw error instanceof OpenCodeError ? error : new RuntimeUnavailableError();
    }
    if (artifact.version !== OPENCODE_VERSION) {
      this.#state = "unavailable";
      throw new VersionMismatchError();
    }
    this.#state = "installed";
    this.#credentials = createServerCredentials();
    const credentials = this.#credentials;
    const trust = runtimeTrustPolicy("opencode", this.#options.trust);
    const base = buildRuntimeEnvironment(
      this.#options.hostEnvironment,
      this.#options.providerEnvironment ?? {},
      this.#options.allowedProviderKeys ?? new Set(),
    );

    for (let attempt = 0; attempt < 3; attempt += 1) {
      const port = await (this.#options.reservePort ?? reserveLoopbackPort)();
      const env = mergeRuntimeEnvironment({
        ...base,
        ...serverCredentialEnvironment(credentials),
        XDG_CONFIG_HOME: join(this.#options.cwd, ".halo", "opencode", "config"),
        XDG_DATA_HOME: join(this.#options.cwd, ".halo", "opencode", "data"),
        XDG_STATE_HOME: join(this.#options.cwd, ".halo", "opencode", "state"),
        OPENCODE_PROFILE: "halo-studio",
      }, trust);
      this.#spawnEnvironment = env;
      this.#state = "starting";
      const args = ["serve", "--hostname", "127.0.0.1", "--port", String(port)] as const;
      try {
        const readinessTimeoutMs = this.#options.readinessTimeoutMs ?? 10_000;
        const spawned = await this.#race(
          Promise.resolve((this.#options.spawn ?? nodeProcessFactory)(artifact.executable, args, { cwd: this.#options.cwd, env })),
          readinessTimeoutMs,
        );
        if (spawned === "timeout") throw new RuntimeUnavailableError();
        this.#process = spawned;
        this.#port = port;
        this.#stopRequested = false;
        this.#attachLifecycle(spawned);

        if (spawned.startup) {
          const earlyFailure = await this.#race(spawned.startup, 0);
          if (earlyFailure !== "timeout") {
            if (earlyFailure === "port-conflict") {
              if (!await this.#teardownCurrentProcess()) throw new RuntimeUnavailableError();
              this.#clearSpawnEnvironment();
              if (attempt < 2) continue;
              break;
            }
            throw new RuntimeUnavailableError();
          }
        }
        const health = this.#options.checkHealth ?? ((options: RuntimeHealthOptions) => checkHealth({
          ...options,
          totalTimeoutMs: readinessTimeoutMs,
        }));
        const startupOutcome = spawned.startup?.then(
          (failure) => ({ type: "startup-failure", failure } as const),
        );
        const healthOutcome = health({ baseUrl: `http://127.0.0.1:${port}`, credentials }).then(
          (result) => ({ type: "healthy", result } as const),
        );
        const handshake = startupOutcome === undefined
          ? healthOutcome
          : Promise.race([startupOutcome, healthOutcome]);
        const outcome = await this.#race(handshake, readinessTimeoutMs);
        if (outcome === "timeout") throw new RuntimeUnavailableError();
        if (outcome.type === "startup-failure") {
          if (outcome.failure === "port-conflict") {
            if (!await this.#teardownCurrentProcess()) throw new RuntimeUnavailableError();
            this.#clearSpawnEnvironment();
            if (attempt < 2) continue;
            break;
          }
          throw new RuntimeUnavailableError();
        }
        if (this.#state !== "starting" || this.#processExited) throw new TransportDisconnectedError();
        this.#version = outcome.result.version;
        this.#state = "healthy";
        return;
      } catch (error) {
        if (isPortConflict(error)) {
          if (!await this.#teardownCurrentProcess()) {
            this.#state = "crashed";
            this.#error = new RuntimeUnavailableError();
            this.#clearSecrets();
            throw this.#error;
          }
          this.#clearSpawnEnvironment();
          if (attempt < 2) continue;
          break;
        }
        await this.#teardownCurrentProcess();
        this.#state = "crashed";
        this.#error = error instanceof OpenCodeError ? error : new RuntimeUnavailableError();
        this.#clearSecrets();
        throw this.#error;
      }
    }

    this.#state = "crashed";
    this.#error = new RuntimeUnavailableError();
    this.#clearSecrets();
    throw this.#error;
  }

  stop(): Promise<void> { return this.#exclusive(() => this.#stopInternal()); }

  async #stopInternal(): Promise<void> {
    if (this.#state === "stopped" || this.#state === "unavailable") {
      this.#state = "stopped";
      this.#clearSecrets();
      return;
    }
    if (!this.#process) {
      this.#state = "stopped";
      this.#clearSecrets();
      return;
    }
    this.#stopRequested = true;
    this.#state = "stopping";
    const stopped = await this.#teardownCurrentProcess();
    this.#clearSecrets();
    if (!stopped) {
      this.#state = "crashed";
      this.#error = new RuntimeUnavailableError();
      throw this.#error;
    }
    this.#state = "stopped";
  }

  #attachLifecycle(process: OpenCodeProcess): void {
    this.#processExited = false;
    this.#exitListener = () => { this.#processExited = true; this.#onExit(); };
    this.#errorListener = () => { this.#processExited = true; this.#onExit(); };
    process.process?.once("exit", this.#exitListener);
    process.process?.once("error", this.#errorListener);
  }

  #detachLifecycle(process: OpenCodeProcess): void {
    if (this.#exitListener) process.process?.off("exit", this.#exitListener);
    if (this.#errorListener) process.process?.off("error", this.#errorListener);
    this.#exitListener = undefined;
    this.#errorListener = undefined;
  }

  #onExit(): void {
    if (this.#state === "starting") return;
    if (this.#stopRequested || this.#state === "stopping" || this.#state === "stopped") return;
    const process = this.#process;
    if (process) {
      this.#detachLifecycle(process);
      process.dispose?.();
    }
    this.#process = undefined;
    this.#state = "crashed";
    this.#error = new TransportDisconnectedError();
    this.#clearSecrets();
    this.#options.onDisconnect?.();
  }

  async #teardownCurrentProcess(): Promise<boolean> {
    const process = this.#process;
    if (!process) return true;
    const timeoutMs = this.#options.stopTimeoutMs ?? 6_000;
    const deadline = Date.now() + timeoutMs;
    const remaining = (): number => Math.max(0, deadline - Date.now());

    await this.#settle(() => process.stdin.end(), remaining());
    const wait = process.wait === undefined
      ? ({ status: "fulfilled", value: { code: 0, signal: null } } as const)
      : await this.#settle(() => process.wait!(), remaining());
    let stopped = wait.status === "fulfilled";
    if (!stopped) {
      const killed = process.kill === undefined
        ? ({ status: "rejected" } as const)
        : await this.#settle(() => process.kill!("SIGKILL"), Math.max(1, timeoutMs));
      stopped = killed.status === "fulfilled" && killed.value !== false;
    }

    this.#detachLifecycle(process);
    process.dispose?.();
    this.#process = undefined;
    return stopped;
  }

  #clearSpawnEnvironment(): void {
    if (this.#spawnEnvironment) {
      this.#spawnEnvironment.OPENCODE_SERVER_PASSWORD = "";
      delete this.#spawnEnvironment.OPENCODE_SERVER_PASSWORD;
    }
    this.#spawnEnvironment = undefined;
  }

  #clearSecrets(): void {
    if (this.#credentials) clearServerCredentials(this.#credentials);
    this.#credentials = undefined;
    this.#clearSpawnEnvironment();
  }

  async #settle<T>(operation: () => T | PromiseLike<T>, timeoutMs: number): Promise<Settlement<T>> {
    const observed = Promise.resolve().then(operation).then(
      (value) => ({ status: "fulfilled", value } as const),
      () => ({ status: "rejected" } as const),
    );
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<{ readonly status: "timeout" }>((resolve) => {
      timer = setTimeout(() => resolve({ status: "timeout" }), Math.max(0, timeoutMs));
    });
    try { return await Promise.race([observed, timeout]); }
    finally { if (timer !== undefined) clearTimeout(timer); }
  }

  async #race<T>(promise: Promise<T>, timeoutMs: number): Promise<T | "timeout"> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<"timeout">((resolve) => {
      timer = setTimeout(() => resolve("timeout"), Math.max(0, timeoutMs));
    });
    try { return await Promise.race([promise, timeout]); }
    finally { if (timer !== undefined) clearTimeout(timer); }
  }

  #exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const previous = this.#operation;
    const current = previous.then(operation, operation);
    this.#operation = current.then(() => undefined, () => undefined);
    return current;
  }
}

export function createOpenCodeRuntime(options: OpenCodeRuntimePublicOptions): OpenCodeRuntime {
  const { resolveArtifact: _ignored, ...productionOptions } = options as OpenCodeRuntimeOptions;
  void _ignored;
  return new OpenCodeRuntime(productionOptions);
}
