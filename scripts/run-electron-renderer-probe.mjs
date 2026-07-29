import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);
const require = createRequire(import.meta.url);
const probeEntry = join(scriptsDirectory, "electron-renderer-probe.cjs");

function waitForExit(child) {
  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
}

export async function runElectronRendererProbe() {
  const temporaryDirectory = await mkdtemp(join(tmpdir(), "halo-electron-renderer-probe-"));
  const markerPath = join(temporaryDirectory, "result.json");
  const environment = {
    ...process.env,
    HALO_ELECTRON_PROBE_RESULT: markerPath,
  };
  delete environment.ELECTRON_RUN_AS_NODE;
  const child = spawn(require("electron"), [
    "--headless",
    "--disable-gpu",
    "--use-angle=swiftshader",
    "--enable-logging=stderr",
    `--user-data-dir=${join(temporaryDirectory, "user-data")}`,
    probeEntry,
  ], {
    cwd: repositoryRoot,
    env: environment,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  let diagnostics = "";
  child.stdout.on("data", (chunk) => { diagnostics += chunk; });
  child.stderr.on("data", (chunk) => { diagnostics += chunk; });
  let timedOut = false;
  const timeout = setTimeout(() => {
    timedOut = true;
    child.kill("SIGTERM");
  }, 15_000);

  try {
    const exit = await waitForExit(child);
    const marker = await readFile(markerPath, "utf8").catch(() => undefined);
    return {
      timedOut,
      exit,
      marker,
      diagnostics,
    };
  } finally {
    clearTimeout(timeout);
    await rm(temporaryDirectory, { recursive: true, force: true });
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const result = await runElectronRendererProbe();
  process.stdout.write(`${JSON.stringify(result)}\n`);
  process.exitCode = result.marker === '{"state":"loaded"}\n' ? 0 : 1;
}
