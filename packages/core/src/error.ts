import type { AppErrorCode } from "@halo-studio/contracts";

export class CoreError extends Error {
  readonly code: AppErrorCode;
  readonly retryable = false;

  constructor(code: AppErrorCode, message: string) {
    super(message);
    this.name = "CoreError";
    this.code = code;
  }
}
