import { existsSync, readFileSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const SCOPE_FILE = join(ROOT, 'halo-scope.json');
const EXTERNAL_REFERENCE = ['D:', 'BitFun-main'].join('\\');
const FORMAL_SCRIPT_KEYS = new Set([
  'dev',
  'build',
  'desktop:dev',
  'desktop:dev:raw',
  'desktop:preview:debug',
  'desktop:build',
  'desktop:build:fast',
  'desktop:build:release-fast',
  'desktop:build:exe',
  'desktop:build:nsis',
  'desktop:build:nsis:fast',
  'desktop:build:linux',
  'desktop:build:linux:deb',
  'desktop:build:linux:rpm',
  'desktop:build:linux:appimage',
]);
const FORBIDDEN_ENTRY_REFERENCES = [
  EXTERNAL_REFERENCE,
  'src/apps/desktop',
  'pyside',
  'qml',
  'electron',
];
const FORBIDDEN_FRONTEND_TERMS = /mini\s*app|miniapp|remote|relay|mobile|office|cowork|办公|协作|远程|移动端|小程序/i;

function fail(message) {
  throw new Error(`HALO_SCOPE_INVALID: ${message}`);
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(`cannot read ${relative(ROOT, path)}: ${error.message}`);
  }
}

function inside(root, candidate) {
  const path = relative(root, candidate);
  return path === '' || (!path.startsWith(`..${sep}`) && path !== '..');
}

function requireFile(root, relativePath) {
  const path = resolve(root, relativePath);
  if (!inside(root, path) || !existsSync(path)) {
    fail(`required file is missing or escapes product root: ${relativePath}`);
  }
  return path;
}

function checkFormalScript(key, value) {
  const expectedScript = key === 'desktop:preview:debug'
    ? 'scripts/halo-workbench-preview.mjs'
    : 'scripts/halo-tauri.mjs';
  if (typeof value !== 'string' || !value.includes(expectedScript)) {
    fail(`formal script ${key} must use ${expectedScript}`);
  }
  for (const reference of FORBIDDEN_ENTRY_REFERENCES) {
    if (value.toLowerCase().includes(reference.toLowerCase())) {
      fail(`formal script ${key} references ${reference}`);
    }
  }
}

export function verifyHaloScope(rootDir = ROOT) {
  const scopePath = join(rootDir, 'halo-scope.json');
  const scope = readJson(scopePath);
  const packageJson = readJson(join(rootDir, 'package.json'));
  const configPath = requireFile(rootDir, scope.tauriConfig);
  const config = readJson(configPath);
  const desktopRoot = requireFile(rootDir, `${scope.desktopRoot}/Cargo.toml`);
  const frontendRoot = requireFile(rootDir, `${scope.frontendRoot}/index.html`);

  if (scope.schemaVersion !== 1) fail('schemaVersion must be 1');
  if (scope.productId !== 'halo-studio') fail('productId must be halo-studio');
  if (scope.displayName !== 'Halo Studio') fail('displayName must be Halo Studio');
  if (scope.bundleIdentifier !== 'com.halostudio.desktop') fail('bundle identifier changed without a product decision');
  if (scope.developmentEntry !== 'node scripts/halo-tauri.mjs dev') fail('developmentEntry is not the Halo Tauri wrapper');
  if (scope.packagingEntry !== 'node scripts/halo-tauri.mjs build') fail('packagingEntry is not the Halo Tauri wrapper');
  if (!Array.isArray(scope.excludedRoutes) || scope.excludedRoutes.length === 0) fail('excludedRoutes must be declared');
  if (!scope.runtimePolicy || Object.values(scope.runtimePolicy).some(value => value !== false && value !== 'local-only')) {
    fail('Halo runtime policy must keep all out-of-scope capabilities disabled');
  }

  for (const key of FORMAL_SCRIPT_KEYS) {
    if (packageJson.scripts?.[key] !== undefined) checkFormalScript(key, packageJson.scripts[key]);
  }
  for (const key of scope.excludedRoutes) {
    if (packageJson.scripts?.[key] !== undefined) fail(`out-of-scope route ${key} must not be exposed by the Halo package`);
  }
  if (packageJson.scripts?.dev !== scope.developmentEntry) fail('package dev script diverges from developmentEntry');
  if (packageJson.scripts?.build !== scope.packagingEntry) fail('package build script diverges from packagingEntry');

  if (config.productName !== scope.displayName) fail('Tauri productName diverges from scope');
  if (config.identifier !== scope.bundleIdentifier) fail('Tauri identifier diverges from scope');
  if (config.build?.frontendDist !== '../../halo-workbench/dist') fail('Tauri frontendDist must be the Halo workbench build');
  if (config.build?.beforeDevCommand !== 'node ../../scripts/halo-workbench-dev-server.mjs') fail('Tauri dev command is not Halo-owned');
  if (config.build?.beforeBuildCommand !== 'node ../../scripts/halo-workbench-build.mjs') fail('Tauri build command is not Halo-owned');
  if (config.app?.windows?.[0]?.visible !== true) fail('Halo main window must be visible by default');
  if (!Array.isArray(config.bundle?.icon) || !config.bundle.icon.includes('icons/halo-icon.ico')) fail('Tauri bundle must use the Halo desktop icon');
  for (const icon of config.bundle.icon) requireFile(rootDir, `${scope.desktopRoot}/${icon}`);

  const entryPaths = [
    configPath,
    desktopRoot,
    frontendRoot,
    join(rootDir, scope.frontendRoot, 'app.js'),
    join(rootDir, scope.frontendRoot, 'styles.css'),
    join(rootDir, scope.desktopRoot, 'src', 'main.rs'),
  ];
  for (const path of entryPaths) {
    const source = readFileSync(path, 'utf8');
    for (const reference of FORBIDDEN_ENTRY_REFERENCES) {
      if (source.toLowerCase().includes(reference.toLowerCase())) {
        fail(`${relative(rootDir, path)} references ${reference}`);
      }
    }
  }

  for (const path of [
    join(rootDir, scope.frontendRoot, 'index.html'),
    join(rootDir, scope.frontendRoot, 'app.js'),
    join(rootDir, scope.frontendRoot, 'styles.css'),
  ]) {
    if (FORBIDDEN_FRONTEND_TERMS.test(readFileSync(path, 'utf8'))) {
      fail(`${relative(rootDir, path)} exposes an out-of-scope feature term`);
    }
  }

  return {
    scopePath,
    configPath,
    desktopRoot: relative(rootDir, desktopRoot),
    frontendRoot: relative(rootDir, frontendRoot),
    includedModules: scope.includedModules,
    excludedModules: scope.excludedModules,
    excludedRoutes: scope.excludedRoutes,
  };
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
  try {
    console.log(JSON.stringify({ ok: true, ...verifyHaloScope() }, null, 2));
  } catch (error) {
    console.error(error.message || String(error));
    process.exitCode = 1;
  }
}
