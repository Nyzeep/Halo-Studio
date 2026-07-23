import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
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
});
