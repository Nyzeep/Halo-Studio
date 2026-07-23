import { randomUUID } from "node:crypto";
import type {
  AgentKind,
  ConfigOperation,
  ConfigPreview,
} from "@halo-studio/contracts";
import type { CredentialVault } from "@halo-studio/storage";
import { CoreError } from "@halo-studio/core";
import { applyJsoncPatch, parseJsonc, ConfigParseError } from "./jsoncPatch.js";
import { createTwoFilesPatch } from "./unifiedDiff.js";
import { fingerprint } from "./fingerprint.js";
import { atomicWrite, AtomicWriteError, guardedRemove, syncParentDirectory } from "./atomicWrite.js";
import { MAX_CONFIG_BYTES, TargetRegistry, UnsafeConfigError } from "./targetRegistry.js";

export { UnsafeConfigError } from "./targetRegistry.js";

const MAX_PREVIEWS = 128;
const MAX_RETAINED_PREVIEW_BYTES = 4 * 1024 * 1024;
const MAX_BACKUPS_PER_TARGET = 32;
const MAX_PENDING_BACKUP_CLEANUPS = 32;
const MAX_AUDIT_ENTRIES = 128;

interface PreviewState {
  readonly previewId: string;
  readonly targetId: string;
  readonly fingerprint: string;
  readonly original: string;
  readonly originalExists: boolean;
  readonly updated: string;
  readonly restartRequired: readonly AgentKind[];
  readonly expiresAt: number;
  readonly retainedBytes: number;
}

interface BackupState {
  readonly backupId: string;
  readonly targetId: string;
  readonly originalFingerprint: string;
  readonly committedFingerprint: string;
  readonly reference: string;
  readonly originalExists: boolean;
}

interface RollbackRestoreResult {
  readonly durabilityFailed: boolean;
}

type PendingCleanupKind = "durability-pending" | "cleanup-only";

interface PendingCleanupState {
  readonly backup: BackupState;
  readonly kind: PendingCleanupKind;
}

export class ConfigConflict extends CoreError {
  constructor() { super("ConfigConflict", "Configuration changed externally"); }
}

export class ConfigPreviewUnavailable extends Error {
  readonly code = "ProtocolViolation" as const;
  constructor() {
    super("Configuration preview unavailable");
    this.name = "ConfigPreviewUnavailable";
  }
}

export class ConfigBackupUnavailable extends Error {
  readonly code = "AuthenticationFailed" as const;
  constructor() {
    super("Encrypted configuration backup unavailable");
    this.name = "ConfigBackupUnavailable";
  }
}

export class ConfigWriteError extends Error {
  readonly code = "ProtocolViolation" as const;
  constructor(message = "Configuration write failed") {
    super(message);
    this.name = "ConfigWriteError";
  }
}

export type ConfigRecoveryReason =
  | "backup-unavailable"
  | "conflict"
  | "unsafe-path"
  | "write-failed";

export class ConfigRecoveryError extends Error {
  readonly code = "ConfigConflict" as const;
  readonly reason: ConfigRecoveryReason;
  constructor(reason: ConfigRecoveryReason) {
    super("Configuration recovery incomplete");
    this.name = "ConfigRecoveryError";
    this.reason = reason;
  }
}

export class EncryptedBackupStore {
  readonly #vault: CredentialVault;
  constructor(vault: CredentialVault) { this.#vault = vault; }

  async store(reference: string, value: string): Promise<void> {
    try {
      if (this.#vault.isAvailable() !== true) throw new ConfigBackupUnavailable();
      await this.#vault.store(reference, value);
    }
    catch { throw new ConfigBackupUnavailable(); }
  }

  async get(reference: string): Promise<string | null> {
    try {
      if (this.#vault.isAvailable() !== true) throw new ConfigBackupUnavailable();
      return await this.#vault.get(reference);
    }
    catch { throw new ConfigBackupUnavailable(); }
  }

  async delete(reference: string): Promise<void> {
    try {
      if (this.#vault.isAvailable() !== true) throw new ConfigBackupUnavailable();
      await this.#vault.delete(reference);
    }
    catch { throw new ConfigBackupUnavailable(); }
  }
}

export class CredentialVaultBackupStore extends EncryptedBackupStore {}

export interface ConfigTransactionOptions {
  readonly vault?: CredentialVault;
  readonly previewTtlMs?: number;
  readonly validateAfterWrite?: (text: string) => Promise<void> | void;
}

interface ConfigTransactionTestHooks {
  readonly syncDirectory?: () => Promise<void>;
}

const transactionTestHooks = new WeakMap<ConfigTransaction, ConfigTransactionTestHooks>();

/** Source-module-only deterministic seam; intentionally omitted from the package index. */
export function setConfigTransactionTestHooks(
  transaction: ConfigTransaction,
  hooks: ConfigTransactionTestHooks,
): void {
  transactionTestHooks.set(transaction, hooks);
}

function recoveryReason(error: unknown): ConfigRecoveryReason {
  if (error instanceof ConfigConflict) return "conflict";
  if (error instanceof UnsafeConfigError) return "unsafe-path";
  if (error instanceof ConfigBackupUnavailable) return "backup-unavailable";
  return "write-failed";
}

export class ConfigTransaction {
  readonly #registry: TargetRegistry;
  readonly #backupStore: EncryptedBackupStore | undefined;
  readonly #ttlMs: number;
  readonly #validateAfterWrite: (text: string) => Promise<void> | void;
  readonly #previews = new Map<string, PreviewState>();
  readonly #previewTimers = new Map<string, ReturnType<typeof setTimeout>>();
  readonly #backups = new Map<string, BackupState>();
  readonly #pendingCleanup = new Map<string, PendingCleanupState>();
  readonly #audit: Array<{
    targetId: string;
    fingerprint: string;
    backupReference: string;
    summary: string;
  }> = [];
  #retainedPreviewBytes = 0;
  #disposed = false;

  constructor(registry: TargetRegistry, options?: ConfigTransactionOptions) {
    this.#registry = registry;
    this.#backupStore = options?.vault
      ? new EncryptedBackupStore(options.vault)
      : undefined;
    this.#ttlMs = options?.previewTtlMs ?? 5 * 60_000;
    this.#validateAfterWrite = options?.validateAfterWrite ?? ((text) => {
      parseJsonc(text);
    });
  }

  async preview(
    targetId: string,
    operations: readonly ConfigOperation[],
  ): Promise<ConfigPreview> {
    if (this.#disposed) throw new ConfigPreviewUnavailable();
    this.#prunePreviews();
    const { target, text, exists } = await this.#registry.read(targetId);
    let updated: string;
    try {
      updated = applyJsoncPatch(text, operations);
      parseJsonc(updated);
    } catch (error) {
      if (error instanceof ConfigParseError) throw error;
      throw new ConfigParseError();
    }
    if (Buffer.byteLength(updated, "utf8") > MAX_CONFIG_BYTES) throw new UnsafeConfigError();
    const previewId = randomUUID();
    const originalFingerprint = fingerprint(text);
    const restartRequired: readonly AgentKind[] =
      target.owner === "pi" ? ["pi"] : [];
    const diffName = `${target.owner}-${target.scope}.jsonc`;
    const state: PreviewState = {
      previewId,
      targetId: target.targetId,
      fingerprint: originalFingerprint,
      original: text,
      originalExists: exists,
      updated,
      restartRequired,
      expiresAt: Date.now() + this.#ttlMs,
      retainedBytes: Buffer.byteLength(text, "utf8") + Buffer.byteLength(updated, "utf8"),
    };
    this.#previews.set(previewId, state);
    this.#retainedPreviewBytes += state.retainedBytes;
    const timer = setTimeout(() => { this.#deletePreview(previewId); }, this.#ttlMs);
    timer.unref?.();
    this.#previewTimers.set(previewId, timer);
    this.#prunePreviews();
    return {
      previewId,
      targetId: state.targetId,
      fingerprint: state.fingerprint,
      unifiedDiff: createTwoFilesPatch(diffName, diffName, text, updated),
      restartRequired: [...state.restartRequired],
    };
  }

  async commit(
    previewId: string,
  ): Promise<{ backupId: string; targetId: string; fingerprint: string }> {
    this.#prunePreviews();
    const state = this.#previews.get(previewId);
    if (state === undefined) throw new ConfigPreviewUnavailable();
    if (this.#pendingCleanup.size >= MAX_PENDING_BACKUP_CLEANUPS) {
      throw new ConfigBackupUnavailable();
    }
    this.#deletePreview(previewId);

    const target = await this.#registry.verifyWritable(state.targetId);
    await this.#assertFingerprint(
      target.targetId,
      state.originalExists,
      state.fingerprint,
    );
    if (this.#backupStore === undefined) throw new ConfigBackupUnavailable();

    const backupId = randomUUID();
    const reference = `config-backup:${backupId}`;
    await this.#backupStore.store(reference, state.original);
    const committedFingerprint = fingerprint(state.updated);
    this.#backups.set(backupId, {
      backupId,
      targetId: target.targetId,
      originalFingerprint: state.fingerprint,
      committedFingerprint,
      reference,
      originalExists: state.originalExists,
    });
    try {
      await this.#pruneBackups(target.targetId);
    } catch (error) {
      this.#backups.delete(backupId);
      await this.#backupStore.delete(reference).catch(() => undefined);
      throw error;
    }

    const assertOriginalUnchanged = async (): Promise<void> => {
      await this.#registry.verifyWritable(target.targetId);
      await this.#assertFingerprint(
        target.targetId,
        state.originalExists,
        state.fingerprint,
      );
    };
    const syncDirectory = transactionTestHooks.get(this)?.syncDirectory;

    try {
      await atomicWrite(target.path, state.updated, {
        beforeCreate: assertOriginalUnchanged,
        beforeRename: assertOriginalUnchanged,
        ...(syncDirectory === undefined ? {} : { syncDirectory }),
      });
    } catch (error) {
      if (error instanceof ConfigConflict || error instanceof UnsafeConfigError) {
        throw error;
      }
      if (error instanceof AtomicWriteError && error.replaced) {
        try {
          await this.#recoverAutomatically(backupId);
        } catch (rollbackError) {
          if (rollbackError instanceof ConfigRecoveryError) throw rollbackError;
          throw new ConfigRecoveryError(recoveryReason(rollbackError));
        }
        throw new ConfigWriteError(
          error.state === "durability-failed"
            ? "Configuration durability failed; original restored"
            : "Configuration replacement failed; original restored",
        );
      }
      throw new ConfigWriteError();
    }

    try {
      const written = await this.#registry.read(target.targetId);
      if (!written.exists || fingerprint(written.text) !== committedFingerprint) {
        throw new ConfigConflict();
      }
      await this.#validateAfterWrite(written.text);
    } catch {
      try {
        await this.#recoverAutomatically(backupId);
      } catch (rollbackError) {
        if (rollbackError instanceof ConfigRecoveryError) throw rollbackError;
        throw new ConfigRecoveryError(recoveryReason(rollbackError));
      }
      throw new ConfigWriteError(
        "Configuration validation failed; original restored",
      );
    }

    this.#audit.push(Object.freeze({
      targetId: target.targetId,
      fingerprint: committedFingerprint,
      backupReference: reference,
      summary: "Configuration updated",
    }));
    while (this.#audit.length > MAX_AUDIT_ENTRIES) this.#audit.shift();
    return { backupId, targetId: target.targetId, fingerprint: committedFingerprint };
  }

  async rollback(
    backupId: string,
  ): Promise<{ backupId: string; targetId: string; fingerprint: string }> {
    const pending = this.#pendingCleanup.get(backupId);
    if (pending !== undefined) {
      await this.#finishPendingBackup(backupId);
      return this.#rollbackReceipt(pending.backup);
    }
    const backup = this.#backups.get(backupId);
    if (backup === undefined) throw new ConfigPreviewUnavailable();
    const restored = await this.#restoreBackup(backup);
    this.#queueCleanup(backup, restored.durabilityFailed ? "durability-pending" : "cleanup-only");
    if (restored.durabilityFailed) throw new ConfigRecoveryError("write-failed");
    await this.#finishPendingBackup(backupId);
    return this.#rollbackReceipt(backup);
  }

  async cleanup(): Promise<void> {
    let failedReason: ConfigRecoveryReason | undefined;
    for (const backupId of [...this.#pendingCleanup.keys()]) {
      try {
        await this.#finishPendingBackup(backupId);
      } catch (error) {
        const reason = error instanceof ConfigRecoveryError
          ? error.reason
          : recoveryReason(error);
        if (failedReason === undefined || reason === "write-failed") failedReason = reason;
      }
    }
    if (failedReason !== undefined) throw new ConfigRecoveryError(failedReason);
  }

  get audit(): readonly {
    targetId: string;
    fingerprint: string;
    backupReference: string;
    summary: string;
  }[] {
    return Object.freeze(this.#audit.map((entry) => Object.freeze({ ...entry })));
  }

  dispose(): void {
    this.#disposed = true;
    for (const previewId of [...this.#previews.keys()]) this.#deletePreview(previewId);
  }

  async #assertFingerprint(
    targetId: string,
    expectedExists: boolean,
    expectedFingerprint: string,
  ): Promise<void> {
    const current = await this.#registry.read(targetId);
    if (
      current.exists !== expectedExists ||
      fingerprint(current.text) !== expectedFingerprint
    ) {
      throw new ConfigConflict();
    }
  }

  async #restoreBackup(backup: BackupState): Promise<RollbackRestoreResult> {
    const target = await this.#registry.verifyWritable(backup.targetId);
    await this.#assertFingerprint(
      backup.targetId,
      true,
      backup.committedFingerprint,
    );
    if (this.#backupStore === undefined) throw new ConfigBackupUnavailable();
    const original = await this.#backupStore.get(backup.reference);
    if (original === null) throw new ConfigBackupUnavailable();

    const assertCommittedUnchanged = async (): Promise<void> => {
      await this.#registry.verifyWritable(backup.targetId);
      await this.#assertFingerprint(
        backup.targetId,
        true,
        backup.committedFingerprint,
      );
    };
    const syncDirectory = transactionTestHooks.get(this)?.syncDirectory;
    let durabilityFailed = false;
    try {
      if (backup.originalExists) {
        await atomicWrite(target.path, original, {
          beforeCreate: assertCommittedUnchanged,
          beforeRename: assertCommittedUnchanged,
          ...(syncDirectory === undefined ? {} : { syncDirectory }),
        });
      } else {
        await guardedRemove(target.path, {
          beforeRemove: assertCommittedUnchanged,
          ...(syncDirectory === undefined ? {} : { syncDirectory }),
        });
      }
    } catch (error) {
      if (!(error instanceof AtomicWriteError) || !error.replaced) throw error;
      durabilityFailed = true;
    }
    await this.#assertRestored(backup);
    return { durabilityFailed };
  }

  async #assertRestored(backup: BackupState): Promise<void> {
    const restored = await this.#registry.read(backup.targetId);
    if (
      restored.exists !== backup.originalExists ||
      fingerprint(restored.text) !== backup.originalFingerprint
    ) {
      throw new ConfigWriteError();
    }
    if (restored.exists) parseJsonc(restored.text);
  }

  async #recoverAutomatically(backupId: string): Promise<void> {
    const backup = this.#backups.get(backupId);
    if (backup === undefined) throw new ConfigPreviewUnavailable();
    const restored = await this.#restoreBackup(backup);
    this.#queueCleanup(backup, restored.durabilityFailed ? "durability-pending" : "cleanup-only");
    if (restored.durabilityFailed) throw new ConfigRecoveryError("write-failed");
    await this.#finishPendingBackup(backupId).catch(() => undefined);
  }

  #queueCleanup(backup: BackupState, kind: PendingCleanupKind): void {
    this.#backups.delete(backup.backupId);
    this.#pendingCleanup.set(backup.backupId, { backup, kind });
  }

  async #finishPendingBackup(backupId: string): Promise<void> {
    const pending = this.#pendingCleanup.get(backupId);
    if (pending === undefined) return;
    if (pending.kind === "durability-pending") {
      await this.#retryDurability(pending.backup);
      this.#pendingCleanup.set(backupId, { backup: pending.backup, kind: "cleanup-only" });
    }
    if (this.#backupStore === undefined) throw new ConfigRecoveryError("backup-unavailable");
    try {
      await this.#backupStore.delete(pending.backup.reference);
    } catch {
      throw new ConfigRecoveryError("backup-unavailable");
    }
    this.#pendingCleanup.delete(backupId);
  }

  async #retryDurability(backup: BackupState): Promise<void> {
    try {
      const target = await this.#registry.verifyWritable(backup.targetId);
      const beforeSync = async (): Promise<void> => {
        await this.#registry.verifyWritable(backup.targetId);
        await this.#assertRestored(backup);
      };
      const syncDirectory = transactionTestHooks.get(this)?.syncDirectory;
      await syncParentDirectory(target.path, {
        beforeSync,
        ...(syncDirectory === undefined ? {} : { syncDirectory }),
      });
      await this.#assertRestored(backup);
    } catch {
      throw new ConfigRecoveryError("write-failed");
    }
  }

  #rollbackReceipt(backup: BackupState): { backupId: string; targetId: string; fingerprint: string } {
    return {
      backupId: backup.backupId,
      targetId: backup.targetId,
      fingerprint: backup.originalFingerprint,
    };
  }

  #prunePreviews(): void {
    const now = Date.now();
    for (const [previewId, state] of this.#previews) {
      if (state.expiresAt <= now) this.#deletePreview(previewId);
    }
    while (this.#previews.size > MAX_PREVIEWS || this.#retainedPreviewBytes > MAX_RETAINED_PREVIEW_BYTES) {
      const oldest = this.#previews.keys().next().value as string | undefined;
      if (oldest === undefined) break;
      this.#deletePreview(oldest);
    }
  }

  #deletePreview(previewId: string): void {
    const state = this.#previews.get(previewId);
    if (state !== undefined) {
      this.#previews.delete(previewId);
      this.#retainedPreviewBytes -= state.retainedBytes;
    }
    const timer = this.#previewTimers.get(previewId);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.#previewTimers.delete(previewId);
    }
  }

  async #pruneBackups(targetId: string): Promise<void> {
    const matching = [...this.#backups.values()].filter((backup) => backup.targetId === targetId);
    while (matching.length > MAX_BACKUPS_PER_TARGET) {
      const oldest = matching.shift();
      if (oldest === undefined || this.#backupStore === undefined) break;
      await this.#backupStore.delete(oldest.reference);
      this.#backups.delete(oldest.backupId);
    }
  }
}
