import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import type { DesktopServices } from "./services.js";

export interface DesktopAppPort {
  whenReady(): Promise<void>;
  getPath(name: "userData"): string;
  on(event: string, listener: (...args: any[]) => void): unknown;
  quit(): void;
}

export interface DesktopMainOptions {
  readonly app: DesktopAppPort;
  readonly platform: NodeJS.Platform;
  readonly getWindowCount: () => number;
  readonly createServices: () => Promise<DesktopServices>;
  readonly registerIpc: (services: DesktopServices) => () => void;
  readonly createWindow: () => Promise<unknown>;
}

export interface DesktopLifecycle {
  readonly ready: Promise<void>;
  idle(): Promise<void>;
}

export function startDesktopMain(options: DesktopMainOptions): DesktopLifecycle {
  let services: DesktopServices | undefined;
  let unregisterIpc: (() => void) | undefined;
  let activity = Promise.resolve();
  let shutdownStarted = false;
  let disposed = false;

  const schedule = (operation: () => Promise<void>): Promise<void> => {
    const current = activity.then(operation, operation);
    activity = current.catch(() => undefined);
    return current;
  };

  const ready = options.app.whenReady().then(async () => {
    services = await options.createServices();
    unregisterIpc = options.registerIpc(services);
    await options.createWindow();
  });

  options.app.on("activate", () => {
    void schedule(async () => {
      await ready;
      if (!shutdownStarted && options.getWindowCount() === 0) await options.createWindow();
    });
  });

  options.app.on("window-all-closed", () => {
    if (options.platform !== "darwin") options.app.quit();
  });

  options.app.on("before-quit", (event: { preventDefault(): void }) => {
    if (disposed) return;
    event.preventDefault();
    if (shutdownStarted) return;
    shutdownStarted = true;
    void schedule(async () => {
      await ready.catch(() => undefined);
      unregisterIpc?.();
      unregisterIpc = undefined;
      await services?.dispose().catch(() => undefined);
      services = undefined;
      disposed = true;
      options.app.quit();
    });
  });

  return {
    ready,
    idle: () => activity,
  };
}

async function bootElectron(): Promise<void> {
  const electron = await import("electron");
  const { createDesktopServices } = await import("./services.js");
  const { registerIpcHandlers } = await import("./ipc/registerIpc.js");
  const { createSecureWindow } = await import("./window.js");
  const outputDirectory = dirname(fileURLToPath(import.meta.url));
  const preloadPath = join(outputDirectory, "..", "preload", "preload.cjs");
  const rendererPath = join(outputDirectory, "..", "renderer", "index.html");

  const lifecycle = startDesktopMain({
    app: electron.app,
    platform: process.platform,
    getWindowCount: () => electron.BrowserWindow.getAllWindows().length,
    createServices: () => createDesktopServices({
      userDataPath: electron.app.getPath("userData"),
      picker: {
        showOpenDialog: async (dialogOptions) => electron.dialog.showOpenDialog({
          properties: [...dialogOptions.properties] as Array<"openDirectory">,
        }),
      },
      safeStorage: electron.safeStorage,
      hostEnvironment: { ...process.env },
    }),
    registerIpc: (services) => registerIpcHandlers(electron.ipcMain, services.handlers),
    createWindow: () => createSecureWindow({
      BrowserWindow: electron.BrowserWindow as unknown as import("./window.js").BrowserWindowConstructor,
      preloadPath,
      rendererPath,
    }),
  });
  await lifecycle.ready;
}

if (process.versions.electron !== undefined) {
  void bootElectron().catch(async () => {
    const { app } = await import("electron");
    app.quit();
  });
}
