import fs from "node:fs/promises";
import path from "node:path";
import type {
  ConfigRollbackRequest,
  ConfigRollbackResult,
  ConfigWriteRequest,
  ConfigWriteResult
} from "../../shared/config.js";
import { createUnifiedDiff } from "./diff.js";

export async function applyConfigWrite(request: ConfigWriteRequest): Promise<ConfigWriteResult> {
  const targetPath = path.resolve(request.targetPath);
  const targetDir = path.dirname(targetPath);
  await fs.mkdir(targetDir, { recursive: true });

  const currentContent = await readExistingFile(targetPath);
  const backupDir = path.join(targetDir, ".halo-backups");
  await fs.mkdir(backupDir, { recursive: true });

  const stamp = createStamp();
  const backupPath = path.join(backupDir, `${path.basename(targetPath)}.${stamp}.bak`);
  await fs.writeFile(backupPath, currentContent, "utf8");

  const tempPath = path.join(targetDir, `.${path.basename(targetPath)}.${stamp}.tmp`);
  await fs.writeFile(tempPath, request.nextContent, "utf8");
  await fs.rename(tempPath, targetPath);

  return {
    targetPath,
    backupPath,
    diff: createUnifiedDiff(currentContent, request.nextContent, request.reason),
    wroteAt: new Date().toISOString()
  };
}

export async function rollbackConfigWrite(request: ConfigRollbackRequest): Promise<ConfigRollbackResult> {
  const targetPath = path.resolve(request.targetPath);
  const backupPath = path.resolve(request.backupPath);
  const backupContent = await fs.readFile(backupPath, "utf8");

  const stamp = createStamp();
  const tempPath = path.join(path.dirname(targetPath), `.${path.basename(targetPath)}.${stamp}.rollback.tmp`);
  await fs.writeFile(tempPath, backupContent, "utf8");
  await fs.rename(tempPath, targetPath);

  return {
    targetPath,
    backupPath,
    restored: true,
    restoredAt: new Date().toISOString()
  };
}

async function readExistingFile(targetPath: string) {
  try {
    return await fs.readFile(targetPath, "utf8");
  } catch (error) {
    if (isNodeError(error) && error.code === "ENOENT") {
      return "";
    }
    throw error;
  }
}

function createStamp() {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}
