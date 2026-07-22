import { createRequire } from "node:module";
import { realpath, stat, readFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { OPENCODE_VERSION } from "./health.js";
import { RuntimeUnavailableError, VersionMismatchError } from "./errors.js";

export interface OpenCodeArtifact {
  readonly executable: string;
  readonly version: typeof OPENCODE_VERSION;
}

export interface ArtifactOptions {
  readonly packageRoot?: string;
  readonly platform?: NodeJS.Platform;
  readonly arch?: string;
}

function candidates(root: string, platform: NodeJS.Platform, arch: string): string[] {
  const names = platform === "win32"
    ? ["opencode.exe", "opencode-win32-x64.exe", `opencode-${platform}-${arch}.exe`]
    : ["opencode", `opencode-${platform}-${arch}`];
  return names.map((name) => join(root, "bin", name));
}

async function packageRootFromRequire(): Promise<string> {
  const require = createRequire(import.meta.url);
  try {
    const packageJson = require.resolve("opencode-ai/package.json") as string;
    return dirname(packageJson);
  } catch {
    try {
      const entry = require.resolve("opencode-ai") as string;
      let current = dirname(entry);
      for (let i = 0; i < 5; i += 1) {
        try { await readFile(join(current, "package.json"), "utf8"); return current; } catch { current = dirname(current); }
      }
    } catch { /* handled below */ }
  }
  throw new RuntimeUnavailableError();
}

export async function resolveOpenCodeArtifact(options: ArtifactOptions = {}): Promise<OpenCodeArtifact> {
  const root = resolve(options.packageRoot ?? await packageRootFromRequire());
  let packageData: unknown;
  try { packageData = JSON.parse(await readFile(join(root, "package.json"), "utf8")) as unknown; } catch { throw new RuntimeUnavailableError(); }
  const version = typeof packageData === "object" && packageData !== null && "version" in packageData && typeof packageData.version === "string" ? packageData.version : undefined;
  if (version !== OPENCODE_VERSION) throw new VersionMismatchError();
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  for (const candidate of candidates(root, platform, arch)) {
    try {
      const info = await stat(candidate);
      if (!info.isFile()) continue;
      const executable = await realpath(candidate);
      const rootReal = await realpath(root);
      const separator = platform === "win32" ? "\\" : "/";
      const executableKey = platform === "win32" ? executable.toLowerCase() : executable;
      const rootKey = platform === "win32" ? rootReal.toLowerCase() : rootReal;
      if (!executableKey.startsWith(`${rootKey}${separator}`)) continue;
      return { executable, version: OPENCODE_VERSION };
    } catch { /* try next bundled candidate */ }
  }
  throw new RuntimeUnavailableError();
}

export const resolveBundledOpenCodeExecutable = resolveOpenCodeArtifact;
export const resolveOpenCodeExecutable = resolveOpenCodeArtifact;
