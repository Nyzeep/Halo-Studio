import { ProtocolViolationError } from "./errors.js";

export type SseSignal =
  | { readonly type: "connected"; readonly data?: unknown }
  | { readonly type: "heartbeat"; readonly data?: unknown }
  | { readonly type: "protocol-violation"; readonly error: ProtocolViolationError }
  | { readonly type: "disconnected" };

export interface SseOptions {
  readonly onSignal: (signal: SseSignal) => void;
  readonly sampleUnknown?: (event: string) => void;
  readonly request?: (url: string, init?: RequestInit) => Promise<unknown>;
  readonly requestUrl?: string;
}

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
  if (options.request && options.requestUrl) await options.request(options.requestUrl, { method: "GET", headers: { accept: "text/event-stream" } });
  const decoder = new TextDecoder();
  let buffer = "";
  let event = "";
  let dataLines: string[] = [];
  const processLine = (raw: string): void => {
    const line = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    if (line === "") {
      dispatch(event, dataLines, options);
      event = "";
      dataLines = [];
      return;
    }
    if (line.startsWith(":")) {
      if (line.slice(1).trim().toLowerCase() === "heartbeat") options.onSignal({ type: "heartbeat" });
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
    if (event !== "" || dataLines.length > 0) dispatch(event, dataLines, options);
  } finally {
    options.onSignal({ type: "disconnected" });
  }
}

export { ProtocolViolationError };
export const parseOpenCodeSse = consumeSse;
export const parseSse = consumeSse;
