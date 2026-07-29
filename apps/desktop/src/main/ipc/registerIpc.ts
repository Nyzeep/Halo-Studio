import {
  ipcContracts,
  type AppErrorCode,
  type DataOf,
  type InputOf,
  type IpcChannel,
  type IpcContractMap,
  type ResponseOf,
} from "@halo-studio/contracts";

export interface IpcMainPort {
  handle(channel: string, handler: (event: unknown, raw: unknown) => Promise<unknown>): void;
  removeHandler?(channel: string): void;
}

export type IpcServiceMap = {
  readonly [K in IpcChannel]: (input: InputOf<K>) => Promise<DataOf<K>>;
};

const errorMessages: Record<AppErrorCode, string> = {
  RuntimeUnavailable: "Runtime is unavailable.",
  VersionMismatch: "Runtime version mismatch.",
  AuthenticationFailed: "Credential protection failed.",
  PermissionRequired: "Permission is required.",
  WorkspaceUntrusted: "Workspace trust is required.",
  TransportDisconnected: "Runtime connection closed.",
  ProtocolViolation: "Invalid IPC request or response.",
  ConfigConflict: "Configuration changed externally.",
  UnsafePath: "The requested path is not allowed.",
  MigrationFailed: "Storage migration failed.",
};

const actions: Partial<Record<AppErrorCode, string>> = {
  WorkspaceUntrusted: "Review workspace trust before starting a runtime.",
  AuthenticationFailed: "Review credential protection availability.",
  VersionMismatch: "Install the supported runtime version.",
};

const codeSet = new Set<AppErrorCode>(Object.keys(errorMessages) as AppErrorCode[]);

function readErrorCode(error: unknown): AppErrorCode | undefined {
  if (typeof error !== "object" || error === null) return undefined;
  try {
    const descriptor = Object.getOwnPropertyDescriptor(error, "code");
    if (descriptor === undefined || !("value" in descriptor) || typeof descriptor.value !== "string") return undefined;
    return codeSet.has(descriptor.value as AppErrorCode) ? descriptor.value as AppErrorCode : undefined;
  } catch {
    return undefined;
  }
}

type PublicError = Extract<ResponseOf<IpcChannel>, { readonly ok: false }>["error"];

function toPublicError(error: unknown): PublicError {
  const knownCode = readErrorCode(error);
  if (knownCode === undefined) {
    return {
      code: "ProtocolViolation",
      message: "The desktop service could not complete the request.",
      retryable: false,
    };
  }
  const code = knownCode;
  const message = errorMessages[code];
  const action = actions[code];
  return {
    code,
    message,
    retryable: code === "TransportDisconnected" || code === "RuntimeUnavailable" || code === "MigrationFailed",
    ...(action === undefined ? {} : { action }),
  };
}

function failure<K extends IpcChannel>(channel: K, error: unknown): ResponseOf<K> {
  return ipcContracts[channel].response.parse({ ok: false, error: toPublicError(error) }) as ResponseOf<K>;
}

function protocolFailure<K extends IpcChannel>(channel: K): ResponseOf<K> {
  return ipcContracts[channel].response.parse({
    ok: false,
    error: {
      code: "ProtocolViolation",
      message: errorMessages.ProtocolViolation,
      retryable: false,
    },
  }) as ResponseOf<K>;
}

export function registerHandler<K extends keyof IpcContractMap>(
  ipcMain: IpcMainPort,
  channel: K,
  handler: (input: InputOf<K>) => Promise<DataOf<K>>,
): () => void {
  if (!(channel in ipcContracts)) throw new Error("Unsupported IPC channel");
  const contract = ipcContracts[channel];
  ipcMain.handle(channel, async (_event, raw) => {
    try {
      const request = contract.request.safeParse(raw);
      if (!request.success) return protocolFailure(channel);
      const data = await handler(request.data as InputOf<K>);
      const parsedData = contract.data.safeParse(data);
      if (!parsedData.success) return protocolFailure(channel);
      return contract.response.parse({ ok: true, data: parsedData.data }) as ResponseOf<K>;
    } catch (error) {
      return failure(channel, error);
    }
  });
  return () => ipcMain.removeHandler?.(channel);
}

export function registerIpcHandlers(
  ipcMain: IpcMainPort,
  services: IpcServiceMap,
): () => void {
  const unregister: Array<() => void> = [];
  for (const channel of Object.keys(ipcContracts) as IpcChannel[]) {
    const service = services[channel];
    if (typeof service !== "function") throw new Error("IPC service is unavailable");
    const correlatedService = service as unknown as (
      input: InputOf<IpcChannel>,
    ) => Promise<DataOf<IpcChannel>>;
    unregister.push(registerHandler<IpcChannel>(ipcMain, channel, correlatedService));
  }
  return () => {
    for (const remove of unregister.splice(0)) remove();
  };
}

export { toPublicError };
