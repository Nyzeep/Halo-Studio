import { mkdtemp, readFile, writeFile, rm, readdir, mkdir, rename, symlink } from "node:fs/promises";
import { watch, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import type { AgentKind } from "@halo-studio/contracts";
import { ConfigTransaction, ConfigConflict, ConfigPreviewUnavailable, UnsafeConfigError } from "./configTransaction.js";
import { TargetRegistry } from "./targetRegistry.js";
import { createTwoFilesPatch } from "./unifiedDiff.js";
import { applyJsoncPatch } from "./jsoncPatch.js";
import { atomicWrite } from "./atomicWrite.js";
import { FileCredentialVault, type CredentialVault, type SecretProtector } from "@halo-studio/storage";

const dirs: string[] = [];
afterEach(async () => { await Promise.all(dirs.splice(0).map((d) => rm(d, { recursive: true, force: true }))); });

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
    const preview = await transaction.preview(targetId, [{ op: "set", path: ["payload"], value: "x".repeat(4 * 1024 * 1024) }]);
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
