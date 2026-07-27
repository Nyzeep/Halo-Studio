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

/** Creates a stable key from a canonical path returned by fs.realpath. */
export function normalizePathKey(
  input: string,
  platform: PathPlatform = process.platform,
): string {
  // JavaScript case folding can collapse distinct filesystem names.
  return normalizeFilesystemPath(input, platform);
}

/** Compares two canonical fs.realpath results using exact path segments. */
export function isPathWithin(
  candidateRealPath: string,
  allowedRealRoot: string,
  platform: PathPlatform = process.platform,
): boolean {
  const api = pathApi(platform);
  const candidateKey = normalizePathKey(candidateRealPath, platform);
  const rootKey = normalizePathKey(allowedRealRoot, platform);
  const candidateRoot = api.parse(candidateKey).root;
  const allowedRoot = api.parse(rootKey).root;

  if (candidateRoot !== allowedRoot) {
    return false;
  }

  const candidateSegments = candidateKey
    .slice(candidateRoot.length)
    .split(api.sep)
    .filter((segment) => segment.length > 0);
  const allowedSegments = rootKey
    .slice(allowedRoot.length)
    .split(api.sep)
    .filter((segment) => segment.length > 0);

  return (
    allowedSegments.length <= candidateSegments.length &&
    allowedSegments.every(
      (segment, index) => segment === candidateSegments[index],
    )
  );
}

/** Derives an id from a canonical path returned by fs.realpath. */
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
