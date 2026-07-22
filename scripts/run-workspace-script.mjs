import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const scriptName = process.argv[2];

if (!scriptName) {
  console.error("Usage: node scripts/run-workspace-script.mjs <script>");
  process.exit(1);
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
  process.exit(0);
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

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

process.exit(result.status ?? 1);
