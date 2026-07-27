import {
  ipcContracts,
  sessionEventChannel,
  sessionEventSchema,
  type DataOf,
  type InputOf,
  type IpcChannel,
  type ResponseOf,
} from "@halo-studio/contracts";

export type IpcInvoke = (channel: string, request: unknown) => Promise<unknown>;

export interface IpcEventPort {
  on(channel: string, listener: (event: unknown, value: unknown) => void): void;
  removeListener(channel: string, listener: (event: unknown, value: unknown) => void): void;
}

export interface ContextBridgePort {
  exposeInMainWorld(key: string, api: unknown): void;
}

export type HaloApi = {
  readonly workspace: {
    readonly pick: (input: InputOf<"workspace.pick">) => Promise<ResponseOf<"workspace.pick">>;
    readonly open: (input: InputOf<"workspace.open">) => Promise<ResponseOf<"workspace.open">>;
    readonly snapshot: (input: InputOf<"workspace.snapshot">) => Promise<ResponseOf<"workspace.snapshot">>;
    readonly setTrust: (input: InputOf<"workspace.trust">) => Promise<ResponseOf<"workspace.trust">>;
  };
  readonly runtime: {
    readonly probe: (input: InputOf<"runtime.probe">) => Promise<ResponseOf<"runtime.probe">>;
    readonly start: (input: InputOf<"runtime.start">) => Promise<ResponseOf<"runtime.start">>;
    readonly stop: (input: InputOf<"runtime.stop">) => Promise<ResponseOf<"runtime.stop">>;
    readonly snapshot: (input: InputOf<"runtime.snapshot">) => Promise<ResponseOf<"runtime.snapshot">>;
  };
  readonly sessions: {
    readonly snapshot: (input: InputOf<"session.snapshot">) => Promise<ResponseOf<"session.snapshot">>;
    readonly create: (input: InputOf<"session.create">) => Promise<ResponseOf<"session.create">>;
    readonly select: (input: InputOf<"session.select">) => Promise<ResponseOf<"session.select">>;
    readonly history: (input: InputOf<"session.history">) => Promise<ResponseOf<"session.history">>;
    readonly send: (input: InputOf<"session.send">) => Promise<ResponseOf<"session.send">>;
    readonly abort: (input: InputOf<"session.abort">) => Promise<ResponseOf<"session.abort">>;
    readonly subscribe: (listener: (event: ReturnType<typeof sessionEventSchema.parse>) => void) => (() => void);
  };
  readonly commands: {
    readonly list: (input: InputOf<"command.list">) => Promise<ResponseOf<"command.list">>;
  };
  readonly config: {
    readonly preview: (input: InputOf<"config.preview">) => Promise<ResponseOf<"config.preview">>;
    readonly commit: (input: InputOf<"config.commit">) => Promise<ResponseOf<"config.commit">>;
    readonly rollback: (input: InputOf<"config.rollback">) => Promise<ResponseOf<"config.rollback">>;
  };
  readonly storage: {
    readonly health: (input: InputOf<"storage.health">) => Promise<ResponseOf<"storage.health">>;
  };
};

class PreloadProtocolError extends Error {
  readonly code = "ProtocolViolation" as const;
  readonly retryable = false;

  constructor() {
    super("Invalid IPC request or response.");
    this.name = "PreloadProtocolError";
  }
}

function fixedInvoke<K extends IpcChannel>(invoke: IpcInvoke, channel: K) {
  return async (raw: InputOf<K>): Promise<ResponseOf<K>> => {
    const request = ipcContracts[channel].request.safeParse(raw);
    if (!request.success) throw new PreloadProtocolError();
    let response: unknown;
    try {
      response = await invoke(channel, request.data);
    } catch {
      throw new PreloadProtocolError();
    }
    const parsed = ipcContracts[channel].response.safeParse(response);
    if (!parsed.success) throw new PreloadProtocolError();
    return parsed.data as ResponseOf<K>;
  };
}

function fixedSubscribe(
  events: IpcEventPort | undefined,
  listener: (event: ReturnType<typeof sessionEventSchema.parse>) => void,
): () => void {
  if (events === undefined || typeof listener !== "function") return () => undefined;
  const wrapped = (_event: unknown, raw: unknown): void => {
    const parsed = sessionEventSchema.safeParse(raw);
    if (!parsed.success) return;
    try { listener(parsed.data); } catch { /* Renderer callback errors must not corrupt preload. */ }
  };
  events.on(sessionEventChannel, wrapped);
  return () => { events.removeListener(sessionEventChannel, wrapped); };
}

export function createHaloApi(invoke: IpcInvoke, events?: IpcEventPort): HaloApi {
  return Object.freeze({
    workspace: Object.freeze({
      pick: fixedInvoke(invoke, "workspace.pick"),
      open: fixedInvoke(invoke, "workspace.open"),
      snapshot: fixedInvoke(invoke, "workspace.snapshot"),
      setTrust: fixedInvoke(invoke, "workspace.trust"),
    }),
    runtime: Object.freeze({
      probe: fixedInvoke(invoke, "runtime.probe"),
      start: fixedInvoke(invoke, "runtime.start"),
      stop: fixedInvoke(invoke, "runtime.stop"),
      snapshot: fixedInvoke(invoke, "runtime.snapshot"),
    }),
    sessions: Object.freeze({
      snapshot: fixedInvoke(invoke, "session.snapshot"),
      create: fixedInvoke(invoke, "session.create"),
      select: fixedInvoke(invoke, "session.select"),
      history: fixedInvoke(invoke, "session.history"),
      send: fixedInvoke(invoke, "session.send"),
      abort: fixedInvoke(invoke, "session.abort"),
      subscribe: (listener: (event: ReturnType<typeof sessionEventSchema.parse>) => void) => fixedSubscribe(events, listener),
    }),
    commands: Object.freeze({
      list: fixedInvoke(invoke, "command.list"),
    }),
    config: Object.freeze({
      preview: fixedInvoke(invoke, "config.preview"),
      commit: fixedInvoke(invoke, "config.commit"),
      rollback: fixedInvoke(invoke, "config.rollback"),
    }),
    storage: Object.freeze({
      health: fixedInvoke(invoke, "storage.health"),
    }),
  });
}

export function installHaloPreload(
  bridge: ContextBridgePort,
  invoke: IpcInvoke,
  events?: IpcEventPort,
): HaloApi {
  const api = createHaloApi(invoke, events);
  bridge.exposeInMainWorld("halo", api);
  return api;
}

export type { DataOf };
