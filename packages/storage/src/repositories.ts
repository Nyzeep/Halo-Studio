import { CoreError } from "@halo-studio/core";
import type Database from "better-sqlite3";
import { types as utilTypes } from "node:util";

export type StorageProvider = "pi" | "opencode";

export interface CredentialReference {
  readonly id: string;
  readonly provider: StorageProvider;
  readonly vaultReference: string;
}

export interface SaveCredentialReference extends CredentialReference {
  readonly profileId?: string | null;
}

export interface CredentialReferenceRepository {
  save(reference: SaveCredentialReference): void;
  get(id: string): CredentialReference | null;
  delete(id: string): void;
}

const MIGRATION_ERROR_MESSAGE = "Database migration failed";
const PROTOCOL_ERROR_MESSAGE = "Invalid credential reference";
const MAX_ID_BYTES = 256;
const MAX_REFERENCE_BYTES = 1_024;

function migrationFailed(): CoreError {
  return new CoreError("MigrationFailed", MIGRATION_ERROR_MESSAGE);
}

function protocolViolation(): CoreError {
  return new CoreError("ProtocolViolation", PROTOCOL_ERROR_MESSAGE);
}

function plainDataRecord(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || utilTypes.isProxy(value)) {
    throw protocolViolation();
  }
  try {
    const prototype = Object.getPrototypeOf(value) as object | null;
    if (prototype !== Object.prototype && prototype !== null) {
      throw protocolViolation();
    }
    for (const key of Reflect.ownKeys(value)) {
      if (typeof key !== "string") {
        throw protocolViolation();
      }
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (descriptor === undefined || !("value" in descriptor)) {
        throw protocolViolation();
      }
    }
    return value as Record<string, unknown>;
  } catch {
    throw protocolViolation();
  }
}

function ownDataValue(record: Record<string, unknown>, key: string): unknown {
  const descriptor = Object.getOwnPropertyDescriptor(record, key);
  if (descriptor === undefined || !("value" in descriptor)) {
    throw protocolViolation();
  }
  return descriptor.value;
}

function boundedString(value: unknown, maximumBytes: number): string {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.includes("\0") ||
    Buffer.byteLength(value, "utf8") > maximumBytes
  ) {
    throw protocolViolation();
  }
  return value;
}

function providerValue(value: unknown): StorageProvider {
  if (value !== "pi" && value !== "opencode") {
    throw protocolViolation();
  }
  return value;
}

function validateSaveReference(value: unknown): Required<SaveCredentialReference> {
  const record = plainDataRecord(value);
  const allowedKeys = new Set([
    "id",
    "profileId",
    "provider",
    "vaultReference",
  ]);
  if (Reflect.ownKeys(record).some((key) => typeof key !== "string" || !allowedKeys.has(key))) {
    throw protocolViolation();
  }
  const profileDescriptor = Object.getOwnPropertyDescriptor(record, "profileId");
  const rawProfileId =
    profileDescriptor === undefined
      ? null
      : "value" in profileDescriptor
        ? profileDescriptor.value
        : (() => {
            throw protocolViolation();
          })();
  return {
    id: boundedString(ownDataValue(record, "id"), MAX_ID_BYTES),
    profileId:
      rawProfileId === null
        ? null
        : boundedString(rawProfileId, MAX_ID_BYTES),
    provider: providerValue(ownDataValue(record, "provider")),
    vaultReference: boundedString(
      ownDataValue(record, "vaultReference"),
      MAX_REFERENCE_BYTES,
    ),
  };
}

function validateStoredReference(value: unknown): CredentialReference {
  const record = plainDataRecord(value);
  return {
    id: boundedString(ownDataValue(record, "id"), MAX_ID_BYTES),
    provider: providerValue(ownDataValue(record, "provider")),
    vaultReference: boundedString(
      ownDataValue(record, "vaultReference"),
      MAX_REFERENCE_BYTES,
    ),
  };
}

export function createCredentialReferenceRepository(
  database: Database.Database,
  readOnlyRecovery: boolean,
): CredentialReferenceRepository {
  function rethrowRepositoryError(error: unknown): never {
    void error;
    void readOnlyRecovery;
    throw migrationFailed();
  }

  return {
    save(reference) {
      const validated = validateSaveReference(reference);
      try {
        const now = new Date().toISOString();
        database
          .prepare(`
            INSERT INTO credential_refs (
              id, profile_id, provider, vault_reference, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
              profile_id = excluded.profile_id,
              provider = excluded.provider,
              vault_reference = excluded.vault_reference,
              updated_at = excluded.updated_at
          `)
          .run(
            validated.id,
            validated.profileId,
            validated.provider,
            validated.vaultReference,
            now,
            now,
          );
      } catch (error) {
        rethrowRepositoryError(error);
      }
    },

    get(id) {
      const validatedId = boundedString(id, MAX_ID_BYTES);
      try {
        const row = database
          .prepare(`
            SELECT id, provider, vault_reference AS vaultReference
            FROM credential_refs
            WHERE id = ?
          `)
          .get(validatedId) as unknown;
        return row === undefined ? null : validateStoredReference(row);
      } catch (error) {
        rethrowRepositoryError(error);
      }
    },

    delete(id) {
      const validatedId = boundedString(id, MAX_ID_BYTES);
      try {
        database.prepare("DELETE FROM credential_refs WHERE id = ?").run(validatedId);
      } catch (error) {
        rethrowRepositoryError(error);
      }
    },
  };
}
