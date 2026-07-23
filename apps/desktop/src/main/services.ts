import {
  createOpenCodeRuntime,
} from "@halo-studio/agent-opencode";
import {
  createPiRuntime,
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
import { mkdir } from "node:fs/promises";
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

export interface CreateDesktopServicesOptions {
  readonly userDataPath: string;
  readonly picker: WorkspaceDialogPort;
  readonly safeStorage: SafeStoragePort;
  readonly hostEnvironment: Readonly<Record<string, string | undefined>>;
}

type PiRuntimeInstance = ReturnType<typeof createPiRuntime>;
type OpenCodeRuntimeInstance = ReturnType<typeof createOpenCodeRuntime>;
type Runtime = PiRuntimeInstance | OpenCodeRuntimeInstance;

interface RuntimeMetadata {
  readonly source: RuntimeBinding["source"];
  readonly executable?: string;
  readonly version?: string;
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

function runtimeHealth(kind: AgentKind, runtime: Runtime): RuntimeBinding["health"] {
  const state = runtime.state;
  if (kind === "pi") {
    if (state === "ready") return "ready";
    if (state === "starting") return "starting";
    if (state === "stopping") return "stopping";
    if (state === "stopped") return "stopped";
    if (state === "crashed") return "crashed";
    if (state === "detected") return "detected";
    return "unavailable";
  }
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
  const registry = new TargetRegistry();
  const config = new ConfigTransaction(registry, { vault });
  const selections = new Map<string, string>();
  const workspaces = new Map<string, Workspace>();
  const runtimes = new Map<string, Runtime>();
  const runtimeMetadata = new Map<string, RuntimeMetadata>();

  const getWorkspace = (workspaceId: string): Workspace => {
    const workspace = workspaces.get(workspaceId);
    if (workspace === undefined) throw workspaceNotFound();
    return workspace;
  };
  const runtimeKey = (workspaceId: string, kind: AgentKind): string => `${workspaceId}:${kind}`;
  const getRuntime = (workspace: Workspace, kind: AgentKind): Runtime => {
    const key = runtimeKey(workspace.id, kind);
    const existing = runtimes.get(key);
    if (existing !== undefined) return existing;
    const runtime = kind === "pi"
      ? createPiRuntime({
          cwd: workspace.realPath,
          session: `halo-${workspace.id}`,
          model: "unconfigured",
          thinking: "default",
          trust: workspace.trustState,
          hostEnvironment: safeHostEnvironment(options.hostEnvironment, piRuntimeDirectory),
        })
      : createOpenCodeRuntime({
          cwd: workspace.realPath,
          trust: workspace.trustState,
          hostEnvironment: safeHostEnvironment(options.hostEnvironment, openCodeRuntimeDirectory),
        });
    runtimes.set(key, runtime);
    return runtime;
  };
  const binding = (workspace: Workspace, kind: AgentKind): RuntimeBinding => {
    const key = runtimeKey(workspace.id, kind);
    const runtime = getRuntime(workspace, kind);
    return createRuntimeBinding(kind, runtimeHealth(kind, runtime), runtimeMetadata.get(key));
  };

  const handlers: IpcServiceMap = {
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
      const workspace = await openWorkspace(path, trustStore);
      workspaces.set(workspace.id, workspace);
      return workspace;
    },
    "workspace.snapshot": async () => [...workspaces.values()].map((workspace) => ({ ...workspace })),
    "workspace.trust": async ({ workspaceId, trustState }) => {
      const workspace = getWorkspace(workspaceId);
      for (const kind of ["pi", "opencode"] as const) {
        const key = runtimeKey(workspace.id, kind);
        const runtime = runtimes.get(key);
        if (runtime !== undefined) await runtime.stop();
        runtimes.delete(key);
        runtimeMetadata.delete(key);
      }
      await trustStore.setDecision(workspace.realPath, trustState as TrustState);
      const updated = { ...workspace, trustState };
      workspaces.set(workspaceId, updated);
      return updated;
    },
    "runtime.probe": async ({ workspaceId }) => {
      const selected = workspaceId === undefined ? [...workspaces.values()] : [getWorkspace(workspaceId)];
      const result: RuntimeBinding[] = [];
      for (const workspace of selected) {
        for (const kind of ["pi", "opencode"] as const) {
          const runtime = getRuntime(workspace, kind);
          try {
            if (kind === "pi") {
              const detection = await (runtime as PiRuntimeInstance).detect();
              runtimeMetadata.set(runtimeKey(workspace.id, kind), {
                source: detection.source,
                ...(detection.executable === undefined ? {} : { executable: detection.executable }),
                ...(detection.version === undefined ? {} : { version: detection.version }),
              });
            } else {
              const artifact = await (runtime as OpenCodeRuntimeInstance).detect();
              runtimeMetadata.set(runtimeKey(workspace.id, kind), {
                source: "bundled",
                executable: artifact.executable,
                version: artifact.version,
              });
            }
          } catch {
            // Probe reports an unavailable binding; raw runtime failures never cross IPC.
          }
          result.push(binding(workspace, kind));
        }
      }
      return result;
    },
    "runtime.start": async ({ workspaceId, agentKind }) => {
      const workspace = getWorkspace(workspaceId);
      if (workspace.trustState !== "trusted") throw untrusted();
      const runtime = getRuntime(workspace, agentKind);
      if (agentKind === "pi") throw runtimeUnavailable();
      await runtime.start();
      return binding(workspace, agentKind);
    },
    "runtime.stop": async ({ workspaceId, agentKind }) => {
      const workspace = getWorkspace(workspaceId);
      const runtime = getRuntime(workspace, agentKind);
      await runtime.stop();
      return binding(workspace, agentKind);
    },
    "runtime.snapshot": async ({ workspaceId }) => {
      const selected = workspaceId === undefined ? [...workspaces.values()] : [getWorkspace(workspaceId)];
      return selected.flatMap((workspace) => [binding(workspace, "pi"), binding(workspace, "opencode")]);
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

  return {
    paths,
    handlers,
    async dispose() {
      for (const runtime of runtimes.values()) await runtime.stop().catch(() => undefined);
      config.dispose();
      database.close();
    },
  };
}

export type DesktopInput<K extends keyof IpcServiceMap> = InputOf<K>;
export type DesktopData<K extends keyof IpcServiceMap> = DataOf<K>;
