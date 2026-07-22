import { randomUUID } from "node:crypto";
import type { AgentKind, ConfigOperation, ConfigPreview } from "@halo-studio/contracts";
import type { CredentialVault } from "@halo-studio/storage";
import { CoreError } from "@halo-studio/core";
import { applyJsoncPatch, parseJsonc, ConfigParseError } from "./jsoncPatch.js";
import { createTwoFilesPatch } from "./unifiedDiff.js";
import { fingerprint } from "./fingerprint.js";
import { atomicWrite } from "./atomicWrite.js";
import { TargetRegistry, UnsafeConfigError } from "./targetRegistry.js";

export { UnsafeConfigError } from "./targetRegistry.js";

interface PreviewState { readonly preview: ConfigPreview; readonly original: string; readonly updated: string; readonly expiresAt: number; }
interface BackupState { readonly backupId: string; readonly targetId: string; readonly original?: string; readonly fingerprint: string; readonly reference: string; }
export class ConfigConflict extends CoreError { constructor() { super("ConfigConflict", "Configuration changed externally"); } }
export class ConfigPreviewUnavailable extends Error { readonly code = "ProtocolViolation" as const; constructor() { super("Configuration preview unavailable"); this.name = "ConfigPreviewUnavailable"; } }

export interface BackupStore { store(reference: string, value: string): Promise<void>; get(reference: string): Promise<string | null>; }
export class EncryptedBackupStore implements BackupStore {
  readonly #vault: CredentialVault;
  constructor(vault: CredentialVault) { this.#vault = vault; }
  store(reference: string, value: string): Promise<void> { return this.#vault.store(reference, value); }
  get(reference: string): Promise<string | null> { return this.#vault.get(reference); }
}
export class CredentialVaultBackupStore extends EncryptedBackupStore {}

export class ConfigTransaction {
  readonly #registry: TargetRegistry;
  readonly #vault: CredentialVault | undefined;
  readonly #backupStore: BackupStore | undefined;
  readonly #ttlMs: number;
  readonly #previews = new Map<string, PreviewState>();
  readonly #backups = new Map<string, BackupState>();
  readonly #audit: Array<{ targetId: string; fingerprint: string; backupReference: string }> = [];

  constructor(registry: TargetRegistry, options?: { vault?: CredentialVault; backupStore?: BackupStore; previewTtlMs?: number }) { this.#registry = registry; this.#vault = options?.vault; this.#backupStore = options?.backupStore ?? (options?.vault ? new EncryptedBackupStore(options.vault) : undefined); this.#ttlMs = options?.previewTtlMs ?? 5 * 60_000; }

  async preview(targetId: string, operations: readonly ConfigOperation[]): Promise<ConfigPreview> {
    const { target, text } = await this.#registry.read(targetId);
    let updated: string;
    try { updated = applyJsoncPatch(text, operations); parseJsonc(updated); } catch (error) { if (error instanceof ConfigParseError) throw error; throw new ConfigParseError(); }
    const diffName = `${target.owner}-${target.scope}.jsonc`;
    const preview: ConfigPreview = { previewId: randomUUID(), targetId: target.targetId, fingerprint: fingerprint(text), unifiedDiff: createTwoFilesPatch(diffName, diffName, text, updated), restartRequired: target.owner === "pi" ? (["pi"] as AgentKind[]) : [] };
    this.#previews.set(preview.previewId, { preview, original: text, updated, expiresAt: Date.now() + this.#ttlMs });
    return preview;
  }

  async commit(previewId: string): Promise<{ backupId: string; targetId: string; fingerprint: string }> {
    const state = this.#previews.get(previewId);
    if (!state || state.expiresAt < Date.now()) throw new ConfigPreviewUnavailable();
    const target = await this.#registry.verifyWritable(state.preview.targetId);
    const current = await this.#registry.read(target.targetId);
    if (fingerprint(current.text) !== state.preview.fingerprint) throw new ConfigConflict();
    const backupId = randomUUID(); const reference = `config-backup:${backupId}`;
    if (this.#backupStore) { if (this.#vault && !this.#vault.isAvailable()) throw new Error("Configuration backup unavailable"); await this.#backupStore.store(reference, state.original); }
    this.#backups.set(backupId, { backupId, targetId: target.targetId, ...(this.#backupStore ? {} : { original: state.original }), fingerprint: state.preview.fingerprint, reference });
    try {
      await atomicWrite(target.path, state.updated, async () => { await this.#registry.verifyWritable(target.targetId); });
      parseJsonc((await this.#registry.read(target.targetId)).text);
    } catch (error) {
      try { await this.#rollbackInternal(backupId); } catch (rollbackError) { if (rollbackError instanceof UnsafeConfigError) throw rollbackError; }
      if (error instanceof UnsafeConfigError) throw error;
      throw new Error("Configuration write failed");
    }
    this.#audit.push({ targetId: target.targetId, fingerprint: fingerprint(state.updated), backupReference: reference });
    this.#previews.delete(previewId);
    return { backupId, targetId: target.targetId, fingerprint: fingerprint(state.updated) };
  }

  async rollback(backupId: string): Promise<{ backupId: string; targetId: string; fingerprint: string }> {
    const backup = this.#backups.get(backupId); if (!backup) throw new ConfigPreviewUnavailable();
    await this.#rollbackInternal(backupId);
    return { backupId, targetId: backup.targetId, fingerprint: backup.fingerprint };
  }

  get audit(): readonly { targetId: string; fingerprint: string; backupReference: string }[] { return this.#audit.slice(); }
  async #rollbackInternal(backupId: string): Promise<void> {
    const backup = this.#backups.get(backupId); if (!backup) throw new ConfigPreviewUnavailable();
    const target = await this.#registry.verifyWritable(backup.targetId);
    let original = backup.original;
    if (this.#backupStore) { const restored = await this.#backupStore.get(backup.reference); if (restored === null) throw new Error("Configuration backup unavailable"); original = restored; }
    if (original === undefined) throw new Error("Configuration backup unavailable");
    await atomicWrite(target.path, original, async () => { await this.#registry.verifyWritable(backup.targetId); });
    parseJsonc((await this.#registry.read(backup.targetId)).text);
  }
}
