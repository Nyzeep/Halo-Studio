import { basicAuthHeader, type ServerCredentials } from "./auth.js";
import {
  AuthenticationFailedError,
  ProtocolViolationError,
  TransportDisconnectedError,
} from "./errors.js";

export type SseSignal =
  | { readonly type: "connected"; readonly data?: unknown }
  | { readonly type: "heartbeat"; readonly data?: unknown }
  | { readonly type: "protocol-violation"; readonly error: ProtocolViolationError }
  | { readonly type: "disconnected" };

export interface SseOptions {
  readonly onSignal: (signal: SseSignal) => void;
  readonly sampleUnknown?: (event: string) => void;
}

export type SseFetch = (url: string, init?: RequestInit) => Promise<Response>;

export interface OpenCodeSseConnectionOptions {
  readonly baseUrl: string;
  readonly credentials: ServerCredentials;
  readonly onSignal: (signal: SseSignal) => void;
  readonly sampleUnknown?: (event: string) => void;
  readonly fetch?: SseFetch;
  readonly signal?: AbortSignal;
}

export interface OpenCodeSseConnection {
  readonly done: Promise<void>;
  close(): Promise<void>;
}

const MAX_UNKNOWN_EVENT_SAMPLES = 10;

function parseData(lines: readonly string[]): unknown {
  const data = lines.join("\n");
  try { return JSON.parse(data) as unknown; } catch { throw new ProtocolViolationError(); }
}

function dispatch(event: string, dataLines: readonly string[], options: SseOptions): void {
  if (event === "") {
    if (dataLines.length === 0) return;
    try {
      const data = parseData(dataLines);
      const type = typeof data === "object" && data !== null && "type" in data && typeof data.type === "string" ? data.type : "";
      if (type === "server.connected" || type === "connected") {
        options.onSignal({ type: "connected", data });
      } else if (type === "server.heartbeat" || type === "heartbeat") {
        options.onSignal({ type: "heartbeat", data });
      } else {
        options.sampleUnknown?.(type || "message");
      }
    } catch (error) {
      options.onSignal({ type: "protocol-violation", error: error instanceof ProtocolViolationError ? error : new ProtocolViolationError() });
    }
    return;
  }
  if (event !== "connected" && event !== "heartbeat") {
    options.sampleUnknown?.(event);
    if (dataLines.length > 0) {
      try { parseData(dataLines); }
      catch (error) {
        options.onSignal({ type: "protocol-violation", error: error instanceof ProtocolViolationError ? error : new ProtocolViolationError() });
      }
    }
    return;
  }
  try {
    const data = dataLines.length > 0 ? parseData(dataLines) : undefined;
    options.onSignal({ type: event, ...(data === undefined ? {} : { data }) });
  } catch (error) {
    options.onSignal({ type: "protocol-violation", error: error instanceof ProtocolViolationError ? error : new ProtocolViolationError() });
  }
}

export async function consumeSse(
  stream: AsyncIterable<string | Uint8Array>,
  options: SseOptions,
): Promise<void> {
  let unknownSamples = 0;
  const boundedOptions: SseOptions = {
    ...options,
    sampleUnknown: (event) => {
      if (unknownSamples >= MAX_UNKNOWN_EVENT_SAMPLES) return;
      unknownSamples += 1;
      options.sampleUnknown?.(event);
    },
  };
  const decoder = new TextDecoder();
  let buffer = "";
  let event = "";
  let dataLines: string[] = [];
  const processLine = (raw: string): void => {
    const line = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    if (line === "") {
      dispatch(event, dataLines, boundedOptions);
      event = "";
      dataLines = [];
      return;
    }
    if (line.startsWith(":")) {
      if (line.slice(1).trim().toLowerCase() === "heartbeat") boundedOptions.onSignal({ type: "heartbeat" });
      return;
    }
    const separator = line.indexOf(":");
    const field = separator < 0 ? line : line.slice(0, separator);
    const value = separator < 0 ? "" : line.slice(separator + 1).replace(/^ /u, "");
    if (field === "event") event = value;
    if (field === "data") dataLines.push(value);
  };

  try {
    for await (const chunk of stream) {
      buffer += typeof chunk === "string" ? chunk : decoder.decode(chunk, { stream: true });
      let newline = buffer.indexOf("\n");
      while (newline >= 0) {
        processLine(buffer.slice(0, newline));
        buffer = buffer.slice(newline + 1);
        newline = buffer.indexOf("\n");
      }
    }
    buffer += decoder.decode();
    if (buffer.length > 0) processLine(buffer);
    if (event !== "" || dataLines.length > 0) dispatch(event, dataLines, boundedOptions);
  } finally {
    boundedOptions.onSignal({ type: "disconnected" });
  }
}

function eventEndpoint(baseUrl: string): string {
  return `${baseUrl.replace(/\/$/u, "")}/global/event`;
}

async function* readResponse(reader: ReadableStreamDefaultReader<Uint8Array>): AsyncIterable<Uint8Array> {
  while (true) {
    const result = await reader.read();
    if (result.done) return;
    yield result.value;
  }
}

function cancelBody(response: Response): void {
  if (!response.body) return;
  try { response.body.cancel().catch(() => undefined); }
  catch { /* Cancellation is best effort for rejected handshakes. */ }
}

export async function connectOpenCodeSse(options: OpenCodeSseConnectionOptions): Promise<OpenCodeSseConnection> {
  const controller = new AbortController();
  const fetcher = options.fetch ?? ((url, init) => fetch(url, init));
  let closeFromCaller: (() => void) | undefined;
  const abortFromCaller = (): void => {
    controller.abort();
    closeFromCaller?.();
  };
  if (options.signal?.aborted) controller.abort();
  else options.signal?.addEventListener("abort", abortFromCaller, { once: true });

  let response: Response;
  try {
    response = await fetcher(eventEndpoint(options.baseUrl), {
      method: "GET",
      headers: {
        authorization: basicAuthHeader(options.credentials),
        accept: "text/event-stream",
      },
      signal: controller.signal,
    });
  } catch {
    options.signal?.removeEventListener("abort", abortFromCaller);
    throw new TransportDisconnectedError();
  }
  if (response.status === 401) {
    cancelBody(response);
    options.signal?.removeEventListener("abort", abortFromCaller);
    throw new AuthenticationFailedError();
  }
  if (response.status !== 200 || !response.body || controller.signal.aborted) {
    cancelBody(response);
    options.signal?.removeEventListener("abort", abortFromCaller);
    throw new TransportDisconnectedError();
  }

  let reader: ReadableStreamDefaultReader<Uint8Array>;
  try { reader = response.body.getReader(); }
  catch {
    cancelBody(response);
    options.signal?.removeEventListener("abort", abortFromCaller);
    throw new TransportDisconnectedError();
  }
  let closeRequested = false;
  let closePromise: Promise<void> | undefined;
  const done = consumeSse(readResponse(reader), {
    onSignal: options.onSignal,
    ...(options.sampleUnknown === undefined ? {} : { sampleUnknown: options.sampleUnknown }),
  }).catch((error: unknown) => {
    if (closeRequested || controller.signal.aborted) return;
    throw error instanceof ProtocolViolationError ? error : new TransportDisconnectedError();
  }).finally(() => {
    options.signal?.removeEventListener("abort", abortFromCaller);
    try { reader.releaseLock(); } catch { /* The reader may already be released by the platform. */ }
  });
  done.catch(() => undefined);

  const close = (): Promise<void> => {
    closeRequested = true;
    controller.abort();
    closePromise ??= Promise.resolve().then(() => reader.cancel()).catch(() => undefined).then(() => done);
    return closePromise;
  };
  closeFromCaller = () => { void close(); };
  return { done, close };
}

export { ProtocolViolationError };
export const parseOpenCodeSse = consumeSse;
export const parseSse = consumeSse;
