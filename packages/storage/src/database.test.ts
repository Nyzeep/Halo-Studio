import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import Database from "better-sqlite3";
import { afterEach, describe, expect, it } from "vitest";

import {
  openDatabase,
  openDatabaseWithConnectionFactory,
  type DatabaseConnectionFactory,
} from "./database.js";
import { migrateDatabaseWithMigrations } from "./migrations.js";
import { createCredentialReferenceRepository } from "./repositories.js";
import {
  executeSql,
  inspectDatabase,
  inspectTableNames,
} from "./testing/databaseInspector.js";

const temporaryDirectories: string[] = [];

async function temporaryDatabasePath(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "halo-storage-数据库-"));
  temporaryDirectories.push(directory);
  return join(directory, "halo studio.sqlite");
}

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((directory) =>
      rm(directory, { force: true, recursive: true }),
    ),
  );
});

describe("database migrations", () => {
  it("advances injected migrations one transaction at a time", () => {
    const database = new Database(":memory:");
    try {
      database.exec(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
      );
      const migrations = [
        {
          version: 1,
          apply(connection: Database.Database) {
            connection.exec("CREATE TABLE migration_steps (name TEXT NOT NULL)");
            connection.prepare("INSERT INTO migration_steps(name) VALUES (?)").run("v1");
          },
        },
        {
          version: 2,
          apply(connection: Database.Database) {
            expect(connection.pragma("user_version", { simple: true })).toBe(1);
            expect(
              (connection.prepare("SELECT version FROM schema_migrations").all() as Array<{ version: number }>).map(({ version }) => version),
            ).toEqual([1]);
            connection.prepare("INSERT INTO migration_steps(name) VALUES (?)").run("v2");
          },
        },
      ];

      expect(migrateDatabaseWithMigrations(database, migrations)).toBe(2);
      expect(
        (database.prepare("SELECT name FROM migration_steps ORDER BY rowid").all() as Array<{ name: string }>).map(({ name }) => name),
      ).toEqual(["v1", "v2"]);
      expect(
        (database.prepare("SELECT version FROM schema_migrations ORDER BY version").all() as Array<{ version: number }>).map(({ version }) => version),
      ).toEqual([1, 2]);
      expect(database.pragma("user_version", { simple: true })).toBe(2);
    } finally {
      database.close();
    }
  });

  it("rolls back only the failing injected migration", () => {
    const database = new Database(":memory:");
    try {
      database.exec(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL); CREATE TABLE migration_steps (name TEXT NOT NULL)",
      );
      expect(() =>
        migrateDatabaseWithMigrations(database, [
          {
            version: 1,
            apply(connection: Database.Database) {
              connection.prepare("INSERT INTO migration_steps(name) VALUES (?)").run("v1");
            },
          },
          {
            version: 2,
            apply(connection: Database.Database) {
              connection.prepare("INSERT INTO migration_steps(name) VALUES (?)").run("v2");
              throw new Error("injected migration failure");
            },
          },
        ]),
      ).toThrow();
      expect(database.pragma("user_version", { simple: true })).toBe(1);
      expect(
        (database.prepare("SELECT version FROM schema_migrations").all() as Array<{ version: number }>).map(({ version }) => version),
      ).toEqual([1]);
      expect(
        (database.prepare("SELECT name FROM migration_steps").all() as Array<{ name: string }>).map(({ name }) => name),
      ).toEqual(["v1"]);
    } finally {
      database.close();
    }
  });

  it("creates schema version one and reports read-write health", async () => {
    const database = openDatabase(await temporaryDatabasePath());

    try {
      expect(database.health()).toEqual({
        mode: "read-write",
        schemaVersion: 1,
      });
    } finally {
      database.close();
    }
  });

  it("creates only the business tables and is idempotent", async () => {
    const path = await temporaryDatabasePath();
    const first = openDatabase(path);
    first.close();
    const second = openDatabase(path);
    second.close();

    expect(inspectTableNames(path)).toEqual([
      "audit_events",
      "config_backups",
      "credential_refs",
      "profiles",
      "runtime_bindings",
      "schema_migrations",
      "workspaces",
    ]);
    expect(inspectDatabase(path)).toMatchObject({
      userVersion: 1,
      migrationVersions: [1],
    });
  });

  it("rolls back a failed migration and reopens read-only with redacted diagnostics", async () => {
    const path = await temporaryDatabasePath();
    executeSql(path, "CREATE VIEW workspaces AS SELECT 1 AS id");

    const database = openDatabase(path);
    try {
      expect(database.health()).toEqual({
        mode: "read-only-recovery",
        schemaVersion: 0,
      });
      expect(database.diagnostics()).toEqual({
        code: "MigrationFailed",
        message: "Database migration failed",
      });
      expect(database.diagnostics()).not.toHaveProperty("path");
      let writeError: unknown;
      try {
        database.credentialReferences.save({
          id: "recovery-write",
          provider: "pi",
          vaultReference: "reference-only",
        });
      } catch (error) {
        writeError = error;
      }
      expect(writeError).toMatchObject({ code: "MigrationFailed" });
      let readError: unknown;
      try {
        database.credentialReferences.get("recovery-read");
      } catch (error) {
        readError = error;
      }
      expect(readError).toMatchObject({
        code: "MigrationFailed",
        message: "Database migration failed",
      });
    } finally {
      database.close();
    }

    expect(inspectDatabase(path)).toMatchObject({
      userVersion: 0,
      migrationVersions: [],
    });
    expect(inspectTableNames(path)).toEqual([]);
  });

  it("fails closed when the failed write connection cannot be closed", async () => {
    const path = await temporaryDatabasePath();
    executeSql(path, "CREATE VIEW workspaces AS SELECT 1 AS id");
    let readonlyOpenCount = 0;
    const factory: DatabaseConnectionFactory = (databasePath, options) => {
      const connection = new Database(databasePath, options);
      if (options?.readonly === true) {
        readonlyOpenCount += 1;
        return connection;
      }
      const close = connection.close.bind(connection);
      Object.defineProperty(connection, "close", {
        configurable: true,
        value: () => {
          close();
          throw new Error("close-canary-secret");
        },
      });
      return connection;
    };

    let error: unknown;
    try {
      openDatabaseWithConnectionFactory(path, factory);
    } catch (caught) {
      error = caught;
    }
    expect(error).toMatchObject({
      code: "MigrationFailed",
      message: "Database migration failed",
    });
    expect(String(error)).not.toContain("close-canary-secret");
    expect(readonlyOpenCount).toBe(0);
  });

  it("closes a write connection when initialization fails", async () => {
    const path = await temporaryDatabasePath();
    let closeCount = 0;
    const factory: DatabaseConnectionFactory = (databasePath, options) => {
      const connection = new Database(databasePath, options);
      const close = connection.close.bind(connection);
      const pragma = connection.pragma.bind(connection);
      Object.defineProperties(connection, {
        close: {
          configurable: true,
          value: () => {
            closeCount += 1;
            close();
          },
        },
        pragma: {
          configurable: true,
          value: (source: string, pragmaOptions?: Database.PragmaOptions) => {
            if (source === "foreign_keys = ON") {
              throw new Error("write-init-canary-secret");
            }
            return pragma(source, pragmaOptions);
          },
        },
      });
      return connection;
    };

    let error: unknown;
    try {
      openDatabaseWithConnectionFactory(path, factory);
    } catch (caught) {
      error = caught;
    }
    expect(error).toMatchObject({
      code: "MigrationFailed",
      message: "Database migration failed",
    });
    expect(String(error)).not.toContain("write-init-canary-secret");
    expect(closeCount).toBe(1);
  });

  it("closes a recovery connection when its initialization fails", async () => {
    const path = await temporaryDatabasePath();
    executeSql(path, "CREATE VIEW workspaces AS SELECT 1 AS id");
    let closeCount = 0;
    const factory: DatabaseConnectionFactory = (databasePath, options) => {
      const connection = new Database(databasePath, options);
      const close = connection.close.bind(connection);
      Object.defineProperty(connection, "close", {
        configurable: true,
        value: () => {
          closeCount += 1;
          close();
        },
      });
      if (options?.readonly === true) {
        Object.defineProperty(connection, "pragma", {
          configurable: true,
          value: () => {
            throw new Error("recovery-init-canary-secret");
          },
        });
      }
      return connection;
    };

    let error: unknown;
    try {
      openDatabaseWithConnectionFactory(path, factory);
    } catch (caught) {
      error = caught;
    }
    expect(error).toMatchObject({
      code: "MigrationFailed",
      message: "Database migration failed",
    });
    expect(String(error)).not.toContain("recovery-init-canary-secret");
    expect(closeCount).toBe(2);
  });

  it("uses a real readonly connection during recovery", async () => {
    const path = await temporaryDatabasePath();
    const initial = openDatabase(path);
    initial.credentialReferences.save({
      id: "existing-reference",
      provider: "pi",
      vaultReference: "provider:key",
    });
    initial.close();
    executeSql(path, "PRAGMA user_version = 0");

    const preparedSql: string[] = [];
    const factory: DatabaseConnectionFactory = (databasePath, options) => {
      const connection = new Database(databasePath, options);
      if (options?.readonly === true) {
        const prepare = connection.prepare.bind(connection);
        Object.defineProperty(connection, "prepare", {
          configurable: true,
          value: (sql: string) => {
            preparedSql.push(sql);
            return prepare(sql);
          },
        });
      }
      return connection;
    };

    const database = openDatabaseWithConnectionFactory(path, factory);
    try {
      expect(database.health()).toEqual({
        mode: "read-only-recovery",
        schemaVersion: 0,
      });
      expect(database.credentialReferences.get("existing-reference")).toEqual({
        id: "existing-reference",
        provider: "pi",
        vaultReference: "provider:key",
      });
      preparedSql.length = 0;

      let saveError: unknown;
      try {
        database.credentialReferences.save({
          id: "readonly-save",
          provider: "pi",
          vaultReference: "provider:save",
        });
      } catch (error) {
        saveError = error;
      }
      let deleteError: unknown;
      try {
        database.credentialReferences.delete("existing-reference");
      } catch (error) {
        deleteError = error;
      }
      expect(saveError).toMatchObject({ code: "MigrationFailed" });
      expect(deleteError).toMatchObject({ code: "MigrationFailed" });
      expect(preparedSql.some((sql) => sql.includes("INSERT INTO credential_refs"))).toBe(true);
      expect(preparedSql.some((sql) => sql.includes("DELETE FROM credential_refs"))).toBe(true);
    } finally {
      database.close();
    }
    expect(inspectDatabase(path).credentialRows).toEqual([
      { id: "existing-reference", provider: "pi", vaultReference: "provider:key" },
    ]);
  });

  it("fails closed for unknown or inconsistent schema versions", async () => {
    const unknownPath = await temporaryDatabasePath();
    executeSql(
      unknownPath,
      "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL); INSERT INTO schema_migrations(version, applied_at) VALUES (2, '2026-01-01T00:00:00.000Z'); PRAGMA user_version = 2;",
    );
    const unknown = openDatabase(unknownPath);
    expect(unknown.health()).toEqual({ mode: "read-only-recovery", schemaVersion: 2 });
    unknown.close();

    const inconsistentPath = await temporaryDatabasePath();
    executeSql(
      inconsistentPath,
      "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL); INSERT INTO schema_migrations(version, applied_at) VALUES (1, '2026-01-01T00:00:00.000Z'); PRAGMA user_version = 0;",
    );
    const inconsistent = openDatabase(inconsistentPath);
    expect(inconsistent.health()).toEqual({ mode: "read-only-recovery", schemaVersion: 0 });
    inconsistent.close();
  });

  it("uses foreign keys and parameterized credential reference storage", async () => {
    const path = await temporaryDatabasePath();
    const database = openDatabase(path);
    try {
      const id = "ref-' OR 1=1 --";
      database.credentialReferences.save({
        id,
        provider: "opencode",
        vaultReference: "provider:key",
      });
      expect(database.credentialReferences.get(id)).toEqual({
        id,
        provider: "opencode",
        vaultReference: "provider:key",
      });
      expect(inspectDatabase(path).credentialRows).toEqual([
        { id, provider: "opencode", vaultReference: "provider:key" },
      ]);
      expect(inspectDatabase(path).foreignKeys).toEqual([]);
    } finally {
      database.close();
    }
  });

  it("rejects accessor and proxy repository inputs without executing them", async () => {
    const database = openDatabase(await temporaryDatabasePath());
    try {
      let getterCalled = false;
      const accessorInput = Object.defineProperty(
        { provider: "pi", vaultReference: "reference-only" },
        "id",
        {
          enumerable: true,
          get() {
            getterCalled = true;
            throw new Error("repository-getter-canary-secret");
          },
        },
      );
      let accessorError: unknown;
      try {
        database.credentialReferences.save(accessorInput as never);
      } catch (error) {
        accessorError = error;
      }
      expect(accessorError).toMatchObject({ code: "ProtocolViolation" });
      expect(String(accessorError)).not.toContain("repository-getter-canary-secret");
      expect(getterCalled).toBe(false);

      const proxyInput = new Proxy(
        { id: "proxy", provider: "pi", vaultReference: "reference-only" },
        {
          get() {
            throw new Error("repository-proxy-canary-secret");
          },
          getPrototypeOf() {
            throw new Error("repository-prototype-canary-secret");
          },
          ownKeys() {
            throw new Error("repository-keys-canary-secret");
          },
        },
      );
      let proxyError: unknown;
      try {
        database.credentialReferences.save(proxyInput as never);
      } catch (error) {
        proxyError = error;
      }
      expect(proxyError).toMatchObject({ code: "ProtocolViolation" });
      expect(String(proxyError)).not.toContain("canary-secret");

      let getProxyError: unknown;
      try {
        database.credentialReferences.get(proxyInput as never);
      } catch (error) {
        getProxyError = error;
      }
      expect(getProxyError).toMatchObject({ code: "ProtocolViolation" });
      let deleteProxyError: unknown;
      try {
        database.credentialReferences.delete(proxyInput as never);
      } catch (error) {
        deleteProxyError = error;
      }
      expect(deleteProxyError).toMatchObject({ code: "ProtocolViolation" });
    } finally {
      database.close();
    }
  });

  it("maps invalid values and storage failures to fixed boundary errors", async () => {
    const database = openDatabase(await temporaryDatabasePath());
    try {
      let invalidProviderError: unknown;
      try {
        database.credentialReferences.save({
          id: "invalid-provider",
          provider: "other" as never,
          vaultReference: "reference-only",
        });
      } catch (error) {
        invalidProviderError = error;
      }
      expect(invalidProviderError).toMatchObject({ code: "ProtocolViolation" });

      let constraintError: unknown;
      try {
        database.credentialReferences.save({
          id: "missing-profile",
          provider: "pi",
          vaultReference: "reference-only",
          profileId: "profile-does-not-exist",
        });
      } catch (error) {
        constraintError = error;
      }
      expect(constraintError).toMatchObject({
        code: "MigrationFailed",
        message: "Database migration failed",
      });
    } finally {
      database.close();
    }

    const databaseFailure = {
      prepare() {
        throw new Error("repository-database-canary-secret");
      },
    } as unknown as Database.Database;
    const repository = createCredentialReferenceRepository(databaseFailure, false);
    for (const operation of [
      () => repository.save({ id: "id", provider: "pi", vaultReference: "ref" }),
      () => repository.get("id"),
      () => repository.delete("id"),
    ]) {
      let error: unknown;
      try {
        operation();
      } catch (caught) {
        error = caught;
      }
      expect(error).toMatchObject({
        code: "MigrationFailed",
        message: "Database migration failed",
      });
      expect(String(error)).not.toContain("repository-database-canary-secret");
    }

    let rowGetterCalled = false;
    const malformedRow = Object.defineProperty(
      { id: "id", provider: "pi" },
      "vaultReference",
      {
        enumerable: true,
        get() {
          rowGetterCalled = true;
          throw new Error("repository-row-canary-secret");
        },
      },
    );
    const malformedRepository = createCredentialReferenceRepository(
      {
        prepare() {
          return { get: () => malformedRow };
        },
      } as unknown as Database.Database,
      false,
    );
    let malformedError: unknown;
    try {
      malformedRepository.get("id");
    } catch (error) {
      malformedError = error;
    }
    expect(malformedError).toMatchObject({ code: "MigrationFailed" });
    expect(String(malformedError)).not.toContain("repository-row-canary-secret");
    expect(rowGetterCalled).toBe(false);
  });
});
