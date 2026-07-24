import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);
const nativeCacheDirectory = join(repositoryRoot, ".halo-runtime", "native-build-cache");

export function nativeRuntimeConfiguration(runtime, options) {
  if (runtime !== "node" && runtime !== "electron") {
    throw new Error('Usage: node scripts/prepare-native-runtime.mjs <node|electron>');
  }

  const environment = { ...options.baseEnvironment };
  for (const key of [
    "npm_config_runtime",
    "npm_config_target",
    "npm_config_disturl",
    "npm_config_build_from_source",
    "npm_config_devdir",
    "npm_config_nodedir",
  ]) {
    delete environment[key];
  }

  if (runtime === "electron") {
    environment.npm_config_runtime = "electron";
    environment.npm_config_target = options.electronVersion;
    environment.npm_config_disturl = "https://electronjs.org/headers";
    environment.npm_config_build_from_source = "true";
    environment.npm_config_devdir = join(options.cacheDirectory, "node-gyp");
  }
  return environment;
}

function npmInvocation() {
  const npmCli = process.env.npm_execpath
    ?? join(dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
  if (existsSync(npmCli)) {
    return { command: process.execPath, args: [npmCli] };
  }
  return {
    command: process.platform === "win32" ? "npm.cmd" : "npm",
    args: [],
  };
}

function betterSqlitePackage() {
  const packagePath = join(repositoryRoot, "node_modules", "better-sqlite3", "package.json");
  return JSON.parse(readFileSync(packagePath, "utf8"));
}

function electronVersion() {
  const packagePath = join(repositoryRoot, "node_modules", "electron", "package.json");
  return JSON.parse(readFileSync(packagePath, "utf8")).version;
}

function nativeModulePath() {
  return join(
    repositoryRoot,
    "node_modules",
    "better-sqlite3",
    "build",
    "Release",
    "better_sqlite3.node",
  );
}

function nativeCachePath(runtime) {
  const packageVersion = betterSqlitePackage().version;
  const target = runtime === "node"
    ? `node-abi-${process.versions.modules}`
    : `electron-${electronVersion()}`;
  return join(nativeCacheDirectory, `${runtime}-better-sqlite3-${packageVersion}-${target}.node`);
}

function copyToCache(runtime) {
  const source = nativeModulePath();
  if (!existsSync(source)) throw new Error("better-sqlite3 did not produce a native module.");
  mkdirSync(nativeCacheDirectory, { recursive: true });
  copyFileSync(source, nativeCachePath(runtime));
}

function restoreFromCache(runtime) {
  const source = nativeCachePath(runtime);
  if (!existsSync(source)) return false;
  const destination = nativeModulePath();
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
  return true;
}

function hostNodeCanLoadBetterSqlite() {
  const result = spawnSync(process.execPath, [
    "-e",
    "const Database = require('better-sqlite3'); const database = new Database(':memory:'); database.close();",
  ], {
    cwd: repositoryRoot,
    stdio: "ignore",
    windowsHide: true,
  });
  return result.status === 0;
}

function runRebuild(runtime) {
  const npm = npmInvocation();
  const result = spawnSync(
    npm.command,
    [...npm.args, "rebuild", "better-sqlite3", "--workspace", "@halo-studio/storage"],
    {
      cwd: repositoryRoot,
      env: nativeRuntimeConfiguration(runtime, {
        baseEnvironment: process.env,
        cacheDirectory: nativeCacheDirectory,
        electronVersion: electronVersion(),
      }),
      stdio: "inherit",
      windowsHide: true,
    },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`better-sqlite3 ${runtime} ABI preparation failed (exit ${result.status ?? "unknown"}).`);
  }
}

function prepareNodeRuntime() {
  if (hostNodeCanLoadBetterSqlite()) {
    copyToCache("node");
    console.log("better-sqlite3 already uses the current Node ABI.");
    return;
  }
  if (restoreFromCache("node") && hostNodeCanLoadBetterSqlite()) {
    console.log("Restored better-sqlite3 for the current Node ABI from the local cache.");
    return;
  }
  console.log("Rebuilding better-sqlite3 for the current Node ABI.");
  runRebuild("node");
  if (!hostNodeCanLoadBetterSqlite()) {
    throw new Error("better-sqlite3 was rebuilt but cannot be loaded by the current Node ABI.");
  }
  copyToCache("node");
}

function prepareElectronRuntime() {
  if (restoreFromCache("electron")) {
    console.log(`Restored better-sqlite3 for Electron ${electronVersion()} from the local cache.`);
    return;
  }
  if (hostNodeCanLoadBetterSqlite()) copyToCache("node");
  console.log(`Rebuilding better-sqlite3 for Electron ${electronVersion()}.`);
  runRebuild("electron");
  copyToCache("electron");
}

function run() {
  const runtime = process.argv[2];
  if (runtime === "node") {
    prepareNodeRuntime();
  } else if (runtime === "electron") {
    prepareElectronRuntime();
  } else {
    throw new Error('Usage: node scripts/prepare-native-runtime.mjs <node|electron>');
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  try {
    run();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
