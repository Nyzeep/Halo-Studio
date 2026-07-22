import { mkdtemp, readFile, writeFile, rm, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { ConfigTransaction, ConfigConflict, ConfigPreviewUnavailable, UnsafeConfigError } from "./configTransaction.js";
import { TargetRegistry } from "./targetRegistry.js";
import { createTwoFilesPatch } from "./unifiedDiff.js";
import { FileCredentialVault, type SecretProtector } from "@halo-studio/storage";

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

describe("unified diff", () => {
  it("emits standard two-file patch headers", () => {
    const patch = createTwoFilesPatch("a.jsonc", "b.jsonc", "{\n  \"x\": 1\n}\n", "{\n  \"x\": 2\n}\n");
    expect(patch).toContain("--- a.jsonc"); expect(patch).toContain("+++ b.jsonc"); expect(patch).toContain("@@");
  });
});

describe("config transaction", () => {
  it("preserves JSONC comments and unknown fields while redacting secret diff values", async () => {
    const { file, registry, targetId } = await fixture();
    const tx = new ConfigTransaction(registry);
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
});
