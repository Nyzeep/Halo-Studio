import { createHash } from "node:crypto";
import path from "node:path";

export type PathPlatform = "win32" | "posix" | (string & {});

function pathApi(platform: PathPlatform): typeof path.win32 {
  return platform === "win32" ? path.win32 : path.posix;
}

export function normalizeFilesystemPath(
  input: string,
  platform: PathPlatform = process.platform,
): string {
  const api = pathApi(platform);
  const normalized = api.normalize(input);
  const root = api.parse(normalized).root;
  let withoutTrailingSeparators = normalized;

  while (
    withoutTrailingSeparators.length > root.length &&
    withoutTrailingSeparators.endsWith(api.sep)
  ) {
    withoutTrailingSeparators = withoutTrailingSeparators.slice(0, -1);
  }

  return withoutTrailingSeparators;
}

export function normalizePathKey(
  input: string,
  platform: PathPlatform = process.platform,
): string {
  const normalized = normalizeFilesystemPath(input, platform);
  return platform === "win32"
    ? normalized.toLocaleLowerCase("en-US")
    : normalized;
}

export function isPathWithin(
  candidateRealPath: string,
  allowedRealRoot: string,
  platform: PathPlatform = process.platform,
): boolean {
  const api = pathApi(platform);
  const candidateKey = normalizePathKey(candidateRealPath, platform);
  const rootKey = normalizePathKey(allowedRealRoot, platform);
  const relativePath = api.relative(rootKey, candidateKey);

  if (relativePath === "") {
    return true;
  }

  const [firstSegment] = relativePath.split(api.sep);
  return firstSegment !== ".." && !api.isAbsolute(relativePath);
}

export function workspaceIdForPath(
  realPath: string,
  platform: PathPlatform = process.platform,
): string {
  return createHash("sha256")
    .update(normalizePathKey(realPath, platform), "utf8")
    .digest("hex");
}

export function resolveUserPath(
  userPath: string,
  cwd: string,
  platform: PathPlatform = process.platform,
): string {
  return normalizeFilesystemPath(pathApi(platform).resolve(cwd, userPath), platform);
}
