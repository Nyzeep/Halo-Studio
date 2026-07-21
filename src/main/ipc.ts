import { app, BrowserWindow, ipcMain } from "electron";
import path from "node:path";
import { createAgentRegistry } from "./agents/registry.js";
import { applyConfigWrite, listConfigBackups, rollbackConfigWrite } from "./config/configFileService.js";
import { applyConfirmedConfigWrite, planRealConfigWrite } from "./config/writeGuard.js";
import { createMcpConfigPreviews } from "./mcp/configPreview.js";
import { PtyManager } from "./pty/ptyManager.js";

export function registerIpcHandlers(mainWindow: BrowserWindow) {
  const registry = createAgentRegistry();
  const ptyManager = new PtyManager({
    onData: (sessionId, data) => {
      mainWindow.webContents.send("sessions:data", { sessionId, data });
    },
    onExit: (sessionId, exitCode) => {
      mainWindow.webContents.send("sessions:exit", { sessionId, exitCode });
    }
  });

  ipcMain.handle("agents:detectAll", () => registry.detectAll());
  ipcMain.handle("config:applyDemoWrite", (_event, request) => {
    return applyConfigWrite({
      ...request,
      targetPath: resolveDemoTargetPath(app.getPath("userData"), request.targetPath)
    });
  });
  ipcMain.handle("config:listDemoBackups", (_event, targetPath: string) =>
    listConfigBackups(resolveDemoTargetPath(app.getPath("userData"), targetPath))
  );
  ipcMain.handle("config:planRealWrite", (_event, request) => planRealConfigWrite(request));
  ipcMain.handle("config:applyConfirmedWrite", (_event, request) => applyConfirmedConfigWrite(request));
  ipcMain.handle("config:rollbackWrite", (_event, request) => rollbackConfigWrite(request));
  ipcMain.handle("mcp:previewConfig", (_event, server) => createMcpConfigPreviews(server));
  ipcMain.handle("sessions:start", (_event, request) => ptyManager.start(request));
  ipcMain.handle("sessions:stop", (_event, sessionId: string) => ptyManager.stop(sessionId));
  ipcMain.handle("sessions:write", (_event, sessionId: string, data: string) => ptyManager.write(sessionId, data));
  ipcMain.handle("sessions:resize", (_event, sessionId: string, cols: number, rows: number) =>
    ptyManager.resize(sessionId, cols, rows)
  );
}

function resolveDemoTargetPath(userDataPath: string, requestedTargetPath: string) {
  const previewDir = path.join(userDataPath, "preview-configs");
  const safeName = path.basename(requestedTargetPath).replace(/[^\w.-]/g, "_");
  return path.join(previewDir, safeName || "mcp-preview.txt");
}
