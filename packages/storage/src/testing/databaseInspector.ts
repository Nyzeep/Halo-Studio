import Database from "better-sqlite3";

export function executeSql(path: string, sql: string): void {
  const database = new Database(path);
  try {
    database.exec(sql);
  } finally {
    database.close();
  }
}

export function inspectTableNames(path: string): string[] {
  const database = new Database(path, { readonly: true });
  try {
    return (database
      .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
      .all() as Array<{ name: string }>)
      .map(({ name }) => name)
      .filter((name) => !name.startsWith("sqlite_"));
  } finally {
    database.close();
  }
}

export function inspectDatabase(path: string): {
  userVersion: number;
  migrationVersions: number[];
  credentialRows: Array<{ id: string; provider: string; vaultReference: string }>;
  foreignKeys: Array<Record<string, unknown>>;
} {
  const database = new Database(path, { readonly: true });
  try {
    const userVersion = (database.pragma("user_version", { simple: true }) as number);
    const hasMigrationTable = database
      .prepare("SELECT 1 AS present FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'")
      .get() !== undefined;
    const hasCredentialTable = database
      .prepare("SELECT 1 AS present FROM sqlite_master WHERE type = 'table' AND name = 'credential_refs'")
      .get() !== undefined;
    return {
      userVersion,
      migrationVersions: hasMigrationTable
        ? (database.prepare("SELECT version FROM schema_migrations ORDER BY version").all() as Array<{ version: number }>).map(({ version }) => version)
        : [],
      credentialRows: hasCredentialTable
        ? (database.prepare("SELECT id, provider, vault_reference AS vaultReference FROM credential_refs ORDER BY id").all() as Array<{ id: string; provider: string; vaultReference: string }>)
        : [],
      foreignKeys: database.prepare("PRAGMA foreign_key_check").all() as Array<Record<string, unknown>>,
    };
  } finally {
    database.close();
  }
}

export function inspectCredentialStorage(path: string): {
  schemaSql: string;
  columns: string[];
  rows: Array<Record<string, unknown>>;
} {
  const database = new Database(path, { readonly: true });
  try {
    const schema = database
      .prepare(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'credential_refs'",
      )
      .get() as { sql: string } | undefined;
    const columns = database.prepare("PRAGMA table_info(credential_refs)").all() as Array<{
      name: string;
    }>;
    return {
      schemaSql: schema?.sql ?? "",
      columns: columns.map(({ name }) => name),
      rows: database.prepare("SELECT * FROM credential_refs").all() as Array<
        Record<string, unknown>
      >,
    };
  } finally {
    database.close();
  }
}
