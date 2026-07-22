import { mkdtemp, mkdir, writeFile, rm, symlink, rename } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { TargetRegistry, UnsafeConfigError } from "./targetRegistry.js";
import { isPathWithin } from "@halo-studio/core";

const dirs: string[] = [];
afterEach(async () => { await Promise.all(dirs.splice(0).map((d) => rm(d, { recursive: true, force: true }))); });

describe("target path policy", () => {
  it("keeps Windows case and Unicode compatibility characters exact", () => {
    expect(isPathWithin("C:\\Root\\file", "c:\\Root", "win32")).toBe(false);
    expect(isPathWithin("C:\\Root\\K\\file", "C:\\Root\\K", "win32")).toBe(false);
  });
  it("rejects paths outside the declared root and arbitrary target ids", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-path-中文 space-")); dirs.push(root);
    const outside = await mkdtemp(join(tmpdir(), "halo-outside-")); dirs.push(outside);
    await writeFile(join(root, "settings.jsonc"), "{}\n");
    const registry = new TargetRegistry();
    await expect(registry.register({ scope: "global", owner: "opencode", path: join(outside, "x.jsonc"), format: "jsonc", source: "native", writable: true, allowedRoot: root })).rejects.toBeInstanceOf(UnsafeConfigError);
    expect(() => registry.get("../../etc")).toThrow(UnsafeConfigError);
    const pathWithParentSegment = `${root}${sep}nested${sep}..${sep}settings.jsonc`;
    await expect(registry.register({ scope: "global", owner: "opencode", path: pathWithParentSegment, format: "jsonc", source: "native", writable: true, allowedRoot: root })).rejects.toBeInstanceOf(UnsafeConfigError);
  });

  it("resolves a not-yet-existing target under an existing ancestor", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-path-")); dirs.push(root); await mkdir(join(root, "nested"));
    const registry = new TargetRegistry();
    const id = await registry.register({ scope: "project", owner: "opencode", path: join(root, "nested", "new.jsonc"), format: "jsonc", source: "managed", writable: true, allowedRoot: root });
    expect(id).toMatch(/^[0-9a-f-]{36}$/);
  });

  it("rejects a missing target whose nearest existing ancestor is a file", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-file-ancestor-")); dirs.push(root);
    const fileAncestor = join(root, "not-a-directory"); await writeFile(fileAncestor, "x", "utf8");
    const registry = new TargetRegistry();
    await expect(registry.register({ scope: "project", owner: "opencode", path: join(fileAncestor, "settings.jsonc"), format: "jsonc", source: "native", writable: true, allowedRoot: root })).rejects.toBeInstanceOf(UnsafeConfigError);
  });

  it("rejects link escape and a parent replaced after registration", async (context) => {
    const root = await mkdtemp(join(tmpdir(), "halo-race-中文 space-")); dirs.push(root);
    const outside = await mkdtemp(join(tmpdir(), "halo-race-outside-")); dirs.push(outside);
    await writeFile(join(outside, "settings.jsonc"), "{}\n");
    const link = join(root, "linked");
    try { await symlink(outside, link, process.platform === "win32" ? "junction" : "dir"); } catch (error) {
      const code = typeof error === "object" && error !== null && "code" in error ? String(error.code) : "";
      if (process.platform === "win32" && (code === "EPERM" || code === "EACCES")) { context.skip(); return; }
      throw error;
    }
    const registry = new TargetRegistry();
    await expect(registry.register({ scope: "project", owner: "opencode", path: join(link, "settings.jsonc"), format: "jsonc", source: "native", writable: true, allowedRoot: root })).rejects.toBeInstanceOf(UnsafeConfigError);

    const parent = join(root, "parent"); await mkdir(parent); const target = join(parent, "settings.jsonc"); await writeFile(target, "{}\n");
    const id = await registry.register({ scope: "project", owner: "opencode", path: target, format: "jsonc", source: "native", writable: true, allowedRoot: root });
    await rename(parent, join(root, "original-parent"));
    await symlink(outside, parent, process.platform === "win32" ? "junction" : "dir");
    await expect(registry.verify(id)).rejects.toBeInstanceOf(UnsafeConfigError);
  });
});
