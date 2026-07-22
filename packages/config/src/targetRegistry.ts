import { realpath, stat } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { dirname, isAbsolute, normalize } from "node:path";
import { isPathWithin } from "@halo-studio/core";

export type ConfigScope = "global" | "project";
export type ConfigOwner = "pi" | "opencode";
export type ConfigFormat = "jsonc";
export type ConfigSource = "native" | "managed";
export type ConfigTargetKind = "config" | "mcp";

export interface TargetRegistration {
  readonly scope: ConfigScope;
  readonly owner: ConfigOwner;
  readonly kind?: ConfigTargetKind;
  readonly path: string;
  readonly format: ConfigFormat;
  readonly source: ConfigSource;
  readonly writable: boolean;
  readonly allowedRoot: string;
}

export interface ConfigTarget extends TargetRegistration {
  readonly targetId: string;
  readonly path: string;
  readonly allowedRoot: string;
}

export class UnsafeConfigError extends Error {
  readonly code = "UnsafePath" as const;
  constructor() { super("Unsafe configuration target"); this.name = "UnsafeConfigError"; }
}

function reject(): never { throw new UnsafeConfigError(); }
function validString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && !value.includes("\0");
}
function hasParentSegment(value: string): boolean {
  return value.split(/[\\/]/u).some((part) => part === "..");
}
function isInside(child: string, root: string): boolean {
  return isPathWithin(child, root, process.platform === "win32" ? "win32" : "posix");
}
function filesystemErrorCode(error: unknown): string | undefined {
  if (typeof error !== "object" || error === null) return undefined;
  const descriptor = Object.getOwnPropertyDescriptor(error, "code");
  return descriptor !== undefined && "value" in descriptor && typeof descriptor.value === "string"
    ? descriptor.value
    : undefined;
}
function isMissingPathError(error: unknown): boolean {
  const code = filesystemErrorCode(error);
  return code === "ENOENT" || code === "ENOTDIR";
}

export class TargetRegistry {
  readonly #targets = new Map<string, ConfigTarget>();

  async register(input: TargetRegistration): Promise<string> {
    if (!validString(input.path) || !validString(input.allowedRoot) || !isAbsolute(input.path) || !isAbsolute(input.allowedRoot)) reject();
    if (hasParentSegment(input.path) || hasParentSegment(input.allowedRoot)) reject();
    if ((input.owner !== "pi" && input.owner !== "opencode") || (input.scope !== "global" && input.scope !== "project") || input.format !== "jsonc" || (input.source !== "native" && input.source !== "managed") || typeof input.writable !== "boolean") reject();
    if (input.kind === "mcp" && input.owner === "pi") reject();
    if (input.kind !== undefined && input.kind !== "config" && input.kind !== "mcp") reject();
    const rootReal = await this.#realExisting(input.allowedRoot);
    const targetReal = await this.#realOrAncestor(input.path);
    if (!isInside(targetReal, rootReal)) reject();
    const targetId = randomUUID();
    this.#targets.set(targetId, { ...input, targetId, allowedRoot: rootReal });
    return targetId;
  }

  get(targetId: string): ConfigTarget {
    if (typeof targetId !== "string" || !/^[0-9a-f-]{36}$/u.test(targetId)) reject();
    const target = this.#targets.get(targetId);
    if (target === undefined) reject();
    return { ...target };
  }

  async verify(targetId: string): Promise<ConfigTarget> {
    const target = this.get(targetId);
    const rootReal = await this.#realExisting(target.allowedRoot);
    const current = await this.#realOrAncestor(target.path);
    if (!isInside(current, rootReal)) reject();
    if (await this.#exists(target.path)) {
      const targetReal = await realpath(target.path).catch(() => reject());
      if (!isInside(targetReal, rootReal)) reject();
      const parentReal = await realpath(dirname(target.path)).catch(() => reject());
      if (!isInside(parentReal, rootReal)) reject();
    }
    return target;
  }

  async read(targetId: string): Promise<{ target: ConfigTarget; text: string; exists: boolean }> {
    const target = await this.verify(targetId);
    const { readFile } = await import("node:fs/promises");
    try {
      return { target, text: await readFile(target.path, "utf8"), exists: true };
    } catch (error) {
      if (filesystemErrorCode(error) === "ENOENT") {
        await this.verify(targetId);
        return { target, text: "{}\n", exists: false };
      }
      reject();
    }
  }

  async verifyWritable(targetId: string): Promise<ConfigTarget> {
    const target = await this.verify(targetId);
    if (!target.writable) reject();
    return target;
  }

  async #realExisting(path: string): Promise<string> {
    try { const info = await stat(path); if (!info.isDirectory()) reject(); return await realpath(path); } catch { reject(); }
  }

  async #realOrAncestor(path: string): Promise<string> {
    let current = normalize(path);
    let isOriginalTarget = true;
    while (true) {
      try {
        const resolved = await realpath(current);
        if (!isOriginalTarget && !(await stat(resolved)).isDirectory()) reject();
        return resolved;
      } catch (error) {
        if (!isMissingPathError(error)) reject();
        const parent = dirname(current);
        if (parent === current) reject();
        current = parent;
        isOriginalTarget = false;
      }
    }
  }

  async #exists(path: string): Promise<boolean> {
    try { await stat(path); return true; }
    catch (error) { if (isMissingPathError(error)) return false; reject(); }
  }
}

export interface DefaultConfigTargetPaths {
  readonly piGlobal: string;
  readonly piProject: string;
  readonly opencodeGlobal: string;
  readonly opencodeProject: string;
  readonly allowedRoot: string;
}

/** Registers the four supported native/managed JSONC targets; callers retain only ids. */
export async function registerDefaultConfigTargets(registry: TargetRegistry, paths: DefaultConfigTargetPaths): Promise<Record<"piGlobal" | "piProject" | "opencodeGlobal" | "opencodeProject", string>> {
  const [piGlobal, piProject, opencodeGlobal, opencodeProject] = await Promise.all([
    registry.register({ scope: "global", owner: "pi", path: paths.piGlobal, format: "jsonc", source: "native", writable: true, allowedRoot: paths.allowedRoot }),
    registry.register({ scope: "project", owner: "pi", path: paths.piProject, format: "jsonc", source: "native", writable: true, allowedRoot: paths.allowedRoot }),
    registry.register({ scope: "global", owner: "opencode", path: paths.opencodeGlobal, format: "jsonc", source: "native", writable: true, allowedRoot: paths.allowedRoot }),
    registry.register({ scope: "project", owner: "opencode", path: paths.opencodeProject, format: "jsonc", source: "native", writable: true, allowedRoot: paths.allowedRoot }),
  ]);
  return { piGlobal, piProject, opencodeGlobal, opencodeProject };
}
