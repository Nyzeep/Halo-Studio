import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";
import * as publicApi from "./index.js";

describe("config package public contract", () => {
  it("exports only the registry and transaction runtime API", () => {
    expect(Object.keys(publicApi).sort()).toEqual([
      "ConfigBackupUnavailable",
      "ConfigConflict",
      "ConfigParseError",
      "ConfigPatchError",
      "ConfigPreviewUnavailable",
      "ConfigRecoveryError",
      "ConfigTransaction",
      "ConfigWriteError",
      "TargetRegistry",
      "UnsafeConfigError",
      "registerDefaultConfigTargets",
    ]);
  });

  it("does not retain the unused atomic-write dependency", async () => {
    const packageJson = JSON.parse(await readFile(new URL("../package.json", import.meta.url), "utf8")) as {
      dependencies?: Record<string, string>;
    };
    const lockfile = await readFile(new URL("../../../package-lock.json", import.meta.url), "utf8");
    expect(packageJson.dependencies).not.toHaveProperty("write-file-atomic");
    expect(lockfile).not.toContain("write-file-atomic");
  });

  it("documents the residual pure-Node replacement risk and prerequisites", async () => {
    const security = await readFile(new URL("../SECURITY.md", import.meta.url), "utf8");
    expect(security).toContain("openat");
    expect(security).toContain("renameat");
    expect(security).toContain("ReplaceFileW");
    expect(security).toContain("exclusive write access");
    expect(security).toContain("POSIX mode");
    expect(security).toContain("Windows ACL");
    expect(security).toContain("opaque `targetId`");
    expect(security).toContain("Renderer must never submit a path");
    expect(security).toContain("backup reference must never be exposed");
    expect(security).toContain("temporary pathname creation");
    expect(security).toContain("temporary pathname cleanup");
    expect(security).toContain("final pathname rename");
    expect(security).toContain("missing-target unlink");
  });
});
