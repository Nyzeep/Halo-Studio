import { basicAuthHeader, type ServerCredentials } from "./auth.js";
import {
  AuthenticationFailedError,
  ProtocolViolationError,
  TransportDisconnectedError,
} from "./errors.js";

const MAX_SESSION_ID_LENGTH = 512;
const MAX_TEXT_LENGTH = 32_768;

export interface OpenCodeSseSession {
  readonly sessionId: string;
  readonly title?: string;
  readonly updatedAt?: string;
}

/** Safe projection of the small event subset required by the R2 session UI. */
export type OpenCodeSessionStreamEvent =
  | { readonly type: "session-created"; readonly session: OpenCodeSseSession }
  | { readonly type: "session-updated"; readonly session: OpenCodeSseSession }
  | { readonly type: "session-status"; readonly sessionId: string; readonly active: boolean }
  | { readonly type: "session-idle"; readonly sessionId: string }
  | { readonly type: "session-error"; readonly sessionId: string }
  | { readonly type: "message-updated"; readonly sessionId: string; readonly messageId: string; readonly role: "user" | "assistant" }
  | {
      readonly type: "message-part-updated";
      readonly sessionId: string;
      readonly messageId: string;
      readonly text: string;
      readonly delta?: string;
    };

export type SseSignal =
  | { readonly type: "connected"; readonly data?: unknown }
  | { readonly type: "heartbeat"; readonly data?: unknown }
  | { readonly type: "event"; readonly event: OpenCodeSessionStreamEvent }
  | { readonly type: "protocol-violation"; readonly error: ProtocolViolationError }
  | { readonly type: "disconnected" };

export interface SseOptions {
  readonly onSignal: (signal: SseSignal) => void;
  readonly sampleUnknown?: (event: string) => void;
  /** Fixed by Main from the runtime workspace; never sourced from Renderer. */
  readonly workspaceDirectory?: string;
}

export type SseFetch = (url: string, init?: RequestInit) => Promise<Response>;

export interface OpenCodeSseConnectionOptions {
  readonly baseUrl: string;
  readonly credentials: ServerCredentials;
  readonly onSignal: (signal: SseSignal) => void;
  readonly sampleUnknown?: (event: string) => void;
  readonly fetch?: SseFetch;
  readonly signal?: AbortSignal;
  readonly workspaceDirectory?: string;
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function safeSessionId(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_SESSION_ID_LENGTH) return undefined;
  if (/[\u0000-\u001f\u007f]/u.test(value)) return undefined;
  return value;
}

function boundedText(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_TEXT_LENGTH) return undefined;
  return value;
}

function optionalText(record: Record<string, unknown>, key: string): string | undefined | null {
  if (!(key in record) || record[key] === undefined || record[key] === null) return undefined;
  return boundedText(record[key]) ?? null;
}

function optionalUpdatedAt(record: Record<string, unknown>): string | undefined | null {
  if (!("time" in record) || record.time === undefined || record.time === null) return undefined;
  if (!isRecord(record.time) || typeof record.time.updated !== "number" || !Number.isFinite(record.time.updated)) return null;
  const date = new Date(record.time.updated);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
}

function projectSession(value: unknown): OpenCodeSseSession | undefined {
  if (!isRecord(value)) return undefined;
  const sessionId = safeSessionId(value.id);
  if (!sessionId) return undefined;
  const title = optionalText(value, "title");
  const updatedAt = optionalUpdatedAt(value);
  if (title === null || updatedAt === null) return undefined;
  return {
    sessionId,
    ...(title === undefined ? {} : { title }),
    ...(updatedAt === undefined ? {} : { updatedAt }),
  };
}

function projectGlobalEvent(value: unknown, workspaceDirectory: string): SseSignal | undefined {
  if (!isRecord(value) || !isRecord(value.payload) || typeof value.payload.type !== "string") return undefined;
  const payload = value.payload;
  if (payload.type === "server.connected") return { type: "connected" };
  if (payload.type === "server.heartbeat") return { type: "heartbeat" };
  if (value.directory !== workspaceDirectory || !isRecord(payload.properties)) return undefined;
  const properties = payload.properties;

  if (payload.type === "session.created" || payload.type === "session.updated") {
    const session = projectSession(properties.info);
    if (!session) return undefined;
    return { type: "event", event: { type: payload.type === "session.created" ? "session-created" : "session-updated", session } };
  }
  if (payload.type === "session.status") {
    const sessionId = safeSessionId(properties.sessionID);
    if (!sessionId || !isRecord(properties.status)) return undefined;
    const status = properties.status.type;
    if (status !== "busy" && status !== "idle" && status !== "retry") return undefined;
    return { type: "event", event: { type: "session-status", sessionId, active: status !== "idle" } };
  }
  if (payload.type === "session.idle" || payload.type === "session.error") {
    const sessionId = safeSessionId(properties.sessionID);
    if (!sessionId) return undefined;
    return { type: "event", event: { type: payload.type === "session.idle" ? "session-idle" : "session-error", sessionId } };
  }
  if (payload.type === "message.updated") {
    if (!isRecord(properties.info)) return undefined;
    const sessionId = safeSessionId(properties.info.sessionID);
    const messageId = safeSessionId(properties.info.id);
    const role = properties.info.role;
    if (!sessionId || !messageId || (role !== "user" && role !== "assistant")) return undefined;
    return { type: "event", event: { type: "message-updated", sessionId, messageId, role } };
  }
  if (payload.type === "message.part.updated") {
    if (!isRecord(properties.part) || properties.part.type !== "text") return undefined;
    const sessionId = safeSessionId(properties.part.sessionID);
    const messageId = safeSessionId(properties.part.messageID);
    const text = boundedText(properties.part.text);
    const delta = optionalText(properties, "delta");
    if (!sessionId || !messageId || !text || delta === null) return undefined;
    return {
      type: "event",
      event: {
        type: "message-part-updated",
        sessionId,
        messageId,
        text,
        ...(delta === undefined ? {} : { delta }),
      },
    };
  }
  return undefined;
}

function dispatch(event: string, dataLines: readonly string[], options: SseOptions): void {
  if (event === "message" && options.workspaceDirectory !== undefined) {
    if (dataLines.length === 0) return;
    try {
      const signal = projectGlobalEvent(parseData(dataLines), options.workspaceDirectory);
      if (signal) options.onSignal(signal);
      else options.sampleUnknown?.("message");
    } catch (error) {
      options.onSignal({ type: "protocol-violation", error: error instanceof ProtocolViolationError ? error : new ProtocolViolationError() });
    }
    return;
  }
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
    ...(options.workspaceDirectory === undefined ? {} : { workspaceDirectory: options.workspaceDirectory }),
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
