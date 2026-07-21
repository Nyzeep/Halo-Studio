import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { applyConfirmedConfigWrite, planRealConfigWrite } from "../main/config/writeGuard";

let workspaceRoot: string;

beforeEach(async () => {
  workspaceRoot = await fs.mkdtemp(path.join(os.tmpdir(), "halo-workspace-"));
});

afterEach(async () => {
  await fs.rm(workspaceRoot, { recursive: true, force: true });
});

describe("real config write guard", () => {
  it("allows project-local targets", () => {
    const targetPath = path.join(workspaceRoot, ".mcp.json");

    const plan = planRealConfigWrite({
      workspaceRoot,
      targetPath,
      nextContent: "{}\n",
      reason: "项目 MCP"
    });

    expect(plan.allowed).toBe(true);
    expect(plan.risk).toBe("low");
    expect(plan.confirmationPhrase).toBe("APPLY .mcp.json");
  });

  it("blocks targets outside the workspace root", () => {
    const targetPath = path.join(os.tmpdir(), "outside-config.json");

    const plan = planRealConfigWrite({
      workspaceRoot,
      targetPath,
      nextContent: "{}\n",
      reason: "外部配置"
    });

    expect(plan.allowed).toBe(false);
    expect(plan.risk).toBe("blocked");
    expect(plan.warnings.join(" ")).toContain("工作区");
  });

  it("blocks dangerous workspace directories", () => {
    const targetPath = path.join(workspaceRoot, ".git", "config");

    const plan = planRealConfigWrite({
      workspaceRoot,
      targetPath,
      nextContent: "{}\n",
      reason: "危险配置"
    });

    expect(plan.allowed).toBe(false);
    expect(plan.warnings.join(" ")).toContain(".git");
  });

  it("rejects writes with the wrong confirmation phrase", async () => {
    const targetPath = path.join(workspaceRoot, ".mcp.json");

    await expect(
      applyConfirmedConfigWrite({
        workspaceRoot,
        targetPath,
        nextContent: "{}\n",
        reason: "项目 MCP",
        confirmation: "wrong"
      })
    ).rejects.toThrow("确认短语");

    await expect(fs.readFile(targetPath, "utf8")).rejects.toMatchObject({ code: "ENOENT" });
  });
});
