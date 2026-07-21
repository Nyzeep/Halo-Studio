import path from "node:path";

interface ElectronRuntimeApp {
  commandLine: {
    appendSwitch(name: string, value?: string): void;
  };
  disableHardwareAcceleration(): void;
  setPath(name: "sessionData", path: string): void;
}

interface ElectronRuntimeOptions {
  cwd?: string;
  isDev?: boolean;
}

export function configureElectronRuntime(
  app: ElectronRuntimeApp,
  options: ElectronRuntimeOptions = {}
) {
  app.disableHardwareAcceleration();
  app.commandLine.appendSwitch("disable-gpu");
  app.commandLine.appendSwitch("disable-gpu-sandbox");
  app.commandLine.appendSwitch("disable-direct-composition");

  if (options.isDev) {
    const runtimeRoot = path.join(options.cwd ?? process.cwd(), ".halo-runtime");
    app.setPath("sessionData", path.join(runtimeRoot, "electron-session"));
  }
}
