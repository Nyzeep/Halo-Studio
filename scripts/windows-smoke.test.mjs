import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));

test("Windows smoke restores the Node ABI before launching desktop tests", () => {
  const source = readFileSync(join(scriptsDirectory, "windows-smoke.mjs"), "utf8");
  const preparation = source.indexOf("prepare-native-runtime.mjs");
  const desktopTest = source.indexOf('"workspace-runtime.integration.test.ts"');

  assert.notEqual(preparation, -1);
  assert.notEqual(desktopTest, -1);
  assert.ok(preparation < desktopTest);
});
