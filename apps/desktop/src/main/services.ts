import {
  createOpenCodeRuntime,
  type OpenCodeSessionAdapter,
  type OpenCodeSessionEvent,
  type OpenCodeSessionSubscription,
} from "@halo-studio/agent-opencode";
import {
  createPiRuntime,
  detectPi,
  type PiDetection,
  type PiLifecycleState,
} from "@halo-studio/agent-pi";
import {
  type AgentKind,
  type AgentEventEnvelope,
  type CommandDescriptor,
  type DataOf,
  type RuntimeBinding,
  type SessionHistory,
  type SessionSummary,
  type TrustState,
  type Workspace,
  type InputOf,
  sessionEventSchema,
} from "@halo-studio/contracts";
import {
  ConfigTransaction,
  TargetRegistry,
} from "@halo-studio/config";
import {
  CoreError,
  MemoryTrustStore,
  openWorkspace,
  redactLogValue,
} from "@halo-studio/core";
import {
  FileCredentialVault,
  openDatabase,
  type CredentialVault,
  type HaloDatabase,
} from "@halo-studio/storage";
import { mkdir, stat } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { createElectronSecretProtector, type SafeStoragePort } from "./electronSecretProtector.js";
import type { IpcServiceMap } from "./ipc/registerIpc.js";
import {
  createEnvironmentPiLaunchResolver,
  type PiLaunchResolver,
  validatePiLaunchConfiguration,
} from "./piLaunchResolver.js";
import {
  createSessionCoordinator,
  type ManagedSessionAdapter,
} from "./sessionCoordinator.js";

export interface WorkspaceDialogPort {
  showOpenDialog(options: { readonly properties: readonly string[] }): Promise<{
    readonly canceled: boolean;
    readonly filePaths: readonly string[];
  }>;
}

export interface DesktopDataPaths {
  readonly userDataDirectory: string;
  readonly storageDirectory: string;
  readonly databasePath: string;
  readonly credentialDirectory: string;
  readonly piRuntimeDirectory: string;
  readonly openCodeRuntimeDirectory: string;
}

export interface DesktopServices {
  readonly paths: DesktopDataPaths;
  readonly handlers: IpcServiceMap;
  subscribeSessionEvents(listener: (event: AgentEventEnvelope) => void): () => void;
  dispose(): Promise<void>;
}

interface WorkspaceIdentity {
  readonly device: bigint;
  readonly inode: bigint;
}

type WorkspaceIdentityPort = (realPath: string) => Promise<WorkspaceIdentity>;
type PiDetector = (options: Parameters<typeof detectPi>[0]) => Promise<PiDetection>;

export interface CreateDesktopServicesOptions {
  readonly userDataPath: string;
  readonly picker: WorkspaceDialogPort;
  readonly safeStorage: SafeStoragePort;
  readonly hostEnvironment: Readonly<Record<string, string | undefined>>;
  /** Main-process test seam; production uses the core workspace opener. */
  readonly openWorkspace?: typeof openWorkspace;
  /** Main-process test seam; production uses the bundled OpenCode runtime factory. */
  readonly createOpenCodeRuntime?: OpenCodeRuntimeFactory;
  /** Main-process test seam; production uses the Pi runtime factory. */
  readonly createPiRuntime?: PiRuntimeFactory;
  /** Main-process test seam; production probes the installed Pi executable. */
  readonly detectPi?: PiDetector;
  /**
   * Main-only source for Pi model/thinking and Provider credentials. This is
   * intentionally not an IPC capability and is never visible to Renderer.
   */
  readonly resolvePiLaunch?: PiLaunchResolver;
  /** Main-process test seam; production uses filesystem device and inode identity. */
  readonly workspaceIdentity?: WorkspaceIdentityPort;
}

type OpenCodeRuntimeInstance = Pick<ReturnType<typeof createOpenCodeRuntime>, "state" | "detect" | "start" | "stop">
  & Partial<Pick<ReturnType<typeof createOpenCodeRuntime>, "createSessionAdapter">>;
type OpenCodeRuntimeFactory = (options: Parameters<typeof createOpenCodeRuntime>[0]) => OpenCodeRuntimeInstance;
type PiRuntimeInstance = Pick<
  ReturnType<typeof createPiRuntime>,
  "state" | "detect" | "start" | "stop"
> & Partial<Pick<
  ReturnType<typeof createPiRuntime>,
  "prompt" | "abort" | "getSessionState" | "newSession" | "getMessages" | "getCommands"
>>;
type PiRuntimeFactory = (options: Parameters<typeof createPiRuntime>[0]) => PiRuntimeInstance;
type PiSessionRuntime = Pick<
  ReturnType<typeof createPiRuntime>,
  "prompt" | "abort" | "getSessionState" | "newSession" | "getMessages" | "getCommands"
>;

interface RuntimeMetadata {
  readonly source: RuntimeBinding["source"];
  readonly executable?: string;
  readonly version?: string;
}

interface PiBindingState {
  readonly health: RuntimeBinding["health"];
  readonly metadata: RuntimeMetadata;
}

interface OpenCodeSessionSubscriptionRecord {
  readonly runtime: OpenCodeRuntimeInstance;
  readonly subscription: OpenCodeSessionSubscription;
}

const unavailableCapabilities = {
  sessions: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  streamingMessages: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  toolEvents: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  permissions: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  diff: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  commands: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  mcp: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  skills: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  prompts: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  extensions: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  packages: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  models: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
  usage: { supported: false, channel: "unavailable", restartRequired: false, reason: "Capability is not exposed by the desktop bridge yet." },
} as const;

function runtimeCapabilities(
  kind: AgentKind,
  health: RuntimeBinding["health"],
): RuntimeBinding["capabilities"] {
  if (kind === "pi" && health === "ready") {
    return {
      ...unavailableCapabilities,
      sessions: { supported: true, channel: "rpc", restartRequired: false },
      streamingMessages: { supported: true, channel: "rpc", restartRequired: false },
      toolEvents: { supported: true, channel: "rpc", restartRequired: false },
      commands: { supported: true, channel: "rpc", restartRequired: false },
    };
  }
  if (kind === "opencode" && health === "healthy") {
    return {
      ...unavailableCapabilities,
      sessions: { supported: true, channel: "http", restartRequired: false },
      streamingMessages: { supported: true, channel: "sse", restartRequired: false },
      toolEvents: { supported: true, channel: "sse", restartRequired: false },
    };
  }
  return unavailableCapabilities;
}

function openCodeRuntimeHealth(runtime: OpenCodeRuntimeInstance): RuntimeBinding["health"] {
  const state = runtime.state;
  if (state === "healthy") return "healthy";
  if (state === "starting") return "starting";
  if (state === "stopping") return "stopping";
  if (state === "stopped") return "stopped";
  if (state === "crashed") return "crashed";
  if (state === "installed") return "installed";
  return "unavailable";
}

function piRuntimeHealth(runtime: PiRuntimeInstance): RuntimeBinding["health"] {
  const state: PiLifecycleState = runtime.state;
  if (state === "ready") return "ready";
  if (state === "starting") return "starting";
  if (state === "stopping") return "stopping";
  if (state === "stopped") return "stopped";
  if (state === "crashed") return "crashed";
  if (state === "detected") return "detected";
  return "unavailable";
}

export function createRuntimeBinding(
  kind: AgentKind,
  health: RuntimeBinding["health"],
  metadata: RuntimeMetadata = {
    source: kind === "opencode" ? "bundled" : "managed",
  },
): RuntimeBinding {
  return {
    agentKind: kind,
    source: metadata.source,
    ...(metadata.executable === undefined ? {} : { executable: metadata.executable }),
    ...(metadata.version === undefined ? {} : { version: metadata.version }),
    health,
    capabilities: runtimeCapabilities(kind, health),
  };
}

function workspaceNotFound(): CoreError {
  return new CoreError("ProtocolViolation", "Workspace is unavailable.");
}

function untrusted(): CoreError {
  return new CoreError("WorkspaceUntrusted", "Workspace trust is required.");
}

function runtimeUnavailable(): CoreError {
  return new CoreError("RuntimeUnavailable", "Runtime is unavailable.");
}

function safeHostEnvironment(
  source: Readonly<Record<string, string | undefined>>,
  runtimeDirectory: string,
): Readonly<Record<string, string | undefined>> {
  return { ...source, HOME: runtimeDirectory, USERPROFILE: runtimeDirectory };
}

function sessionMismatch(): CoreError {
  return new CoreError("ProtocolViolation", "Managed session is unavailable.");
}

function requirePiSessionRuntime(runtime: PiRuntimeInstance): PiSessionRuntime {
  if (
    runtime.prompt === undefined
    || runtime.abort === undefined
    || runtime.getSessionState === undefined
    || runtime.newSession === undefined
    || runtime.getMessages === undefined
    || runtime.getCommands === undefined
  ) throw runtimeUnavailable();
  return runtime as PiSessionRuntime;
}

function piSessionSummary(
  state: Awaited<ReturnType<PiSessionRuntime["getSessionState"]>>,
): SessionSummary {
  const title = state.sessionName?.trim();
  return {
    agentKind: "pi",
    sessionId: state.sessionId,
    ...(title === undefined || title.length === 0 ? {} : { title }),
    active: state.isStreaming || state.isCompacting || state.pendingMessageCount > 0,
  };
}

async function requireCurrentPiSession(
  runtime: PiSessionRuntime,
  sessionId: string,
): Promise<SessionSummary> {
  const summary = piSessionSummary(await runtime.getSessionState());
  if (summary.sessionId !== sessionId) throw sessionMismatch();
  return summary;
}

function createPiSessionAdapter(runtime: PiRuntimeInstance): ManagedSessionAdapter {
  const pi = requirePiSessionRuntime(runtime);
  return {
    agentKind: "pi",
    snapshot: async () => [piSessionSummary(await pi.getSessionState())],
    create: async () => {
      const result = await pi.newSession();
      if (result.cancelled) throw runtimeUnavailable();
      return piSessionSummary(await pi.getSessionState());
    },
    get: async (sessionId) => requireCurrentPiSession(pi, sessionId),
    history: async (sessionId): Promise<SessionHistory> => {
      const session = await requireCurrentPiSession(pi, sessionId);
      const messages = await pi.getMessages();
      return {
        session,
        messages: messages.map((message, ordinal) => ({
          agentKind: "pi",
          sessionId: session.sessionId,
          ordinal,
          role: message.role,
          text: message.text,
        })),
      };
    },
    send: async (sessionId, message) => {
      await requireCurrentPiSession(pi, sessionId);
      const response = await pi.prompt(message);
      if (!response.success || response.command !== "prompt") throw runtimeUnavailable();
    },
    abort: async (sessionId) => {
      await requireCurrentPiSession(pi, sessionId);
      const response = await pi.abort();
      if (!response.success || response.command !== "abort") throw runtimeUnavailable();
    },
    commands: async (): Promise<readonly CommandDescriptor[]> => {
      const commands = await pi.getCommands();
      return commands.map((command) => ({
        name: `/${command.name}`,
        ...(command.description === undefined ? {} : { description: command.description }),
        agentKind: "pi",
        source: command.source,
        channel: "rpc",
        allowedWhileRunning: false,
        mutatesGlobalDefaults: false,
        tuiOnly: false,
      }));
    },
  };
}

function openCodeSessionSummary(
  summary: Awaited<ReturnType<OpenCodeSessionAdapter["get"]>>,
): SessionSummary {
  return {
    agentKind: "opencode",
    sessionId: summary.sessionId,
    ...(summary.title === undefined ? {} : { title: summary.title }),
    ...(summary.updatedAt === undefined ? {} : { updatedAt: summary.updatedAt }),
    active: summary.active,
  };
}

function createOpenCodeManagedSessionAdapter(adapter: OpenCodeSessionAdapter): ManagedSessionAdapter {
  return {
    agentKind: "opencode",
    snapshot: async () => (await adapter.list()).map(openCodeSessionSummary),
    create: async () => openCodeSessionSummary(await adapter.create()),
    get: async (sessionId) => openCodeSessionSummary(await adapter.get(sessionId)),
    history: async (sessionId): Promise<SessionHistory> => {
      const history = await adapter.history(sessionId);
      const session = openCodeSessionSummary(history.session);
      return {
        session,
        messages: history.messages.map((message) => ({
          agentKind: "opencode",
          sessionId: session.sessionId,
          ordinal: message.ordinal,
          role: message.role,
          text: message.text,
        })),
      };
    },
    send: async (sessionId, message) => adapter.startPrompt(sessionId, message),
    abort: async (sessionId) => adapter.abort(sessionId),
    commands: async () => [],
  };
}

function sessionIdFromOpenCodeEvent(event: OpenCodeSessionEvent): string | undefined {
  if ("sessionId" in event) return event.sessionId;
  if ("session" in event) return event.session.sessionId;
  return undefined;
}

function redactSessionEvent(event: AgentEventEnvelope): AgentEventEnvelope | undefined {
  const parsed = sessionEventSchema.safeParse(redactLogValue(event));
  return parsed.success ? parsed.data : undefined;
}

async function readWorkspaceIdentity(realPath: string): Promise<WorkspaceIdentity> {
  const details = await stat(realPath, { bigint: true });
  if (!details.isDirectory()) throw new Error("Workspace path is unavailable.");
  return { device: details.dev, inode: details.ino };
}

function sameWorkspaceIdentity(left: WorkspaceIdentity, right: WorkspaceIdentity): boolean {
  return left.device === right.device && left.inode === right.inode;
}

export async function createDesktopServices(options: CreateDesktopServicesOptions): Promise<DesktopServices> {
  const storageDirectory = join(options.userDataPath, "storage");
  const databasePath = join(storageDirectory, "halo-studio.sqlite3");
  const credentialDirectory = join(options.userDataPath, "credentials");
  const piRuntimeDirectory = join(options.userDataPath, "runtime", "pi");
  const openCodeRuntimeDirectory = join(options.userDataPath, "runtime", "opencode");
  const paths: DesktopDataPaths = {
    userDataDirectory: options.userDataPath,
    storageDirectory,
    databasePath,
    credentialDirectory,
    piRuntimeDirectory,
    openCodeRuntimeDirectory,
  };

  await Promise.all([
    mkdir(storageDirectory, { recursive: true }),
    mkdir(credentialDirectory, { recursive: true }),
    mkdir(piRuntimeDirectory, { recursive: true }),
    mkdir(openCodeRuntimeDirectory, { recursive: true }),
  ]);

  // Keep construction order explicit: storage -> trust/workspace -> config -> runtimes.
  const database: HaloDatabase = openDatabase(databasePath);
  const protector = createElectronSecretProtector(options.safeStorage);
  const vault: CredentialVault = new FileCredentialVault(credentialDirectory, protector);
  const trustStore = new MemoryTrustStore();
  const workspaceOpener = options.openWorkspace ?? openWorkspace;
  const openCodeRuntimeFactory = options.createOpenCodeRuntime ?? createOpenCodeRuntime;
  const piRuntimeFactory = options.createPiRuntime ?? createPiRuntime;
  const piDetector = options.detectPi ?? detectPi;
  const piLaunchResolver = options.resolvePiLaunch ?? createEnvironmentPiLaunchResolver({
    environment: options.hostEnvironment,
    vault,
  });
  const workspaceIdentityReader = options.workspaceIdentity ?? readWorkspaceIdentity;
  const registry = new TargetRegistry();
  const config = new ConfigTransaction(registry, { vault });
  const selections = new Map<string, string>();
  const workspaces = new Map<string, Workspace>();
  const workspaceIdentities = new Map<string, WorkspaceIdentity>();
  const openCodeRuntimes = new Map<string, OpenCodeRuntimeInstance>();
  const retainedOpenCodeRuntimes = new Map<string, Set<OpenCodeRuntimeInstance>>();
  const openCodeMetadata = new Map<string, RuntimeMetadata>();
  const openCodeSessionAdapters = new Map<string, {
    readonly runtime: OpenCodeRuntimeInstance;
    readonly adapter: ManagedSessionAdapter;
  }>();
  const openCodeSessionSubscriptions = new Map<string, OpenCodeSessionSubscriptionRecord>();
  const openCodeEventSequences = new Map<string, number>();
  const piRuntimes = new Map<string, PiRuntimeInstance>();
  const retainedPiRuntimes = new Map<string, Set<PiRuntimeInstance>>();
  const piBindingStates = new Map<string, PiBindingState>();
  const workspaceLifecycleOperations = new Map<string, Promise<void>>();
  const activeServiceOperations = new Set<Promise<void>>();
  let activeWorkspaceId: string | undefined;
  let workspaceSelectionOperation: Promise<void> = Promise.resolve();
  let shutdownStarted = false;
  let disposePromise: Promise<void> | undefined;

  const sessionCoordinator = createSessionCoordinator((workspaceId, agentKind) => {
    const workspace = workspaces.get(workspaceId);
    if (workspace?.trustState !== "trusted") return undefined;
    if (agentKind === "pi") {
      const runtime = piRuntimes.get(workspaceId);
      return runtime?.state === "ready" ? createPiSessionAdapter(runtime) : undefined;
    }
    const record = openCodeSessionAdapters.get(workspaceId);
    return record?.runtime.state === "healthy" ? record.adapter : undefined;
  });
  const publishSessionEvent = (event: AgentEventEnvelope): void => {
    const redacted = redactSessionEvent(event);
    if (redacted !== undefined) sessionCoordinator.publish(redacted);
  };

  const getWorkspace = (workspaceId: string): Workspace => {
    const workspace = workspaces.get(workspaceId);
    if (workspace === undefined) throw workspaceNotFound();
    return workspace;
  };
  const confirmPiLaunchWorkspace = async (workspace: Workspace): Promise<Workspace> => {
    const current = workspaces.get(workspace.id);
    if (
      current === undefined
      || current.realPath !== workspace.realPath
      || current.trustState !== "trusted"
    ) throw runtimeUnavailable();
    let reopened: Workspace;
    let identity: WorkspaceIdentity;
    try {
      reopened = await workspaceOpener(workspace.rootPath, trustStore);
      identity = await workspaceIdentityReader(reopened.realPath);
    } catch {
      throw runtimeUnavailable();
    }
    const expectedIdentity = workspaceIdentities.get(workspace.id);
    if (
      reopened.id !== workspace.id
      || reopened.realPath !== workspace.realPath
      || reopened.trustState !== "trusted"
      || expectedIdentity === undefined
      || !sameWorkspaceIdentity(expectedIdentity, identity)
    ) throw runtimeUnavailable();
    return reopened;
  };
  const evictPiRuntime = (
    workspaceId: string,
    runtime: PiRuntimeInstance,
    health: RuntimeBinding["health"] = "crashed",
  ): void => {
    if (piRuntimes.get(workspaceId) !== runtime) return;
    piRuntimes.delete(workspaceId);
    const current = piBindingStates.get(workspaceId);
    piBindingStates.set(workspaceId, {
      health,
      metadata: current?.metadata ?? { source: "managed" },
    });
  };
  const retainPiRuntime = (workspaceId: string, runtime: PiRuntimeInstance): void => {
    const retained = retainedPiRuntimes.get(workspaceId);
    if (piRuntimes.get(workspaceId) !== runtime && !retained?.has(runtime)) return;
    if (piRuntimes.get(workspaceId) === runtime) piRuntimes.delete(workspaceId);
    const next = new Set(retained ?? []);
    next.add(runtime);
    retainedPiRuntimes.set(workspaceId, next);
    const current = piBindingStates.get(workspaceId);
    piBindingStates.set(workspaceId, {
      health: "crashed",
      metadata: current?.metadata ?? { source: "managed" },
    });
  };
  const getOpenCodeRuntime = (workspace: Workspace): OpenCodeRuntimeInstance => {
    if (retainedOpenCodeRuntimes.has(workspace.id)) throw runtimeUnavailable();
    const existing = openCodeRuntimes.get(workspace.id);
    if (existing !== undefined) return existing;
    const runtime = openCodeRuntimeFactory({
      cwd: workspace.realPath,
      trust: workspace.trustState,
      hostEnvironment: safeHostEnvironment(options.hostEnvironment, openCodeRuntimeDirectory),
    });
    openCodeRuntimes.set(workspace.id, runtime);
    return runtime;
  };
  const getPiRuntime = (workspace: Workspace): PiRuntimeInstance => {
    if (workspace.trustState !== "trusted") throw untrusted();
    if (retainedPiRuntimes.has(workspace.id)) throw runtimeUnavailable();
    const existing = piRuntimes.get(workspace.id);
    if (existing !== undefined) return existing;
    let runtime: PiRuntimeInstance | undefined;
    runtime = piRuntimeFactory({
      cwd: workspace.realPath,
      session: randomUUID(),
      trust: workspace.trustState,
      hostEnvironment: safeHostEnvironment(options.hostEnvironment, piRuntimeDirectory),
      // Detection remains Main-owned and credential-blind. It is deliberately
      // omitted for untrusted workspaces by the caller below.
      detect: () => piDetector({
        cwd: workspace.realPath,
        hostEnvironment: safeHostEnvironment(options.hostEnvironment, piRuntimeDirectory),
      }),
      // This resolver does not capture a provider value. It is invoked by
      // PiRuntime only after its credential-blind version probe succeeds.
      resolveRpcLaunch: async () => {
        // Detection can take time. Revalidate again immediately before the
        // resolver reads a credential or the RPC child is spawned.
        const confirmed = await confirmPiLaunchWorkspace(workspace);
        return validatePiLaunchConfiguration(await piLaunchResolver({
          workspace: {
            id: confirmed.id,
            realPath: confirmed.realPath,
            trustState: confirmed.trustState,
          },
        }));
      },
      onCrashed: () => {
        if (runtime !== undefined) evictPiRuntime(workspace.id, runtime);
      },
      onCrashCleanupFailed: () => {
        if (runtime !== undefined) retainPiRuntime(workspace.id, runtime);
      },
      onEvent: publishSessionEvent,
      workspaceId: workspace.id,
    });
    piRuntimes.set(workspace.id, runtime);
    return runtime;
  };
  const openCodeBinding = (workspace: Workspace, runtime = openCodeRuntimes.get(workspace.id)): RuntimeBinding => {
    return createRuntimeBinding("opencode", runtime === undefined ? "unavailable" : openCodeRuntimeHealth(runtime), openCodeMetadata.get(workspace.id));
  };
  const piBinding = (workspace: Workspace): RuntimeBinding => {
    const runtime = piRuntimes.get(workspace.id);
    const state = piBindingStates.get(workspace.id);
    return createRuntimeBinding(
      "pi",
      runtime === undefined ? (state?.health ?? "unavailable") : piRuntimeHealth(runtime),
      state?.metadata,
    );
  };
  const piMetadata = (detection: PiDetection): RuntimeMetadata => ({
    source: detection.source,
    ...(detection.executable === undefined ? {} : { executable: detection.executable }),
    ...(detection.version === undefined ? {} : { version: detection.version }),
  });
  const detachOpenCodeSessionAdapter = async (workspaceId: string): Promise<void> => {
    const subscription = openCodeSessionSubscriptions.get(workspaceId);
    openCodeSessionSubscriptions.delete(workspaceId);
    openCodeSessionAdapters.delete(workspaceId);
    openCodeEventSequences.delete(workspaceId);
    if (subscription === undefined) return;
    try { await subscription.subscription.unsubscribe(); }
    catch { /* Runtime stop owns its transport cleanup when a subscription is already disconnected. */ }
  };
  const attachOpenCodeSessionAdapter = async (
    workspace: Workspace,
    runtime: OpenCodeRuntimeInstance,
  ): Promise<void> => {
    if (runtime.state !== "healthy" || runtime.createSessionAdapter === undefined) throw runtimeUnavailable();
    const current = openCodeSessionAdapters.get(workspace.id);
    if (current?.runtime === runtime && openCodeSessionSubscriptions.get(workspace.id)?.runtime === runtime) return;
    await detachOpenCodeSessionAdapter(workspace.id);
    const nativeAdapter = runtime.createSessionAdapter();
    const adapter = createOpenCodeManagedSessionAdapter(nativeAdapter);
    openCodeSessionAdapters.set(workspace.id, { runtime, adapter });
    try {
      const subscription = await nativeAdapter.subscribe((event) => {
        const sequence = openCodeEventSequences.get(workspace.id) ?? 0;
        const sessionId = sessionIdFromOpenCodeEvent(event);
        openCodeEventSequences.set(workspace.id, sequence + 1);
        publishSessionEvent({
          eventId: randomUUID(),
          workspaceId: workspace.id,
          ...(sessionId === undefined ? {} : { sessionId }),
          sequence,
          timestamp: new Date().toISOString(),
          agentKind: "opencode",
          payload: {
            protocol: "opencode-sse",
            type: event.type,
            data: redactLogValue(event),
          },
        });
      });
      openCodeSessionSubscriptions.set(workspace.id, { runtime, subscription });
    } catch (error) {
      const currentAdapter = openCodeSessionAdapters.get(workspace.id);
      if (currentAdapter?.runtime === runtime) openCodeSessionAdapters.delete(workspace.id);
      openCodeEventSequences.delete(workspace.id);
      throw error;
    }
  };
  const workspaceOpenCodeRuntimes = (workspaceId: string): Set<OpenCodeRuntimeInstance> => {
    const current = openCodeRuntimes.get(workspaceId);
    return new Set<OpenCodeRuntimeInstance>([
      ...(current === undefined ? [] : [current]),
      ...(retainedOpenCodeRuntimes.get(workspaceId) ?? []),
    ]);
  };
  const stopWorkspaceOpenCodeRuntimes = async (workspaceId: string): Promise<boolean> => {
    await detachOpenCodeSessionAdapter(workspaceId);
    const retained = new Set<OpenCodeRuntimeInstance>();
    for (const runtime of workspaceOpenCodeRuntimes(workspaceId)) {
      try { await runtime.stop(); }
      catch { retained.add(runtime); }
    }
    if (retained.size === 0) retainedOpenCodeRuntimes.delete(workspaceId);
    else retainedOpenCodeRuntimes.set(workspaceId, retained);
    return retained.size === 0;
  };
  const workspacePiRuntimes = (workspaceId: string): Set<PiRuntimeInstance> => {
    const current = piRuntimes.get(workspaceId);
    return new Set<PiRuntimeInstance>([
      ...(current === undefined ? [] : [current]),
      ...(retainedPiRuntimes.get(workspaceId) ?? []),
    ]);
  };
  const stopWorkspacePiRuntimes = async (workspaceId: string): Promise<boolean> => {
    const retained = new Set<PiRuntimeInstance>();
    for (const runtime of workspacePiRuntimes(workspaceId)) {
      try { await runtime.stop(); }
      catch { retained.add(runtime); }
    }
    if (retained.size === 0) retainedPiRuntimes.delete(workspaceId);
    else retainedPiRuntimes.set(workspaceId, retained);
    return retained.size === 0;
  };
  const stopOrphanedOpenCodeRuntimes = async (workspaceId: string): Promise<RuntimeBinding | undefined> => {
    if (workspaceOpenCodeRuntimes(workspaceId).size === 0) return undefined;
    if (!await stopWorkspaceOpenCodeRuntimes(workspaceId)) throw runtimeUnavailable();
    const metadata = openCodeMetadata.get(workspaceId);
    openCodeRuntimes.delete(workspaceId);
    openCodeMetadata.delete(workspaceId);
    return createRuntimeBinding("opencode", "stopped", metadata);
  };
  const stopOrphanedPiRuntimes = async (workspaceId: string): Promise<RuntimeBinding | undefined> => {
    if (workspacePiRuntimes(workspaceId).size === 0) return undefined;
    if (!await stopWorkspacePiRuntimes(workspaceId)) throw runtimeUnavailable();
    const metadata = piBindingStates.get(workspaceId)?.metadata;
    piRuntimes.delete(workspaceId);
    piBindingStates.delete(workspaceId);
    return createRuntimeBinding("pi", "stopped", metadata);
  };
  const discardWorkspaceRuntimeState = async (workspaceId: string): Promise<boolean> => {
    const [openCodeStopped, piStopped] = await Promise.all([
      stopWorkspaceOpenCodeRuntimes(workspaceId),
      stopWorkspacePiRuntimes(workspaceId),
    ]);
    openCodeRuntimes.delete(workspaceId);
    openCodeMetadata.delete(workspaceId);
    piRuntimes.delete(workspaceId);
    if (piStopped) {
      piBindingStates.delete(workspaceId);
    } else {
      const current = piBindingStates.get(workspaceId);
      piBindingStates.set(workspaceId, {
        health: "crashed",
        metadata: current?.metadata ?? { source: "managed" },
      });
    }
    sessionCoordinator.discardWorkspace(workspaceId);
    return openCodeStopped && piStopped;
  };
  const invalidateWorkspace = async (workspaceId: string): Promise<boolean> => {
    const stopped = await discardWorkspaceRuntimeState(workspaceId);
    workspaces.delete(workspaceId);
    workspaceIdentities.delete(workspaceId);
    if (activeWorkspaceId === workspaceId) activeWorkspaceId = undefined;
    sessionCoordinator.discardWorkspace(workspaceId);
    return stopped;
  };
  const revalidateWorkspace = async (workspace: Workspace): Promise<Workspace> => {
    let reopened: Workspace;
    let identity: WorkspaceIdentity;
    try {
      reopened = await workspaceOpener(workspace.rootPath, trustStore);
      identity = await workspaceIdentityReader(reopened.realPath);
    }
    catch {
      if (!await invalidateWorkspace(workspace.id)) throw runtimeUnavailable();
      throw workspaceNotFound();
    }
    const expectedIdentity = workspaceIdentities.get(workspace.id);
    if (
      reopened.id !== workspace.id
      || reopened.realPath !== workspace.realPath
      || expectedIdentity === undefined
      || !sameWorkspaceIdentity(expectedIdentity, identity)
    ) {
      if (!await invalidateWorkspace(workspace.id)) throw runtimeUnavailable();
      throw workspaceNotFound();
    }
    workspaces.set(workspace.id, reopened);
    return reopened;
  };
  const runWorkspaceLifecycle = <T>(workspaceId: string, operation: () => Promise<T>): Promise<T> => {
    const previous = workspaceLifecycleOperations.get(workspaceId) ?? Promise.resolve();
    const current = previous.then(operation, operation);
    const settled = current.then(() => undefined, () => undefined);
    workspaceLifecycleOperations.set(workspaceId, settled);
    return current.finally(() => {
      if (workspaceLifecycleOperations.get(workspaceId) === settled) workspaceLifecycleOperations.delete(workspaceId);
    });
  };
  const runWorkspaceSelection = <T>(operation: () => Promise<T>): Promise<T> => {
    const previous = workspaceSelectionOperation;
    const current = previous.then(operation, operation);
    const settled = current.then(() => undefined, () => undefined);
    workspaceSelectionOperation = settled;
    return current;
  };
  const runServiceOperation = <T>(operation: () => Promise<T>): Promise<T> => {
    if (shutdownStarted) return Promise.reject(runtimeUnavailable());
    const current = Promise.resolve().then(operation);
    const settled = current.then(() => undefined, () => undefined);
    activeServiceOperations.add(settled);
    void settled.then(() => { activeServiceOperations.delete(settled); });
    return current;
  };
  const guardHandler = <Input, Output>(handler: (input: Input) => Promise<Output>) => {
    return (input: Input): Promise<Output> => runServiceOperation(() => handler(input));
  };
  const selectedWorkspaceIds = (workspaceId: string | undefined): string[] => workspaceId === undefined
    ? (activeWorkspaceId === undefined ? [] : [activeWorkspaceId])
    : [workspaceId];
  const deactivateOtherWorkspaces = async (nextWorkspaceId: string): Promise<void> => {
    for (const workspaceId of [...workspaces.keys()]) {
      if (workspaceId === nextWorkspaceId) continue;
      const stopped = await runWorkspaceLifecycle(workspaceId, () => discardWorkspaceRuntimeState(workspaceId));
      if (!stopped) throw runtimeUnavailable();
      workspaces.delete(workspaceId);
      workspaceIdentities.delete(workspaceId);
      if (activeWorkspaceId === workspaceId) activeWorkspaceId = undefined;
    }
  };
  const probeWorkspace = (workspaceId: string): Promise<RuntimeBinding[]> => runWorkspaceLifecycle(workspaceId, async () => {
    const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
    if (workspace.trustState !== "trusted") {
      // A workspace cannot influence command lookup or run a child until the
      // user has made an explicit trust decision.
      piBindingStates.set(workspace.id, { health: "unavailable", metadata: { source: "managed" } });
    } else if (!retainedPiRuntimes.has(workspace.id)) {
      const runtime = getPiRuntime(workspace);
      try {
        const detection = await runtime.detect();
        piBindingStates.set(workspace.id, detection.status === "detected"
          ? { health: "detected", metadata: piMetadata(detection) }
          : { health: "unavailable", metadata: { source: "managed" } });
      } catch {
        if (!retainedPiRuntimes.has(workspace.id)) {
          piBindingStates.set(workspace.id, { health: "unavailable", metadata: { source: "managed" } });
        }
      }
    }
    const runtime = getOpenCodeRuntime(workspace);
    try {
      const artifact = await runtime.detect();
      openCodeMetadata.set(workspace.id, {
        source: "bundled",
        executable: artifact.executable,
        version: artifact.version,
      });
    } catch {
      // Probe reports an unavailable binding; raw runtime failures never cross IPC.
    }
    return [piBinding(workspace), openCodeBinding(workspace, runtime)];
  });
  const snapshotWorkspace = (workspaceId: string): Promise<RuntimeBinding[]> => runWorkspaceLifecycle(workspaceId, async () => {
    const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
    return [piBinding(workspace), openCodeBinding(workspace)];
  });
  const sessionWorkspace = async (workspaceId: string): Promise<Workspace> => {
    const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
    if (workspace.trustState !== "trusted") throw untrusted();
    return workspace;
  };

  const rawHandlers: IpcServiceMap = {
    "workspace.pick": async () => {
      const result = await options.picker.showOpenDialog({ properties: ["openDirectory"] });
      const path = result.canceled ? undefined : result.filePaths[0];
      if (path === undefined || path.length === 0) return null;
      const selectionId = randomUUID();
      selections.set(selectionId, path);
      return { selectionId, displayPath: path };
    },
    "workspace.open": async ({ selectionId }) => {
      const path = selections.get(selectionId);
      if (path === undefined) throw workspaceNotFound();
      const workspace = await workspaceOpener(path, trustStore);
      return runWorkspaceSelection(() => runWorkspaceLifecycle(workspace.id, async () => {
        if (retainedOpenCodeRuntimes.has(workspace.id) || retainedPiRuntimes.has(workspace.id)) throw runtimeUnavailable();
        let identity: WorkspaceIdentity;
        try {
          identity = await workspaceIdentityReader(workspace.realPath);
        } catch {
          if (workspaces.has(workspace.id) && !await invalidateWorkspace(workspace.id)) throw runtimeUnavailable();
          throw workspaceNotFound();
        }
        const previous = workspaces.get(workspace.id);
        const previousIdentity = workspaceIdentities.get(workspace.id);
        if (
          previous !== undefined
          && (
            previous.realPath !== workspace.realPath
            || previousIdentity === undefined
            || !sameWorkspaceIdentity(previousIdentity, identity)
          )
        ) {
          const stopped = await invalidateWorkspace(workspace.id);
          if (!stopped) throw runtimeUnavailable();
        }
        await deactivateOtherWorkspaces(workspace.id);
        workspaces.set(workspace.id, workspace);
        workspaceIdentities.set(workspace.id, identity);
        activeWorkspaceId = workspace.id;
        return workspace;
      }));
    },
    "workspace.snapshot": async () => {
      if (activeWorkspaceId === undefined) return [];
      const workspace = workspaces.get(activeWorkspaceId);
      if (workspace === undefined) {
        activeWorkspaceId = undefined;
        return [];
      }
      return [{ ...workspace }];
    },
    "workspace.trust": ({ workspaceId, trustState }) => runWorkspaceLifecycle(workspaceId, async () => {
      const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
      const stopped = await discardWorkspaceRuntimeState(workspace.id);
      if (!stopped) throw runtimeUnavailable();
      await trustStore.setDecision(workspace.realPath, trustState as TrustState);
      const updated = { ...workspace, trustState };
      workspaces.set(workspaceId, updated);
      return updated;
    }),
    "runtime.probe": async ({ workspaceId }) => {
      const result: RuntimeBinding[] = [];
      for (const id of selectedWorkspaceIds(workspaceId)) result.push(...await probeWorkspace(id));
      return result;
    },
    "runtime.start": ({ workspaceId, agentKind }) => runWorkspaceLifecycle(workspaceId, async () => {
      const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
      if (retainedOpenCodeRuntimes.has(workspace.id) || retainedPiRuntimes.has(workspace.id)) throw runtimeUnavailable();
      if (workspace.trustState !== "trusted") throw untrusted();
      if (agentKind === "pi") {
        const runtime = getPiRuntime(workspace);
        try {
          const detection = await runtime.detect();
          piBindingStates.set(workspace.id, {
            health: detection.status === "detected" ? "detected" : "unavailable",
            metadata: detection.status === "detected" ? piMetadata(detection) : { source: "managed" },
          });
          await runtime.start();
          return piBinding(workspace);
        } catch (error) {
          // A duplicate start is rejected while the existing child remains
          // ready. A crashed child remains cached until its cleanup callback
          // has confirmed exit or retained it for an explicit stop retry.
          if (runtime.state !== "ready" && runtime.state !== "crashed") {
            evictPiRuntime(
              workspace.id,
              runtime,
              "unavailable",
            );
          }
          throw error;
        }
      }
      const runtime = getOpenCodeRuntime(workspace);
      const artifact = await runtime.detect();
      openCodeMetadata.set(workspace.id, {
        source: "bundled",
        executable: artifact.executable,
        version: artifact.version,
      });
      await runtime.start();
      if (runtime.createSessionAdapter !== undefined) {
        try {
          await attachOpenCodeSessionAdapter(workspace, runtime);
        } catch (error) {
          if (!await stopWorkspaceOpenCodeRuntimes(workspace.id)) throw runtimeUnavailable();
          throw error;
        }
      }
      return openCodeBinding(workspace, runtime);
    }),
    "runtime.stop": ({ workspaceId, agentKind }) => runWorkspaceLifecycle(workspaceId, async () => {
      if (!workspaces.has(workspaceId)) {
        const stopped = agentKind === "pi"
          ? await stopOrphanedPiRuntimes(workspaceId)
          : await stopOrphanedOpenCodeRuntimes(workspaceId);
        if (stopped !== undefined) return stopped;
        throw workspaceNotFound();
      }
      const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
      if (agentKind === "pi") {
        if (!await stopWorkspacePiRuntimes(workspace.id)) throw runtimeUnavailable();
        const current = piBindingStates.get(workspace.id);
        piRuntimes.delete(workspace.id);
        piBindingStates.set(workspace.id, {
          health: "stopped",
          metadata: current?.metadata ?? { source: "managed" },
        });
        return piBinding(workspace);
      }
      if (!await stopWorkspaceOpenCodeRuntimes(workspace.id)) throw runtimeUnavailable();
      return openCodeBinding(workspace);
    }),
    "runtime.snapshot": async ({ workspaceId }) => {
      const result: RuntimeBinding[] = [];
      for (const id of selectedWorkspaceIds(workspaceId)) result.push(...await snapshotWorkspace(id));
      return result;
    },
    "session.snapshot": ({ workspaceId }) => runWorkspaceLifecycle(workspaceId, async () => {
      await sessionWorkspace(workspaceId);
      return [...await sessionCoordinator.snapshot(workspaceId)];
    }),
    "session.create": ({ workspaceId, agentKind }) => runWorkspaceLifecycle(workspaceId, async () => {
      await sessionWorkspace(workspaceId);
      return sessionCoordinator.create(workspaceId, agentKind);
    }),
    "session.select": ({ workspaceId, agentKind, sessionId }) => runWorkspaceLifecycle(workspaceId, async () => {
      await sessionWorkspace(workspaceId);
      return sessionCoordinator.select(workspaceId, agentKind, sessionId);
    }),
    "session.history": ({ workspaceId, agentKind, sessionId }) => runWorkspaceLifecycle(workspaceId, async () => {
      await sessionWorkspace(workspaceId);
      return sessionCoordinator.history(workspaceId, agentKind, sessionId);
    }),
    "session.send": ({ workspaceId, agentKind, sessionId, message, clientRequestId }) => runWorkspaceLifecycle(workspaceId, async () => {
      await sessionWorkspace(workspaceId);
      return sessionCoordinator.send(workspaceId, agentKind, sessionId, message, clientRequestId);
    }),
    "session.abort": ({ workspaceId, agentKind, sessionId }) => runWorkspaceLifecycle(workspaceId, async () => {
      await sessionWorkspace(workspaceId);
      return sessionCoordinator.abort(workspaceId, agentKind, sessionId);
    }),
    "command.list": ({ workspaceId, agentKind }) => runWorkspaceLifecycle(workspaceId, async () => {
      await sessionWorkspace(workspaceId);
      return [...await sessionCoordinator.commands(workspaceId, agentKind)];
    }),
    "config.preview": async () => { throw runtimeUnavailable(); },
    "config.commit": async () => { throw runtimeUnavailable(); },
    "config.rollback": async () => { throw runtimeUnavailable(); },
    "storage.health": async () => {
      const health = database.health();
      const diagnostics = database.diagnostics();
      return {
        mode: health.mode,
        schemaVersion: health.schemaVersion,
        diagnostics: diagnostics === null ? [] : [diagnostics.message],
      };
    },
  };
  const handlers: IpcServiceMap = {
    "workspace.pick": guardHandler(rawHandlers["workspace.pick"]),
    "workspace.open": guardHandler(rawHandlers["workspace.open"]),
    "workspace.snapshot": guardHandler(rawHandlers["workspace.snapshot"]),
    "workspace.trust": guardHandler(rawHandlers["workspace.trust"]),
    "runtime.probe": guardHandler(rawHandlers["runtime.probe"]),
    "runtime.start": guardHandler(rawHandlers["runtime.start"]),
    "runtime.stop": guardHandler(rawHandlers["runtime.stop"]),
    "runtime.snapshot": guardHandler(rawHandlers["runtime.snapshot"]),
    "session.snapshot": guardHandler(rawHandlers["session.snapshot"]),
    "session.create": guardHandler(rawHandlers["session.create"]),
    "session.select": guardHandler(rawHandlers["session.select"]),
    "session.history": guardHandler(rawHandlers["session.history"]),
    "session.send": guardHandler(rawHandlers["session.send"]),
    "session.abort": guardHandler(rawHandlers["session.abort"]),
    "command.list": guardHandler(rawHandlers["command.list"]),
    "config.preview": guardHandler(rawHandlers["config.preview"]),
    "config.commit": guardHandler(rawHandlers["config.commit"]),
    "config.rollback": guardHandler(rawHandlers["config.rollback"]),
    "storage.health": guardHandler(rawHandlers["storage.health"]),
  };
  const dispose = (): Promise<void> => {
    if (disposePromise !== undefined) return disposePromise;
    shutdownStarted = true;
    const current = (async () => {
      await Promise.all([...activeServiceOperations]);
      await Promise.all([...workspaceLifecycleOperations.values()]);
      const workspaceIds = new Set<string>([
        ...openCodeRuntimes.keys(),
        ...retainedOpenCodeRuntimes.keys(),
        ...piRuntimes.keys(),
        ...retainedPiRuntimes.keys(),
      ]);
      for (const workspaceId of workspaceIds) {
        const [openCodeStopped, piStopped] = await Promise.all([
          stopWorkspaceOpenCodeRuntimes(workspaceId),
          stopWorkspacePiRuntimes(workspaceId),
        ]);
        if (!openCodeStopped || !piStopped) throw runtimeUnavailable();
      }
      config.dispose();
      database.close();
    })();
    disposePromise = current;
    void current.then(
      () => undefined,
      () => { if (disposePromise === current) disposePromise = undefined; },
    );
    return current;
  };

  return {
    paths,
    handlers,
    subscribeSessionEvents: (listener) => sessionCoordinator.subscribe(listener),
    dispose,
  };
}

export type DesktopInput<K extends keyof IpcServiceMap> = InputOf<K>;
export type DesktopData<K extends keyof IpcServiceMap> = DataOf<K>;
