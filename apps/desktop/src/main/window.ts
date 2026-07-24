export interface NavigationEvent {
  preventDefault(): void;
}

export interface WebContentsPort {
  on(event: "will-navigate" | "will-redirect" | "new-window", listener: (event: NavigationEvent, ...args: unknown[]) => void): unknown;
  setWindowOpenHandler(handler: (details: { readonly url: string }) => { readonly action: "deny" }): void;
  send?(channel: string, ...args: unknown[]): void;
}

export interface BrowserWindowPort {
  readonly webContents: WebContentsPort;
  loadFile(path: string): Promise<void> | void;
  loadURL(url: string): Promise<void> | void;
}

export type BrowserWindowConstructor = new (options: Record<string, unknown>) => BrowserWindowPort;

export interface SecureWindowOptions {
  readonly BrowserWindow: BrowserWindowConstructor;
  readonly preloadPath: string;
  readonly rendererPath: string;
  readonly developmentUrl?: string;
}

function validateDevelopmentUrl(value: string): string {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("Invalid development URL");
  }
  const loopback = url.hostname === "127.0.0.1" || url.hostname === "localhost" || url.hostname === "[::1]";
  if (url.protocol !== "http:" || !loopback || url.username !== "" || url.password !== "") {
    throw new Error("Invalid development URL");
  }
  return url.href.replace(/\/$/u, "");
}

export async function createSecureWindow(options: SecureWindowOptions): Promise<BrowserWindowPort> {
  const window = new options.BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 800,
    minHeight: 600,
    show: true,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
      devTools: false,
      preload: options.preloadPath,
    },
  });

  const prevent = (event: NavigationEvent): void => event.preventDefault();
  window.webContents.on("will-navigate", prevent);
  window.webContents.on("will-redirect", prevent);
  window.webContents.on("new-window", prevent);
  window.webContents.setWindowOpenHandler(() => ({ action: "deny" }));

  if (options.developmentUrl === undefined) {
    await window.loadFile(options.rendererPath);
  } else {
    await window.loadURL(validateDevelopmentUrl(options.developmentUrl));
  }
  return window;
}
