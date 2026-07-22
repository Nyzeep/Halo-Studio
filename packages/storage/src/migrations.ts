import type Database from "better-sqlite3";

export const CURRENT_SCHEMA_VERSION = 1;

export interface MigrationDefinition {
  readonly version: number;
  readonly apply: (database: Database.Database) => void;
}

const productionMigrations: readonly MigrationDefinition[] = [
  {
    version: 1,
    apply(database) {
      database.exec(`
        CREATE TABLE schema_migrations (
          version INTEGER PRIMARY KEY CHECK (version > 0),
          applied_at TEXT NOT NULL
        );

        CREATE TABLE workspaces (
          id TEXT PRIMARY KEY CHECK (length(id) > 0),
          path TEXT NOT NULL UNIQUE CHECK (length(path) > 0),
          trusted INTEGER NOT NULL DEFAULT 0 CHECK (trusted IN (0, 1)),
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE runtime_bindings (
          id TEXT PRIMARY KEY CHECK (length(id) > 0),
          workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
          provider TEXT NOT NULL CHECK (provider IN ('pi', 'opencode')),
          executable_path TEXT NOT NULL CHECK (length(executable_path) > 0),
          version TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          UNIQUE (workspace_id, provider)
        );

        CREATE TABLE profiles (
          id TEXT PRIMARY KEY CHECK (length(id) > 0),
          name TEXT NOT NULL CHECK (length(name) > 0),
          provider TEXT NOT NULL CHECK (provider IN ('pi', 'opencode')),
          config_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE credential_refs (
          id TEXT PRIMARY KEY CHECK (length(id) > 0),
          profile_id TEXT REFERENCES profiles(id) ON DELETE CASCADE,
          provider TEXT NOT NULL CHECK (provider IN ('pi', 'opencode')),
          vault_reference TEXT NOT NULL CHECK (length(vault_reference) > 0),
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE config_backups (
          id TEXT PRIMARY KEY CHECK (length(id) > 0),
          workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
          provider TEXT NOT NULL CHECK (provider IN ('pi', 'opencode')),
          backup_path TEXT NOT NULL CHECK (length(backup_path) > 0),
          created_at TEXT NOT NULL
        );

        CREATE TABLE audit_events (
          id TEXT PRIMARY KEY CHECK (length(id) > 0),
          workspace_id TEXT REFERENCES workspaces(id) ON DELETE SET NULL,
          event_type TEXT NOT NULL CHECK (length(event_type) > 0),
          details_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL
        );
      `);

    },
  },
];

function tableExists(database: Database.Database, table: string): boolean {
  return database
    .prepare(
      "SELECT 1 AS present FROM sqlite_master WHERE type = 'table' AND name = ?",
    )
    .get(table) !== undefined;
}

function migrationVersions(database: Database.Database): number[] {
  if (!tableExists(database, "schema_migrations")) {
    return [];
  }
  return (
    database
      .prepare("SELECT version FROM schema_migrations ORDER BY version")
      .all() as Array<{ version: number }>
  ).map(({ version }) => version);
}

function assertVersionConsistency(
  userVersion: number,
  versions: readonly number[],
  targetVersion: number,
): void {
  if (!Number.isSafeInteger(userVersion) || userVersion < 0) {
    throw new Error("Invalid schema version");
  }
  if (userVersion > targetVersion) {
    throw new Error("Database schema is newer than this application");
  }
  if (versions.length !== userVersion) {
    throw new Error("Schema version metadata is inconsistent");
  }
  for (let index = 0; index < versions.length; index += 1) {
    if (versions[index] !== index + 1) {
      throw new Error("Schema migration sequence is invalid");
    }
  }
}

export function migrateDatabase(database: Database.Database): number {
  return migrateDatabaseWithMigrations(database, productionMigrations);
}

export function migrateDatabaseWithMigrations(
  database: Database.Database,
  migrations: readonly MigrationDefinition[],
): number {
  const targetVersion = migrations.reduce(
    (maximum, migration) => Math.max(maximum, migration.version),
    0,
  );
  const initialVersion = database.pragma("user_version", {
    simple: true,
  }) as number;
  const appliedVersions = migrationVersions(database);
  assertVersionConsistency(initialVersion, appliedVersions, targetVersion);

  let currentVersion = initialVersion;
  for (const migration of migrations) {
    if (migration.version <= currentVersion) {
      continue;
    }
    if (migration.version !== currentVersion + 1) {
      throw new Error("Schema migration sequence is invalid");
    }
    database.transaction(() => {
      migration.apply(database);
      if (!tableExists(database, "schema_migrations")) {
        throw new Error("Schema migration metadata table is missing");
      }
      database
        .prepare(
          "INSERT INTO schema_migrations(version, applied_at) VALUES (?, ?)",
        )
        .run(migration.version, new Date().toISOString());
      database.pragma(`user_version = ${migration.version}`);
    })();
    currentVersion = migration.version;
    assertVersionConsistency(
      currentVersion,
      migrationVersions(database),
      targetVersion,
    );
  }

  const finalVersion = database.pragma("user_version", {
    simple: true,
  }) as number;
  assertVersionConsistency(finalVersion, migrationVersions(database), targetVersion);
  if (finalVersion !== targetVersion) {
    throw new Error("Database schema is not current");
  }
  return finalVersion;
}
