import type { IpcEnvelope } from "@halo-studio/contracts";
import type { HaloApi } from "../preload/preload.js";

export type WorkbenchApi = Pick<HaloApi, "workspace" | "runtime">;

class PublicIpcError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PublicIpcError";
  }
}

export function defaultWorkbenchApi(): WorkbenchApi | undefined {
  return typeof window === "undefined" ? undefined : window.halo;
}

export function unwrapEnvelope<T>(response: IpcEnvelope<T>): T {
  if (response.ok) return response.data;
  throw new PublicIpcError(response.error.message);
}

export function publicRequestMessage(error: unknown): string {
  return error instanceof PublicIpcError ? error.message : "桌面桥接不可用。";
}
