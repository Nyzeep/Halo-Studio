import {
  createOpenCodeRuntime,
} from "@halo-studio/agent-opencode";
import {
  detectPi,
} from "@halo-studio/agent-pi";
import {
  type AgentKind,
  type DataOf,
  type RuntimeBinding,
  type TrustState,
  type Workspace,
  type InputOf,
} from "@halo-studio/contracts";
import {
  ConfigTransaction,
  TargetRegistry,
} from "@halo-studio/config";
import {
  CoreError,
  MemoryTrustStore,
  openWorkspace,
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
  dispose(): Promise<void>;
}

interface WorkspaceIdentity {
  readonly device: bigint;
  readonly inode: bigint;
}

type WorkspaceIdentityPort = (realPath: string) => Promise<WorkspaceIdentity>;

export interface CreateDesktopServicesOptions {
  readonly userDataPath: string;
  readonly picker: WorkspaceDialogPort;
  readonly safeStorage: SafeStoragePort;
  readonly hostEnvironment: Readonly<Record<string, string | undefined>>;
  /** Main-process test seam; production uses the core workspace opener. */
  readonly openWorkspace?: typeof openWorkspace;
  /** Main-process test seam; production uses the bundled OpenCode runtime factory. */
  readonly createOpenCodeRuntime?: OpenCodeRuntimeFactory;
  /** Main-process test seam; production uses filesystem device and inode identity. */
  readonly workspaceIdentity?: WorkspaceIdentityPort;
}

type OpenCodeRuntimeInstance = Pick<ReturnType<typeof createOpenCodeRuntime>, "state" | "detect" | "start" | "stop">;
type OpenCodeRuntimeFactory = (options: Parameters<typeof createOpenCodeRuntime>[0]) => OpenCodeRuntimeInstance;

interface RuntimeMetadata {
  readonly source: RuntimeBinding["source"];
  readonly executable?: string;
  readonly version?: string;
}

interface PiBindingState {
  readonly health: RuntimeBinding["health"];
  readonly metadata: RuntimeMetadata;
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
    capabilities: unavailableCapabilities,
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
  const workspaceIdentityReader = options.workspaceIdentity ?? readWorkspaceIdentity;
  const registry = new TargetRegistry();
  const config = new ConfigTransaction(registry, { vault });
  const selections = new Map<string, string>();
  const workspaces = new Map<string, Workspace>();
  const workspaceIdentities = new Map<string, WorkspaceIdentity>();
  const openCodeRuntimes = new Map<string, OpenCodeRuntimeInstance>();
  const retainedOpenCodeRuntimes = new Map<string, Set<OpenCodeRuntimeInstance>>();
  const openCodeMetadata = new Map<string, RuntimeMetadata>();
  const piBindingStates = new Map<string, PiBindingState>();
  const workspaceLifecycleOperations = new Map<string, Promise<void>>();
  const activeServiceOperations = new Set<Promise<void>>();
  let shutdownStarted = false;
  let disposePromise: Promise<void> | undefined;

  const getWorkspace = (workspaceId: string): Workspace => {
    const workspace = workspaces.get(workspaceId);
    if (workspace === undefined) throw workspaceNotFound();
    return workspace;
  };
  const getOpenCodeRuntime = (workspace: Workspace): OpenCodeRuntimeInstance => {
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
  const openCodeBinding = (workspace: Workspace, runtime = openCodeRuntimes.get(workspace.id)): RuntimeBinding => {
    return createRuntimeBinding("opencode", runtime === undefined ? "unavailable" : openCodeRuntimeHealth(runtime), openCodeMetadata.get(workspace.id));
  };
  const piBinding = (workspace: Workspace): RuntimeBinding => {
    const state = piBindingStates.get(workspace.id);
    return createRuntimeBinding("pi", state?.health ?? "unavailable", state?.metadata);
  };
  const discardWorkspaceRuntimeState = async (workspaceId: string): Promise<boolean> => {
    const runtimes = new Set<OpenCodeRuntimeInstance>([
      ...(openCodeRuntimes.get(workspaceId) === undefined ? [] : [openCodeRuntimes.get(workspaceId)!]),
      ...(retainedOpenCodeRuntimes.get(workspaceId) ?? []),
    ]);
    const retained = new Set<OpenCodeRuntimeInstance>();
    let stopped = true;
    for (const runtime of runtimes) {
      try { await runtime.stop(); }
      catch {
        stopped = false;
        retained.add(runtime);
      }
    }
    openCodeRuntimes.delete(workspaceId);
    if (retained.size === 0) retainedOpenCodeRuntimes.delete(workspaceId);
    else retainedOpenCodeRuntimes.set(workspaceId, retained);
    openCodeMetadata.delete(workspaceId);
    piBindingStates.delete(workspaceId);
    return stopped;
  };
  const invalidateWorkspace = async (workspaceId: string): Promise<boolean> => {
    const stopped = await discardWorkspaceRuntimeState(workspaceId);
    workspaces.delete(workspaceId);
    workspaceIdentities.delete(workspaceId);
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
    ? [...workspaces.keys()]
    : [workspaceId];
  const probeWorkspace = (workspaceId: string): Promise<RuntimeBinding[]> => runWorkspaceLifecycle(workspaceId, async () => {
    const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
    try {
      const detection = await detectPi({
        cwd: workspace.realPath,
        hostEnvironment: safeHostEnvironment(options.hostEnvironment, piRuntimeDirectory),
      });
      piBindingStates.set(workspace.id, detection.status === "detected"
        ? {
            health: "detected",
            metadata: {
              source: detection.source,
              ...(detection.executable === undefined ? {} : { executable: detection.executable }),
              ...(detection.version === undefined ? {} : { version: detection.version }),
            },
          }
        : { health: "unavailable", metadata: { source: "managed" } });
    } catch {
      piBindingStates.set(workspace.id, { health: "unavailable", metadata: { source: "managed" } });
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
      return runWorkspaceLifecycle(workspace.id, async () => {
        if (retainedOpenCodeRuntimes.has(workspace.id)) throw runtimeUnavailable();
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
        workspaces.set(workspace.id, workspace);
        workspaceIdentities.set(workspace.id, identity);
        return workspace;
      });
    },
    "workspace.snapshot": async () => [...workspaces.values()].map((workspace) => ({ ...workspace })),
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
      if (retainedOpenCodeRuntimes.has(workspace.id)) throw runtimeUnavailable();
      if (workspace.trustState !== "trusted") throw untrusted();
      if (agentKind === "pi") throw runtimeUnavailable();
      const runtime = getOpenCodeRuntime(workspace);
      await runtime.start();
      return openCodeBinding(workspace, runtime);
    }),
    "runtime.stop": ({ workspaceId, agentKind }) => runWorkspaceLifecycle(workspaceId, async () => {
      const workspace = await revalidateWorkspace(getWorkspace(workspaceId));
      if (agentKind === "pi") {
        const current = piBindingStates.get(workspace.id);
        piBindingStates.set(workspace.id, {
          health: "stopped",
          metadata: current?.metadata ?? { source: "managed" },
        });
        return piBinding(workspace);
      }
      const runtime = openCodeRuntimes.get(workspace.id);
      if (runtime !== undefined) await runtime.stop();
      return openCodeBinding(workspace, runtime);
    }),
    "runtime.snapshot": async ({ workspaceId }) => {
      const result: RuntimeBinding[] = [];
      for (const id of selectedWorkspaceIds(workspaceId)) result.push(...await snapshotWorkspace(id));
      return result;
    },
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
    "config.preview": guardHandler(rawHandlers["config.preview"]),
    "config.commit": guardHandler(rawHandlers["config.commit"]),
    "config.rollback": guardHandler(rawHandlers["config.rollback"]),
    "storage.health": guardHandler(rawHandlers["storage.health"]),
  };
  const dispose = (): Promise<void> => {
    if (disposePromise !== undefined) return disposePromise;
    shutdownStarted = true;
    disposePromise = (async () => {
      await Promise.all([...activeServiceOperations]);
      await Promise.all([...workspaceLifecycleOperations.values()]);
      const runtimes = new Set<OpenCodeRuntimeInstance>([
        ...openCodeRuntimes.values(),
        ...[...retainedOpenCodeRuntimes.values()].flatMap((retained) => [...retained]),
      ]);
      for (const runtime of runtimes) await runtime.stop().catch(() => undefined);
      config.dispose();
      database.close();
    })();
    return disposePromise;
  };

  return {
    paths,
    handlers,
    dispose,
  };
}

export type DesktopInput<K extends keyof IpcServiceMap> = InputOf<K>;
export type DesktopData<K extends keyof IpcServiceMap> = DataOf<K>;
