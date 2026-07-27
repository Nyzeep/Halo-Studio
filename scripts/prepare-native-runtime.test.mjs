import assert from "node:assert/strict";
import test from "node:test";

import { nativeRuntimeConfiguration } from "./prepare-native-runtime.mjs";

test("prepares a locked Electron ABI without inheriting a host Node target", () => {
  const environment = nativeRuntimeConfiguration("electron", {
    baseEnvironment: {
      npm_config_runtime: "node",
      npm_config_target: "24.14.1",
      KEEP_ME: "yes",
    },
    cacheDirectory: "D:\\cache",
    electronVersion: "33.4.11",
    nodeHeadersDirectory: "D:\\headers",
  });

  assert.equal(environment.KEEP_ME, "yes");
  assert.equal(environment.npm_config_runtime, "electron");
  assert.equal(environment.npm_config_target, "33.4.11");
  assert.equal(environment.npm_config_disturl, "https://electronjs.org/headers");
  assert.equal(environment.npm_config_nodedir, undefined);
  assert.equal(environment.npm_config_build_from_source, "true");
});

test("prepares the host Node ABI after Electron work", () => {
  const environment = nativeRuntimeConfiguration("node", {
    baseEnvironment: {
      npm_config_runtime: "electron",
      npm_config_target: "33.4.11",
      npm_config_disturl: "https://electronjs.org/headers",
    },
    cacheDirectory: "D:\\cache",
    electronVersion: "33.4.11",
    nodeHeadersDirectory: "D:\\headers",
  });

  assert.equal(environment.npm_config_runtime, undefined);
  assert.equal(environment.npm_config_target, undefined);
  assert.equal(environment.npm_config_disturl, undefined);
  assert.equal(environment.npm_config_nodedir, undefined);
  assert.equal(environment.npm_config_build_from_source, undefined);
});
