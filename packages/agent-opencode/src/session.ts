import {
  AuthenticationFailedError,
  OpenCodeError,
  ProtocolViolationError,
  RuntimeUnavailableError,
  TransportDisconnectedError,
} from "./errors.js";
import type { OpenCodeSessionStreamEvent, OpenCodeSseConnection, SseSignal } from "./sse.js";

const MAX_SESSION_ID_LENGTH = 512;
const MAX_TEXT_LENGTH = 32_768;
const MAX_SESSIONS = 128;
const MAX_HISTORY_MESSAGES = 128;
const MAX_HISTORY_PARTS = 256;
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

export interface OpenCodeSessionSummary {
  readonly sessionId: string;
  readonly title?: string;
  readonly updatedAt?: string;
  readonly active: boolean;
}

export interface OpenCodeSessionMessage {
  readonly sessionId: string;
  readonly ordinal: number;
  readonly role: "user" | "assistant";
  readonly text: string;
}

export interface OpenCodeSessionHistory {
  readonly session: OpenCodeSessionSummary;
  readonly messages: readonly OpenCodeSessionMessage[];
}

export type OpenCodeSessionEvent =
  | { readonly type: "connected" }
  | { readonly type: "heartbeat" }
  | { readonly type: "disconnected" }
  | { readonly type: "transport-error" }
  | OpenCodeSessionStreamEvent;

export interface OpenCodeSessionSubscription {
  unsubscribe(): Promise<void>;
}

/**
 * Main-only session API. It intentionally projects native responses down to
 * session summaries and text-only history rather than exposing the server API.
 */
export interface OpenCodeSessionAdapter {
  list(): Promise<readonly OpenCodeSessionSummary[]>;
  create(): Promise<OpenCodeSessionSummary>;
  get(sessionId: string): Promise<OpenCodeSessionSummary>;
  history(sessionId: string): Promise<OpenCodeSessionHistory>;
  startPrompt(sessionId: string, text: string): Promise<void>;
  abort(sessionId: string): Promise<void>;
  subscribe(listener: (event: OpenCodeSessionEvent) => void): Promise<OpenCodeSessionSubscription>;
}

export interface OpenCodeSessionResponse {
  readonly response: Response;
  close(): void;
}

/** @internal Runtime-owned transport seam for deterministic package tests. */
export interface OpenCodeSessionTransport {
  list(): Promise<OpenCodeSessionResponse>;
  create(): Promise<OpenCodeSessionResponse>;
  get(sessionId: string): Promise<OpenCodeSessionResponse>;
  history(sessionId: string): Promise<OpenCodeSessionResponse>;
  startPrompt(sessionId: string, text: string): Promise<OpenCodeSessionResponse>;
  abort(sessionId: string): Promise<OpenCodeSessionResponse>;
  subscribe(onSignal: (signal: SseSignal) => void): Promise<OpenCodeSseConnection>;
}

/** @internal Constructed only by OpenCodeRuntime. */
export function createOpenCodeSessionAdapter(transport: OpenCodeSessionTransport): OpenCodeSessionAdapter {
  return new SessionAdapter(transport);
}

class SessionAdapter implements OpenCodeSessionAdapter {
  readonly #transport: OpenCodeSessionTransport;
  readonly #active = new Set<string>();

  constructor(transport: OpenCodeSessionTransport) {
    this.#transport = transport;
  }

  async list(): Promise<readonly OpenCodeSessionSummary[]> {
    const value = await this.#json(() => this.#transport.list(), 200);
    if (!Array.isArray(value) || value.length > MAX_SESSIONS) throw new ProtocolViolationError();
    const summaries: OpenCodeSessionSummary[] = [];
    for (const item of value) {
      const summary = parseSummary(item, this.#active);
      if (summary) summaries.push(summary);
    }
    return summaries;
  }

  async create(): Promise<OpenCodeSessionSummary> {
    const value = await this.#json(() => this.#transport.create(), 200);
    const summary = parseSummary(value, this.#active);
    if (!summary) throw new ProtocolViolationError();
    return summary;
  }

  async get(sessionId: string): Promise<OpenCodeSessionSummary> {
    const id = requireSessionId(sessionId);
    const value = await this.#json(() => this.#transport.get(id), 200);
    const summary = parseSummary(value, this.#active);
    if (!summary || summary.sessionId !== id) throw new ProtocolViolationError();
    return summary;
  }

  async history(sessionId: string): Promise<OpenCodeSessionHistory> {
    const id = requireSessionId(sessionId);
    const session = await this.get(id);
    const value = await this.#json(() => this.#transport.history(id), 200);
    if (!Array.isArray(value) || value.length > MAX_HISTORY_MESSAGES) throw new ProtocolViolationError();
    const messages: OpenCodeSessionMessage[] = [];
    for (const item of value) {
      const message = parseHistoryMessage(item, id, messages.length);
      if (message) messages.push(message);
    }
    return { session, messages };
  }

  async startPrompt(sessionId: string, text: string): Promise<void> {
    const id = requireSessionId(sessionId);
    const message = requirePromptText(text);
    await this.#noContent(() => this.#transport.startPrompt(id, message), 204);
    this.#active.add(id);
  }

  async abort(sessionId: string): Promise<void> {
    const id = requireSessionId(sessionId);
    const value = await this.#json(() => this.#transport.abort(id), 200);
    if (value !== true) throw new ProtocolViolationError();
    this.#active.delete(id);
  }

  async subscribe(listener: (event: OpenCodeSessionEvent) => void): Promise<OpenCodeSessionSubscription> {
    if (typeof listener !== "function") throw new ProtocolViolationError();
    try {
      const connection = await this.#transport.subscribe((signal) => {
        const event = projectSignal(signal);
        if (!event) return;
        this.#track(event);
        try { listener(event); } catch { /* Subscriber failures must not terminate Main-owned SSE. */ }
      });
      return { unsubscribe: async () => {
        try { await connection.close(); }
        catch (error) { throw stableError(error); }
      } };
    } catch (error) {
      throw stableError(error);
    }
  }

  async #json(request: () => Promise<OpenCodeSessionResponse>, expectedStatus: number): Promise<unknown> {
    let ticket: OpenCodeSessionResponse | undefined;
    try {
      ticket = await request();
      const response = ticket.response;
      assertStatus(response, expectedStatus);
      return await readJson(response);
    } catch (error) {
      throw stableError(error);
    } finally {
      ticket?.close();
    }
  }

  async #noContent(request: () => Promise<OpenCodeSessionResponse>, expectedStatus: number): Promise<void> {
    let ticket: OpenCodeSessionResponse | undefined;
    try {
      ticket = await request();
      assertStatus(ticket.response, expectedStatus);
    } catch (error) {
      throw stableError(error);
    } finally {
      ticket?.close();
    }
  }

  #track(event: OpenCodeSessionEvent): void {
    if (event.type === "session-status") {
      if (event.active) this.#active.add(event.sessionId);
      else this.#active.delete(event.sessionId);
      return;
    }
    if (event.type === "session-idle" || event.type === "session-error") {
      this.#active.delete(event.sessionId);
    }
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function boundedText(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_TEXT_LENGTH) return undefined;
  return value;
}

function safeSessionId(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > MAX_SESSION_ID_LENGTH) return undefined;
  if (/[\u0000-\u001f\u007f]/u.test(value)) return undefined;
  return value;
}

function requireSessionId(value: unknown): string {
  const id = safeSessionId(value);
  if (!id) throw new ProtocolViolationError();
  return id;
}

function requirePromptText(value: unknown): string {
  const text = boundedText(value);
  if (!text || text.trim().length === 0) throw new ProtocolViolationError();
  return text;
}

function parseOptionalText(record: Record<string, unknown>, key: string): string | undefined | null {
  if (!(key in record) || record[key] === undefined || record[key] === null) return undefined;
  return boundedText(record[key]) ?? null;
}

function parseUpdatedAt(record: Record<string, unknown>): string | undefined | null {
  if (!("time" in record) || record.time === undefined || record.time === null) return undefined;
  if (!isRecord(record.time) || typeof record.time.updated !== "number" || !Number.isFinite(record.time.updated)) return null;
  const date = new Date(record.time.updated);
  if (Number.isNaN(date.getTime())) return null;
  return date.toISOString();
}

function parseSummary(value: unknown, active: ReadonlySet<string>): OpenCodeSessionSummary | undefined {
  if (!isRecord(value)) return undefined;
  const sessionId = safeSessionId(value.id);
  if (!sessionId) return undefined;
  const title = parseOptionalText(value, "title");
  const updatedAt = parseUpdatedAt(value);
  if (title === null || updatedAt === null) return undefined;
  return {
    sessionId,
    ...(title === undefined ? {} : { title }),
    ...(updatedAt === undefined ? {} : { updatedAt }),
    active: active.has(sessionId),
  };
}

function parseHistoryMessage(value: unknown, sessionId: string, ordinal: number): OpenCodeSessionMessage | undefined {
  if (!isRecord(value) || !isRecord(value.info) || !Array.isArray(value.parts) || value.parts.length > MAX_HISTORY_PARTS) return undefined;
  if (value.info.sessionID !== sessionId) return undefined;
  const role = value.info.role;
  if (role !== "user" && role !== "assistant") return undefined;
  const parts: string[] = [];
  let length = 0;
  for (const part of value.parts) {
    if (!isRecord(part) || part.type !== "text") continue;
    const text = boundedText(part.text);
    if (!text || part.sessionID !== sessionId) return undefined;
    length += text.length;
    if (length > MAX_TEXT_LENGTH) return undefined;
    parts.push(text);
  }
  const text = parts.join("");
  if (!text) return undefined;
  return { sessionId, ordinal, role, text };
}

function projectSignal(signal: SseSignal): OpenCodeSessionEvent | undefined {
  if (signal.type === "connected") return { type: "connected" };
  if (signal.type === "heartbeat") return { type: "heartbeat" };
  if (signal.type === "disconnected") return { type: "disconnected" };
  if (signal.type === "protocol-violation") return { type: "transport-error" };
  if (signal.type === "event") return signal.event;
  return undefined;
}

function assertStatus(response: Response, expectedStatus: number): void {
  if (response.status === 401) {
    cancelResponse(response);
    throw new AuthenticationFailedError();
  }
  if (response.status !== expectedStatus) {
    cancelResponse(response);
    throw response.status >= 500 ? new TransportDisconnectedError() : new RuntimeUnavailableError();
  }
}

function cancelResponse(response: Response): void {
  try { response.body?.cancel().catch(() => undefined); } catch { /* Best effort after a rejected response. */ }
}

async function readJson(response: Response): Promise<unknown> {
  const declaredLength = response.headers.get("content-length");
  if (declaredLength !== null && (!/^\d+$/u.test(declaredLength) || Number(declaredLength) > MAX_RESPONSE_BYTES)) {
    cancelResponse(response);
    throw new ProtocolViolationError();
  }
  if (!response.body) throw new ProtocolViolationError();
  let reader: ReadableStreamDefaultReader<Uint8Array>;
  try { reader = response.body.getReader(); } catch { throw new ProtocolViolationError(); }
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const item = await reader.read();
      if (item.done) break;
      length += item.value.byteLength;
      if (length > MAX_RESPONSE_BYTES) {
        await reader.cancel().catch(() => undefined);
        throw new ProtocolViolationError();
      }
      chunks.push(item.value);
    }
  } catch (error) {
    if (error instanceof OpenCodeError) throw error;
    throw new TransportDisconnectedError();
  } finally {
    try { reader.releaseLock(); } catch { /* The reader may already be released. */ }
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  try {
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)) as unknown;
  } catch {
    throw new ProtocolViolationError();
  }
}

function stableError(error: unknown): OpenCodeError {
  if (error instanceof OpenCodeError) return error;
  return new TransportDisconnectedError();
}
