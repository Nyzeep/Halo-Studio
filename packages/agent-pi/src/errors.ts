import type { AppErrorCode } from "@halo-studio/contracts";

const messages: Record<AppErrorCode, string> = {
  RuntimeUnavailable: "Pi runtime unavailable.",
  VersionMismatch: "Unsupported Pi version.",
  AuthenticationFailed: "Pi authentication failed.",
  PermissionRequired: "Pi permission required.",
  WorkspaceUntrusted: "Workspace is untrusted.",
  TransportDisconnected: "Pi transport disconnected.",
  ProtocolViolation: "Invalid Pi RPC protocol.",
  ConfigConflict: "Pi configuration conflict.",
  UnsafePath: "Unsafe workspace path.",
  MigrationFailed: "Pi migration failed.",
};

export class PiError extends Error {
  readonly code: AppErrorCode;
  readonly retryable = false;

  constructor(code: AppErrorCode, message = messages[code]) {
    super(message);
    this.name = "PiError";
    this.code = code;
  }
}

export class TransportDisconnectedError extends PiError {
  constructor() { super("TransportDisconnected"); }
}

export class ProtocolViolationError extends PiError {
  constructor() { super("ProtocolViolation"); }
}

export class RuntimeUnavailableError extends PiError {
  constructor() { super("RuntimeUnavailable"); }
}

export class VersionMismatchError extends PiError {
  constructor() { super("VersionMismatch"); }
}
