import type { HaloApi } from "./preload.js";

declare global {
  interface Window {
    readonly halo: HaloApi;
  }
}

export {};
