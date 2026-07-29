import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { describe, expect, it } from "vitest";
import { resolveOpenCodeArtifact } from "./artifact.js";

describe("bundled OpenCode artifact", () => {
  it("accepts the package bin/opencode.exe shim on Linux and Darwin too", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-opencode-artifact-"));
    try {
      await mkdir(join(root, "bin"));
      await writeFile(join(root, "package.json"), JSON.stringify({ name: "opencode-ai", version: "1.18.4" }));
      await writeFile(join(root, "bin", "opencode.exe"), "shim");
      const linux = await resolveOpenCodeArtifact({ packageRoot: root, platform: "linux", arch: "x64" });
      const darwin = await resolveOpenCodeArtifact({ packageRoot: root, platform: "darwin", arch: "x64" });
      expect(linux.executable).toContain("bin");
      expect(darwin.executable).toContain("bin");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it("rejects an executable found only through PATH", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-opencode-package-"));
    const outside = await mkdtemp(join(tmpdir(), "halo-opencode-path-"));
    const originalPath = process.env.PATH;
    try {
      await writeFile(join(root, "package.json"), JSON.stringify({ name: "opencode-ai", version: "1.18.4" }));
      await writeFile(join(outside, "opencode.exe"), "outside");
      process.env.PATH = outside;
      await expect(resolveOpenCodeArtifact({ packageRoot: root, platform: "win32", arch: "x64" }))
        .rejects.toMatchObject({ code: "RuntimeUnavailable" });
    } finally {
      if (originalPath === undefined) delete process.env.PATH;
      else process.env.PATH = originalPath;
      await Promise.all([rm(root, { recursive: true, force: true }), rm(outside, { recursive: true, force: true })]);
    }
  });

  it("rejects a package bin directory whose realpath escapes the package", async (context) => {
    const root = await mkdtemp(join(tmpdir(), "halo-opencode-package-"));
    const outside = await mkdtemp(join(tmpdir(), "halo-opencode-outside-"));
    try {
      await writeFile(join(root, "package.json"), JSON.stringify({ name: "opencode-ai", version: "1.18.4" }));
      await writeFile(join(outside, "opencode.exe"), "outside");
      try {
        await symlink(outside, join(root, "bin"), process.platform === "win32" ? "junction" : "dir");
      } catch (error) {
        const code = (error as NodeJS.ErrnoException).code;
        if (code === "EPERM" || code === "EACCES") {
          context.skip();
          return;
        }
        throw error;
      }
      await expect(resolveOpenCodeArtifact({ packageRoot: root, platform: process.platform, arch: process.arch }))
        .rejects.toMatchObject({ code: "RuntimeUnavailable" });
    } finally {
      await Promise.all([rm(root, { recursive: true, force: true }), rm(outside, { recursive: true, force: true })]);
    }
  });
});
