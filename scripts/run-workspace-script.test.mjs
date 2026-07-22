import assert from "node:assert/strict";
import test from "node:test";

import { normalizeSpawnResult } from "./run-workspace-script.mjs";

for (const [signal, expectedExitCode] of [
  ["SIGINT", 130],
  ["SIGTERM", 143],
]) {
  test(`maps ${signal} to exit code ${expectedExitCode} and reports it`, () => {
    const messages = [];
    const exitCode = normalizeSpawnResult(
      { error: undefined, signal, status: null },
      (message) => messages.push(message),
    );

    assert.equal(exitCode, expectedExitCode);
    assert.deepEqual(messages, [
      `Workspace script terminated by ${signal}; exiting with code ${expectedExitCode}.`,
    ]);
  });
}
