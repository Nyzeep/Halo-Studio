import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { verifyHaloScope } from './halo-scope.mjs';

const ROOT = join(import.meta.dirname, '..');

function read(relativePath) {
  return readFileSync(join(ROOT, relativePath), 'utf8');
}

test('formal development and packaging entries resolve to the Halo Tauri wrapper', () => {
  const packageJson = JSON.parse(read('package.json'));
  assert.equal(packageJson.scripts.dev, 'node scripts/halo-tauri.mjs dev');
  assert.equal(packageJson.scripts.build, 'node scripts/halo-tauri.mjs build');
  assert.equal(packageJson.scripts['desktop:dev'], packageJson.scripts.dev);
  assert.equal(packageJson.scripts['desktop:build'], packageJson.scripts.build);
  assert.equal(packageJson.scripts['desktop:preview:debug'], 'node scripts/halo-web-ui-preview.mjs');
});

test('Halo scope exposes the local coding module set on the BitFun Web UI root', () => {
  const result = verifyHaloScope(ROOT);
  assert.equal(result.frontendRoot, join('src', 'web-ui', 'index.html'));
  assert.equal(result.frontendEntry, join('src', 'web-ui', 'src', 'main.tsx'));
  assert.equal(result.viteConfig, join('src', 'web-ui', 'vite.config.ts'));
  assert.equal(result.buildOutDir, 'dist');
  assert.equal(result.devUrl, 'http://localhost:1422');
  assert.equal(result.frontendDist, '../../../dist');
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
});

test('BitFun Web UI entry carries Halo markers used by native smoke checks', () => {
  const html = read(join('src', 'web-ui', 'index.html'));
  assert.match(html, /<html lang="zh-CN" data-halo-scope="local-coding" data-product-id="halo-studio">/);
  assert.match(html, /<script type="module" src="\/src\/main\.tsx"><\/script>/);
  assert.match(html, /Halo Studio/);
  assert.match(html, /\/halo-icon\.svg/);
  assert.doesNotMatch(html, /src\/halo-workbench/);
});

test('Tauri hooks resolve to the BitFun Web UI dev server and dist output', () => {
  const scope = verifyHaloScope(ROOT);
  const config = JSON.parse(readFileSync(scope.configPath, 'utf8'));
  assert.equal(config.build.beforeDevCommand, 'node ../../scripts/halo-web-ui-dev-server.mjs');
  assert.equal(config.build.devUrl, 'http://localhost:1422');
  assert.equal(config.build.beforeBuildCommand, 'node ../../scripts/halo-web-ui-build.mjs');
  assert.equal(config.build.frontendDist, '../../../dist');
  assert.equal(config.app.security.csp, null);
  assert.equal(config.app.withGlobalTauri, true);
  assert.deepEqual(config.bundle.icon, [
    'icons/halo-icon.icns',
    'icons/halo-icon.ico',
    'icons/halo-icon.png',
  ]);
});

test('Vite desktop build uses the real BitFun Web UI entry and Halo dist path', () => {
  const webPackageJson = JSON.parse(read(join('src', 'web-ui', 'package.json')));
  const viteConfig = read(join('src', 'web-ui', 'vite.config.ts'));
  assert.equal(webPackageJson.scripts.dev, 'vite');
  assert.equal(webPackageJson.scripts['build:desktop'], 'vite build --mode desktop');
  assert.match(viteConfig, /port:\s*1422/);
  assert.match(viteConfig, /port:\s*1421/);
  assert.match(viteConfig, /strictPort:\s*true/);
  assert.match(viteConfig, /outDir:\s*'\.\.\/\.\.\/dist'/);
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
  const wrapper = read(join('scripts', 'halo-tauri.mjs'));
  const haloMain = read(join('src', 'apps', 'halo-desktop', 'src', 'main.rs'));
  assert.match(wrapper, /const tauriArgs = \[\];/);
  assert.match(wrapper, /const cargoArgs = \[\];/);
  assert.match(wrapper, /cargoArgs\.push\('--profile', profile\)/);
  assert.match(wrapper, /\.\.\.\(cargoArgs\.length > 0 \? \['--', \.\.\.cargoArgs\] : \[\]\)/);
  assert.match(haloMain, /bitfun_desktop_lib::run_with_context\(tauri::generate_context!\(\)\)\.await/);
  assert.doesNotMatch(haloMain, /tauri::Builder::default\(\)\s*\.run/);
});

test('Halo preview launches the debug binary beside the Web UI dev server', () => {
  const preview = read(join('scripts', 'halo-web-ui-preview.mjs'));
  assert.match(preview, /halo-studio\.exe/);
  assert.match(preview, /halo-web-ui-dev-server\.mjs/);
  assert.match(preview, /--force-rebuild/);
  assert.match(preview, /http:\/\/localhost:1422/);
  assert.doesNotMatch(preview, /tauri(?:\.cmd)?\s+dev/);
});

test('Halo Web UI assembly omits visible out-of-scope navigation and settings tabs', () => {
  const navPanel = read(join('src', 'web-ui', 'src', 'app', 'components', 'NavPanel', 'NavPanel.tsx'));
  const footer = read(join('src', 'web-ui', 'src', 'app', 'components', 'NavPanel', 'components', 'PersistentFooterActions.tsx'));
  const search = read(join('src', 'web-ui', 'src', 'app', 'components', 'NavPanel', 'NavSearchDialog.tsx'));
  const settingsConfig = read(join('src', 'web-ui', 'src', 'app', 'scenes', 'settings', 'settingsConfig.ts'));
  const dispatchTargetPicker = read(join('src', 'web-ui', 'src', 'features', 'dispatch', 'DispatchTargetPicker.tsx'));

  assert.doesNotMatch(navPanel, /PeerRemoteBadge/);
  assert.doesNotMatch(footer, /RemoteConnectDialog|remoteControl|ToolbarModeContext|openScene\('browser'\)|openScene\('insights'\)/);
  assert.doesNotMatch(search, /assistantWorkspacesList|groupAssistants|openScene\('assistant'\)/);
  assert.match(dispatchTargetPicker, /data-dispatch-kind="local"/);
  assert.doesNotMatch(dispatchTargetPicker, /SSHConnectionDialog|DispatchInstallDialog|useDispatchTargets|chatInput\.dispatch\.sshSection|chatInput\.dispatch\.addSsh/);
  for (const tab of [
    'models',
    'worktrees',
    'session-personalization',
    'session-permissions',
    'quick-actions',
    'voice-input',
    'review',
    'memories',
    'mcp-tools',
    'external-sources',
    'hooks',
    'acp-agents',
  ]) {
    assert.doesNotMatch(settingsConfig, new RegExp(`id:\\s*'${tab}'`));
  }
});
