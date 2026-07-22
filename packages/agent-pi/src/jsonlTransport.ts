import { StringDecoder } from "node:string_decoder";
import { randomUUID } from "node:crypto";
import { piCommandSchema, piEventSchema, piResponseSchema, type PiCommand, type PiEvent, type PiResponse } from "./schemas.js";
import { PiError, ProtocolViolationError, TransportDisconnectedError } from "./errors.js";

export interface ProcessStream {
  on(event: "data" | "end" | "error" | "close", listener: (...args: any[]) => void): unknown;
  off?(event: "data" | "end" | "error" | "close", listener: (...args: any[]) => void): unknown;
  removeListener?(event: "data" | "end" | "error" | "close", listener: (...args: any[]) => void): unknown;
  once?(event: "end" | "error" | "close", listener: (...args: any[]) => void): unknown;
}

export interface ProcessStdin {
  write(data: string, callback?: (error?: Error | null) => void): unknown;
  end(callback?: () => void): unknown;
}

export interface ProcessExit {
  readonly code: number | null;
  readonly signal: string | null;
}

export interface ProcessPort {
  readonly stdin: ProcessStdin;
  readonly stdout: ProcessStream;
  readonly stderr?: ProcessStream;
  readonly process?: { on(event: "exit" | "error" | "close", listener: (...args: any[]) => void): unknown; off?(event: "exit" | "error" | "close", listener: (...args: any[]) => void): unknown; removeListener?(event: "exit" | "error" | "close", listener: (...args: any[]) => void): unknown; once?(event: "exit" | "error" | "close", listener: (...args: any[]) => void): unknown };
  on?(event: "exit" | "error" | "close", listener: (...args: any[]) => void): unknown;
  off?(event: "exit" | "error" | "close", listener: (...args: any[]) => void): unknown;
  removeListener?(event: "exit" | "error" | "close", listener: (...args: any[]) => void): unknown;
  wait?(): Promise<ProcessExit>;
  kill?(signal?: string): boolean | void | Promise<boolean | void>;
}

export interface RequestOptions {
  readonly timeoutMs?: number;
}

export interface TransportOptions {
  readonly defaultTimeoutMs?: number;
}

type EventListener = (event: PiEvent) => void;
interface Pending {
  readonly id: string;
  readonly command: PiCommand;
  readonly resolve: (response: PiResponse) => void;
  readonly reject: (error: Error) => void;
  readonly done: Promise<void>;
  readonly finish: () => void;
  timer?: ReturnType<typeof setTimeout>;
  sent: boolean;
}

export class PiJsonlTransport {
  readonly #port: ProcessPort;
  readonly #pending = new Map<string, Pending>();
  readonly #queue: Pending[] = [];
  readonly #events = new Set<EventListener>();
  readonly #decoder = new StringDecoder("utf8");
  #buffer = "";
  #closed = false;
  #pumping = false;
  readonly #defaultTimeoutMs: number;
  readonly #onDisconnect: (error: PiError) => void;
  readonly #subscriptions: Array<{ target: ProcessStream | ProcessPort | NonNullable<ProcessPort["process"]>; event: string; listener: (...args: any[]) => void }> = [];

  constructor(port: ProcessPort, options: TransportOptions & { readonly onDisconnect?: (error: PiError) => void } = {}) {
    this.#port = port;
    this.#defaultTimeoutMs = options.defaultTimeoutMs ?? 30_000;
    this.#onDisconnect = options.onDisconnect ?? (() => undefined);
    this.#listen(port.stdout, "data", (chunk: Buffer | string) => this.#onData(chunk));
    this.#listen(port.stdout, "end", () => {
      const tail = this.#decoder.end();
      if (tail.length > 0) this.#onData(tail);
      this.close(new TransportDisconnectedError());
    });
    this.#listen(port.stdout, "close", () => this.close(new TransportDisconnectedError()));
    this.#listen(port.stdout, "error", () => this.close(new TransportDisconnectedError()));
    if (port.stderr) {
      this.#listen(port.stderr, "data", () => undefined);
      this.#listen(port.stderr, "error", () => this.close(new TransportDisconnectedError()));
    }
    const processEvents = port.process ?? (() => {
      if (port.on === undefined) return undefined;
      return {
        on: port.on.bind(port),
        ...(port.off === undefined ? {} : { off: port.off.bind(port) }),
        ...(port.removeListener === undefined ? {} : { removeListener: port.removeListener.bind(port) }),
      };
    })();
    if (processEvents) {
      this.#listen(processEvents, "error", () => this.close(new TransportDisconnectedError()));
      this.#listen(processEvents, "exit", () => this.close(new TransportDisconnectedError()));
      this.#listen(processEvents, "close", () => this.close(new TransportDisconnectedError()));
    }
  }

  request(command: PiCommand, options: RequestOptions = {}): Promise<PiResponse> {
    const parsed = piCommandSchema.safeParse(command);
    if (!parsed.success) return Promise.reject(new ProtocolViolationError());
    if (this.#closed) return Promise.reject(new TransportDisconnectedError());
    const id = parsed.data.id ?? `pi_${randomUUID()}`;
    const fullCommand: PiCommand = { ...parsed.data, id };
    if (this.#pending.has(id)) return Promise.reject(new ProtocolViolationError());
    return new Promise<PiResponse>((resolve, reject) => {
      let finish!: () => void;
      const done = new Promise<void>((resolveDone) => { finish = resolveDone; });
      const pending: Pending = { id, command: fullCommand, resolve, reject, done, finish, sent: false };
      const timeoutMs = options.timeoutMs ?? this.#defaultTimeoutMs;
      if (timeoutMs > 0 && Number.isFinite(timeoutMs)) {
        pending.timer = setTimeout(() => this.#timeout(pending), timeoutMs);
      }
      this.#pending.set(id, pending);
      if (fullCommand.type === "abort" || fullCommand.type === "steer") {
        this.#send(pending);
      } else {
        this.#queue.push(pending);
        void this.#pump();
      }
    });
  }

  onEvent(listener: EventListener): () => void {
    this.#events.add(listener);
    return () => this.#events.delete(listener);
  }

  close(error: Error = new TransportDisconnectedError()): void {
    if (this.#closed) return;
    this.#closed = true;
    for (const pending of this.#pending.values()) {
      if (pending.timer !== undefined) clearTimeout(pending.timer);
      pending.reject(error instanceof PiError ? error : new TransportDisconnectedError());
      pending.finish();
    }
    this.#pending.clear();
    this.#queue.length = 0;
    for (const subscription of this.#subscriptions.splice(0)) {
      subscription.target.off?.(subscription.event as never, subscription.listener as never);
      subscription.target.removeListener?.(subscription.event as never, subscription.listener as never);
    }
    this.#events.clear();
    this.#onDisconnect?.(error instanceof PiError ? error : new TransportDisconnectedError());
  }

  dispose(): void { this.close(); }

  get closed(): boolean { return this.#closed; }

  #listen(target: ProcessStream | ProcessPort | NonNullable<ProcessPort["process"]>, event: string, listener: (...args: any[]) => void): void {
    target.on?.(event as never, listener as never);
    this.#subscriptions.push({ target, event, listener });
  }

  async #pump(): Promise<void> {
    if (this.#pumping || this.#closed) return;
    this.#pumping = true;
    try {
      while (!this.#closed && this.#queue.length > 0) {
        const pending = this.#queue.shift();
        if (!pending || !this.#pending.has(pending.id)) continue;
        pending.sent = true;
        await this.#send(pending);
        if (pending.command.type !== "abort" && pending.command.type !== "steer") await pending.done;
      }
    } finally {
      this.#pumping = false;
    }
  }

  async #send(pending: Pending): Promise<void> {
    if (this.#closed || !this.#pending.has(pending.id)) return;
    try {
      const result = this.#port.stdin.write(`${JSON.stringify(pending.command)}\n`, (error) => {
        if (error) this.#failPending(pending, new TransportDisconnectedError());
      });
      if (result && typeof (result as Promise<void>).then === "function") await result;
    } catch {
      this.#pending.delete(pending.id);
      if (pending.timer !== undefined) clearTimeout(pending.timer);
      pending.reject(new TransportDisconnectedError());
      pending.finish();
      this.close(new TransportDisconnectedError());
    }
  }

  #failPending(pending: Pending, error: Error): void {
    if (!this.#pending.delete(pending.id)) return;
    if (pending.timer !== undefined) clearTimeout(pending.timer);
    pending.reject(error);
    pending.finish();
    this.close(error);
  }

  #timeout(pending: Pending): void {
    if (!this.#pending.delete(pending.id)) return;
    if (pending.timer !== undefined) clearTimeout(pending.timer);
    pending.reject(new TransportDisconnectedError());
    pending.finish();
    this.close(new TransportDisconnectedError());
  }

  #onData(chunk: Buffer | string): void {
    if (this.#closed) return;
    this.#buffer += typeof chunk === "string" ? chunk : this.#decoder.write(chunk);
    while (true) {
      const index = this.#buffer.indexOf("\n");
      if (index < 0) break;
      let line = this.#buffer.slice(0, index);
      this.#buffer = this.#buffer.slice(index + 1);
      if (line.endsWith("\r")) line = line.slice(0, -1);
      if (line.length === 0) continue;
      let parsed: unknown;
      try { parsed = JSON.parse(line); } catch { this.close(new ProtocolViolationError()); return; }
      const isResponse = typeof parsed === "object" && parsed !== null && (parsed as { type?: unknown }).type === "response";
      const response = piResponseSchema.safeParse(parsed);
      if (isResponse && !response.success) { this.close(new ProtocolViolationError()); return; }
      if (response.success && response.data.type === "response") {
        if (!response.data.id) { this.close(new ProtocolViolationError()); return; }
        const pending = this.#pending.get(response.data.id);
        if (!pending) continue;
        this.#pending.delete(response.data.id);
        if (pending.timer !== undefined) clearTimeout(pending.timer);
        pending.resolve(response.data);
        pending.finish();
        continue;
      }
      const event = piEventSchema.safeParse(parsed);
      if (!event.success) { this.close(new ProtocolViolationError()); return; }
      for (const listener of this.#events) {
        try { listener(event.data); } catch { /* listener errors do not corrupt the wire */ }
      }
    }
  }
}

export const JsonlTransport = PiJsonlTransport;

export { PiError, ProtocolViolationError, TransportDisconnectedError };
