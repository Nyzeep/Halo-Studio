import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "win32") {
  console.error("Windows smoke must run on Windows.");
  process.exitCode = 1;
} else {
  const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
  const run = (command, args) => {
    const child = spawn(command, args, {
      cwd: repositoryRoot,
      stdio: "inherit",
      windowsHide: true,
    });
    return new Promise((resolve) => {
      child.once("error", () => resolve(1));
      child.once("exit", (code) => resolve(code ?? 1));
    });
  };
  const nativePreparationExitCode = await run(process.execPath, [
    fileURLToPath(new URL("./prepare-native-runtime.mjs", import.meta.url)),
    "node",
  ]);
  if (nativePreparationExitCode !== 0) {
    console.error("Windows smoke failed: unable to restore the current Node ABI for better-sqlite3.");
    process.exitCode = nativePreparationExitCode;
  } else {
    const npmCli = join(dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js");
    const exitCode = await run(process.execPath, [
      npmCli,
      "test",
      "--workspace",
      "@halo-studio/desktop",
      "--",
      "workspace-runtime.integration.test.ts",
    ]);
    if (exitCode !== 0) {
      process.exitCode = exitCode;
    } else {
      console.log("Windows smoke passed: Pi readiness, OpenCode health/version, graceful stop, and temporary cleanup.");
    }
  }
}
