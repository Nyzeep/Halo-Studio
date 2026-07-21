import { app, BrowserWindow } from "electron";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { getPreloadPath } from "./electronPaths.js";
import { configureElectronRuntime } from "./electronRuntime.js";
import { registerIpcHandlers } from "./ipc.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

configureElectronRuntime(app, {
  isDev: Boolean(process.env.VITE_DEV_SERVER_URL)
});

async function createWindow() {
  const mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 1100,
    minHeight: 720,
    backgroundColor: "#0b0f14",
    title: "Halo Studio",
    webPreferences: {
      preload: getPreloadPath(__dirname),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });

  registerIpcHandlers(mainWindow);

  if (process.env.VITE_DEV_SERVER_URL) {
    await mainWindow.loadURL(process.env.VITE_DEV_SERVER_URL);
  } else {
    await mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"));
  }
}

app.whenReady().then(createWindow);

app.on("window-all-closed", () => {
  if (process.platform !== "darwin") {
    app.quit();
  }
});

app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) {
    void createWindow();
  }
});
