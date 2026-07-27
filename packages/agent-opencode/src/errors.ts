import type { AppErrorCode } from "@halo-studio/contracts";

const messages: Record<AppErrorCode, string> = {
  RuntimeUnavailable: "OpenCode runtime unavailable.",
  VersionMismatch: "Unsupported OpenCode version.",
  AuthenticationFailed: "OpenCode authentication failed.",
  PermissionRequired: "OpenCode permission required.",
  WorkspaceUntrusted: "Workspace is untrusted.",
  TransportDisconnected: "OpenCode transport disconnected.",
  ProtocolViolation: "Invalid OpenCode protocol.",
  ConfigConflict: "OpenCode configuration conflict.",
  UnsafePath: "OpenCode path is unsafe.",
  MigrationFailed: "OpenCode migration failed.",
};

export class OpenCodeError extends Error {
  readonly code: AppErrorCode;
  readonly retryable: boolean;

  constructor(code: AppErrorCode, message = messages[code], retryable = false) {
    super(message);
    this.name = "OpenCodeError";
    this.code = code;
    this.retryable = retryable;
  }
}

export class AuthenticationFailedError extends OpenCodeError {
  constructor() { super("AuthenticationFailed"); }
}

export class VersionMismatchError extends OpenCodeError {
  constructor() { super("VersionMismatch"); }
}

export class RuntimeUnavailableError extends OpenCodeError {
  constructor() { super("RuntimeUnavailable"); }
}

export class ProtocolViolationError extends OpenCodeError {
  constructor() { super("ProtocolViolation"); }
}

export class TransportDisconnectedError extends OpenCodeError {
  constructor() { super("TransportDisconnected", messages.TransportDisconnected, true); }
}
