import { execFile } from "node:child_process";

export async function commandExists(command: string): Promise<boolean> {
  const locator = process.platform === "win32" ? "where.exe" : "which";

  return new Promise((resolve) => {
    execFile(locator, [command], { windowsHide: true }, (error) => {
      resolve(!error);
    });
  });
}

export async function readVersion(command: string, args: string[]): Promise<string | null> {
  return new Promise((resolve) => {
    execFile(command, args, { timeout: 5000, windowsHide: true }, (error, stdout, stderr) => {
      if (error) {
        resolve(null);
        return;
      }

      const output = `${stdout}${stderr}`.trim();
      resolve(output.length > 0 ? output.split(/\r?\n/)[0] ?? output : null);
    });
  });
}
