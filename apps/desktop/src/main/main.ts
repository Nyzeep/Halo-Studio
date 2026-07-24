import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { sessionEventChannel, type AgentEventEnvelope } from "@halo-studio/contracts";
import type { DesktopServices } from "./services.js";

export interface DesktopWindowPort {
  readonly webContents: {
    send?(channel: string, ...args: unknown[]): void;
  };
}

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
  readonly createWindow: () => Promise<DesktopWindowPort>;
}

export interface DesktopLifecycle {
  readonly ready: Promise<void>;
  idle(): Promise<void>;
}

const desktopDevelopmentServerUrl = "http://127.0.0.1:5173";
const developmentSmokeMarkerFileName = "halo-desktop-dev-smoke-ready";

/**
 * Development mode is deliberately limited to the Vite server launched by our
 * development command. Any other inherited environment value falls back to
 * the packaged local renderer.
 */
export function resolveDesktopDevelopmentServerUrl(value: string | undefined): string | undefined {
  if (value !== desktopDevelopmentServerUrl && value !== `${desktopDevelopmentServerUrl}/`) {
    return undefined;
  }
  return desktopDevelopmentServerUrl;
}

export function getDesktopDevelopmentSmokeMarkerPath(userDataPath: string): string {
  return join(userDataPath, developmentSmokeMarkerFileName);
}

export function startDesktopMain(options: DesktopMainOptions): DesktopLifecycle {
  let services: DesktopServices | undefined;
  let unregisterIpc: (() => void) | undefined;
  let unsubscribeSessionEvents: (() => void) | undefined;
  let currentWindow: DesktopWindowPort | undefined;
  let activity = Promise.resolve();
  let shutdownStarted = false;
  let disposed = false;

  const schedule = (operation: () => Promise<void>): Promise<void> => {
    const current = activity.then(operation, operation);
    activity = current.catch(() => undefined);
    return current;
  };

  const createWindow = async (): Promise<void> => {
    currentWindow = await options.createWindow();
  };
  const forwardSessionEvent = (event: AgentEventEnvelope): void => {
    try { currentWindow?.webContents.send?.(sessionEventChannel, event); }
    catch { /* A closing renderer must not affect the Main-owned runtime. */ }
  };

  const ready = options.app.whenReady().then(async () => {
    services = await options.createServices();
    unregisterIpc = options.registerIpc(services);
    if (typeof services.subscribeSessionEvents === "function") {
      unsubscribeSessionEvents = services.subscribeSessionEvents(forwardSessionEvent);
    }
    await createWindow();
  });

  options.app.on("activate", () => {
    void schedule(async () => {
      await ready;
      if (!shutdownStarted && options.getWindowCount() === 0) await createWindow();
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
      try {
        await services?.dispose();
      } catch {
        // Keep service ownership intact so a later quit request can retry shutdown.
        shutdownStarted = false;
        return;
      }
      unsubscribeSessionEvents?.();
      unsubscribeSessionEvents = undefined;
      unregisterIpc?.();
      unregisterIpc = undefined;
      currentWindow = undefined;
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
  const developmentUrl = resolveDesktopDevelopmentServerUrl(
    process.env.HALO_DESKTOP_DEV_SERVER_URL,
  );

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
      ...(developmentUrl === undefined ? {} : { developmentUrl }),
    }),
  });
  await lifecycle.ready;
  if (developmentUrl !== undefined && process.env.HALO_DESKTOP_DEV_SMOKE === "1") {
    await writeFile(
      getDesktopDevelopmentSmokeMarkerPath(electron.app.getPath("userData")),
      `${developmentUrl}\n`,
      "utf8",
    );
    console.log(`HALO_DESKTOP_DEV_SMOKE_READY ${developmentUrl}`);
    electron.app.quit();
  }
}

if (process.versions.electron !== undefined) {
  void bootElectron().catch(async (error) => {
    const { app } = await import("electron");
    if (process.env.HALO_DESKTOP_DEV_SMOKE === "1") {
      const userDataPath = app.getPath("userData");
      await mkdir(userDataPath, { recursive: true });
      const reason = error instanceof Error ? error.message : "Unknown startup error.";
      await writeFile(
        getDesktopDevelopmentSmokeMarkerPath(userDataPath),
        `FAILED ${reason}\n`,
        "utf8",
      );
    }
    app.quit();
  });
}
