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
import { atomicWrite } from "./atomicWrite.js";
import { TargetRegistry, UnsafeConfigError } from "./targetRegistry.js";

export { UnsafeConfigError } from "./targetRegistry.js";

const MAX_PREVIEWS = 128;

interface PreviewState {
  readonly previewId: string;
  readonly targetId: string;
  readonly fingerprint: string;
  readonly original: string;
  readonly originalExists: boolean;
  readonly updated: string;
  readonly restartRequired: readonly AgentKind[];
  readonly expiresAt: number;
}

interface BackupState {
  readonly backupId: string;
  readonly targetId: string;
  readonly originalFingerprint: string;
  readonly committedFingerprint: string;
  readonly reference: string;
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
    if (!this.#vault.isAvailable()) throw new ConfigBackupUnavailable();
    try { await this.#vault.store(reference, value); }
    catch { throw new ConfigBackupUnavailable(); }
  }

  async get(reference: string): Promise<string | null> {
    if (!this.#vault.isAvailable()) throw new ConfigBackupUnavailable();
    try { return await this.#vault.get(reference); }
    catch { throw new ConfigBackupUnavailable(); }
  }
}

export class CredentialVaultBackupStore extends EncryptedBackupStore {}

export interface ConfigTransactionOptions {
  readonly vault?: CredentialVault;
  readonly previewTtlMs?: number;
  readonly validateAfterWrite?: (text: string) => Promise<void> | void;
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
  readonly #backups = new Map<string, BackupState>();
  readonly #audit: Array<{
    targetId: string;
    fingerprint: string;
    backupReference: string;
    summary: string;
  }> = [];

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
    };
    this.#previews.set(previewId, state);
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
    this.#previews.delete(previewId);

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
    });

    const assertOriginalUnchanged = async (): Promise<void> => {
      await this.#registry.verifyWritable(target.targetId);
      await this.#assertFingerprint(
        target.targetId,
        state.originalExists,
        state.fingerprint,
      );
    };

    try {
      await atomicWrite(target.path, state.updated, {
        beforeCreate: assertOriginalUnchanged,
        beforeRename: assertOriginalUnchanged,
      });
    } catch (error) {
      if (error instanceof ConfigConflict || error instanceof UnsafeConfigError) {
        throw error;
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
        await this.#rollbackInternal(backupId);
      } catch (rollbackError) {
        throw new ConfigRecoveryError(recoveryReason(rollbackError));
      }
      throw new ConfigWriteError(
        "Configuration validation failed; original restored",
      );
    }

    this.#audit.push({
      targetId: target.targetId,
      fingerprint: committedFingerprint,
      backupReference: reference,
      summary: "Configuration updated",
    });
    return { backupId, targetId: target.targetId, fingerprint: committedFingerprint };
  }

  async rollback(
    backupId: string,
  ): Promise<{ backupId: string; targetId: string; fingerprint: string }> {
    const backup = this.#backups.get(backupId);
    if (backup === undefined) throw new ConfigPreviewUnavailable();
    await this.#rollbackInternal(backupId);
    this.#backups.delete(backupId);
    return {
      backupId,
      targetId: backup.targetId,
      fingerprint: backup.originalFingerprint,
    };
  }

  get audit(): readonly {
    targetId: string;
    fingerprint: string;
    backupReference: string;
    summary: string;
  }[] {
    return this.#audit.slice();
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

  async #rollbackInternal(backupId: string): Promise<void> {
    const backup = this.#backups.get(backupId);
    if (backup === undefined) throw new ConfigPreviewUnavailable();
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
    await atomicWrite(target.path, original, {
      beforeCreate: assertCommittedUnchanged,
      beforeRename: assertCommittedUnchanged,
    });
    const restored = await this.#registry.read(backup.targetId);
    if (
      !restored.exists ||
      fingerprint(restored.text) !== backup.originalFingerprint
    ) {
      throw new ConfigWriteError();
    }
    parseJsonc(restored.text);
  }

  #prunePreviews(): void {
    const now = Date.now();
    for (const [previewId, state] of this.#previews) {
      if (state.expiresAt <= now) this.#previews.delete(previewId);
    }
    while (this.#previews.size > MAX_PREVIEWS) {
      const oldest = this.#previews.keys().next().value as string | undefined;
      if (oldest === undefined) break;
      this.#previews.delete(oldest);
    }
  }
}
