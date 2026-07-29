import { mkdtemp, mkdir, writeFile, rm, symlink, rename } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, sep } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { registerDefaultConfigTargets, setTargetRegistryTestHooks, TargetRegistry, UnsafeConfigError, type TargetRegistration } from "./targetRegistry.js";
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

  it.each(["afterOpen", "beforePostValidate"] as const)(
    "rejects a parent replaced outside the root at the %s read stage",
    async (stage, context) => {
      const root = await mkdtemp(join(tmpdir(), "halo-read-race-中文 space-")); dirs.push(root);
      const outside = await mkdtemp(join(tmpdir(), "halo-read-outside-")); dirs.push(outside);
      const parent = join(root, "parent"); await mkdir(parent);
      const file = join(parent, "settings.jsonc"); await writeFile(file, "{\n  \"inside\": true\n}\n", "utf8");
      await writeFile(join(outside, "settings.jsonc"), "{\n  \"secret\": \"OUTSIDE-READ-CANARY\"\n}\n", "utf8");
      let replaced = false;
      const replaceParent = async (): Promise<void> => {
        if (replaced) return;
        replaced = true;
        await rename(parent, join(root, "original-parent"));
        try { await symlink(outside, parent, process.platform === "win32" ? "junction" : "dir"); } catch (error) {
          const code = typeof error === "object" && error !== null && "code" in error ? String(error.code) : "";
          if (process.platform === "win32" && (code === "EPERM" || code === "EACCES")) { context.skip(); return; }
          throw error;
        }
      };
      const registry = new TargetRegistry();
      setTargetRegistryTestHooks(registry, { [stage]: replaceParent });
      const id = await registry.register({ scope: "project", owner: "opencode", path: file, format: "jsonc", source: "native", writable: true, allowedRoot: root });
      let error: unknown;
      try { await registry.read(id); } catch (caught) { error = caught; }
      expect(error).toBeInstanceOf(UnsafeConfigError);
      expect(String(error)).not.toContain("OUTSIDE-READ-CANARY");
      expect(String(error)).not.toContain(outside);
    },
  );

  it("rejects configuration files larger than one MiB", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-read-limit-")); dirs.push(root);
    const file = join(root, "settings.jsonc");
    await writeFile(file, `{"payload":"${"x".repeat(1024 * 1024)}"}\n`, "utf8");
    const registry = new TargetRegistry();
    const id = await registry.register({ scope: "global", owner: "pi", path: file, format: "jsonc", source: "native", writable: true, allowedRoot: root });
    await expect(registry.read(id)).rejects.toBeInstanceOf(UnsafeConfigError);
  });

  it("snapshots every registration field exactly once and freezes returned targets", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-registration-")); dirs.push(root);
    const values: TargetRegistration = {
      scope: "project",
      owner: "opencode",
      kind: "config",
      path: join(root, "settings.jsonc"),
      format: "jsonc",
      source: "managed",
      writable: true,
      allowedRoot: root,
    };
    const counts = new Map<keyof TargetRegistration, number>();
    const input = {} as TargetRegistration;
    for (const field of ["scope", "owner", "kind", "path", "format", "source", "writable", "allowedRoot"] as const) {
      Object.defineProperty(input, field, {
        enumerable: true,
        get: () => {
          counts.set(field, (counts.get(field) ?? 0) + 1);
          return values[field];
        },
      });
    }
    const registry = new TargetRegistry();
    const id = await registry.register(input);
    for (const count of counts.values()) expect(count).toBe(1);
    const target = registry.get(id);
    expect(Object.isFrozen(target)).toBe(true);
    expect(() => { (target as { path: string }).path = join(root, "mutated.jsonc"); }).toThrow(TypeError);
    expect(registry.get(id).path).toBe(values.path);
  });

  it("maps registration proxy and getter failures to the fixed unsafe error", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-registration-error-")); dirs.push(root);
    const canary = `${root} REGISTRATION-GETTER-CANARY`;
    const input = new Proxy({
      scope: "project",
      owner: "opencode",
      path: join(root, "settings.jsonc"),
      format: "jsonc",
      source: "managed",
      writable: true,
      allowedRoot: root,
    } as TargetRegistration, {
      get: (target, property, receiver) => {
        if (property === "path") throw new Error(canary);
        return Reflect.get(target, property, receiver);
      },
    });
    let error: unknown;
    try { await new TargetRegistry().register(input); } catch (caught) { error = caught; }
    expect(error).toBeInstanceOf(UnsafeConfigError);
    expect(String(error)).toBe("UnsafeConfigError: Unsafe configuration target");
    expect(String(error)).not.toContain(canary);
  });

  it("registers four default targets under independent roots", async () => {
    const roots = await Promise.all(["pi-global", "pi-project", "opencode-global", "opencode-project"].map(async (name) => {
      const root = await mkdtemp(join(tmpdir(), `halo-default-${name}-`)); dirs.push(root);
      const nested = join(root, "中文 space"); await mkdir(nested);
      return nested;
    }));
    const registry = new TargetRegistry();
    const ids = await registerDefaultConfigTargets(registry, {
      piGlobal: { path: join(roots[0]!, "settings.jsonc"), allowedRoot: roots[0]! },
      piProject: { path: join(roots[1]!, "settings.jsonc"), allowedRoot: roots[1]! },
      opencodeGlobal: { path: join(roots[2]!, "settings.jsonc"), allowedRoot: roots[2]! },
      opencodeProject: { path: join(roots[3]!, "settings.jsonc"), allowedRoot: roots[3]! },
    });
    expect(registry.get(ids.piGlobal).allowedRoot).toBe(roots[0]);
    expect(registry.get(ids.piProject).allowedRoot).toBe(roots[1]);
    expect(registry.get(ids.opencodeGlobal).allowedRoot).toBe(roots[2]);
    expect(registry.get(ids.opencodeProject).allowedRoot).toBe(roots[3]);
  });
});
