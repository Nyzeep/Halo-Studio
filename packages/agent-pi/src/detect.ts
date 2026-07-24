import type { ProcessExit, ProcessPort } from "./jsonlTransport.js";
import { spawn } from "node:child_process";
import { readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";
import { PI_VERSION, type PiDetection, type PiLaunchTarget } from "./schemas.js";
import { RuntimeUnavailableError } from "./errors.js";
import { buildRuntimeEnvironment, isPathWithin, type PathPlatform } from "@halo-studio/core";

export interface ProcessFactoryOptions {
  readonly cwd?: string;
  readonly env?: Readonly<Record<string, string>>;
}

export type ProcessFactory = (executable: string, args: readonly string[], options: ProcessFactoryOptions) => ProcessPort;

/**
 * A version probe could not prove that its child has exited. Callers must keep
 * the associated runtime managed and use `retryCleanup` before attempting a
 * replacement probe or RPC launch.
 */
export class PiProbeCleanupError extends RuntimeUnavailableError {
  readonly #retry: () => Promise<boolean>;

  constructor(retry: () => Promise<boolean>) {
    super();
    this.name = "PiProbeCleanupError";
    this.#retry = retry;
  }

  retryCleanup(): Promise<boolean> {
    return this.#retry();
  }
}

/** Production adapter; tests should inject a deterministic ProcessPort instead. */
export function nodeProcessFactory(executable: string, args: readonly string[], options: ProcessFactoryOptions): ProcessPort {
  const child = spawn(executable, [...args], {
    ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
    ...(options.env === undefined ? {} : { env: options.env }),
    stdio: ["pipe", "pipe", "pipe"],
    shell: false,
    windowsHide: true,
  });
  const wait = new Promise<ProcessExit>((resolve) => {
    // `close` waits for stdio handles as well as process exit.  Runtime stop
    // must not release a workspace cache while a child still has that cwd open.
    child.once("close", (code, signal) => resolve({ code, signal }));
    child.once("error", () => resolve({ code: null, signal: null }));
  });
  if (!child.stdin || !child.stdout || !child.stderr) throw new Error("Pi process stdio unavailable");
  return {
    stdin: {
      write: (data, callback) => {
        child.stdin!.write(data, (error) => callback?.(error ?? null));
      },
      end: (callback) => { child.stdin!.end(callback); },
    },
    stdout: child.stdout,
    stderr: child.stderr,
    process: child,
    wait: () => wait,
    kill: (signal) => { child.kill(signal as NodeJS.Signals | undefined); },
  };
}

export interface DetectOptions {
  readonly processFactory?: ProcessFactory;
  readonly cwd?: string;
  readonly hostEnvironment?: Readonly<Record<string, string | undefined>>;
  readonly timeoutMs?: number;
  /** Test seam; production resolves canonical candidates from the host PATH. */
  readonly resolveExecutables?: PiExecutableResolver;
  /** Test seam; production reads from the host filesystem. */
  readonly filesystem?: PiExecutableFilesystem;
  /** Test seam for Windows npm-shim resolution. */
  readonly platform?: PathPlatform;
}

interface ProbeOptions extends DetectOptions {
  readonly env: Readonly<Record<string, string>>;
}

export interface PiExecutableFilesystem {
  stat(path: string): Promise<{ readonly isFile: () => boolean }>;
  realpath(path: string): Promise<string>;
  readFile(path: string): Promise<string>;
}

export interface PiExecutableResolutionOptions {
  readonly cwd?: string;
  readonly environment: Readonly<Record<string, string>>;
  readonly filesystem?: PiExecutableFilesystem;
  readonly platform?: PathPlatform;
}

export type PiExecutableResolver = (
  executableName: PiHostExecutableName,
  options: PiExecutableResolutionOptions,
) => Promise<readonly string[]>;

export type PiHostExecutableName = "pi" | "pi.exe" | "pi.cmd" | "node.exe";

type BoundedResult<T> =
  | { readonly status: "fulfilled"; readonly value: T }
  | { readonly status: "rejected" }
  | { readonly status: "timeout" };

type ProcessEventSource = ProcessPort | NonNullable<ProcessPort["process"]>;

interface ExitWait {
  readonly wait: Promise<ProcessExit> | undefined;
  release(): void;
}

const nodeExecutableFilesystem: PiExecutableFilesystem = {
  stat,
  realpath,
  readFile: (file) => readFile(file, "utf8"),
};

function pathForPlatform(platform: PathPlatform): typeof path.win32 {
  return platform === "win32" ? path.win32 : path.posix;
}

function unquotePathEntry(entry: string): string {
  const trimmed = entry.trim();
  return trimmed.length >= 2 && trimmed.startsWith("\"") && trimmed.endsWith("\"")
    ? trimmed.slice(1, -1)
    : trimmed;
}

/**
 * Resolves only canonical executable files from the Main-owned PATH. Relative
 * PATH entries are rejected so a workspace can never affect command lookup.
 */
export async function resolvePiExecutables(
  executableName: PiHostExecutableName,
  options: PiExecutableResolutionOptions,
): Promise<readonly string[]> {
  const platform = options.platform ?? process.platform;
  const api = pathForPlatform(platform);
  const pathValue = options.environment.PATH;
  if (pathValue === undefined || pathValue.length === 0) return [];

  const filesystem = options.filesystem ?? nodeExecutableFilesystem;
  let workspaceInput: string | undefined;
  let workspaceReal: string | undefined;
  if (options.cwd !== undefined) {
    if (!api.isAbsolute(options.cwd)) return [];
    workspaceInput = api.resolve(options.cwd);
    try {
      workspaceReal = await filesystem.realpath(options.cwd);
    } catch {
      return [];
    }
    if (!api.isAbsolute(workspaceReal)) return [];
  }

  const resolved: string[] = [];
  const seen = new Set<string>();
  for (const rawDirectory of pathValue.split(api.delimiter)) {
    const directory = unquotePathEntry(rawDirectory);
    if (!api.isAbsolute(directory)) continue;
    const candidate = api.resolve(directory, executableName);
    if (workspaceInput !== undefined && isPathWithin(candidate, workspaceInput, platform)) continue;
    try {
      const details = await filesystem.stat(candidate);
      if (!details.isFile()) continue;
      const executable = await filesystem.realpath(candidate);
      if (!api.isAbsolute(executable)) continue;
      if (
        (workspaceInput !== undefined && isPathWithin(executable, workspaceInput, platform))
        || (workspaceReal !== undefined && (
          isPathWithin(candidate, workspaceReal, platform)
          || isPathWithin(executable, workspaceReal, platform)
        ))
        || seen.has(executable)
      ) continue;
      seen.add(executable);
      resolved.push(executable);
    } catch {
      // A PATH entry is merely a candidate. Continue with the next host-owned
      // directory without ever falling back to the workspace cwd.
    }
  }
  return resolved;
}

const PI_NPM_PACKAGE_NAME = "@earendil-works/pi-coding-agent";
const PI_NPM_BIN_ENTRY = "dist/cli.js";
const MINIMUM_PI_NODE_VERSION = [22, 19, 0] as const;

interface WorkspaceBoundary {
  readonly input?: string;
  readonly real?: string;
}

interface PiNpmManifest {
  readonly name: typeof PI_NPM_PACKAGE_NAME;
  readonly version: typeof PI_VERSION;
  readonly bin: { readonly pi: typeof PI_NPM_BIN_ENTRY };
}

function isOutsideWorkspace(
  candidate: string,
  workspace: WorkspaceBoundary,
  platform: PathPlatform,
): boolean {
  return (
    (workspace.input === undefined || !isPathWithin(candidate, workspace.input, platform))
    && (workspace.real === undefined || !isPathWithin(candidate, workspace.real, platform))
  );
}

async function resolveWorkspaceBoundary(
  options: PiExecutableResolutionOptions,
  api: typeof path.win32,
  filesystem: PiExecutableFilesystem,
): Promise<WorkspaceBoundary | undefined> {
  if (options.cwd === undefined) return {};
  if (!api.isAbsolute(options.cwd)) return undefined;
  try {
    const real = await filesystem.realpath(options.cwd);
    if (!api.isAbsolute(real)) return undefined;
    return { input: api.resolve(options.cwd), real };
  } catch {
    return undefined;
  }
}

async function canonicalPathOutsideWorkspace(
  candidate: string,
  api: typeof path.win32,
  filesystem: PiExecutableFilesystem,
  workspace: WorkspaceBoundary,
  platform: PathPlatform,
): Promise<string | undefined> {
  if (!api.isAbsolute(candidate) || !isOutsideWorkspace(candidate, workspace, platform)) return undefined;
  try {
    const canonical = await filesystem.realpath(candidate);
    if (!api.isAbsolute(canonical) || !isOutsideWorkspace(canonical, workspace, platform)) return undefined;
    return canonical;
  } catch {
    return undefined;
  }
}

async function canonicalFileOutsideWorkspace(
  candidate: string,
  api: typeof path.win32,
  filesystem: PiExecutableFilesystem,
  workspace: WorkspaceBoundary,
  platform: PathPlatform,
): Promise<string | undefined> {
  if (!api.isAbsolute(candidate) || !isOutsideWorkspace(candidate, workspace, platform)) return undefined;
  try {
    const details = await filesystem.stat(candidate);
    if (!details.isFile()) return undefined;
  } catch {
    return undefined;
  }
  return canonicalPathOutsideWorkspace(candidate, api, filesystem, workspace, platform);
}

function parsePiNpmManifest(value: string): PiNpmManifest | undefined {
  try {
    const parsed: unknown = JSON.parse(value);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return undefined;
    const record = parsed as Record<string, unknown>;
    if (record.name !== PI_NPM_PACKAGE_NAME || record.version !== PI_VERSION) return undefined;
    if (typeof record.bin !== "object" || record.bin === null || Array.isArray(record.bin)) return undefined;
    const bin = record.bin as Record<string, unknown>;
    if (bin.pi !== PI_NPM_BIN_ENTRY) return undefined;
    return {
      name: PI_NPM_PACKAGE_NAME,
      version: PI_VERSION,
      bin: { pi: PI_NPM_BIN_ENTRY },
    };
  } catch {
    return undefined;
  }
}

function isCompatibleNodeVersion(output: string): boolean {
  const match = /^v(\d+)\.(\d+)\.(\d+)\s*$/.exec(output.trim());
  if (!match) return false;
  const actual = match.slice(1).map(Number);
  return actual[0]! > MINIMUM_PI_NODE_VERSION[0]
    || (actual[0] === MINIMUM_PI_NODE_VERSION[0] && (
      actual[1]! > MINIMUM_PI_NODE_VERSION[1]
      || (actual[1] === MINIMUM_PI_NODE_VERSION[1] && actual[2]! >= MINIMUM_PI_NODE_VERSION[2])
    ));
}

async function resolveNpmShimLaunch(
  shimCandidate: string,
  resolutionOptions: PiExecutableResolutionOptions,
  probeOptions: ProbeOptions,
  resolveExecutables: PiExecutableResolver,
): Promise<PiLaunchTarget | undefined> {
  const platform = resolutionOptions.platform ?? process.platform;
  if (platform !== "win32") return undefined;
  const api = pathForPlatform(platform);
  const filesystem = resolutionOptions.filesystem ?? nodeExecutableFilesystem;
  const workspace = await resolveWorkspaceBoundary(resolutionOptions, api, filesystem);
  if (workspace === undefined) return undefined;

  const shim = await canonicalFileOutsideWorkspace(
    shimCandidate,
    api,
    filesystem,
    workspace,
    platform,
  );
  if (shim === undefined || api.extname(shim).toLowerCase() !== ".cmd") return undefined;

  const npmRoot = await canonicalPathOutsideWorkspace(
    api.join(api.dirname(shim), "node_modules"),
    api,
    filesystem,
    workspace,
    platform,
  );
  if (npmRoot === undefined) return undefined;

  const packageRoot = await canonicalPathOutsideWorkspace(
    api.join(npmRoot, "@earendil-works", "pi-coding-agent"),
    api,
    filesystem,
    workspace,
    platform,
  );
  if (packageRoot === undefined || !isPathWithin(packageRoot, npmRoot, platform)) return undefined;

  const packageJson = await canonicalFileOutsideWorkspace(
    api.join(packageRoot, "package.json"),
    api,
    filesystem,
    workspace,
    platform,
  );
  if (
    packageJson === undefined
    || !isPathWithin(packageJson, npmRoot, platform)
    || !isPathWithin(packageJson, packageRoot, platform)
  ) return undefined;

  let manifest: PiNpmManifest | undefined;
  try {
    manifest = parsePiNpmManifest(await filesystem.readFile(packageJson));
  } catch {
    return undefined;
  }
  if (manifest === undefined) return undefined;

  const entrypoint = await canonicalFileOutsideWorkspace(
    api.join(packageRoot, "dist", "cli.js"),
    api,
    filesystem,
    workspace,
    platform,
  );
  if (
    entrypoint === undefined
    || !isPathWithin(entrypoint, npmRoot, platform)
    || !isPathWithin(entrypoint, packageRoot, platform)
  ) return undefined;

  const nodeCandidates: string[] = [];
  const siblingNode = await canonicalFileOutsideWorkspace(
    api.join(api.dirname(shim), "node.exe"),
    api,
    filesystem,
    workspace,
    platform,
  );
  if (siblingNode !== undefined) nodeCandidates.push(siblingNode);
  try {
    const hostNodes = await resolveExecutables("node.exe", resolutionOptions);
    for (const nodeCandidate of hostNodes) {
      const node = await canonicalFileOutsideWorkspace(
        nodeCandidate,
        api,
        filesystem,
        workspace,
        platform,
      );
      if (node !== undefined && !nodeCandidates.includes(node)) nodeCandidates.push(node);
    }
  } catch {
    // A sibling node.exe is sufficient; a broken PATH must not make it unsafe.
  }

  for (const node of nodeCandidates) {
    if (api.basename(node).toLowerCase() !== "node.exe") continue;
    const output = await probeOutput(node, ["--version"], probeOptions);
    if (!output || !isCompatibleNodeVersion(output)) continue;
    return { executable: node, argvPrefix: [entrypoint], displayPath: shim };
  }
  return undefined;
}

function parseVersion(output: string): string | undefined {
  const normalized = output.trim();
  if (normalized === PI_VERSION || normalized === `pi ${PI_VERSION}`) return PI_VERSION;
  return undefined;
}

async function settleWithin<T>(
  operation: () => T | PromiseLike<T>,
  timeoutMs: number,
): Promise<BoundedResult<T>> {
  const observed = Promise.resolve().then(operation).then(
    (value) => ({ status: "fulfilled", value } as const),
    () => ({ status: "rejected" } as const),
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

function processEventSource(port: ProcessPort): ProcessEventSource | undefined {
  if (port.process !== undefined) return port.process;
  return port.on === undefined ? undefined : port;
}

function createExitWait(port: ProcessPort): ExitWait {
  if (port.wait !== undefined) {
    try {
      return { wait: port.wait(), release: () => undefined };
    } catch {
      // Fall through to an explicit close listener when a test adapter throws
      // synchronously while constructing its wait promise.
    }
  }
  const source = processEventSource(port);
  if (source === undefined) return { wait: undefined, release: () => undefined };

  let reportedExit: ProcessExit = { code: null, signal: null };
  let released = false;
  let resolveClose!: (exit: ProcessExit) => void;
  const onExit = (code: unknown, signal: unknown): void => {
    reportedExit = {
      code: typeof code === "number" ? code : null,
      signal: typeof signal === "string" ? signal : null,
    };
  };
  const onClose = (): void => {
    release();
    resolveClose(reportedExit);
  };
  const release = (): void => {
    if (released) return;
    released = true;
    source.off?.("exit", onExit);
    source.off?.("close", onClose);
    source.removeListener?.("exit", onExit);
    source.removeListener?.("close", onClose);
  };
  const wait = new Promise<ProcessExit>((resolve) => {
    resolveClose = resolve;
    source.on?.("exit", onExit);
    source.on?.("close", onClose);
  });
  return { wait, release };
}

async function endProbeStdin(port: ProcessPort, timeoutMs: number): Promise<void> {
  await settleWithin(() => port.stdin.end(), timeoutMs);
}

async function cleanupProbePort(
  port: ProcessPort,
  exitWait: Promise<ProcessExit> | undefined,
  timeoutMs: number,
  forceKill: boolean,
): Promise<boolean> {
  await endProbeStdin(port, timeoutMs);
  if (exitWait === undefined) {
    await settleWithin(() => port.kill?.("SIGTERM"), timeoutMs);
    return false;
  }

  const initial = forceKill
    ? ({ status: "timeout" } as const)
    : await settleWithin(() => exitWait, timeoutMs);
  if (initial.status === "fulfilled") return true;

  const killed = await settleWithin(() => port.kill?.("SIGTERM"), timeoutMs);
  if (port.kill === undefined || killed.status !== "fulfilled" || killed.value === false) return false;

  const afterKill = await settleWithin(() => exitWait, timeoutMs);
  return afterKill.status === "fulfilled";
}

async function probeOutput(
  executable: string,
  args: readonly string[],
  options: ProbeOptions,
): Promise<string | undefined> {
  let port: ProcessPort;
  try {
    const factoryOptions: ProcessFactoryOptions = {
      ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
      ...(options.env === undefined ? {} : { env: options.env }),
    };
    port = (options.processFactory ?? nodeProcessFactory)(executable, args, factoryOptions);
  } catch { return undefined; }
  const timeoutMs = options.timeoutMs ?? 10_000;
  const exit = createExitWait(port);
  let retainExitWait = false;
  let output = "";
  const subscriptions: Array<{ target: ProcessPort["stdout"] | ProcessPort | NonNullable<ProcessPort["process"]>; event: string; listener: (...args: any[]) => void }> = [];
  const subscribe = (target: ProcessPort["stdout"] | ProcessPort | NonNullable<ProcessPort["process"]>, event: string, listener: (...args: any[]) => void): void => {
    target.on?.(event as never, listener as never);
    subscriptions.push({ target, event, listener });
  };
  const unsubscribe = (): void => {
    for (const subscription of subscriptions.splice(0)) {
      subscription.target.off?.(subscription.event as never, subscription.listener as never);
      subscription.target.removeListener?.(subscription.event as never, subscription.listener as never);
    }
  };
  subscribe(port.stdout, "data", (chunk: Buffer | string) => { output += typeof chunk === "string" ? chunk : chunk.toString("utf8"); });
  if (port.stderr) subscribe(port.stderr, "data", () => undefined);
  const streamClosed = (stream: ProcessPort["stdout"] | undefined): Promise<boolean> => {
    if (!stream) return Promise.resolve(false);
    return new Promise<boolean>((resolve) => {
      let settled = false;
      const done = (errored = false) => { if (!settled) { settled = true; resolve(errored); } };
      subscribe(stream, "end", done);
      subscribe(stream, "close", done);
      subscribe(stream, "error", () => done(true));
    });
  };
  const retryCleanup = (): Promise<boolean> => cleanupProbePort(port, exit.wait, timeoutMs, false);
  const failAfterCleanup = async (forceKill: boolean): Promise<undefined> => {
    if (await cleanupProbePort(port, exit.wait, timeoutMs, forceKill)) return undefined;
    retainExitWait = true;
    throw new PiProbeCleanupError(retryCleanup);
  };
  try {
    if (exit.wait === undefined) return await failAfterCleanup(true);
    const observation = Promise.all([streamClosed(port.stdout), streamClosed(port.stderr), exit.wait]);
    const result = await settleWithin(() => observation, timeoutMs);
    if (result.status !== "fulfilled") return await failAfterCleanup(true);
    const [stdoutErrored, stderrErrored, processExit] = result.value;
    if (stdoutErrored || stderrErrored || processExit.code !== 0) return undefined;
  } catch {
    if (retainExitWait) throw new PiProbeCleanupError(retryCleanup);
    return await failAfterCleanup(true);
  } finally {
    unsubscribe();
    if (!retainExitWait) exit.release();
  }
  return output;
}

async function probePi(
  launch: PiLaunchTarget,
  options: ProbeOptions,
): Promise<PiDetection | undefined> {
  const output = await probeOutput(launch.executable, [...launch.argvPrefix, "--version"], options);
  const version = output === undefined ? undefined : parseVersion(output);
  if (version !== PI_VERSION) return undefined;
  return {
    status: "detected",
    source: "system",
    executable: launch.executable,
    launch,
    version,
  };
}

function isNativePiCandidate(
  executable: string,
  executableName: "pi" | "pi.exe",
  api: typeof path.win32,
): boolean {
  if (!api.isAbsolute(executable)) return false;
  return api.basename(executable).toLowerCase() === executableName;
}

export const detectPiRuntime = detectPi;

export async function detectPi(options: DetectOptions = {}): Promise<PiDetection> {
  if (options.hostEnvironment === undefined) throw new RuntimeUnavailableError();
  let env: Record<string, string>;
  try {
    // Detection is deliberately credential-blind. A version probe is not a
    // confirmed RPC launch and must never receive provider values.
    env = buildRuntimeEnvironment(options.hostEnvironment);
  } catch { throw new RuntimeUnavailableError(); }
  const probeOptions: ProbeOptions = {
    ...(options.processFactory === undefined ? {} : { processFactory: options.processFactory }),
    ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
    ...(options.timeoutMs === undefined ? {} : { timeoutMs: options.timeoutMs }),
    ...(options.resolveExecutables === undefined ? {} : { resolveExecutables: options.resolveExecutables }),
    ...(options.filesystem === undefined ? {} : { filesystem: options.filesystem }),
    ...(options.platform === undefined ? {} : { platform: options.platform }),
    hostEnvironment: options.hostEnvironment,
    env,
  };
  const resolveExecutables = options.resolveExecutables ?? resolvePiExecutables;
  const platform = options.platform ?? process.platform;
  const api = pathForPlatform(platform);
  const resolutionOptions: PiExecutableResolutionOptions = {
    ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
    environment: env,
    ...(options.filesystem === undefined ? {} : { filesystem: options.filesystem }),
    ...(options.platform === undefined ? {} : { platform: options.platform }),
  };
  for (const executableName of ["pi", "pi.exe"] as const) {
    let executables: readonly string[];
    try {
      executables = await resolveExecutables(executableName, resolutionOptions);
    } catch {
      continue;
    }
    for (const executable of executables) {
      if (!isNativePiCandidate(executable, executableName, api)) continue;
      const found = await probePi({ executable, argvPrefix: [] }, probeOptions);
      if (found) return found;
    }
  }
  if (platform === "win32") {
    let shims: readonly string[];
    try {
      shims = await resolveExecutables("pi.cmd", resolutionOptions);
    } catch {
      shims = [];
    }
    for (const shim of shims) {
      const launch = await resolveNpmShimLaunch(shim, resolutionOptions, probeOptions, resolveExecutables);
      if (launch === undefined) continue;
      const found = await probePi(launch, probeOptions);
      if (found) return found;
    }
  }
  return { status: "unavailable", source: "managed", managedInstall: "available" };
}
