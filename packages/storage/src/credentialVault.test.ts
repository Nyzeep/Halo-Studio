import {
  mkdir,
  mkdtemp,
  open as openFile,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import {
  FileCredentialVault,
  syncParentDirectoryAfterRename,
  type SecretProtector,
} from "./credentialVault.js";
import { openDatabase } from "./database.js";
import { inspectCredentialStorage } from "./testing/databaseInspector.js";

const temporaryDirectories: string[] = [];

async function temporaryDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "halo-vault-凭据-"));
  temporaryDirectories.push(directory);
  return directory;
}

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((directory) =>
      rm(directory, { force: true, recursive: true }),
    ),
  );
});

class XorProtector implements SecretProtector {
  isAvailable(): boolean {
    return true;
  }

  protect(value: Buffer): Buffer {
    return Buffer.from(value.map((byte) => byte ^ 0xa5));
  }

  unprotect(value: Buffer): Buffer {
    return Buffer.from(value.map((byte) => byte ^ 0xa5));
  }
}

const unavailableProtector: SecretProtector = {
  isAvailable: () => false,
  protect: () => {
    throw new Error("must not protect");
  },
  unprotect: () => {
    throw new Error("must not unprotect");
  },
};

describe("FileCredentialVault", () => {
  it("系统保护不可用时不写明文", async () => {
    const tempDir = await temporaryDirectory();
    const vault = new FileCredentialVault(tempDir, unavailableProtector);
    expect(vault.isAvailable()).toBe(false);
    await expect(vault.store("provider:key", "plaintext")).rejects.toMatchObject({
      code: "AuthenticationFailed",
    });
    expect(await readdir(tempDir)).toHaveLength(0);
  });

  it("reports unavailable when protector availability throws", async () => {
    const tempDir = await temporaryDirectory();
    const protector: SecretProtector = {
      isAvailable: () => {
        throw new Error("availability-canary-secret");
      },
      protect: (value) => value,
      unprotect: (value) => value,
    };

    expect(new FileCredentialVault(tempDir, protector).isAvailable()).toBe(false);
    expect(await readdir(tempDir)).toHaveLength(0);
  });

  it("stores, reads, overwrites, and deletes an encrypted credential", async () => {
    const tempDir = await temporaryDirectory();
    const vault = new FileCredentialVault(tempDir, new XorProtector());

    expect(vault.isAvailable()).toBe(true);
    expect(await vault.get("provider:key")).toBeNull();
    await vault.store("provider:key", "first-canary-secret");
    expect(await vault.get("provider:key")).toBe("first-canary-secret");
    await vault.store("provider:key", "second-canary-secret");
    expect(await vault.get("provider:key")).toBe("second-canary-secret");

    const files = await readdir(tempDir);
    expect(files).toHaveLength(1);
    const file = join(tempDir, files[0]!);
    const contents = await readFile(file);
    expect(contents.toString("utf8")).not.toContain("canary");
    expect(contents.toString("utf8")).not.toContain("second-canary-secret");
    if (process.platform !== "win32") {
      expect((await stat(file)).mode & 0o777).toBe(0o600);
    }

    await vault.delete("provider:key");
    expect(await vault.get("provider:key")).toBeNull();
    expect(await readdir(tempDir)).toHaveLength(0);
  });

  it("does not use a reference as a file path", async () => {
    const tempDir = await temporaryDirectory();
    const vault = new FileCredentialVault(tempDir, new XorProtector());
    const outside = join(dirname(tempDir), "escaped-credential");

    await vault.store("../../escaped-credential", "path-canary-secret");

    expect(await readdir(tempDir)).toHaveLength(1);
    await expect(stat(outside)).rejects.toMatchObject({ code: "ENOENT" });
    expect(await vault.get("../../escaped-credential")).toBe("path-canary-secret");
  });

  it("maps protector failures to a fixed error without leaking paths or secrets", async () => {
    const tempDir = await temporaryDirectory();
    const canary = "protector-canary-secret";
    const throwingProtector: SecretProtector = {
      isAvailable: () => true,
      protect: () => {
        throw new Error(`${tempDir}:${canary}`);
      },
      unprotect: () => {
        throw new Error(`${tempDir}:${canary}`);
      },
    };
    const vault = new FileCredentialVault(tempDir, throwingProtector);

    await expect(vault.store("provider:key", canary)).rejects.toMatchObject({
      code: "AuthenticationFailed",
      message: "Credential protection failed",
    });
    await expect(vault.get("provider:key")).resolves.toBeNull();
    expect(await readdir(tempDir)).toHaveLength(0);

    const encryptedFile = new FileCredentialVault(tempDir, new XorProtector());
    await encryptedFile.store("provider:key", canary);
    const failingReader = new FileCredentialVault(tempDir, throwingProtector);
    let readError: unknown;
    try {
      await failingReader.get("provider:key");
    } catch (error) {
      readError = error;
    }
    expect(readError).toMatchObject({
      code: "AuthenticationFailed",
      message: "Credential protection failed",
    });
    expect(String(readError)).not.toContain(canary);
    expect(String(readError)).not.toContain(tempDir);
  });

  it("bounds ciphertext output and file reads", async () => {
    const tempDir = await temporaryDirectory();
    const oversizedProtector: SecretProtector = {
      isAvailable: () => true,
      protect: () => Buffer.alloc(2 * 1024 * 1024),
      unprotect: (value) => value,
    };
    const oversizedVault = new FileCredentialVault(tempDir, oversizedProtector);
    await expect(oversizedVault.store("provider:key", "secret")).rejects.toMatchObject({
      code: "AuthenticationFailed",
      message: "Credential protection failed",
    });
    expect(await readdir(tempDir)).toHaveLength(0);
    const emptyVault = new FileCredentialVault(tempDir, {
      isAvailable: () => true,
      protect: () => Buffer.alloc(0),
      unprotect: (value) => value,
    });
    await expect(emptyVault.store("provider:key", "secret")).rejects.toMatchObject({
      code: "AuthenticationFailed",
    });
    expect(await readdir(tempDir)).toHaveLength(0);

    const vault = new FileCredentialVault(tempDir, new XorProtector());
    await vault.store("provider:key", "secret");
    const [filename] = await readdir(tempDir);
    const sparseFile = await openFile(join(tempDir, filename!), "w");
    await sparseFile.truncate(2 * 1024 * 1024);
    await sparseFile.close();
    await expect(vault.get("provider:key")).rejects.toMatchObject({
      code: "AuthenticationFailed",
      message: "Credential protection failed",
    });
    await writeFile(join(tempDir, filename!), Buffer.alloc(0));
    await expect(vault.get("provider:key")).rejects.toMatchObject({
      code: "AuthenticationFailed",
      message: "Credential protection failed",
    });
  });

  it("clears protector buffers before I/O and after decrypt", async () => {
    const tempDir = await temporaryDirectory();
    let capturedPlaintext: Buffer | undefined;
    let capturedCiphertext: Buffer | undefined;
    let capturedUnprotected: Buffer | undefined;
    const protector: SecretProtector = {
      isAvailable: () => true,
      protect: (value) => {
        capturedPlaintext = value;
        return Buffer.from(value.map((byte) => byte ^ 0xa5));
      },
      unprotect: (value) => {
        capturedCiphertext = value;
        capturedUnprotected = Buffer.from(value.map((byte) => byte ^ 0xa5));
        return capturedUnprotected;
      },
    };
    const vault = new FileCredentialVault(tempDir, protector);
    const storing = vault.store("provider:key", "zeroization-canary");
    expect(capturedPlaintext).toBeDefined();
    const clearedBeforeAwait = capturedPlaintext!.every((byte) => byte === 0);
    await storing;
    expect(clearedBeforeAwait).toBe(true);
    expect(capturedPlaintext!.every((byte) => byte === 0)).toBe(true);

    await expect(vault.get("provider:key")).resolves.toBe("zeroization-canary");
    expect(capturedCiphertext!.every((byte) => byte === 0)).toBe(true);
    expect(capturedUnprotected!.every((byte) => byte === 0)).toBe(true);
  });

  it("bounds and validates unprotected plaintext before copying", async () => {
    const tempDir = await temporaryDirectory();
    const writer = new FileCredentialVault(tempDir, new XorProtector());
    await writer.store("provider:key", "seed");
    const maxSecretBytes = 1024 * 1024;

    let oversizedOutput = Buffer.alloc(maxSecretBytes + 1, 0x61);
    const oversized = new FileCredentialVault(tempDir, {
      isAvailable: () => true,
      protect: (value) => value,
      unprotect: () => oversizedOutput,
    });
    await expect(oversized.get("provider:key")).rejects.toMatchObject({
      code: "AuthenticationFailed",
      message: "Credential protection failed",
    });
    expect(oversizedOutput.every((byte) => byte === 0)).toBe(true);

    let emptyOutput = Buffer.alloc(0);
    const empty = new FileCredentialVault(tempDir, {
      isAvailable: () => true,
      protect: (value) => value,
      unprotect: () => emptyOutput,
    });
    await expect(empty.get("provider:key")).rejects.toMatchObject({
      code: "AuthenticationFailed",
    });

    let maximumOutput = Buffer.alloc(maxSecretBytes, 0x61);
    const maximum = new FileCredentialVault(tempDir, {
      isAvailable: () => true,
      protect: (value) => value,
      unprotect: () => maximumOutput,
    });
    const maximumValue = await maximum.get("provider:key");
    expect(Buffer.byteLength(maximumValue!, "utf8")).toBe(maxSecretBytes);
    expect(maximumOutput.every((byte) => byte === 0)).toBe(true);

    let invalidUtf8Output = Buffer.from([0xff]);
    const invalidUtf8 = new FileCredentialVault(tempDir, {
      isAvailable: () => true,
      protect: (value) => value,
      unprotect: () => invalidUtf8Output,
    });
    await expect(invalidUtf8.get("provider:key")).rejects.toMatchObject({
      code: "AuthenticationFailed",
    });
    expect(invalidUtf8Output.every((byte) => byte === 0)).toBe(true);

    const overlongUtf8Bytes = Buffer.from(
      "界".repeat(Math.floor(maxSecretBytes / 3) + 1),
      "utf8",
    );
    let overlongUtf8Output = overlongUtf8Bytes;
    const overlongUtf8 = new FileCredentialVault(tempDir, {
      isAvailable: () => true,
      protect: (value) => value,
      unprotect: () => overlongUtf8Output,
    });
    await expect(overlongUtf8.get("provider:key")).rejects.toMatchObject({
      code: "AuthenticationFailed",
    });
    expect(overlongUtf8Output.every((byte) => byte === 0)).toBe(true);
  });

  it("supports an unprotect Buffer alias while still clearing it", async () => {
    const tempDir = await temporaryDirectory();
    const writer = new FileCredentialVault(tempDir, new XorProtector());
    await writer.store("provider:key", "alias-secret");
    let alias: Buffer | undefined;
    const reader = new FileCredentialVault(tempDir, {
      isAvailable: () => true,
      protect: (value) => value,
      unprotect: (value) => {
        alias = value;
        for (let index = 0; index < value.length; index += 1) {
          value[index] = value[index]! ^ 0xa5;
        }
        return value;
      },
    });

    await expect(reader.get("provider:key")).resolves.toBe("alias-secret");
    expect(alias!.every((byte) => byte === 0)).toBe(true);
  });

  it("preserves a leading UTF-8 BOM in credential values", async () => {
    const tempDir = await temporaryDirectory();
    const vault = new FileCredentialVault(tempDir, new XorProtector());
    const value = "\uFEFFsecret";

    await vault.store("provider:key", value);

    await expect(vault.get("provider:key")).resolves.toBe(value);
  });

  it("syncs a POSIX parent directory, skips Windows, and redacts sync failures", async () => {
    let syncCount = 0;
    let closeCount = 0;
    await syncParentDirectoryAfterRename("ignored", "linux", async () => ({
      sync: async () => {
        syncCount += 1;
      },
      close: async () => {
        closeCount += 1;
      },
    }));
    expect(syncCount).toBe(1);
    expect(closeCount).toBe(1);

    let windowsOpenCalled = false;
    await syncParentDirectoryAfterRename("ignored", "win32", async () => {
      windowsOpenCalled = true;
      throw new Error("windows-directory-sync-canary-secret");
    });
    expect(windowsOpenCalled).toBe(false);

    let failedCloseCount = 0;
    let syncError: unknown;
    try {
      await syncParentDirectoryAfterRename("ignored", "linux", async () => ({
        sync: async () => {
          throw new Error("directory-sync-canary-secret");
        },
        close: async () => {
          failedCloseCount += 1;
        },
      }));
    } catch (error) {
      syncError = error;
    }
    expect(syncError).toMatchObject({
      code: "AuthenticationFailed",
      message: "Credential protection failed",
    });
    expect(String(syncError)).not.toContain("directory-sync-canary-secret");
    expect(failedCloseCount).toBe(1);
  });

  it("fails closed for invalid runtime inputs and unavailable delete", async () => {
    const tempDir = await temporaryDirectory();
    const vault = new FileCredentialVault(tempDir, new XorProtector());
    const unavailable = new FileCredentialVault(tempDir, unavailableProtector);

    await expect(vault.store("", "secret")).rejects.toMatchObject({
      code: "AuthenticationFailed",
    });
    await expect(
      vault.store("provider:key", 42 as unknown as string),
    ).rejects.toMatchObject({ code: "AuthenticationFailed" });
    await expect(unavailable.delete("provider:key")).rejects.toMatchObject({
      code: "AuthenticationFailed",
    });
    const identityProtector: SecretProtector = {
      isAvailable: () => true,
      protect: (value) => value,
      unprotect: (value) => value,
    };
    await expect(
      new FileCredentialVault(tempDir, identityProtector).store(
        "provider:key",
        "plaintext-canary-secret",
      ),
    ).rejects.toMatchObject({ code: "AuthenticationFailed" });
    expect(await readdir(tempDir)).toHaveLength(0);
  });

  it("stores only the vault reference in SQLite", async () => {
    const tempDir = await temporaryDirectory();
    const databasePath = join(tempDir, "storage.sqlite");
    const vaultDirectory = join(tempDir, "vault");
    await mkdir(vaultDirectory);
    const vault = new FileCredentialVault(vaultDirectory, new XorProtector());
    const database = openDatabase(databasePath);
    const secret = "sqlite-canary-plaintext-secret";

    try {
      await vault.store("provider:key", secret);
      database.credentialReferences.save({
        id: "credential-ref-1",
        provider: "pi",
        vaultReference: "provider:key",
      });
    } finally {
      database.close();
    }

    const inspection = inspectCredentialStorage(databasePath);
    const serialized = JSON.stringify(inspection);
    const rawDatabase = await readFile(databasePath);
    expect(serialized).toContain("provider:key");
    expect(inspection.columns).toEqual([
      "id",
      "profile_id",
      "provider",
      "vault_reference",
      "created_at",
      "updated_at",
    ]);
    expect(inspection.rows).toHaveLength(1);
    expect(serialized).not.toContain(secret);
    expect(serialized).not.toContain("plaintext");
    expect(serialized).not.toContain("canary");
    expect(rawDatabase.includes(Buffer.from(secret, "utf8"))).toBe(false);
    expect(rawDatabase.includes(Buffer.from("plaintext", "utf8"))).toBe(false);
    expect(rawDatabase.includes(Buffer.from("canary", "utf8"))).toBe(false);
  });
});
