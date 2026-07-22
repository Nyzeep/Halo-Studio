import { chmod, mkdtemp, readFile, writeFile, rm, readdir, mkdir, rename, stat, symlink } from "node:fs/promises";
import { watch, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AgentKind } from "@halo-studio/contracts";
import { ConfigTransaction, ConfigBackupUnavailable, ConfigConflict, ConfigPreviewUnavailable, ConfigRecoveryError, ConfigWriteError, UnsafeConfigError, setConfigTransactionTestHooks } from "./configTransaction.js";
import { TargetRegistry } from "./targetRegistry.js";
import { createTwoFilesPatch } from "./unifiedDiff.js";
import { applyJsoncPatch } from "./jsoncPatch.js";
import { atomicWrite } from "./atomicWrite.js";
import { FileCredentialVault, type CredentialVault, type SecretProtector } from "@halo-studio/storage";

const dirs: string[] = [];
afterEach(async () => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  await Promise.all(dirs.splice(0).map((d) => rm(d, { recursive: true, force: true })));
});

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "halo-config-中文 space-")); dirs.push(root);
  const file = join(root, "settings.jsonc");
  await writeFile(file, "// keep this comment\n{\n  \"known\": true,\n  \"apiKey\": \"top-secret-canary\",\n  \"unknown\": 7\n}\n", "utf8");
  const registry = new TargetRegistry();
  const targetId = await registry.register({ scope: "global", owner: "pi", path: file, format: "jsonc", source: "native", writable: true, allowedRoot: root });
  return { root, file, registry, targetId };
}

function memoryVault(onStore?: () => Promise<void>): CredentialVault {
  const values = new Map<string, string>();
  return {
    isAvailable: () => true,
    store: async (reference, value) => {
      await onStore?.();
      values.set(reference, value);
    },
    get: async (reference) => values.get(reference) ?? null,
    delete: async (reference) => { values.delete(reference); },
  };
}

describe("unified diff", () => {
  it("emits standard two-file patch headers", () => {
    const patch = createTwoFilesPatch("a.jsonc", "b.jsonc", "{\n  \"x\": 1\n}\n", "{\n  \"x\": 2\n}\n");
    expect(patch).toContain("--- a.jsonc"); expect(patch).toContain("+++ b.jsonc"); expect(patch).toContain("@@");
  });

  it("redacts nested multiline secret values without damaging patch markers", () => {
    const canaries = ["api-canary", "token-canary", "secret-canary", "password-canary", "authorization-canary"];
    const oldText = `{
  "nested": {
    "apiKey"
      :
      "${canaries[0]}",
    "token": [
      "${canaries[1]}"
    ],
    "secret": {
      "value": "${canaries[2]}"
    },
    "password":
      "${canaries[3]}",
    "authorization": "${canaries[4]}"
  }
}\n`;
    const patch = createTwoFilesPatch("old.jsonc", "new.jsonc", oldText, oldText.replace("nested", "changed"));
    for (const canary of canaries) expect(patch).not.toContain(canary);
    expect(patch).toContain("--- old.jsonc"); expect(patch).toContain("+++ new.jsonc"); expect(patch).toContain("@@");
  });
});

describe("JSONC formatting", () => {
  it("preserves CRLF and tab indentation", () => {
    const input = "{\r\n\t\"existing\": true\r\n}\r\n";
    const output = applyJsoncPatch(input, [{ op: "set", path: ["added"], value: 1 }]);
    expect(output).toBe("{\r\n\t\"existing\": true,\r\n\t\"added\": 1\r\n}\r\n");
  });

  it("preserves four-space indentation", () => {
    const input = "{\n    \"existing\": true\n}\n";
    const output = applyJsoncPatch(input, [{ op: "set", path: ["added"], value: 1 }]);
    expect(output).toContain("\n    \"added\": 1\n");
  });
});

describe("config transaction", () => {
  it("preserves JSONC comments and unknown fields while redacting secret diff values", async () => {
    const { file, registry, targetId } = await fixture();
    const tx = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await tx.preview(targetId, [{ op: "set", path: ["known"], value: false }, { op: "set", path: ["token"], value: "new-secret-canary" }]);
    expect(preview.restartRequired).toEqual(["pi"]);
    expect(preview.unifiedDiff).not.toContain("top-secret-canary");
    expect(preview.unifiedDiff).not.toContain("new-secret-canary");
    expect(preview.unifiedDiff).not.toContain(file);
    expect(preview.unifiedDiff).toContain("apiKey");
    await tx.commit(preview.previewId);
    const text = await readFile(file, "utf8");
    expect(text).toContain("keep this comment"); expect(text).toContain("unknown"); expect(text).toContain("false");
  });

  it("rejects external edits as a fixed ConfigConflict", async () => {
    const { file, registry, targetId } = await fixture();
    const tx = new ConfigTransaction(registry);
    const preview = await tx.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    await writeFile(file, "{}\n", "utf8");
    await expect(tx.commit(preview.previewId)).rejects.toBeInstanceOf(ConfigConflict);
  });

  it("rejects Pi MCP targets at registration", async () => {
    const { root, registry } = await fixture();
    await expect(registry.register({ scope: "project", owner: "pi", kind: "mcp", path: join(root, "mcp.json"), format: "jsonc", source: "managed", writable: true, allowedRoot: root })).rejects.toBeInstanceOf(UnsafeConfigError);
  });

  it("maps invalid documents and expired previews to fixed safe errors", async () => {
    const { file, root, registry, targetId } = await fixture();
    const secret = "invalid-document-canary";
    await writeFile(file, `{ \"password\": \"${secret}\",`, "utf8");
    let parseError: unknown;
    try { await new ConfigTransaction(registry).preview(targetId, [{ op: "set", path: ["x"], value: 1 }]); } catch (error) { parseError = error; }
    expect(String(parseError)).toBe("ConfigParseError: Invalid configuration document");
    expect(String(parseError)).not.toContain(secret); expect(String(parseError)).not.toContain(root);

    await writeFile(file, "{}\n", "utf8");
    const transaction = new ConfigTransaction(registry, { previewTtlMs: 0 });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["x"], value: 1 }]);
    await new Promise((resolve) => setTimeout(resolve, 5));
    await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(ConfigPreviewUnavailable);
    await expect(transaction.commit("unknown-preview")).rejects.toBeInstanceOf(ConfigPreviewUnavailable);
  });

  it("stores encrypted backup bytes, keeps audit metadata secret-free, and rolls back", async () => {
    const { root, file, registry, targetId } = await fixture();
    const protector: SecretProtector = {
      isAvailable: () => true,
      protect: (value) => Buffer.from(value.map((byte) => byte ^ 0xa5)),
      unprotect: (value) => Buffer.from(value.map((byte) => byte ^ 0xa5)),
    };
    const vaultDir = join(root, "vault");
    const transaction = new ConfigTransaction(registry, { vault: new FileCredentialVault(vaultDir, protector) });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    const committed = await transaction.commit(preview.previewId);
    const [backupFile] = await readdir(vaultDir);
    const backupBytes = await readFile(join(vaultDir, backupFile!));
    expect(backupBytes.includes(Buffer.from("top-secret-canary"))).toBe(false);
    expect(JSON.stringify(transaction.audit)).not.toContain("top-secret-canary");
    await transaction.rollback(committed.backupId);
    expect(await readFile(file, "utf8")).toContain("top-secret-canary");
  });

  it("refuses commit without a credential vault", async () => {
    const { registry, targetId } = await fixture();
    const transaction = new ConfigTransaction(registry);
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    await expect(transaction.commit(preview.previewId)).rejects.toMatchObject({
      name: "ConfigBackupUnavailable",
      message: "Encrypted configuration backup unavailable",
    });
  });

  it("backs up before any target write", async () => {
    const { file, registry, targetId } = await fixture();
    const original = await readFile(file, "utf8");
    const vault = memoryVault(async () => {
      expect(await readFile(file, "utf8")).toBe(original);
    });
    const transaction = new ConfigTransaction(registry, { vault });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    await transaction.commit(preview.previewId);
  });

  it("maps vault availability failure during backup store without leaking details", async () => {
    const { root, registry, targetId } = await fixture();
    const canary = `${root}\\vault.bin VAULT-SECRET-CANARY`;
    const vault: CredentialVault = {
      isAvailable: () => { throw new Error(canary); },
      store: async () => { throw new Error("store must not run"); },
      get: async () => null,
      delete: async () => undefined,
    };
    const transaction = new ConfigTransaction(registry, { vault });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    let error: unknown;
    try { await transaction.commit(preview.previewId); } catch (caught) { error = caught; }
    expect(error).toBeInstanceOf(ConfigBackupUnavailable);
    expect(String(error)).toBe("ConfigBackupUnavailable: Encrypted configuration backup unavailable");
    expect(String(error)).not.toContain(root); expect(String(error)).not.toContain("VAULT-SECRET-CANARY");
  });

  it("maps vault availability failure during rollback get without leaking details", async () => {
    const { root, file, registry, targetId } = await fixture();
    const values = new Map<string, string>();
    const canary = `${root}\\vault.bin VAULT-SECRET-CANARY`;
    let availabilityThrows = false;
    const vault: CredentialVault = {
      isAvailable: () => {
        if (availabilityThrows) throw new Error(canary);
        return true;
      },
      store: async (reference, value) => { values.set(reference, value); },
      get: async (reference) => values.get(reference) ?? null,
      delete: async (reference) => { values.delete(reference); },
    };
    const transaction = new ConfigTransaction(registry, { vault });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    const committed = await transaction.commit(preview.previewId);
    availabilityThrows = true;
    let error: unknown;
    try { await transaction.rollback(committed.backupId); } catch (caught) { error = caught; }
    expect(error).toBeInstanceOf(ConfigBackupUnavailable);
    expect(String(error)).toBe("ConfigBackupUnavailable: Encrypted configuration backup unavailable");
    expect(String(error)).not.toContain(root); expect(String(error)).not.toContain("VAULT-SECRET-CANARY");
    expect(await readFile(file, "utf8")).toContain("false");
  });

  it("previews and creates a registered missing target from an empty JSONC baseline", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-missing-中文 space-")); dirs.push(root);
    const file = join(root, "new.jsonc");
    const registry = new TargetRegistry();
    const targetId = await registry.register({ scope: "project", owner: "opencode", path: file, format: "jsonc", source: "managed", writable: true, allowedRoot: root });
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["created"], value: true }]);
    expect(preview.unifiedDiff).toContain('+  "created": true');
    await transaction.commit(preview.previewId);
    expect(await readFile(file, "utf8")).toContain('"created": true');
  });

  it("rolls an originally missing target back to ENOENT", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-missing-rollback-")); dirs.push(root);
    const file = join(root, "new.jsonc");
    const registry = new TargetRegistry();
    const targetId = await registry.register({ scope: "project", owner: "opencode", path: file, format: "jsonc", source: "managed", writable: true, allowedRoot: root });
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["created"], value: true }]);
    const committed = await transaction.commit(preview.previewId);
    await transaction.rollback(committed.backupId);
    await expect(stat(file)).rejects.toMatchObject({ code: "ENOENT" });
    expect((await registry.read(targetId)).exists).toBe(false);
  });

  it("deletes the vault backup after a successful manual rollback", async () => {
    const { registry, targetId } = await fixture();
    const values = new Map<string, string>();
    const deleted: string[] = [];
    const vault: CredentialVault = {
      isAvailable: () => true,
      store: async (reference, value) => { values.set(reference, value); },
      get: async (reference) => values.get(reference) ?? null,
      delete: async (reference) => { deleted.push(reference); values.delete(reference); },
    };
    const transaction = new ConfigTransaction(registry, { vault });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    const committed = await transaction.commit(preview.previewId);
    await transaction.rollback(committed.backupId);
    expect(deleted).toEqual([`config-backup:${committed.backupId}`]);
    expect(values.size).toBe(0);
  });

  it("retries only vault cleanup after delete failure and never restores twice", async () => {
    const { file, registry, targetId } = await fixture();
    const values = new Map<string, string>();
    let deleteAttempts = 0;
    const vault: CredentialVault = {
      isAvailable: () => true,
      store: async (reference, value) => { values.set(reference, value); },
      get: async (reference) => values.get(reference) ?? null,
      delete: async (reference) => {
        deleteAttempts += 1;
        if (deleteAttempts <= 2) throw new Error("DELETE-CANARY");
        values.delete(reference);
      },
    };
    const transaction = new ConfigTransaction(registry, { vault });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    const committed = await transaction.commit(preview.previewId);
    let rollbackWrites = 0;
    setConfigTransactionTestHooks(transaction, { syncDirectory: async () => { rollbackWrites += 1; } });

    await expect(transaction.rollback(committed.backupId)).rejects.toMatchObject({
      name: "ConfigRecoveryError",
      reason: "backup-unavailable",
    });
    const restored = await readFile(file, "utf8");
    await expect(transaction.rollback(committed.backupId)).rejects.toBeInstanceOf(ConfigRecoveryError);
    expect(await readFile(file, "utf8")).toBe(restored);
    await expect(transaction.rollback(committed.backupId)).resolves.toMatchObject({ backupId: committed.backupId });
    expect(rollbackWrites).toBe(1);
    expect(deleteAttempts).toBe(3);
    expect(values.size).toBe(0);
  });

  it("keeps automatic rollback cleanup reachable without masking the write error", async () => {
    const { file, registry, targetId } = await fixture();
    const original = await readFile(file, "utf8");
    const values = new Map<string, string>();
    let failDelete = true;
    const vault: CredentialVault = {
      isAvailable: () => true,
      store: async (reference, value) => { values.set(reference, value); },
      get: async (reference) => values.get(reference) ?? null,
      delete: async (reference) => {
        if (failDelete) throw new Error("DELETE-CANARY");
        values.delete(reference);
      },
    };
    const transaction = new ConfigTransaction(registry, {
      vault,
      validateAfterWrite: async () => { throw new Error("VALIDATION-CANARY"); },
    });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    await expect(transaction.commit(preview.previewId)).rejects.toMatchObject({
      name: "ConfigWriteError",
      message: "Configuration validation failed; original restored",
    });
    expect(await readFile(file, "utf8")).toBe(original);
    expect(values.size).toBe(1);
    failDelete = false;
    await (transaction as ConfigTransaction & { cleanup(): Promise<void> }).cleanup();
    expect(values.size).toBe(0);
  });

  it("bounds pending vault cleanup and blocks a new commit at the limit", async () => {
    const { registry, targetId } = await fixture();
    const values = new Map<string, string>();
    let stores = 0;
    const vault: CredentialVault = {
      isAvailable: () => true,
      store: async (reference, value) => { stores += 1; values.set(reference, value); },
      get: async (reference) => values.get(reference) ?? null,
      delete: async () => { throw new Error("DELETE-CANARY"); },
    };
    const transaction = new ConfigTransaction(registry, { vault });
    for (let index = 0; index < 32; index += 1) {
      const preview = await transaction.preview(targetId, [{ op: "set", path: ["index"], value: index }]);
      const committed = await transaction.commit(preview.previewId);
      await expect(transaction.rollback(committed.backupId)).rejects.toMatchObject({
        name: "ConfigRecoveryError",
        reason: "backup-unavailable",
      });
    }
    const blockedPreview = await transaction.preview(targetId, [{ op: "set", path: ["blocked"], value: true }]);
    await expect(transaction.commit(blockedPreview.previewId)).rejects.toBeInstanceOf(ConfigBackupUnavailable);
    expect(stores).toBe(32);
    expect(values.size).toBe(32);
  }, 15_000);

  it("maps rollback durability failure after replacement and retries as cleanup only", async () => {
    const { file, registry, targetId } = await fixture();
    const original = await readFile(file, "utf8");
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    const committed = await transaction.commit(preview.previewId);
    let durabilityCalls = 0;
    setConfigTransactionTestHooks(transaction, {
      syncDirectory: async () => { durabilityCalls += 1; throw new Error("DURABILITY-CANARY"); },
    });
    await expect(transaction.rollback(committed.backupId)).rejects.toMatchObject({
      name: "ConfigRecoveryError",
      message: "Configuration recovery incomplete",
      reason: "write-failed",
    });
    expect(await readFile(file, "utf8")).toBe(original);
    await expect(transaction.rollback(committed.backupId)).resolves.toMatchObject({ backupId: committed.backupId });
    expect(durabilityCalls).toBe(1);
  });

  it("maps missing-target unlink durability failure and retries as cleanup only", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-missing-durability-")); dirs.push(root);
    const file = join(root, "new.jsonc");
    const registry = new TargetRegistry();
    const targetId = await registry.register({ scope: "project", owner: "opencode", path: file, format: "jsonc", source: "managed", writable: true, allowedRoot: root });
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["created"], value: true }]);
    const committed = await transaction.commit(preview.previewId);
    let durabilityCalls = 0;
    setConfigTransactionTestHooks(transaction, {
      syncDirectory: async () => { durabilityCalls += 1; throw new Error("DURABILITY-CANARY"); },
    });
    await expect(transaction.rollback(committed.backupId)).rejects.toMatchObject({
      name: "ConfigRecoveryError",
      reason: "write-failed",
    });
    await expect(stat(file)).rejects.toMatchObject({ code: "ENOENT" });
    await expect(transaction.rollback(committed.backupId)).resolves.toMatchObject({ backupId: committed.backupId });
    expect(durabilityCalls).toBe(1);
  });

  it("rejects a preview whose generated configuration exceeds one MiB", async () => {
    const { registry, targetId } = await fixture();
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    await expect(transaction.preview(targetId, [
      { op: "set", path: ["payload"], value: "x".repeat(1024 * 1024) },
    ])).rejects.toBeInstanceOf(UnsafeConfigError);
  });

  it("bounds aggregate retained preview plaintext by bytes", async () => {
    const { registry, targetId } = await fixture();
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    let firstId = "";
    for (let index = 0; index < 5; index += 1) {
      const preview = await transaction.preview(targetId, [
        { op: "set", path: ["payload"], value: `${index}${"x".repeat(900 * 1024)}` },
      ]);
      if (index === 0) firstId = preview.previewId;
    }
    await expect(transaction.commit(firstId)).rejects.toBeInstanceOf(ConfigPreviewUnavailable);
  });

  it("expires previews on unreferenced timers and dispose clears retained plaintext", async () => {
    const { registry, targetId } = await fixture();
    const timeoutSpy = vi.spyOn(globalThis, "setTimeout");
    const transaction = new ConfigTransaction(registry, { vault: memoryVault(), previewTtlMs: 1_000 });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    const timer = timeoutSpy.mock.results.at(-1)?.value as NodeJS.Timeout | undefined;
    expect(timer?.hasRef()).toBe(false);
    transaction.dispose();
    await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(ConfigPreviewUnavailable);

    timeoutSpy.mockRestore();
    vi.useFakeTimers();
    const expiring = new ConfigTransaction(registry, { vault: memoryVault(), previewTtlMs: 1_000 });
    const second = await expiring.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    expect(vi.getTimerCount()).toBe(1);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(vi.getTimerCount()).toBe(0);
    await expect(expiring.commit(second.previewId)).rejects.toBeInstanceOf(ConfigPreviewUnavailable);
  });

  it("bounds backup history per target and deletes evicted vault entries", async () => {
    const { registry, targetId } = await fixture();
    const values = new Map<string, string>();
    const deleted: string[] = [];
    const vault: CredentialVault = {
      isAvailable: () => true,
      store: async (reference, value) => { values.set(reference, value); },
      get: async (reference) => values.get(reference) ?? null,
      delete: async (reference) => { deleted.push(reference); values.delete(reference); },
    };
    const transaction = new ConfigTransaction(registry, { vault });
    let firstBackupId = "";
    for (let index = 0; index < 33; index += 1) {
      const preview = await transaction.preview(targetId, [{ op: "set", path: ["index"], value: index }]);
      const committed = await transaction.commit(preview.previewId);
      if (index === 0) firstBackupId = committed.backupId;
    }
    expect(deleted).toContain(`config-backup:${firstBackupId}`);
    await expect(transaction.rollback(firstBackupId)).rejects.toBeInstanceOf(ConfigPreviewUnavailable);
  });

  it("returns deeply frozen audit snapshots and bounds audit history", async () => {
    const { registry, targetId } = await fixture();
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    for (let index = 0; index < 129; index += 1) {
      const preview = await transaction.preview(targetId, [{ op: "set", path: ["index"], value: index }]);
      await transaction.commit(preview.previewId);
    }
    const audit = transaction.audit;
    expect(audit).toHaveLength(128);
    expect(Object.isFrozen(audit)).toBe(true);
    expect(Object.isFrozen(audit[0])).toBe(true);
    expect(() => { (audit[0] as { summary: string }).summary = "mutated"; }).toThrow(TypeError);
    expect(transaction.audit[0]?.summary).toBe("Configuration updated");
  }, 15_000);

  it("refuses rollback after an external edit and preserves that edit", async () => {
    const { file, registry, targetId } = await fixture();
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    const committed = await transaction.commit(preview.previewId);
    const external = "{\n  \"external\": true\n}\n";
    await writeFile(file, external, "utf8");
    await expect(transaction.rollback(committed.backupId)).rejects.toBeInstanceOf(ConfigConflict);
    expect(await readFile(file, "utf8")).toBe(external);
  });

  it("runs path guards before creating a temp file and again before rename", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-atomic-中文 space-")); dirs.push(root);
    const file = join(root, "settings.jsonc"); await writeFile(file, "{}\n", "utf8");
    const calls: string[] = [];
    await atomicWrite(file, "{\n  \"x\": 1\n}\n", {
      beforeCreate: async () => { calls.push("before-create"); },
      beforeRename: async () => { calls.push("before-rename"); },
    });
    expect(calls).toEqual(["before-create", "before-rename"]);
  });

  it("rejects a staged temp file replaced before rename without deleting the replacement", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-atomic-temp-")); dirs.push(root);
    const file = join(root, "settings.jsonc"); await writeFile(file, "{}\n", "utf8");
    let replacement = "";
    await expect(atomicWrite(file, "{\n  \"safe\": true\n}\n", {
      beforeCreate: async () => undefined,
      beforeRename: async () => undefined,
      afterTempSync: async (temporary) => {
        replacement = temporary;
        await rename(temporary, `${temporary}.original`);
        await writeFile(temporary, "ATTACKER-TEMP-CANARY", "utf8");
      },
    })).rejects.toMatchObject({ state: "not-replaced", replaced: false });
    expect(await readFile(file, "utf8")).toBe("{}\n");
    expect(await readFile(replacement, "utf8")).toBe("ATTACKER-TEMP-CANARY");
  });

  it("rejects a parent directory replaced after staging", async (context) => {
    const root = await mkdtemp(join(tmpdir(), "halo-atomic-parent-")); dirs.push(root);
    const parent = join(root, "parent"); await mkdir(parent);
    const file = join(parent, "settings.jsonc"); await writeFile(file, "{}\n", "utf8");
    let replacementAttempted = false;
    try {
      await expect(atomicWrite(file, "{\n  \"safe\": true\n}\n", {
        beforeCreate: async () => undefined,
        beforeRename: async () => undefined,
        afterTempSync: async (temporary) => {
          replacementAttempted = true;
          try {
            await rename(parent, join(root, "original-parent"));
          } catch (error) {
            const code = typeof error === "object" && error !== null && "code" in error ? String(error.code) : "";
            if (process.platform === "win32" && (code === "EPERM" || code === "EACCES")) { context.skip(); return; }
            throw error;
          }
          await mkdir(parent);
          await writeFile(join(parent, "settings.jsonc"), "ATTACKER-TARGET-CANARY", "utf8");
          await writeFile(temporary, "ATTACKER-TEMP-CANARY", "utf8");
        },
      })).rejects.toMatchObject({ state: "not-replaced", replaced: false });
    } finally {
      if (!replacementAttempted) context.skip();
    }
    expect(await readFile(join(parent, "settings.jsonc"), "utf8")).toBe("ATTACKER-TARGET-CANARY");
  });

  it("restores the original after rename succeeds but directory durability fails", async () => {
    const { file, registry, targetId } = await fixture();
    const original = await readFile(file, "utf8");
    let durabilityCalls = 0;
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    setConfigTransactionTestHooks(transaction, {
      syncDirectory: async () => {
        durabilityCalls += 1;
        if (durabilityCalls === 1) throw new Error("DIRECTORY-SYNC-CANARY");
      },
    });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    let error: unknown;
    try { await transaction.commit(preview.previewId); } catch (caught) { error = caught; }
    expect(error).toMatchObject({
      name: "ConfigWriteError",
      message: "Configuration durability failed; original restored",
    });
    expect(String(error)).not.toContain("DIRECTORY-SYNC-CANARY");
    expect(await readFile(file, "utf8")).toBe(original);
  });

  it("preserves the existing POSIX mode across atomic replacement", async (context) => {
    if (process.platform === "win32") { context.skip(); return; }
    const root = await mkdtemp(join(tmpdir(), "halo-atomic-mode-")); dirs.push(root);
    const file = join(root, "settings.jsonc"); await writeFile(file, "{}\n", "utf8");
    await chmod(file, 0o640);
    await atomicWrite(file, "{\n  \"safe\": true\n}\n", {
      beforeCreate: async () => undefined,
      beforeRename: async () => undefined,
    });
    expect((await stat(file)).mode & 0o777).toBe(0o640);
  });

  it("does not create a temp file outside the root when backup is followed by parent replacement", async (context) => {
    const root = await mkdtemp(join(tmpdir(), "halo-commit-race-中文 space-")); dirs.push(root);
    const outside = await mkdtemp(join(tmpdir(), "halo-commit-outside-")); dirs.push(outside);
    const parent = join(root, "parent"); await mkdir(parent);
    const file = join(parent, "settings.jsonc"); await writeFile(file, "{}\n", "utf8");
    const registry = new TargetRegistry();
    const targetId = await registry.register({ scope: "project", owner: "opencode", path: file, format: "jsonc", source: "native", writable: true, allowedRoot: root });
    const events: string[] = [];
    const watcher = watch(outside, (_event, filename) => { if (filename) events.push(String(filename)); });
    const vault = memoryVault(async () => {
      await rename(parent, join(root, "original-parent"));
      try { await symlink(outside, parent, process.platform === "win32" ? "junction" : "dir"); } catch (error) {
        const code = typeof error === "object" && error !== null && "code" in error ? String(error.code) : "";
        if (process.platform === "win32" && (code === "EPERM" || code === "EACCES")) { context.skip(); }
        throw error;
      }
    });
    try {
      const transaction = new ConfigTransaction(registry, { vault });
      const preview = await transaction.preview(targetId, [{ op: "set", path: ["x"], value: 1 }]);
      await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(UnsafeConfigError);
      await new Promise((resolve) => setTimeout(resolve, 25));
      expect(events.filter((name) => name.endsWith(".tmp"))).toEqual([]);
    } finally { watcher.close(); }
  });

  it("rejects rollback when its parent path is replaced", async (context) => {
    const root = await mkdtemp(join(tmpdir(), "halo-rollback-race-中文 space-")); dirs.push(root);
    const parent = join(root, "parent"); await mkdir(parent);
    const file = join(parent, "settings.jsonc"); await writeFile(file, "{}\n", "utf8");
    const registry = new TargetRegistry();
    const targetId = await registry.register({ scope: "project", owner: "opencode", path: file, format: "jsonc", source: "native", writable: true, allowedRoot: root });
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: true }]);
    const committed = await transaction.commit(preview.previewId);
    await rename(parent, join(root, "original-parent"));
    const outside = await mkdtemp(join(tmpdir(), "halo-rollback-outside-")); dirs.push(outside);
    await writeFile(join(outside, "settings.jsonc"), "{\n  \"outside\": true\n}\n", "utf8");
    try { await symlink(outside, parent, process.platform === "win32" ? "junction" : "dir"); } catch (error) {
      const code = typeof error === "object" && error !== null && "code" in error ? String(error.code) : "";
      if (process.platform === "win32" && (code === "EPERM" || code === "EACCES")) { context.skip(); return; }
      throw error;
    }
    await expect(transaction.rollback(committed.backupId)).rejects.toBeInstanceOf(UnsafeConfigError);
    expect(await readFile(join(outside, "settings.jsonc"), "utf8")).toContain("outside");
  });

  it("rejects commit for a registered non-writable target", async () => {
    const { root, file } = await fixture();
    const registry = new TargetRegistry();
    const targetId = await registry.register({ scope: "global", owner: "pi", path: file, format: "jsonc", source: "native", writable: false, allowedRoot: root });
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(UnsafeConfigError);
  });

  it("automatically restores the encrypted backup after post-write validation failure", async () => {
    const { file, registry, targetId } = await fixture();
    const original = await readFile(file, "utf8");
    const canary = "post-write-validator-canary";
    const transaction = new ConfigTransaction(registry, {
      vault: memoryVault(),
      validateAfterWrite: async () => { throw new Error(canary); },
    });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    let error: unknown;
    try { await transaction.commit(preview.previewId); } catch (caught) { error = caught; }
    expect(error).toMatchObject({
      name: "ConfigWriteError",
      message: "Configuration validation failed; original restored",
    });
    expect(String(error)).not.toContain(canary);
    expect(await readFile(file, "utf8")).toBe(original);
  });

  it("reports incomplete recovery without overwriting a concurrent post-write edit", async () => {
    const { root, file, registry, targetId } = await fixture();
    const external = "{\n  \"external\": true\n}\n";
    const canary = "recovery-validator-canary";
    const transaction = new ConfigTransaction(registry, {
      vault: memoryVault(),
      validateAfterWrite: async () => {
        await writeFile(file, external, "utf8");
        throw new Error(canary);
      },
    });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    let error: unknown;
    try { await transaction.commit(preview.previewId); } catch (caught) { error = caught; }
    expect(error).toMatchObject({
      name: "ConfigRecoveryError",
      message: "Configuration recovery incomplete",
      reason: "conflict",
    });
    expect(String(error)).not.toContain(canary); expect(String(error)).not.toContain(root);
    expect(await readFile(file, "utf8")).toBe(external);
  });

  it("does not share the returned preview object with transaction state", async () => {
    const { file, registry, targetId } = await fixture();
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    (preview as { targetId: string }).targetId = "mutated-target";
    (preview as { fingerprint: string }).fingerprint = "0".repeat(64);
    (preview.restartRequired as AgentKind[]).splice(0);
    await transaction.commit(preview.previewId);
    expect(await readFile(file, "utf8")).toContain("false");
  });

  it("consumes a failed preview so plaintext is not retained for retry", async () => {
    const { file, registry, targetId } = await fixture();
    const original = await readFile(file, "utf8");
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["known"], value: false }]);
    await writeFile(file, "{}\n", "utf8");
    await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(ConfigConflict);
    await writeFile(file, original, "utf8");
    await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(ConfigPreviewUnavailable);
  });

  it("detects an external edit while the temp file is being prepared", async () => {
    const { file, registry, targetId } = await fixture();
    const external = "{\n  \"externalDuringTemp\": true\n}\n";
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["payload"], value: "x".repeat(768 * 1024) }]);
    let changed = false;
    const watcher = watch(join(file, ".."), (_event, filename) => {
      if (!changed && filename?.toString().endsWith(".tmp")) {
        changed = true;
        writeFileSync(file, external, "utf8");
      }
    });
    try {
      await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(ConfigConflict);
      expect(changed).toBe(true);
      expect(await readFile(file, "utf8")).toBe(external);
    } finally { watcher.close(); }
  });

  it("treats concurrent creation of a missing target as a conflict", async () => {
    const root = await mkdtemp(join(tmpdir(), "halo-create-race-中文 space-")); dirs.push(root);
    const file = join(root, "new.jsonc"); const registry = new TargetRegistry();
    const targetId = await registry.register({ scope: "project", owner: "opencode", path: file, format: "jsonc", source: "managed", writable: true, allowedRoot: root });
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["managed"], value: true }]);
    const external = "{}\n"; await writeFile(file, external, "utf8");
    await expect(transaction.commit(preview.previewId)).rejects.toBeInstanceOf(ConfigConflict);
    expect(await readFile(file, "utf8")).toBe(external);
  });

  it("bounds retained plaintext previews", async () => {
    const { registry, targetId } = await fixture();
    const transaction = new ConfigTransaction(registry, { vault: memoryVault() });
    let firstId = "";
    for (let index = 0; index < 129; index += 1) {
      const preview = await transaction.preview(targetId, [{ op: "set", path: ["index"], value: index }]);
      if (index === 0) firstId = preview.previewId;
    }
    await expect(transaction.commit(firstId)).rejects.toBeInstanceOf(ConfigPreviewUnavailable);
  });
});
