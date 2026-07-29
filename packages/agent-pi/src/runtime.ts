import type { AgentEventEnvelope, JsonValue, PiEventPayload, TrustState } from "@halo-studio/contracts";
import { buildRuntimeEnvironment, isPathWithin, mergeRuntimeEnvironment, runtimeTrustPolicy } from "@halo-studio/core";
import { randomUUID } from "node:crypto";
import { basename, extname, isAbsolute } from "node:path";
import { PiJsonlTransport, type ProcessExit, type ProcessPort } from "./jsonlTransport.js";
import { detectPi, nodeProcessFactory, PiProbeCleanupError, type PiExecutableResolver, type ProcessFactory, type ProcessFactoryOptions } from "./detect.js";
import { PiError, ProtocolViolationError, RuntimeUnavailableError, TransportDisconnectedError, VersionMismatchError } from "./errors.js";
import {
  PI_VERSION,
  parsePiSessionMessages,
  piNewSessionResponseSchema,
  piSessionCommandSchema,
  piSessionCommandsResponseSchema,
  piSessionMessagesResponseSchema,
  piSessionStateResponseSchema,
  type PiDetection,
  type PiEvent,
  type PiLaunchTarget,
  type PiLifecycleState,
  type PiNewSessionResult,
  type PiResponse,
  type PiSessionCommand,
  type PiSessionCommandDescriptor,
  type PiSessionMessage,
  type PiSessionState,
} from "./schemas.js";

export interface PiSpawnOptions extends ProcessFactoryOptions {
  readonly cwd: string;
  readonly env: Readonly<Record<string, string>>;
}

/**
 * Main-owned data consumed only while constructing a confirmed Pi RPC child.
 * PiRuntime never caches the returned object or any provider value.
 */
export interface PiRpcLaunch {
  readonly model: string;
  readonly thinking: string;
  readonly providerEnvironment: Readonly<Record<string, string>>;
  readonly allowedProviderKeys: ReadonlySet<string>;
}

export type PiRpcLaunchResolver = () => PiRpcLaunch | Promise<PiRpcLaunch>;

export interface PiRuntimeOptions {
  readonly detection?: PiDetection;
  readonly detect?: () => Promise<PiDetection>;
  readonly spawn?: ProcessFactory;
  readonly resolveExecutables?: PiExecutableResolver;
  readonly cwd: string;
  readonly session: string;
  /** Legacy non-secret launch settings for callers without provider values. */
  readonly model?: string;
  /** Legacy non-secret launch settings for callers without provider values. */
  readonly thinking?: string;
  /**
   * Called only after detection succeeds and immediately before `--mode rpc`
   * is spawned. Its result is intentionally kept out of runtime fields.
   */
  readonly resolveRpcLaunch?: PiRpcLaunchResolver;
  readonly trust: TrustState;
  readonly hostEnvironment: Readonly<Record<string, string | undefined>>;
  readonly readinessTimeoutMs?: number;
  readonly stopTimeoutMs?: number;
  readonly onEvent?: (event: AgentEventEnvelope) => void;
  /** Notifies Main only after an unexpected child has been fully cleaned up. */
  readonly onCrashed?: () => void;
  /** Notifies Main when cleanup could not prove the child has stopped. */
  readonly onCrashCleanupFailed?: () => void;
  readonly workspaceId?: string;
}

export type PiRuntimePublicOptions = Omit<PiRuntimeOptions, "detection">;

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
  #crashCleanup: Promise<boolean> | undefined;
  #probeCleanup: PiProbeCleanupError | undefined;
  #crashNotified = false;
  #crashCleanupFailureNotified = false;
  #hostEnvironmentInvalid = false;

  constructor(options: PiRuntimeOptions) {
    let hostEnvironment: Readonly<Record<string, string | undefined>>;
    try {
      // Keep only the audited host values in the long-lived runtime object.
      // Unknown JavaScript properties (including legacy provider fields) are
      // also deliberately omitted below instead of retaining the caller's
      // original options object.
      hostEnvironment = buildRuntimeEnvironment(options.hostEnvironment);
    } catch {
      hostEnvironment = {};
      this.#hostEnvironmentInvalid = true;
    }
    this.#options = {
      ...(options.detection === undefined ? {} : { detection: options.detection }),
      ...(options.detect === undefined ? {} : { detect: options.detect }),
      ...(options.spawn === undefined ? {} : { spawn: options.spawn }),
      ...(options.resolveExecutables === undefined ? {} : { resolveExecutables: options.resolveExecutables }),
      cwd: options.cwd,
      session: options.session,
      ...(options.model === undefined ? {} : { model: options.model }),
      ...(options.thinking === undefined ? {} : { thinking: options.thinking }),
      ...(options.resolveRpcLaunch === undefined ? {} : { resolveRpcLaunch: options.resolveRpcLaunch }),
      trust: options.trust,
      hostEnvironment,
      ...(options.readinessTimeoutMs === undefined ? {} : { readinessTimeoutMs: options.readinessTimeoutMs }),
      ...(options.stopTimeoutMs === undefined ? {} : { stopTimeoutMs: options.stopTimeoutMs }),
      ...(options.onEvent === undefined ? {} : { onEvent: options.onEvent }),
      ...(options.onCrashed === undefined ? {} : { onCrashed: options.onCrashed }),
      ...(options.onCrashCleanupFailed === undefined ? {} : { onCrashCleanupFailed: options.onCrashCleanupFailed }),
      ...(options.workspaceId === undefined ? {} : { workspaceId: options.workspaceId }),
    };
    if (this.#options.detection) {
      this.#detection = this.#options.detection;
      this.#state = this.#options.detection.status === "detected" ? "detected" : "unavailable";
    }
  }

  get state(): PiLifecycleState { return this.#state; }
  get running(): boolean { return this.#running; }
  get detection(): PiDetection | undefined { return this.#detection; }

  detect(): Promise<PiDetection> {
    return this.#exclusive(() => this.#detectInternal());
  }

  async #detectInternal(): Promise<PiDetection> {
    if (this.#hostEnvironmentInvalid) throw new RuntimeUnavailableError();
    if (this.#state === "ready" || this.#state === "starting" || this.#state === "stopping" || this.#state === "stopped" || this.#state === "crashed") {
      if (this.#detection) return this.#detection;
      throw new RuntimeUnavailableError();
    }
    try {
      const detection = await (this.#options.detect ? this.#options.detect() : detectPi({
        processFactory: this.#options.spawn ?? nodeProcessFactory,
        cwd: this.#options.cwd,
        hostEnvironment: this.#options.hostEnvironment,
        ...(this.#options.resolveExecutables === undefined ? {} : { resolveExecutables: this.#options.resolveExecutables }),
      }));
      const launch = detection.status === "detected" ? this.#launchFor(detection) : undefined;
      let normalized: PiDetection;
      if (detection.status !== "detected") {
        normalized = detection;
      } else if (launch === undefined) {
        normalized = { status: "unavailable", source: "managed", managedInstall: "available" };
      } else {
        normalized = { ...detection, launch };
      }
      this.#detection = normalized;
      this.#state = normalized.status === "detected" ? "detected" : "unavailable";
      return normalized;
    } catch (error) {
      if (error instanceof PiProbeCleanupError) {
        this.#detection = undefined;
        this.#running = false;
        this.#state = "crashed";
        this.#probeCleanup = error;
        this.#notifyCrashCleanupFailed();
      }
      throw error;
    }
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
    const launch = detection?.status === "detected" ? this.#launchFor(detection) : undefined;
    if (!detection || detection.status !== "detected" || detection.version !== PI_VERSION || launch === undefined) {
      this.#state = "unavailable";
      throw detection?.version && detection.version !== PI_VERSION ? new VersionMismatchError() : new RuntimeUnavailableError();
    }
    this.#state = "starting";
    this.#stopRequested = false;
    try {
      const trustPolicy = runtimeTrustPolicy("pi", this.#options.trust);
      const port = await this.#createRpcPort(launch, trustPolicy);
      this.#port = port;
      this.#transport = new PiJsonlTransport(port, { onDisconnect: (error) => this.#onDisconnect(error) });
      this.#unsubscribe = this.#transport.onEvent((event) => this.#onEvent(event));
      const readiness = await this.#transport.request({ type: "get_state" }, { timeoutMs: this.#options.readinessTimeoutMs ?? 10_000 });
      if (!readiness.success) throw new RuntimeUnavailableError();
      if (this.#state !== "starting" || this.#transport.closed || this.#port !== port) throw new TransportDisconnectedError();
      this.#state = "ready";
    } catch (error) {
      const cleaned = await this.#crash();
      if (!cleaned) throw new RuntimeUnavailableError();
      if (error instanceof PiError) throw error;
      throw new RuntimeUnavailableError();
    }
  }

  async #createRpcPort(launchTarget: PiLaunchTarget, trustPolicy: ReturnType<typeof runtimeTrustPolicy>): Promise<ProcessPort> {
    // Keep the launch object and its provider values scoped to the actual
    // spawn call. Neither the runtime nor its service cache retains them.
    const launch = await this.#resolveRpcLaunch();
    const baseEnvironment = buildRuntimeEnvironment(
      this.#options.hostEnvironment,
      launch.providerEnvironment,
      launch.allowedProviderKeys,
    );
    const env = mergeRuntimeEnvironment(baseEnvironment, trustPolicy);
    const args = [
      ...launchTarget.argvPrefix,
      "--mode",
      "rpc",
      "--session-id",
      this.#options.session,
      "--model",
      launch.model,
      "--thinking",
      launch.thinking,
      ...trustPolicy.args,
    ] as const;
    return (this.#options.spawn ?? nodeProcessFactory)(launchTarget.executable, args, { cwd: this.#options.cwd, env });
  }

  #launchFor(detection: PiDetection): PiLaunchTarget | undefined {
    if (detection.status !== "detected") return undefined;
    const launch = detection.launch ?? (
      detection.executable === undefined
        ? undefined
        : { executable: detection.executable, argvPrefix: [] }
    );
    if (
      launch === undefined
      || !isAbsolute(launch.executable)
      || !Array.isArray(launch.argvPrefix)
      || !launch.argvPrefix.every((argument) => typeof argument === "string")
      || isPathWithin(launch.executable, this.#options.cwd)
    ) return undefined;

    const executableName = basename(launch.executable).toLowerCase();
    if (launch.argvPrefix.length === 0) {
      return executableName === "pi" || executableName === "pi.exe" ? launch : undefined;
    }
    const entrypoint = launch.argvPrefix[0];
    if (
      executableName !== "node.exe"
      || entrypoint === undefined
      || !isAbsolute(entrypoint)
      || extname(entrypoint).toLowerCase() !== ".js"
      || isPathWithin(entrypoint, this.#options.cwd)
    ) return undefined;
    return launch;
  }

  async #resolveRpcLaunch(): Promise<PiRpcLaunch> {
    if (this.#options.resolveRpcLaunch !== undefined) return this.#options.resolveRpcLaunch();
    if (typeof this.#options.model !== "string" || typeof this.#options.thinking !== "string") {
      throw new RuntimeUnavailableError();
    }
    return {
      model: this.#options.model,
      thinking: this.#options.thinking,
      providerEnvironment: {},
      allowedProviderKeys: new Set(),
    };
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

  /** Returns the bounded, path-free projection of Pi's current native session. */
  async getSessionState(): Promise<PiSessionState> {
    const { transport, response } = await this.#requestSessionCommand({ type: "get_state" });
    const parsed = piSessionStateResponseSchema.safeParse(response);
    if (!parsed.success) return this.#failSessionProtocol(transport);
    return parsed.data.data;
  }

  /** Creates a new native Pi session without accepting a parent path or ID. */
  async newSession(): Promise<PiNewSessionResult> {
    const { transport, response } = await this.#requestSessionCommand({ type: "new_session" });
    const parsed = piNewSessionResponseSchema.safeParse(response);
    if (!parsed.success) return this.#failSessionProtocol(transport);
    return parsed.data.data;
  }

  /** Returns only bounded user, assistant, and system text from Pi history. */
  async getMessages(): Promise<readonly PiSessionMessage[]> {
    const { transport, response } = await this.#requestSessionCommand({ type: "get_messages" });
    const parsed = piSessionMessagesResponseSchema.safeParse(response);
    if (!parsed.success) return this.#failSessionProtocol(transport);
    const messages = parsePiSessionMessages(parsed.data.data);
    if (messages === undefined) return this.#failSessionProtocol(transport);
    return messages;
  }

  /** Returns Pi's declared slash-command metadata, without an execution API. */
  async getCommands(): Promise<readonly PiSessionCommandDescriptor[]> {
    const { transport, response } = await this.#requestSessionCommand({ type: "get_commands" });
    const parsed = piSessionCommandsResponseSchema.safeParse(response);
    if (!parsed.success) return this.#failSessionProtocol(transport);
    return parsed.data.data.commands;
  }

  async #requestSessionCommand(command: PiSessionCommand): Promise<{
    readonly transport: PiJsonlTransport;
    readonly response: PiResponse;
  }> {
    const parsedCommand = piSessionCommandSchema.safeParse(command);
    if (!parsedCommand.success) throw new ProtocolViolationError();
    const transport = this.#readyTransport();
    const response = await transport.request(parsedCommand.data);
    if (response.command !== parsedCommand.data.type) return this.#failSessionProtocol(transport);
    if (!response.success) throw new RuntimeUnavailableError();
    return { transport, response };
  }

  #readyTransport(): PiJsonlTransport {
    if (this.#state !== "ready" || !this.#transport) throw new RuntimeUnavailableError();
    return this.#transport;
  }

  #failSessionProtocol(transport: PiJsonlTransport): never {
    const error = new ProtocolViolationError();
    transport.close(error);
    throw error;
  }

  stop(): Promise<void> {
    return this.#exclusive(() => this.#stopInternal());
  }

  async #stopInternal(): Promise<void> {
    if (this.#state === "stopped" || this.#state === "unavailable") {
      this.#clearRuntimeReferences();
      this.#state = "stopped";
      return;
    }
    if (this.#state === "crashed" && this.#probeCleanup !== undefined) {
      const probeCleanup = this.#probeCleanup;
      if (!await probeCleanup.retryCleanup()) throw new RuntimeUnavailableError();
      if (this.#probeCleanup === probeCleanup) this.#probeCleanup = undefined;
      this.#clearRuntimeReferences();
      this.#state = "stopped";
      return;
    }
    if (this.#state === "crashed" && this.#crashCleanup !== undefined) {
      const cleaned = await this.#crashCleanup;
      if (cleaned) {
        this.#clearRuntimeReferences();
        this.#state = "stopped";
        return;
      }
    }
    if (!this.#port) {
      this.#clearRuntimeReferences();
      this.#state = "stopped";
      return;
    }
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
      this.#running = false;
      this.#state = "crashed";
      throw new RuntimeUnavailableError();
    }
    this.#clearRuntimeReferences();
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
    if (
      this.#stopRequested
      || this.#state === "stopped"
      || this.#state === "unavailable"
      || this.#state === "stopping"
      || this.#state === "crashed"
    ) return;
    void this.#crash().catch(() => undefined);
  }

  #crash(): Promise<boolean> {
    if (this.#state === "crashed") return this.#crashCleanup ?? Promise.resolve(true);

    const port = this.#port;
    const transport = this.#transport;
    const unsubscribe = this.#unsubscribe;
    this.#transport = undefined;
    this.#unsubscribe = undefined;
    this.#detection = undefined;
    this.#running = false;
    this.#state = "crashed";

    try { unsubscribe?.(); } catch { /* event cleanup is best effort */ }
    try { transport?.close(); } catch { /* transport may already be gone */ }

    this.#crashCleanup = this.#teardownPort(port).then(
      (cleaned) => {
        if (cleaned) {
          if (this.#port === port) this.#port = undefined;
          this.#notifyCrashed();
        } else {
          this.#notifyCrashCleanupFailed();
        }
        return cleaned;
      },
      () => {
        this.#notifyCrashCleanupFailed();
        return false;
      },
    );
    return this.#crashCleanup;
  }

  #notifyCrashed(): void {
    if (this.#crashNotified) return;
    this.#crashNotified = true;
    try { this.#options.onCrashed?.(); } catch { /* cache notification must not corrupt cleanup */ }
  }

  #notifyCrashCleanupFailed(): void {
    if (this.#crashCleanupFailureNotified) return;
    this.#crashCleanupFailureNotified = true;
    try { this.#options.onCrashCleanupFailed?.(); } catch { /* cache notification must not corrupt cleanup */ }
  }

  #clearRuntimeReferences(): void {
    const transport = this.#transport;
    const unsubscribe = this.#unsubscribe;
    this.#port = undefined;
    this.#transport = undefined;
    this.#unsubscribe = undefined;
    this.#detection = undefined;
    this.#probeCleanup = undefined;
    this.#running = false;
    try { unsubscribe?.(); } catch { /* event cleanup is best effort */ }
    try { transport?.close(); } catch { /* transport may already be gone */ }
  }

  async #teardownPort(port: ProcessPort | undefined): Promise<boolean> {
    if (!port) return true;
    const timeoutMs = this.#options.stopTimeoutMs ?? 1_000;
    await this.#endPortStdin(port, timeoutMs);
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
    await this.#endPortStdin(this.#port, timeoutMs);
  }

  async #endPortStdin(port: ProcessPort | undefined, timeoutMs: number): Promise<void> {
    if (!port) return;
    try {
      const result = port.stdin.end();
      if (result && typeof (result as PromiseLike<unknown>).then === "function") {
        await this.#raceWithTimeout(() => result as PromiseLike<unknown>, timeoutMs);
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
