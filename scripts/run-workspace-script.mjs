import { spawnSync } from "node:child_process";
import { existsSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const signalExitCodes = new Map([
  ["SIGINT", 130],
  ["SIGTERM", 143],
]);

export function normalizeSpawnResult(
  result,
  report = (message) => console.error(message),
) {
  if (result.error) {
    report(result.error.message);
    return 1;
  }

  if (Number.isInteger(result.status)) {
    return result.status;
  }

  if (result.signal) {
    const exitCode = signalExitCodes.get(result.signal) ?? 1;
    report(
      `Workspace script terminated by ${result.signal}; exiting with code ${exitCode}.`,
    );
    return exitCode;
  }

  report("Workspace script ended without an exit status or signal.");
  return 1;
}

function run() {
  const scriptName = process.argv[2];

  if (!scriptName) {
    console.error("Usage: node scripts/run-workspace-script.mjs <script>");
    return 1;
  }

  const workspaceRoots = ["apps", "packages"];
  const hasWorkspaces = workspaceRoots.some((root) => {
    if (!existsSync(root)) {
      return false;
    }

    return readdirSync(root, { withFileTypes: true }).some(
      (entry) => entry.isDirectory() && existsSync(join(root, entry.name, "package.json")),
    );
  });

  if (!hasWorkspaces) {
    console.log(`No workspaces found; skipping "${scriptName}".`);
    return 0;
  }

  const npmExecPath = process.env.npm_execpath;
  const command = npmExecPath
    ? process.execPath
    : process.platform === "win32"
      ? "npm.cmd"
      : "npm";
  const args = npmExecPath
    ? [npmExecPath, "run", scriptName, "--workspaces", "--if-present"]
    : ["run", scriptName, "--workspaces", "--if-present"];
  const result = spawnSync(command, args, { stdio: "inherit" });

  return normalizeSpawnResult(result);
}

const entryPoint = process.argv[1]
  ? pathToFileURL(resolve(process.argv[1])).href
  : undefined;

if (entryPoint === import.meta.url) {
  process.exitCode = run();
}
