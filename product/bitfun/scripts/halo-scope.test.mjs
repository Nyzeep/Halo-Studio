import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { verifyHaloScope } from './halo-scope.mjs';

const ROOT = join(import.meta.dirname, '..');

test('formal development and packaging entries resolve to the same Halo Tauri wrapper', () => {
  const packageJson = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
  assert.equal(packageJson.scripts.dev, 'node scripts/halo-tauri.mjs dev');
  assert.equal(packageJson.scripts.build, 'node scripts/halo-tauri.mjs build');
  assert.equal(packageJson.scripts['desktop:dev'], packageJson.scripts.dev);
  assert.equal(packageJson.scripts['desktop:build'], packageJson.scripts.build);
  assert.equal(packageJson.scripts['desktop:preview:debug'], 'node scripts/halo-workbench-preview.mjs');
});

test('Halo scope exposes only the local coding modules', () => {
  const result = verifyHaloScope(ROOT);
  assert.deepEqual(result.includedModules, [
    'local-workspaces',
    'coding-sessions',
    'file-explorer',
    'editor',
    'git',
    'terminal',
  ]);
  assert.deepEqual(result.excludedModules, [
    'office-collaboration',
    'mini-app',
    'remote-workspace',
    'relay',
    'mobile-client',
  ]);
  const packageJson = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
  for (const route of result.excludedRoutes) assert.equal(packageJson.scripts[route], undefined, `${route} must stay unavailable`);
});

test('Halo frontend has the local-coding root marker used by smoke checks', () => {
  const html = readFileSync(join(ROOT, 'src', 'halo-workbench', 'index.html'), 'utf8');
  assert.match(html, /data-halo-scope="local-coding"/);
  assert.match(html, /Halo Studio/);
  assert.match(html, /lang="zh-CN"/);
});

test('Tauri hooks resolve from the workspace project directory', () => {
  const scope = verifyHaloScope(ROOT);
  const config = JSON.parse(readFileSync(scope.configPath, 'utf8'));
  assert.equal(config.build.beforeDevCommand, 'node ../../scripts/halo-workbench-dev-server.mjs');
  assert.equal(config.build.beforeBuildCommand, 'node ../../scripts/halo-workbench-build.mjs');
  assert.deepEqual(config.bundle.icon, [
    'icons/halo-icon.icns',
    'icons/halo-icon.ico',
    'icons/halo-icon.png',
  ]);
});

test('Halo wrapper rejects alternate Tauri configuration input', () => {
  const result = spawnSync(process.execPath, ['scripts/halo-tauri.mjs', 'dev', '--config', 'src/apps/desktop/tauri.conf.json'], {
    cwd: ROOT,
    encoding: 'utf8',
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /configuration overrides are disabled/);
});

test('Halo wrapper forwards Cargo profiles after the Tauri arguments', () => {
  const result = spawnSync(process.execPath, ['scripts/halo-tauri.mjs', 'build', '--no-bundle', '--profile', 'release-fast', '--help'], {
    cwd: ROOT,
    encoding: 'utf8',
  });
  assert.equal(result.status, 0);
  assert.doesNotMatch(result.stderr, /unexpected argument '--profile'/);
});

test('Halo preview reuses the debug binary instead of tauri dev', () => {
  const preview = readFileSync(join(ROOT, 'scripts', 'halo-workbench-preview.mjs'), 'utf8');
  assert.match(preview, /halo-studio\.exe/);
  assert.match(preview, /halo-workbench-dev-server\.mjs/);
  assert.match(preview, /--force-rebuild/);
  assert.doesNotMatch(preview, /tauri(?:\.cmd)?\s+dev/);
});
