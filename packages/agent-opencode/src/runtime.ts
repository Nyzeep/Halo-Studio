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
import {
  connectOpenCodeSse,
  type OpenCodeSseConnection,
  type SseFetch,
  type SseSignal,
} from "./sse.js";

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

const PORT_CONFLICT_PATTERN = /(?:^|\s)listen(?:\s+\w+)?\s+EADDRINUSE\b|(?:^|\s)listen\b[^\r\n]*address already in use/iu;
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
    const onStdout = (): void => undefined;
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
    child.stdout.on("data", onStdout);
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
      child.stdout.off("data", onStdout);
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

export interface RuntimeSseOptions {
  readonly onSignal: (signal: SseSignal) => void;
  readonly sampleUnknown?: (event: string) => void;
  /** Source-module seam for deterministic transport tests. */
  readonly fetch?: SseFetch;
}

interface RuntimeSseRecord {
  readonly controller: AbortController;
  connection?: OpenCodeSseConnection;
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
  /** Source-module seam for tests; production callers use Main-owned random credentials. */
  readonly credentialsFactory?: () => ServerCredentials;
}

export type OpenCodeRuntimePublicOptions = Omit<OpenCodeRuntimeOptions, "resolveArtifact" | "credentialsFactory">;

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
  return code === "EADDRINUSE";
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
  #artifact: OpenCodeArtifact | undefined;
  #acceptSse = false;
  readonly #sseRecords = new Set<RuntimeSseRecord>();
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

  detect(): Promise<OpenCodeArtifact> { return this.#exclusive(() => this.#detectInternal()); }

  async #detectInternal(): Promise<OpenCodeArtifact> {
    if (this.#artifact) {
      if (this.#state === "unavailable") {
        this.#state = "installed";
        this.#version = this.#artifact.version;
      }
      return this.#artifact;
    }
    try {
      const artifact = await (this.#options.resolveArtifact ?? resolveOpenCodeArtifact)();
      if (artifact.version !== OPENCODE_VERSION) throw new VersionMismatchError();
      this.#artifact = artifact;
      if (this.#state === "unavailable" || this.#state === "installed") {
        this.#state = "installed";
        this.#version = artifact.version;
      }
      return artifact;
    } catch (error) {
      if (this.#state === "unavailable" || this.#state === "installed") this.#state = "unavailable";
      throw error instanceof OpenCodeError ? error : new RuntimeUnavailableError();
    }
  }

  start(): Promise<void> { return this.#exclusive(() => this.#startInternal()); }

  async #startInternal(): Promise<void> {
    if (this.#state !== "unavailable" && this.#state !== "installed") throw new RuntimeUnavailableError();
    this.#acceptSse = false;
    this.#error = undefined;
    const artifact = await this.#detectInternal();
    this.#state = "installed";
    let retainCredentials = false;
    try {
    this.#credentials = (this.#options.credentialsFactory ?? createServerCredentials)();
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
        const spawnPromise = Promise.resolve((this.#options.spawn ?? nodeProcessFactory)(artifact.executable, args, { cwd: this.#options.cwd, env }));
        spawnPromise.catch(() => undefined);
        const spawned = await this.#race(spawnPromise, readinessTimeoutMs);
        if (spawned === "timeout") {
          void spawnPromise.then(
            (lateProcess) => this.#teardownProcess(lateProcess, this.#options.stopTimeoutMs ?? 6_000).then(() => undefined, () => undefined),
            () => undefined,
          );
          throw new RuntimeUnavailableError();
        }
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
              this.#clearRuntimeFields();
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
            this.#clearRuntimeFields();
            if (attempt < 2) continue;
            break;
          }
          throw new RuntimeUnavailableError();
        }
        if (this.#state !== "starting" || this.#processExited) throw new TransportDisconnectedError();
        this.#version = outcome.result.version;
        this.#state = "healthy";
        this.#acceptSse = true;
        retainCredentials = true;
        return;
      } catch (error) {
        if (isPortConflict(error)) {
          if (!await this.#teardownCurrentProcess()) {
            this.#state = "crashed";
            this.#error = new RuntimeUnavailableError();
            this.#clearSecrets();
            this.#clearRuntimeFields(false);
            throw this.#error;
          }
          this.#clearSpawnEnvironment();
          this.#clearRuntimeFields();
          if (attempt < 2) continue;
          break;
        }
        await this.#teardownCurrentProcess();
        this.#state = "crashed";
        this.#error = error instanceof OpenCodeError ? error : new RuntimeUnavailableError();
        this.#clearSecrets();
        this.#clearRuntimeFields(false);
        throw this.#error;
      }
    }

    this.#state = "crashed";
    this.#error = new RuntimeUnavailableError();
    this.#clearSecrets();
    this.#clearRuntimeFields(false);
    throw this.#error;
    } catch (error) {
      if (this.#state === "crashed" && this.#error !== undefined) throw this.#error;
      this.#state = "crashed";
      this.#error = error instanceof OpenCodeError ? error : new RuntimeUnavailableError();
      this.#clearSecrets();
      this.#clearRuntimeFields(false);
      throw this.#error;
    } finally {
      if (!retainCredentials) this.#clearSecrets();
    }
  }

  connectSse(options: RuntimeSseOptions): Promise<OpenCodeSseConnection> {
    return this.#exclusive(() => this.#connectSseInternal(options));
  }

  async #connectSseInternal(options: RuntimeSseOptions): Promise<OpenCodeSseConnection> {
    const port = this.#port;
    const credentials = this.#credentials;
    if (this.#state !== "healthy" || !this.#acceptSse || port === undefined || credentials === undefined) {
      throw new RuntimeUnavailableError();
    }

    const controller = new AbortController();
    const record: RuntimeSseRecord = { controller };
    this.#sseRecords.add(record);
    let rejectAbort!: () => void;
    const aborted = new Promise<never>((_resolve, reject) => {
      rejectAbort = () => reject(new TransportDisconnectedError());
      if (controller.signal.aborted) rejectAbort();
      else controller.signal.addEventListener("abort", rejectAbort, { once: true });
    });
    const connecting = connectOpenCodeSse({
      baseUrl: `http://127.0.0.1:${port}`,
      credentials,
      onSignal: options.onSignal,
      ...(options.sampleUnknown === undefined ? {} : { sampleUnknown: options.sampleUnknown }),
      ...(options.fetch === undefined ? {} : { fetch: options.fetch }),
      signal: controller.signal,
    });
    connecting.catch(() => undefined);
    connecting.then(
      (lateConnection) => {
        if (controller.signal.aborted) void lateConnection.close().catch(() => undefined);
      },
      () => undefined,
    );

    try {
      const connection = await Promise.race([connecting, aborted]);
      if (controller.signal.aborted) {
        await connection.close().catch(() => undefined);
        throw new TransportDisconnectedError();
      }
      record.connection = connection;
      const completed = connection.done.finally(() => { this.#sseRecords.delete(record); });
      completed.catch(() => undefined);
      return connection;
    } catch (error) {
      this.#sseRecords.delete(record);
      throw error instanceof OpenCodeError ? error : new TransportDisconnectedError();
    } finally {
      controller.signal.removeEventListener("abort", rejectAbort);
    }
  }

  stop(): Promise<void> {
    this.#acceptSse = false;
    this.#abortSseConnections();
    return this.#exclusive(() => this.#stopInternal());
  }

  async #stopInternal(): Promise<void> {
    await this.#closeSseConnections();
    if (this.#state === "stopped" || this.#state === "unavailable") {
      this.#state = "stopped";
      this.#clearSecrets();
      this.#clearRuntimeFields();
      return;
    }
    if (!this.#process) {
      this.#state = "stopped";
      this.#clearSecrets();
      this.#clearRuntimeFields();
      return;
    }
    this.#stopRequested = true;
    this.#state = "stopping";
    const stopped = await this.#teardownCurrentProcess();
    this.#clearSecrets();
    if (!stopped) {
      this.#state = "crashed";
      this.#error = new RuntimeUnavailableError();
      this.#clearRuntimeFields(false);
      throw this.#error;
    }
    this.#state = "stopped";
    this.#clearRuntimeFields();
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
    this.#acceptSse = false;
    const process = this.#process;
    if (process) {
      this.#detachLifecycle(process);
      process.dispose?.();
    }
    this.#process = undefined;
    this.#state = "crashed";
    this.#error = new TransportDisconnectedError();
    this.#clearRuntimeFields(false);
    void this.#closeSseConnections().then(
      () => this.#clearSecrets(),
      () => this.#clearSecrets(),
    );
    this.#options.onDisconnect?.();
  }

  #abortSseConnections(): void {
    for (const record of this.#sseRecords) record.controller.abort();
  }

  async #closeSseConnections(): Promise<void> {
    this.#acceptSse = false;
    const records = [...this.#sseRecords];
    for (const record of records) record.controller.abort();
    const timeoutMs = this.#options.stopTimeoutMs ?? 6_000;
    await Promise.all(records.map(async (record) => {
      if (record.connection) await this.#settle(() => record.connection!.close(), timeoutMs);
      this.#sseRecords.delete(record);
    }));
  }

  async #teardownProcess(process: OpenCodeProcess, timeoutMs: number): Promise<boolean> {
    const deadline = Date.now() + timeoutMs;
    const remaining = (): number => Math.max(0, deadline - Date.now());

    await this.#settle(() => process.stdin.end(), remaining());
    const exit = process.wait === undefined ? undefined : Promise.resolve().then(() => process.wait!());
    const wait = exit === undefined
      ? ({ status: "rejected" } as const)
      : await this.#settle(() => exit, remaining());
    if (wait.status === "fulfilled") {
      process.dispose?.();
      return true;
    }
    const killed = process.kill === undefined
      ? ({ status: "rejected" } as const)
      : await this.#settle(() => process.kill!("SIGKILL"), Math.max(1, remaining()));
    if (killed.status !== "fulfilled" || killed.value === false) {
      process.dispose?.();
      return false;
    }
    const afterKill = exit === undefined
      ? ({ status: "rejected" } as const)
      : await this.#settle(() => exit, Math.max(1, timeoutMs));
    process.dispose?.();
    return afterKill.status === "fulfilled";
  }

  async #teardownCurrentProcess(): Promise<boolean> {
    const process = this.#process;
    if (!process) return true;
    const timeoutMs = this.#options.stopTimeoutMs ?? 6_000;
    this.#detachLifecycle(process);
    this.#process = undefined;
    return this.#teardownProcess(process, timeoutMs);
  }

  #clearSpawnEnvironment(): void {
    if (this.#spawnEnvironment) {
      this.#spawnEnvironment.OPENCODE_SERVER_PASSWORD = "";
      delete this.#spawnEnvironment.OPENCODE_SERVER_PASSWORD;
    }
    this.#spawnEnvironment = undefined;
  }

  #clearSecrets(): void {
    this.#acceptSse = false;
    this.#abortSseConnections();
    if (this.#credentials) clearServerCredentials(this.#credentials);
    this.#credentials = undefined;
    this.#clearSpawnEnvironment();
  }

  #clearRuntimeFields(clearError = true): void {
    this.#port = undefined;
    this.#version = undefined;
    if (clearError) this.#error = undefined;
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
  const { resolveArtifact: _ignoredArtifact, credentialsFactory: _ignoredCredentials, ...productionOptions } = options as OpenCodeRuntimeOptions;
  void _ignoredArtifact;
  void _ignoredCredentials;
  return new OpenCodeRuntime(productionOptions);
}
