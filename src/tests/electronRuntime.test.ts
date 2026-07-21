import { describe, expect, it, vi } from "vitest";
import { configureElectronRuntime } from "../main/electronRuntime";

describe("configureElectronRuntime", () => {
  it("disables GPU acceleration before the app creates a window", () => {
    const appendSwitch = vi.fn();
    const app = {
      commandLine: { appendSwitch },
      disableHardwareAcceleration: vi.fn(),
      getPath: vi.fn(),
      setPath: vi.fn()
    };

    configureElectronRuntime(app);

    expect(app.disableHardwareAcceleration).toHaveBeenCalledTimes(1);
    expect(appendSwitch).toHaveBeenCalledWith("disable-gpu");
    expect(appendSwitch).toHaveBeenCalledWith("disable-gpu-sandbox");
    expect(appendSwitch).toHaveBeenCalledWith("disable-direct-composition");
    expect(appendSwitch).not.toHaveBeenCalledWith("disable-software-rasterizer");
  });

  it("stores Electron session data inside the project runtime folder during development", () => {
    const app = {
      commandLine: { appendSwitch: vi.fn() },
      disableHardwareAcceleration: vi.fn(),
      getPath: vi.fn(),
      setPath: vi.fn()
    };

    configureElectronRuntime(app, { isDev: true, cwd: "D:\\Halo Studio" });

    expect(app.setPath).toHaveBeenCalledWith("sessionData", "D:\\Halo Studio\\.halo-runtime\\electron-session");
  });
});
