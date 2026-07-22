import type { AgentEventEnvelope, JsonValue, PiEventPayload, TrustState } from "@halo-studio/contracts";
import { buildRuntimeEnvironment, mergeRuntimeEnvironment, runtimeTrustPolicy } from "@halo-studio/core";
import { randomUUID } from "node:crypto";
import { PiJsonlTransport, type ProcessExit, type ProcessPort } from "./jsonlTransport.js";
import { detectPi, nodeProcessFactory, type ProcessFactory, type ProcessFactoryOptions } from "./detect.js";
import { PiError, RuntimeUnavailableError, TransportDisconnectedError, VersionMismatchError } from "./errors.js";
import { PI_VERSION, type PiDetection, type PiEvent, type PiLifecycleState, type PiResponse } from "./schemas.js";

export interface PiSpawnOptions extends ProcessFactoryOptions {
  readonly cwd: string;
  readonly env: Readonly<Record<string, string>>;
}

export interface PiRuntimeOptions {
  readonly detection?: PiDetection;
  readonly detect?: () => Promise<PiDetection>;
  readonly spawn?: ProcessFactory;
  readonly cwd: string;
  readonly session: string;
  readonly model: string;
  readonly thinking: string;
  readonly trust: TrustState;
  readonly hostEnvironment: Readonly<Record<string, string | undefined>>;
  readonly providerEnvironment?: Readonly<Record<string, string>>;
  readonly allowedProviderKeys?: ReadonlySet<string>;
  readonly readinessTimeoutMs?: number;
  readonly stopTimeoutMs?: number;
  readonly onEvent?: (event: AgentEventEnvelope) => void;
  readonly workspaceId?: string;
}

export type PiRuntimePublicOptions = Omit<PiRuntimeOptions, "detection" | "detect">;

const lifecycle: readonly PiLifecycleState[] = ["unavailable", "detected", "starting", "ready", "stopping", "stopped", "crashed"];

export class PiRuntime {
  readonly #options: PiRuntimeOptions;
  #detection: PiDetection | undefined;
  #port: ProcessPort | undefined;
  #transport: PiJsonlTransport | undefined;
  #unsubscribe: (() => void) | undefined;
  #state: PiLifecycleState = "unavailable";
  #running = false;
  #sequence = 0;
  #stopRequested = false;
  #operation: Promise<void> = Promise.resolve();

  constructor(options: PiRuntimeOptions) {
    this.#options = options;
    if (options.detection) {
      this.#detection = options.detection;
      this.#state = options.detection.status === "detected" ? "detected" : "unavailable";
    }
  }

  get state(): PiLifecycleState { return this.#state; }
  get running(): boolean { return this.#running; }
  get detection(): PiDetection | undefined { return this.#detection; }

  detect(): Promise<PiDetection> {
    return this.#exclusive(() => this.#detectInternal());
  }

  async #detectInternal(): Promise<PiDetection> {
    if (this.#state === "ready" || this.#state === "starting" || this.#state === "stopping" || this.#state === "stopped" || this.#state === "crashed") {
      if (this.#detection) return this.#detection;
      throw new RuntimeUnavailableError();
    }
    this.#detection = await (this.#options.detect ? this.#options.detect() : detectPi({
      processFactory: this.#options.spawn ?? nodeProcessFactory,
      cwd: this.#options.cwd,
      hostEnvironment: this.#options.hostEnvironment,
      ...(this.#options.providerEnvironment === undefined ? {} : { providerEnvironment: this.#options.providerEnvironment }),
      ...(this.#options.allowedProviderKeys === undefined ? {} : { allowedProviderKeys: this.#options.allowedProviderKeys }),
    }));
    this.#state = this.#detection.status === "detected" ? "detected" : "unavailable";
    return this.#detection;
  }

  start(): Promise<void> {
    return this.#exclusive(() => this.#startInternal());
  }

  async #startInternal(): Promise<void> {
    if (this.#state === "ready" || this.#state === "starting" || this.#state === "stopping" || this.#state === "stopped" || this.#state === "crashed") {
      throw new RuntimeUnavailableError();
    }
    await this.#detectInternal();
    const detection = this.#detection;
    if (!detection || detection.status !== "detected" || detection.version !== PI_VERSION || !detection.executable) {
      this.#state = "unavailable";
      throw detection?.version && detection.version !== PI_VERSION ? new VersionMismatchError() : new RuntimeUnavailableError();
    }
    this.#state = "starting";
    this.#stopRequested = false;
    try {
      const trustPolicy = runtimeTrustPolicy("pi", this.#options.trust);
      const baseEnvironment = buildRuntimeEnvironment(this.#options.hostEnvironment, this.#options.providerEnvironment ?? {}, this.#options.allowedProviderKeys ?? new Set());
      const env = mergeRuntimeEnvironment(baseEnvironment, trustPolicy);
      const args = ["--mode", "rpc", "--session", this.#options.session, "--model", this.#options.model, "--thinking", this.#options.thinking, ...trustPolicy.args] as const;
      const port = (this.#options.spawn ?? nodeProcessFactory)(detection.executable, args, { cwd: this.#options.cwd, env });
      this.#port = port;
      this.#transport = new PiJsonlTransport(port, { onDisconnect: (error) => this.#onDisconnect(error) });
      this.#unsubscribe = this.#transport.onEvent((event) => this.#onEvent(event));
      const readiness = await this.#transport.request({ type: "get_state" }, { timeoutMs: this.#options.readinessTimeoutMs ?? 10_000 });
      if (!readiness.success) throw new RuntimeUnavailableError();
      if (this.#state !== "starting" || this.#transport.closed || this.#port !== port) throw new TransportDisconnectedError();
      this.#state = "ready";
    } catch (error) {
      this.#state = "crashed";
      this.#transport?.close();
      this.#unsubscribe?.();
      this.#unsubscribe = undefined;
      await this.#terminateFailedStart();
      if (error instanceof PiError) throw error;
      throw new RuntimeUnavailableError();
    }
  }

  prompt(message: string): Promise<PiResponse> {
    if (this.#state !== "ready" || !this.#transport) return Promise.reject(new RuntimeUnavailableError());
    return this.#transport.request({ type: "prompt", message });
  }

  steer(message: string): Promise<PiResponse> {
    if (this.#state !== "ready" || !this.#transport) return Promise.reject(new RuntimeUnavailableError());
    return this.#transport.request({ type: "steer", message });
  }

  abort(): Promise<PiResponse> {
    if ((this.#state !== "ready" && this.#state !== "starting") || !this.#transport) return Promise.reject(new RuntimeUnavailableError());
    return this.#transport.request({ type: "abort" });
  }

  stop(): Promise<void> {
    return this.#exclusive(() => this.#stopInternal());
  }

  async #stopInternal(): Promise<void> {
    if (this.#state === "stopped" || this.#state === "unavailable") { this.#state = "stopped"; return; }
    if (!this.#port) { this.#state = "stopped"; return; }
    this.#stopRequested = true;
    this.#state = "stopping";
    const stopTimeoutMs = this.#options.stopTimeoutMs ?? 5_000;
    await this.#endStdin(stopTimeoutMs);
    const port = this.#port;
    let wait: Promise<ProcessExit>;
    try { wait = port.wait ? port.wait() : Promise.resolve({ code: 0, signal: null } satisfies ProcessExit); } catch { wait = Promise.reject(new Error()); }
    const initialWait = await this.#raceWithTimeout(() => wait, stopTimeoutMs);
    let killFailed = false;
    if (initialWait.status !== "fulfilled") {
      const killResult = await this.#raceWithTimeout(() => port.kill?.("SIGTERM"), stopTimeoutMs);
      killFailed = port.kill === undefined || killResult.status !== "fulfilled" || killResult.value === false;
      if (!killFailed && initialWait.status === "timeout") {
        const afterKill = await this.#raceWithTimeout(() => wait, stopTimeoutMs);
        killFailed = afterKill.status !== "fulfilled";
      }
    }
    this.#transport?.close();
    this.#unsubscribe?.();
    this.#unsubscribe = undefined;
    if (killFailed || initialWait.status === "rejected") {
      this.#state = "crashed";
      throw new RuntimeUnavailableError();
    }
    this.#state = "stopped";
  }

  #onEvent(event: PiEvent): void {
    if (event.type === "agent_start") this.#running = true;
    if (event.type === "agent_settled") this.#running = false;
    const payload: PiEventPayload = { protocol: "pi-rpc", type: event.type, ...(event.data === undefined ? {} : { data: event.data as JsonValue }) };
    this.#options.onEvent?.({
      eventId: randomUUID(),
      workspaceId: this.#options.workspaceId ?? "0".repeat(64),
      sequence: this.#sequence++,
      timestamp: new Date().toISOString(),
      agentKind: "pi",
      payload,
    });
  }

  #onDisconnect(_error: PiError): void {
    if (!this.#stopRequested && this.#state !== "stopped" && this.#state !== "unavailable") {
      this.#state = "crashed";
    }
  }

  async #terminateFailedStart(): Promise<void> {
    const succeeded = await this.#teardownFailedStart();
    if (!succeeded) throw new RuntimeUnavailableError();
  }

  async #teardownFailedStart(): Promise<boolean> {
    const port = this.#port;
    if (!port) return true;
    const timeoutMs = this.#options.stopTimeoutMs ?? 1_000;
    await this.#endStdin(timeoutMs);
    let wait: Promise<ProcessExit>;
    try { wait = port.wait ? port.wait() : Promise.resolve({ code: 0, signal: null } satisfies ProcessExit); } catch { wait = Promise.reject(new Error()); }
    const initialWait = await this.#raceWithTimeout(() => wait, timeoutMs);
    const killResult = await this.#raceWithTimeout(() => port.kill?.("SIGTERM"), timeoutMs);
    if (initialWait.status === "fulfilled") {
      return port.wait !== undefined
        || (port.kill !== undefined && killResult.status === "fulfilled" && killResult.value !== false);
    }
    if (port.kill === undefined || killResult.status !== "fulfilled" || killResult.value === false) return false;
    if (initialWait.status === "rejected") return false;
    const afterKill = await this.#raceWithTimeout(() => wait, timeoutMs);
    return afterKill.status === "fulfilled";
  }

  async #raceWithTimeout<T>(operation: () => T | PromiseLike<T>, timeoutMs: number): Promise<
    | { readonly status: "fulfilled"; readonly value: T }
    | { readonly status: "rejected"; readonly error: unknown }
    | { readonly status: "timeout" }
  > {
    const observed = Promise.resolve().then(operation).then(
      (value) => ({ status: "fulfilled", value } as const),
      (error: unknown) => ({ status: "rejected", error } as const),
    );
    let timer: ReturnType<typeof setTimeout> | undefined;
    const timeout = new Promise<{ readonly status: "timeout" }>((resolve) => {
      timer = setTimeout(() => resolve({ status: "timeout" }), Math.max(0, timeoutMs));
    });
    try {
      return await Promise.race([observed, timeout]);
    } finally {
      if (timer !== undefined) clearTimeout(timer);
    }
  }

  async #endStdin(timeoutMs: number): Promise<void> {
    const port = this.#port;
    if (!port) return;
    try {
      const result = port.stdin.end();
      if (result && typeof (result as PromiseLike<unknown>).then === "function") {
        await Promise.race([result as PromiseLike<unknown>, new Promise<void>((resolve) => setTimeout(resolve, timeoutMs))]);
      }
    } catch { /* process may already be gone */ }
  }

  #exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const previous = this.#operation;
    const current = previous.then(operation, operation);
    this.#operation = current.then(() => undefined, () => undefined);
    return current;
  }
}

export function createPiRuntime(options: PiRuntimePublicOptions): PiRuntime {
  return new PiRuntime(options);
}
