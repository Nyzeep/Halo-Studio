import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { desktopDevelopmentServerUrl } from "./desktop-dev.mjs";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);

test("development command binds Vite and Electron to one fixed loopback URL", () => {
  const rootPackage = JSON.parse(readFileSync(join(repositoryRoot, "package.json"), "utf8"));
  const desktopPackage = JSON.parse(readFileSync(join(repositoryRoot, "apps", "desktop", "package.json"), "utf8"));
  const source = readFileSync(join(scriptsDirectory, "desktop-dev.mjs"), "utf8");

  assert.equal(desktopDevelopmentServerUrl, "http://127.0.0.1:5173");
  assert.match(rootPackage.scripts.dev, /run-workspace-script\.mjs dev/u);
  assert.match(desktopPackage.scripts.dev, /desktop-dev\.mjs/u);
  assert.match(desktopPackage.scripts["smoke:dev"], /electron-dev-smoke\.mjs/u);
  assert.match(desktopPackage.scripts.prebuild, /prepare-native-runtime\.mjs electron/u);
  assert.match(source, /--host", developmentHost/u);
  assert.match(source, /--strictPort/u);
  assert.match(source, /--disable-gpu/u);
  assert.match(source, /--use-angle=swiftshader/u);
  assert.match(source, /--headless/u);
  assert.match(source, /HALO_DESKTOP_DEV_SERVER_URL/u);
  assert.match(source, /resolveElectronBinary/u);
});
