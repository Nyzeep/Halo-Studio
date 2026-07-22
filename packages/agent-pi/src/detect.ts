import type { ProcessExit, ProcessPort } from "./jsonlTransport.js";
import { spawn } from "node:child_process";
import { PI_VERSION, type PiDetection } from "./schemas.js";

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
  readonly env?: Readonly<Record<string, string>>;
}

function parseVersion(output: string): string | undefined {
  const match = /(?:^|\s|v)(\d+\.\d+\.\d+)(?:\s|$)/u.exec(output);
  return match?.[1];
}

async function probe(executable: string, options: DetectOptions): Promise<PiDetection | undefined> {
  let port: ProcessPort;
  try {
    const factoryOptions: ProcessFactoryOptions = {
      ...(options.cwd === undefined ? {} : { cwd: options.cwd }),
      ...(options.env === undefined ? {} : { env: options.env }),
    };
    port = (options.processFactory ?? nodeProcessFactory)(executable, ["--version"], factoryOptions);
  } catch { return undefined; }
  let output = "";
  port.stdout.on("data", (chunk: Buffer | string) => { output += typeof chunk === "string" ? chunk : chunk.toString("utf8"); });
  port.stderr?.on("data", () => undefined);
  const processEvents = port.process ?? (port.on === undefined ? undefined : { on: port.on.bind(port) });
  let reportedExit: ProcessExit | undefined;
  processEvents?.on("exit", (code: unknown, signal: unknown) => {
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
      stream.on("end", done);
      stream.on("close", done);
      stream.on("error", () => done(true));
    });
  };
  const processClosed = (exitPromise: Promise<ProcessExit>): Promise<void> => {
    if (!processEvents) return exitPromise.then(() => undefined);
    return new Promise<void>((resolve) => {
      let settled = false;
      const done = () => { if (!settled) { settled = true; resolve(); } };
      processEvents.on("close", done);
      void exitPromise.then(done, done);
    });
  };
  try {
    const exitPromise: Promise<ProcessExit> = port.wait
      ? port.wait()
      : processEvents
        ? new Promise<ProcessExit>((resolve) => processEvents.on("close", () => resolve(reportedExit ?? { code: null, signal: null })))
        : Promise.resolve({ code: null, signal: null });
    const [stdoutErrored, stderrErrored, exit] = await Promise.all([streamClosed(port.stdout), streamClosed(port.stderr), exitPromise.then((value) => value)]);
    await processClosed(exitPromise);
    if (stdoutErrored || stderrErrored || exit.code !== 0) return undefined;
  } catch { return undefined; }
  const version = parseVersion(output);
  if (version !== PI_VERSION) return undefined;
  return { status: "detected", source: "system", executable, version };
}

export const detectPiRuntime = detectPi;

export async function detectPi(options: DetectOptions = {}): Promise<PiDetection> {
  for (const executable of ["pi", "pi.exe"]) {
    const found = await probe(executable, options);
    if (found) return found;
  }
  return { status: "unavailable", source: "managed", managedInstall: "available" };
}
