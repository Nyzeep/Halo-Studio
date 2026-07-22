import { constants, type BigIntStats } from "node:fs";
import { lstat, open, rename, rm, stat, unlink, type FileHandle } from "node:fs/promises";
import { dirname, join } from "node:path";
import { randomUUID } from "node:crypto";

export interface AtomicWriteGuards {
  readonly beforeCreate: () => Promise<void>;
  readonly beforeRename: () => Promise<void>;
  readonly afterTempSync?: (temporary: string) => Promise<void>;
  readonly syncDirectory?: () => Promise<void>;
}

export type AtomicWriteState = "not-replaced" | "replaced" | "durability-failed";

export class AtomicWriteError extends Error {
  readonly state: AtomicWriteState;
  readonly replaced: boolean;

  constructor(state: AtomicWriteState, replaced: boolean) {
    super("Configuration write failed");
    this.name = "AtomicWriteError";
    this.state = state;
    this.replaced = replaced;
  }
}

interface ObjectIdentity {
  readonly dev: bigint;
  readonly ino: bigint;
}

function identity(info: BigIntStats): ObjectIdentity {
  return { dev: info.dev, ino: info.ino };
}

function sameIdentity(left: ObjectIdentity, right: ObjectIdentity): boolean {
  return left.dev === right.dev && left.ino === right.ino;
}

async function existingMode(path: string): Promise<number | undefined> {
  try {
    const info = await lstat(path);
    if (!info.isFile() || info.isSymbolicLink()) throw new Error("Unsafe target");
    return info.mode & 0o7777;
  } catch (error) {
    if (typeof error === "object" && error !== null && "code" in error && error.code === "ENOENT") return undefined;
    throw error;
  }
}

async function close(handle: FileHandle | undefined): Promise<void> {
  if (handle !== undefined) await handle.close();
}

export interface GuardedRemoveGuards {
  readonly beforeRemove: () => Promise<void>;
  readonly syncDirectory?: () => Promise<void>;
}

export async function guardedRemove(path: string, guards: GuardedRemoveGuards): Promise<void> {
  let parentHandle: FileHandle | undefined;
  let targetHandle: FileHandle | undefined;
  let guardError: unknown;
  let removed = false;
  let durabilityFailed = false;
  try {
    try { await guards.beforeRemove(); } catch (error) { guardError = error; throw error; }
    parentHandle = await open(dirname(path), "r");
    const parentInfo = await parentHandle.stat({ bigint: true });
    if (!parentInfo.isDirectory()) throw new Error("Unsafe parent");
    const parentIdentity = identity(parentInfo);
    const noFollow = typeof constants.O_NOFOLLOW === "number" ? constants.O_NOFOLLOW : 0;
    targetHandle = await open(path, constants.O_RDONLY | noFollow);
    const targetInfo = await targetHandle.stat({ bigint: true });
    if (!targetInfo.isFile() || targetInfo.nlink !== 1n) throw new Error("Unsafe target");
    const targetIdentity = identity(targetInfo);

    try { await guards.beforeRemove(); } catch (error) { guardError = error; throw error; }
    if (!sameIdentity(parentIdentity, identity(await parentHandle.stat({ bigint: true })))) throw new Error("Parent handle changed");
    if (!sameIdentity(parentIdentity, identity(await stat(dirname(path), { bigint: true })))) throw new Error("Parent path changed");
    if (!sameIdentity(targetIdentity, identity(await targetHandle.stat({ bigint: true })))) throw new Error("Target handle changed");
    if (!sameIdentity(targetIdentity, identity(await stat(path, { bigint: true })))) throw new Error("Target path changed");
    await targetHandle.close(); targetHandle = undefined;
    if (!sameIdentity(targetIdentity, identity(await stat(path, { bigint: true })))) throw new Error("Target path changed");
    await unlink(path);
    removed = true;
    try {
      if (guards.syncDirectory !== undefined) await guards.syncDirectory();
      else if (process.platform !== "win32") await parentHandle.sync();
      await parentHandle.close(); parentHandle = undefined;
    } catch (error) {
      durabilityFailed = true;
      throw error;
    }
  } catch (error) {
    await close(targetHandle).catch(() => undefined);
    await close(parentHandle).catch(() => undefined);
    if (guardError !== undefined) throw guardError;
    void error;
    throw new AtomicWriteError(durabilityFailed ? "durability-failed" : removed ? "replaced" : "not-replaced", removed);
  } finally {
    await close(targetHandle).catch(() => undefined);
    await close(parentHandle).catch(() => undefined);
  }
}

export async function atomicWrite(
  path: string,
  content: string,
  guards: AtomicWriteGuards,
): Promise<void> {
  const temporary = join(dirname(path), `.${randomUUID()}.tmp`);
  let handle: FileHandle | undefined;
  let parentHandle: FileHandle | undefined;
  let parentIdentity: ObjectIdentity | undefined;
  let temporaryIdentity: ObjectIdentity | undefined;
  let guardError: unknown;
  let replaced = false;
  let durabilityFailed = false;
  try {
    try { await guards.beforeCreate(); } catch (error) { guardError = error; throw error; }
    parentHandle = await open(dirname(path), "r");
    const parentInfo = await parentHandle.stat({ bigint: true });
    if (!parentInfo.isDirectory()) throw new Error("Unsafe parent");
    parentIdentity = identity(parentInfo);
    if (!sameIdentity(parentIdentity, identity(await stat(dirname(path), { bigint: true })))) throw new Error("Parent changed");

    const mode = await existingMode(path);
    handle = await open(temporary, "wx", mode ?? 0o600);
    temporaryIdentity = identity(await handle.stat({ bigint: true }));
    await handle.writeFile(content, "utf8");
    if (process.platform !== "win32" && mode !== undefined) await handle.chmod(mode);
    await handle.sync();
    await handle.close(); handle = undefined;
    await guards.afterTempSync?.(temporary);
    try { await guards.beforeRename(); } catch (error) { guardError = error; throw error; }
    if (!sameIdentity(parentIdentity, identity(await parentHandle.stat({ bigint: true })))) throw new Error("Parent handle changed");
    if (!sameIdentity(parentIdentity, identity(await stat(dirname(path), { bigint: true })))) throw new Error("Parent path changed");
    if (!sameIdentity(temporaryIdentity, identity(await stat(temporary, { bigint: true })))) throw new Error("Temporary changed");
    await rename(temporary, path);
    replaced = true;
    if (!sameIdentity(temporaryIdentity, identity(await stat(path, { bigint: true })))) throw new Error("Replacement changed");
    try {
      if (guards.syncDirectory !== undefined) await guards.syncDirectory();
      else if (process.platform !== "win32") await parentHandle.sync();
      await parentHandle.close(); parentHandle = undefined;
    } catch (error) {
      durabilityFailed = true;
      throw error;
    }
  } catch (error) {
    await close(handle).catch(() => undefined); handle = undefined;
    await close(parentHandle).catch(() => undefined); parentHandle = undefined;
    if (!replaced && temporaryIdentity !== undefined) {
      const currentIdentity = await stat(temporary, { bigint: true }).then(identity).catch(() => undefined);
      if (currentIdentity !== undefined && sameIdentity(temporaryIdentity, currentIdentity)) {
        await rm(temporary, { force: true }).catch(() => undefined);
      }
    }
    if (guardError !== undefined) throw guardError;
    void error;
    const state: AtomicWriteState = durabilityFailed
      ? "durability-failed"
      : replaced ? "replaced" : "not-replaced";
    throw new AtomicWriteError(state, replaced);
  } finally {
    await close(handle).catch(() => undefined);
    await close(parentHandle).catch(() => undefined);
  }
}
