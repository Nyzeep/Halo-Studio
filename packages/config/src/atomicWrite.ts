import { open, rename, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { randomUUID } from "node:crypto";

export interface AtomicWriteGuards {
  readonly beforeCreate: () => Promise<void>;
  readonly beforeRename: () => Promise<void>;
}

export async function atomicWrite(
  path: string,
  content: string,
  guards: AtomicWriteGuards,
): Promise<void> {
  const temporary = join(dirname(path), `.${randomUUID()}.tmp`);
  let handle: Awaited<ReturnType<typeof open>> | undefined;
  let guardError: unknown;
  try {
    try { await guards.beforeCreate(); } catch (error) { guardError = error; throw error; }
    handle = await open(temporary, "wx", 0o600);
    await handle.writeFile(content, "utf8"); await handle.sync(); await handle.close(); handle = undefined;
    try { await guards.beforeRename(); } catch (error) { guardError = error; throw error; }
    await rename(temporary, path);
    if (process.platform !== "win32") { const directory = await open(dirname(path), "r"); try { await directory.sync(); } finally { await directory.close(); } }
  } catch (error) {
    if (handle) await handle.close().catch(() => undefined);
    await rm(temporary, { force: true }).catch(() => undefined);
    if (guardError !== undefined) throw guardError;
    void error;
    throw new Error("Configuration write failed");
  }
}
