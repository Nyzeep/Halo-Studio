import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { applyConfigWrite, listConfigBackups, rollbackConfigWrite } from "../main/config/configFileService";

let tempDir: string;

beforeEach(async () => {
  tempDir = await fs.mkdtemp(path.join(os.tmpdir(), "halo-config-test-"));
});

afterEach(async () => {
  await fs.rm(tempDir, { recursive: true, force: true });
});

describe("config file service", () => {
  it("creates a backup, writes atomically, and returns a diff", async () => {
    const targetPath = path.join(tempDir, "config.toml");
    await fs.writeFile(targetPath, "model = \"old\"\n", "utf8");

    const result = await applyConfigWrite({
      targetPath,
      nextContent: "model = \"new\"\n",
      reason: "测试写入"
    });

    await expect(fs.readFile(targetPath, "utf8")).resolves.toBe("model = \"new\"\n");
    await expect(fs.readFile(result.backupPath, "utf8")).resolves.toBe("model = \"old\"\n");
    expect(result.diff).toContain("-model = \"old\"");
    expect(result.diff).toContain("+model = \"new\"");
  });

  it("rolls back from a backup", async () => {
    const targetPath = path.join(tempDir, "config.json");
    await fs.writeFile(targetPath, "{\"value\":1}\n", "utf8");

    const result = await applyConfigWrite({
      targetPath,
      nextContent: "{\"value\":2}\n",
      reason: "测试回滚"
    });

    const rollback = await rollbackConfigWrite({
      targetPath,
      backupPath: result.backupPath
    });

    await expect(fs.readFile(targetPath, "utf8")).resolves.toBe("{\"value\":1}\n");
    expect(rollback.restored).toBe(true);
  });

  it("lists backups for a target from newest to oldest", async () => {
    const targetPath = path.join(tempDir, "settings.json");
    await fs.writeFile(targetPath, "{\"value\":1}\n", "utf8");

    await applyConfigWrite({
      targetPath,
      nextContent: "{\"value\":2}\n",
      reason: "第一次写入"
    });
    await new Promise((resolve) => setTimeout(resolve, 5));
    await applyConfigWrite({
      targetPath,
      nextContent: "{\"value\":3}\n",
      reason: "第二次写入"
    });

    const backups = await listConfigBackups(targetPath);

    expect(backups).toHaveLength(2);
    expect(backups[0]?.backupPath).toContain("settings.json");
    expect(new Date(backups[0]?.createdAt ?? 0).getTime()).toBeGreaterThanOrEqual(
      new Date(backups[1]?.createdAt ?? 0).getTime()
    );
  });
});
