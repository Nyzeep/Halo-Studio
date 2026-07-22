import type { ProcessExit, ProcessPort } from "./jsonlTransport.js";
import { spawn } from "node:child_process";
import { PI_VERSION, type PiDetection } from "./schemas.js";
import { RuntimeUnavailableError } from "./errors.js";
import { buildRuntimeEnvironment } from "@halo-studio/core";

export interface ProcessFactoryOptions {
  readonly cwd?: string;
  readonly env?: Readonly<Record<string, string>>;
}

export type ProcessFactory = (executable: string, args: readonly string[], options: ProcessFactoryOptions) => ProcessPort;

/** Production adapter; tests should inject a deterministic ProcessPort instead. */
export function nodeProcessFactory(executable: string, args: readonly string[], options: ProcessFactoryOptions): ProcessPort {
  const child = spawn(executable, [...args], {
    ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
    ...(options.env === undefined ? {} : { env: options.env }),
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true,
  });
  const wait = new Promise<ProcessExit>((resolve) => {
    child.once("exit", (code, signal) => resolve({ code, signal }));
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
  readonly providerEnvironment?: Readonly<Record<string, string>>;
  readonly allowedProviderKeys?: ReadonlySet<string>;
  readonly timeoutMs?: number;
}

interface ProbeOptions extends DetectOptions {
  readonly env: Readonly<Record<string, string>>;
}

function parseVersion(output: string): string | undefined {
  const normalized = output.trim();
  if (normalized === PI_VERSION || normalized === `pi ${PI_VERSION}`) return PI_VERSION;
  return undefined;
}

async function probe(executable: string, options: ProbeOptions): Promise<PiDetection | undefined> {
  let port: ProcessPort;
  try {
    const factoryOptions: ProcessFactoryOptions = {
      ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
      ...(options.env === undefined ? {} : { env: options.env }),
    };
    port = (options.processFactory ?? nodeProcessFactory)(executable, ["--version"], factoryOptions);
  } catch { return undefined; }
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
  const processEvents = port.process ?? (() => {
    if (port.on === undefined) return undefined;
    return {
      on: port.on.bind(port),
      ...(port.off === undefined ? {} : { off: port.off.bind(port) }),
      ...(port.removeListener === undefined ? {} : { removeListener: port.removeListener.bind(port) }),
    };
  })();
  let reportedExit: ProcessExit | undefined;
  if (processEvents) subscribe(processEvents, "exit", (code: unknown, signal: unknown) => {
    reportedExit = {
      code: typeof code === "number" ? code : null,
      signal: typeof signal === "string" ? signal : null,
    };
  });
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
  const processClosed = (exitPromise: Promise<ProcessExit>): Promise<void> => {
    if (!processEvents) return exitPromise.then(() => undefined);
    return new Promise<void>((resolve) => {
      let settled = false;
      const done = () => { if (!settled) { settled = true; resolve(); } };
      subscribe(processEvents, "close", done);
      void exitPromise.then(done, done);
    });
  };
  const boundedKill = async (): Promise<void> => {
    try {
      const result = port.kill?.("SIGTERM");
      if (!result || typeof (result as PromiseLike<unknown>).then !== "function") return;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const settled = Promise.resolve(result).then(() => undefined, () => undefined);
      const timeout = new Promise<void>((resolve) => { timer = setTimeout(resolve, options.timeoutMs ?? 10_000); });
      try { await Promise.race([settled, timeout]); } finally { if (timer !== undefined) clearTimeout(timer); }
    } catch { /* process may already be gone */ }
  };
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const exitPromise: Promise<ProcessExit> = port.wait
      ? port.wait()
      : processEvents
        ? new Promise<ProcessExit>((resolve) => subscribe(processEvents, "close", () => resolve(reportedExit ?? { code: null, signal: null })))
        : Promise.resolve({ code: null, signal: null });
    const observation = Promise.all([streamClosed(port.stdout), streamClosed(port.stderr), exitPromise.then((value) => value)]);
    const timeoutMs = options.timeoutMs ?? 10_000;
    const timedOut = new Promise<undefined>((resolve) => { timer = setTimeout(() => resolve(undefined), timeoutMs); });
    const result = await Promise.race([observation, timedOut]);
    if (result === undefined) {
      try { port.stdin.end(); } catch { /* process may already be gone */ }
      await boundedKill();
      return undefined;
    }
    const [stdoutErrored, stderrErrored, exit] = result;
    await processClosed(exitPromise);
    if (stdoutErrored || stderrErrored || exit.code !== 0) return undefined;
  } catch { return undefined; }
  finally {
    if (timer !== undefined) clearTimeout(timer);
    unsubscribe();
  }
  const version = parseVersion(output);
  if (version !== PI_VERSION) return undefined;
  return { status: "detected", source: "system", executable, version };
}

export const detectPiRuntime = detectPi;

export async function detectPi(options: DetectOptions = {}): Promise<PiDetection> {
  if (options.hostEnvironment === undefined) throw new RuntimeUnavailableError();
  let env: Record<string, string>;
  try {
    env = buildRuntimeEnvironment(options.hostEnvironment, options.providerEnvironment ?? {}, options.allowedProviderKeys ?? new Set());
  } catch { throw new RuntimeUnavailableError(); }
  const probeOptions: ProbeOptions = { ...options, env };
  for (const executable of ["pi", "pi.exe"]) {
    const found = await probe(executable, probeOptions);
    if (found) return found;
  }
  return { status: "unavailable", source: "managed", managedInstall: "available" };
}
