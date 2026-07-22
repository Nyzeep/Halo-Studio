import { CoreError } from "@halo-studio/core";
import Database from "better-sqlite3";

import { migrateDatabase } from "./migrations.js";
import {
  createCredentialReferenceRepository,
  type CredentialReferenceRepository,
} from "./repositories.js";

const MIGRATION_ERROR_MESSAGE = "Database migration failed";

export type DatabaseHealth =
  | { readonly mode: "read-write"; readonly schemaVersion: number }
  | { readonly mode: "read-only-recovery"; readonly schemaVersion: number };

export interface DatabaseDiagnostics {
  readonly code: "MigrationFailed";
  readonly message: typeof MIGRATION_ERROR_MESSAGE;
}

export interface HaloDatabase {
  readonly credentialReferences: CredentialReferenceRepository;
  health(): DatabaseHealth;
  diagnostics(): DatabaseDiagnostics | null;
  close(): void;
}

export type DatabaseConnectionFactory = (
  path: string,
  options?: Database.Options,
) => Database.Database;

function schemaVersion(database: Database.Database): number {
  try {
    const value = database.pragma("user_version", { simple: true });
    return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
      ? value
      : 0;
  } catch {
    return 0;
  }
}

function migrationFailed(): CoreError {
  return new CoreError("MigrationFailed", MIGRATION_ERROR_MESSAGE);
}

function closeConnection(connection: Database.Database): boolean {
  try {
    connection.close();
    return true;
  } catch {
    return false;
  }
}

function createHandle(
  connection: Database.Database,
  health: DatabaseHealth,
  diagnostics: DatabaseDiagnostics | null,
): HaloDatabase {
  let closed = false;
  return {
    credentialReferences: createCredentialReferenceRepository(
      connection,
      health.mode === "read-only-recovery",
    ),
    health: () => ({ ...health }),
    diagnostics: () => (diagnostics === null ? null : { ...diagnostics }),
    close() {
      if (!closed) {
        connection.close();
        closed = true;
      }
    },
  };
}

export function openDatabase(path: string): HaloDatabase {
  return openDatabaseWithConnectionFactory(path, (databasePath, options) =>
    new Database(databasePath, options),
  );
}

export function openDatabaseWithConnectionFactory(
  path: string,
  createConnection: DatabaseConnectionFactory,
): HaloDatabase {
  let writeConnection: Database.Database;
  try {
    writeConnection = createConnection(path);
  } catch {
    throw migrationFailed();
  }
  try {
    writeConnection.pragma("foreign_keys = ON");
  } catch {
    closeConnection(writeConnection);
    throw migrationFailed();
  }

  try {
    const version = migrateDatabase(writeConnection);
    return createHandle(
      writeConnection,
      { mode: "read-write", schemaVersion: version },
      null,
    );
  } catch {
    const failedVersion = schemaVersion(writeConnection);
    if (!closeConnection(writeConnection)) {
      throw migrationFailed();
    }
    let readConnection: Database.Database;
    try {
      readConnection = createConnection(path, {
        fileMustExist: true,
        readonly: true,
      });
    } catch {
      throw migrationFailed();
    }
    try {
      readConnection.pragma("foreign_keys = ON");
      return createHandle(
        readConnection,
        { mode: "read-only-recovery", schemaVersion: failedVersion },
        { code: "MigrationFailed", message: MIGRATION_ERROR_MESSAGE },
      );
    } catch {
      closeConnection(readConnection);
      throw migrationFailed();
    }
  }
}
