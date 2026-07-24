const { writeFile } = require("node:fs/promises");
const { app, BrowserWindow } = require("electron");

const resultPath = process.env.HALO_ELECTRON_PROBE_RESULT;

async function record(result) {
  if (resultPath !== undefined) await writeFile(resultPath, `${JSON.stringify(result)}\n`, "utf8");
}

app.whenReady().then(async () => {
  const window = new BrowserWindow({
    show: false,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      webSecurity: true,
    },
  });
  window.webContents.once("render-process-gone", async (_event, details) => {
    await record({ state: "gone", reason: details.reason, exitCode: details.exitCode });
    app.exit(1);
  });
  try {
    await window.loadURL("data:text/html,<title>Halo Electron probe</title>");
    await record({ state: "loaded" });
    app.quit();
  } catch (error) {
    await record({ state: "load-failed", message: error instanceof Error ? error.message : String(error) });
    app.exit(1);
  }
}).catch(async (error) => {
  await record({ state: "startup-failed", message: error instanceof Error ? error.message : String(error) });
  app.exit(1);
});
