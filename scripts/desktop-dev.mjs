import { spawn, spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);
const desktopDirectory = join(repositoryRoot, "apps", "desktop");
const desktopRequire = createRequire(join(desktopDirectory, "package.json"));

export const desktopDevelopmentServerUrl = "http://127.0.0.1:5173";
const developmentHost = "127.0.0.1";
const developmentPort = "5173";

function resolveViteCli() {
  return join(dirname(desktopRequire.resolve("vite/package.json")), "bin", "vite.js");
}

function resolveElectronBinary() {
  return desktopRequire("electron");
}

function runViteBuild(mode) {
  const result = spawnSync(process.execPath, [resolveViteCli(), "build", "--mode", mode], {
    cwd: desktopDirectory,
    stdio: "inherit",
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`Unable to build Electron ${mode} entry point (exit ${result.status ?? "unknown"}).`);
  }
}

function wait(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitForDevelopmentServer(server) {
  const deadline = Date.now() + 30_000;
  let serverError;
  server.once("error", (error) => { serverError = error; });

  while (Date.now() < deadline) {
    if (serverError !== undefined) throw serverError;
    if (server.exitCode !== null) {
      throw new Error(`Vite development server exited before becoming ready (exit ${server.exitCode}).`);
    }
    try {
      const response = await fetch(desktopDevelopmentServerUrl, {
        signal: AbortSignal.timeout(1_000),
      });
      if (response.ok) return;
    } catch {
      // The server has not accepted connections yet.
    }
    await wait(100);
  }
  throw new Error("Timed out waiting for the Vite development server.");
}

function waitForExit(child) {
  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
}

async function stop(child) {
  if (child === undefined || child.exitCode !== null || child.killed) return;
  child.kill("SIGTERM");
  await Promise.race([
    waitForExit(child).catch(() => undefined),
    wait(5_000),
  ]);
}

function exitCodeFor(result) {
  if (result.code !== null) return result.code;
  return result.signal === "SIGINT" ? 130 : result.signal === "SIGTERM" ? 143 : 1;
}

export async function runDesktopDevelopmentSession(options = {}) {
  runViteBuild("main");
  runViteBuild("preload");

  let vite;
  let electron;
  let receivedSignal;
  const requestShutdown = (signal) => {
    receivedSignal ??= signal;
    void Promise.all([stop(electron), stop(vite)]);
  };
  const onInterrupt = () => requestShutdown("SIGINT");
  const onTerminate = () => requestShutdown("SIGTERM");
  process.once("SIGINT", onInterrupt);
  process.once("SIGTERM", onTerminate);

  try {
    vite = spawn(process.execPath, [resolveViteCli(), "--host", developmentHost, "--port", developmentPort, "--strictPort"], {
      cwd: desktopDirectory,
      stdio: "inherit",
      windowsHide: true,
    });
    await waitForDevelopmentServer(vite);

    const environment = {
      ...process.env,
      HALO_DESKTOP_DEV_SERVER_URL: desktopDevelopmentServerUrl,
      ...(options.smoke ? { HALO_DESKTOP_DEV_SMOKE: "1" } : {}),
    };
    const userDataDirectory = process.env.HALO_DESKTOP_DEV_USER_DATA_DIR;
    electron = spawn(
      resolveElectronBinary(),
      [
        ...(options.smoke ? ["--headless", "--disable-gpu", "--use-angle=swiftshader", "--enable-logging=stderr"] : []),
        ...(userDataDirectory === undefined ? [] : [`--user-data-dir=${userDataDirectory}`]),
        desktopDirectory,
      ],
      {
        cwd: repositoryRoot,
        env: environment,
        stdio: "inherit",
        windowsHide: true,
      },
    );

    const firstExit = await Promise.race([
      waitForExit(vite).then((result) => ({ process: "vite", result })),
      waitForExit(electron).then((result) => ({ process: "electron", result })),
    ]);
    if (receivedSignal !== undefined) {
      return receivedSignal === "SIGINT" ? 130 : 143;
    }
    if (firstExit.process === "vite") {
      throw new Error(`Vite development server stopped unexpectedly (exit ${exitCodeFor(firstExit.result)}).`);
    }
    return exitCodeFor(firstExit.result);
  } finally {
    process.removeListener("SIGINT", onInterrupt);
    process.removeListener("SIGTERM", onTerminate);
    await Promise.all([stop(electron), stop(vite)]);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const smoke = process.argv.slice(2).includes("--smoke");
  try {
    process.exitCode = await runDesktopDevelopmentSession({ smoke });
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
