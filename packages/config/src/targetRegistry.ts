import { constants, type BigIntStats } from "node:fs";
import { lstat, open, realpath, stat, type FileHandle } from "node:fs/promises";
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

interface TargetRegistryReadHooks {
  readonly afterOpen?: () => Promise<void>;
  readonly beforePostValidate?: () => Promise<void>;
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

export const MAX_CONFIG_BYTES = 1024 * 1024;

interface FileIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
  readonly size: bigint;
  readonly mtimeNs: bigint;
  readonly ctimeNs: bigint;
  readonly nlink: bigint;
}

function fileIdentity(info: BigIntStats): FileIdentity {
  if (!info.isFile() || info.nlink !== 1n || info.size > BigInt(MAX_CONFIG_BYTES)) reject();
  return {
    dev: info.dev,
    ino: info.ino,
    size: info.size,
    mtimeNs: info.mtimeNs,
    ctimeNs: info.ctimeNs,
    nlink: info.nlink,
  };
}

function sameFile(left: FileIdentity, right: FileIdentity): boolean {
  return left.dev === right.dev
    && left.ino === right.ino
    && left.size === right.size
    && left.mtimeNs === right.mtimeNs
    && left.ctimeNs === right.ctimeNs
    && left.nlink === right.nlink;
}

function snapshotRegistration(input: TargetRegistration): TargetRegistration {
  try {
    const scope = input.scope;
    const owner = input.owner;
    const kind = input.kind;
    const path = input.path;
    const format = input.format;
    const source = input.source;
    const writable = input.writable;
    const allowedRoot = input.allowedRoot;
    const required = { scope, owner, path, format, source, writable, allowedRoot };
    return kind === undefined ? required : { ...required, kind };
  } catch {
    return reject();
  }
}

export class TargetRegistry {
  readonly #targets = new Map<string, ConfigTarget>();

  constructor() {}

  async register(input: TargetRegistration): Promise<string> {
    const registration = snapshotRegistration(input);
    if (!validString(registration.path) || !validString(registration.allowedRoot) || !isAbsolute(registration.path) || !isAbsolute(registration.allowedRoot)) reject();
    if (hasParentSegment(registration.path) || hasParentSegment(registration.allowedRoot)) reject();
    if ((registration.owner !== "pi" && registration.owner !== "opencode") || (registration.scope !== "global" && registration.scope !== "project") || registration.format !== "jsonc" || (registration.source !== "native" && registration.source !== "managed") || typeof registration.writable !== "boolean") reject();
    if (registration.kind === "mcp" && registration.owner === "pi") reject();
    if (registration.kind !== undefined && registration.kind !== "config" && registration.kind !== "mcp") reject();
    const rootReal = await this.#realExisting(registration.allowedRoot);
    const targetReal = await this.#realOrAncestor(registration.path);
    if (!isInside(targetReal, rootReal)) reject();
    const targetId = randomUUID();
    this.#targets.set(targetId, Object.freeze({ ...registration, targetId, allowedRoot: rootReal }));
    return targetId;
  }

  get(targetId: string): ConfigTarget {
    if (typeof targetId !== "string" || !/^[0-9a-f-]{36}$/u.test(targetId)) reject();
    const target = this.#targets.get(targetId);
    if (target === undefined) reject();
    return Object.freeze({ ...target });
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
    let handle: FileHandle | undefined;
    try {
      const linkInfo = await lstat(target.path);
      if (linkInfo.isSymbolicLink()) reject();
      const noFollow = typeof constants.O_NOFOLLOW === "number" ? constants.O_NOFOLLOW : 0;
      handle = await open(target.path, constants.O_RDONLY | noFollow);
      const initialHandleIdentity = fileIdentity(await handle.stat({ bigint: true }));
      await targetRegistryTestHooks.get(this)?.afterOpen?.();
      await this.#validateOpenFile(targetId, target.path, initialHandleIdentity);

      const bytes = await this.#readBounded(handle);
      await targetRegistryTestHooks.get(this)?.beforePostValidate?.();
      const finalHandleIdentity = fileIdentity(await handle.stat({ bigint: true }));
      if (!sameFile(initialHandleIdentity, finalHandleIdentity)) reject();
      await this.#validateOpenFile(targetId, target.path, finalHandleIdentity);

      let text: string;
      try { text = new TextDecoder("utf-8", { fatal: true }).decode(bytes); }
      catch { reject(); }
      return { target, text, exists: true };
    } catch (error) {
      if (handle === undefined && isMissingPathError(error)) {
        await this.verify(targetId);
        return { target, text: "{}\n", exists: false };
      }
      return reject();
    } finally {
      await handle?.close().catch(() => undefined);
    }
  }

  async #validateOpenFile(targetId: string, path: string, expected: FileIdentity): Promise<void> {
    const linkInfo = await lstat(path).catch(() => reject());
    if (linkInfo.isSymbolicLink()) reject();
    const pathIdentity = fileIdentity(await stat(path, { bigint: true }).catch(() => reject()));
    if (!sameFile(expected, pathIdentity)) reject();
    await this.verify(targetId);
    const confirmedIdentity = fileIdentity(await stat(path, { bigint: true }).catch(() => reject()));
    if (!sameFile(expected, confirmedIdentity)) reject();
  }

  async #readBounded(handle: FileHandle): Promise<Uint8Array> {
    const buffer = Buffer.allocUnsafe(MAX_CONFIG_BYTES + 1);
    let bytesRead = 0;
    while (bytesRead < buffer.length) {
      const result = await handle.read(buffer, bytesRead, buffer.length - bytesRead, bytesRead);
      if (result.bytesRead === 0) break;
      bytesRead += result.bytesRead;
    }
    if (bytesRead > MAX_CONFIG_BYTES) reject();
    return buffer.subarray(0, bytesRead);
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

const targetRegistryTestHooks = new WeakMap<TargetRegistry, TargetRegistryReadHooks>();

/** Source-module-only deterministic seam; intentionally omitted from the package index. */
export function setTargetRegistryTestHooks(
  registry: TargetRegistry,
  hooks: TargetRegistryReadHooks,
): void {
  targetRegistryTestHooks.set(registry, hooks);
}

export interface DefaultConfigTargetPath {
  readonly path: string;
  readonly allowedRoot: string;
}

export interface DefaultConfigTargetPaths {
  readonly piGlobal: DefaultConfigTargetPath;
  readonly piProject: DefaultConfigTargetPath;
  readonly opencodeGlobal: DefaultConfigTargetPath;
  readonly opencodeProject: DefaultConfigTargetPath;
}

/** Registers the four supported native/managed JSONC targets; callers retain only ids. */
export async function registerDefaultConfigTargets(registry: TargetRegistry, paths: DefaultConfigTargetPaths): Promise<Record<"piGlobal" | "piProject" | "opencodeGlobal" | "opencodeProject", string>> {
  const [piGlobal, piProject, opencodeGlobal, opencodeProject] = await Promise.all([
    registry.register({ scope: "global", owner: "pi", path: paths.piGlobal.path, format: "jsonc", source: "native", writable: true, allowedRoot: paths.piGlobal.allowedRoot }),
    registry.register({ scope: "project", owner: "pi", path: paths.piProject.path, format: "jsonc", source: "native", writable: true, allowedRoot: paths.piProject.allowedRoot }),
    registry.register({ scope: "global", owner: "opencode", path: paths.opencodeGlobal.path, format: "jsonc", source: "native", writable: true, allowedRoot: paths.opencodeGlobal.allowedRoot }),
    registry.register({ scope: "project", owner: "opencode", path: paths.opencodeProject.path, format: "jsonc", source: "native", writable: true, allowedRoot: paths.opencodeProject.allowedRoot }),
  ]);
  return { piGlobal, piProject, opencodeGlobal, opencodeProject };
}
