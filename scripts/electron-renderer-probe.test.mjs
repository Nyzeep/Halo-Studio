import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));

test("renderer diagnostic probe preserves BrowserWindow sandboxing", () => {
  const probe = readFileSync(join(scriptsDirectory, "electron-renderer-probe.cjs"), "utf8");
  const runner = readFileSync(join(scriptsDirectory, "run-electron-renderer-probe.mjs"), "utf8");

  assert.match(probe, /contextIsolation:\s*true/u);
  assert.match(probe, /nodeIntegration:\s*false/u);
  assert.match(probe, /sandbox:\s*true/u);
  assert.match(runner, /--headless/u);
  assert.match(runner, /--disable-gpu/u);
  assert.match(runner, /--use-angle=swiftshader/u);
  assert.doesNotMatch(runner, /--(?:in-process-gpu|no-sandbox)/u);
});
