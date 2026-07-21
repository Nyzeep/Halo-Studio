import path from "node:path";

export function getPreloadPath(baseDir: string) {
  return path.join(baseDir, "preload.mjs");
}
