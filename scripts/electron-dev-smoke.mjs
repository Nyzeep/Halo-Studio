import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { runElectronRendererProbe } from "./run-electron-renderer-probe.mjs";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);
const developmentScript = join(scriptsDirectory, "desktop-dev.mjs");
const readyMarker = "http://127.0.0.1:5173\n";

function waitForExit(child) {
  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
}

if (process.platform !== "win32") {
  console.error("Electron development smoke must run on Windows.");
  process.exitCode = 1;
} else {
  const temporaryDirectory = await mkdtemp(join(tmpdir(), "halo-electron-dev-smoke-"));
  const environment = {
    ...process.env,
    HALO_DESKTOP_DEV_USER_DATA_DIR: join(temporaryDirectory, "user-data"),
  };
  delete environment.ELECTRON_RUN_AS_NODE;

  const child = spawn(process.execPath, [developmentScript, "--smoke"], {
    cwd: repositoryRoot,
    env: environment,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  let output = "";
  child.stdout.on("data", (chunk) => {
    output += chunk;
    process.stdout.write(chunk);
  });
  child.stderr.on("data", (chunk) => {
    output += chunk;
    process.stderr.write(chunk);
  });

  let timedOut = false;
  const timeout = setTimeout(() => {
    timedOut = true;
    child.kill("SIGTERM");
  }, 60_000);

  try {
    const result = await waitForExit(child);
    const markerPath = join(temporaryDirectory, "user-data", "halo-desktop-dev-smoke-ready");
    const marker = await readFile(markerPath, "utf8").catch(() => undefined);
    if (timedOut || result.code !== 0 || marker !== readyMarker) {
      const exit = result.code === null ? result.signal ?? "unknown" : result.code;
      const markerState = marker === undefined ? "missing" : `unexpected: ${marker.trim()}`;
      const probe = await runElectronRendererProbe();
      const probeExit = probe.exit.code === null ? probe.exit.signal ?? "unknown" : probe.exit.code;
      const probeMarker = probe.marker?.trim() ?? "missing";
      if (probe.diagnostics !== "") process.stderr.write(probe.diagnostics);
      console.error(`Electron development smoke failed: Vite or Electron did not reach the local development window (exit ${exit}; marker ${markerState}; sandboxed renderer probe exit ${probeExit}, marker ${probeMarker}${probe.timedOut ? ", timed out" : ""}).`);
      process.exitCode = 1;
    } else {
      console.log("Electron development smoke passed: Vite served the renderer and Electron loaded the loopback development window.");
    }
  } finally {
    clearTimeout(timeout);
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}
