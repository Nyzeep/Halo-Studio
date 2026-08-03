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

test('Halo declares the main window without letting Tauri create it', () => {
  const config = JSON.parse(read('src/apps/halo-desktop/tauri.conf.json'));
  assert.equal(config.app.windows?.length, 1);
  assert.equal(config.app.windows[0].label, 'main');
  assert.equal(config.app.windows[0].create, false);
  assert.match(config.app.windows[0].title, /Halo Studio/);
});

test('Halo grants the native window commands through its own capability', () => {
  const config = JSON.parse(read('src/apps/halo-desktop/tauri.conf.json'));
  const capability = JSON.parse(read('src/apps/halo-desktop/capabilities/default.json'));
  assert.deepEqual(config.app.security.capabilities, ['halo-default']);
  assert.equal(capability.identifier, 'halo-default');
  assert.deepEqual(capability.windows, ['main']);
  for (const permission of [
    'core:window:allow-is-maximized',
    'core:window:allow-maximize',
    'core:window:allow-unmaximize',
  ]) {
    assert.ok(capability.permissions.includes(permission), permission);
  }
});

test('Halo keeps runtime logging and storage cleanup on its own log root', () => {
  const haloMain = read('src/apps/halo-desktop/src/main.rs');
  const desktopLib = read('src/apps/desktop/src/lib.rs');
  const logging = read('src/apps/desktop/src/logging.rs');
  const storageCommands = read('src/apps/desktop/src/api/storage_commands.rs');
  const storageCleanup = read('src/crates/assembly/core/src/infrastructure/storage/cleanup.rs');
  const pathManager = read('src/crates/assembly/core/src/infrastructure/app_paths/path_manager.rs');
  const haloCargo = read('src/apps/halo-desktop/Cargo.toml');

  assert.match(haloMain, /product_logs_root\("Halo Studio"\)/);
  assert.match(haloMain, /DesktopRunOptions::with_logs_root/);
  assert.match(desktopLib, /run_with_context_and_options/);
  assert.match(logging, /set_logs_root_override/);
  assert.match(logging, /pub fn product_logs_root/);
  assert.match(logging, /HALO_LOG_DIR/);
  assert.match(storageCommands, /crate::logging::logs_root\(\)/);
  assert.match(storageCleanup, /new_with_logs_dir/);
  assert.doesNotMatch(
    pathManager,
    /pub async fn initialize_user_directories[\s\S]*?self\.logs_dir\(\)/
  );
  assert.doesNotMatch(haloMain, /BITFUN_LOG_DIR|BITFUN_E2E_LOG_DIR/);
  for (const dependency of [
    'tauri-plugin-log.workspace = true',
    'tauri-plugin-window-state.workspace = true',
  ]) {
    assert.match(haloCargo, new RegExp(dependency.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  }
});

test('Halo uses its product identifier for the default WebView2 profile', () => {
  const config = JSON.parse(read('src/apps/halo-desktop/tauri.conf.json'));
  const theme = read(join('src', 'apps', 'desktop', 'src', 'theme.rs'));
  assert.equal(config.identifier, 'com.halostudio.desktop');
  assert.doesNotMatch(theme, /\.data_directory\(|HALO_WEBVIEW_DATA_DIR|isolated WebView2 data directory/);
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
  assert.match(haloMain, /bitfun_desktop_lib::run_with_context_and_options/);
  assert.doesNotMatch(haloMain, /tauri::Builder::default\(\)\s*\.run/);
});

test('Halo production builds enable Tauri custom protocol while dev keeps Vite', () => {
  const wrapper = read(join('scripts', 'halo-tauri.mjs'));
  const desktopCargo = read(join('src', 'apps', 'halo-desktop', 'Cargo.toml'));
  assert.match(wrapper, /mode === 'build'/);
  assert.match(wrapper, /'--features', 'custom-protocol'/);
  assert.match(desktopCargo, /custom-protocol = \["tauri\/custom-protocol"\]/);
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

test('Halo execution UI is wired only to the Workbench Runtime interface', () => {
  const app = read(join('src', 'web-ui', 'src', 'app', 'App.tsx'));
  const appLayout = read(join('src', 'web-ui', 'src', 'app', 'layout', 'AppLayout.tsx'));
  const mainNav = read(join('src', 'web-ui', 'src', 'app', 'components', 'NavPanel', 'MainNav.tsx'));
  const workspaceItem = read(join('src', 'web-ui', 'src', 'app', 'components', 'NavPanel', 'sections', 'workspaces', 'WorkspaceItem.tsx'));
  const client = read(join('src', 'web-ui', 'src', 'infrastructure', 'workbench-runtime', 'client.ts'));

  assert.match(app, /isHaloLocalCodingScope\(\) && isTauriRuntime\(\)/);
  assert.match(app, /workbenchRuntimeStore\.getState\(\)\.start\(\)/);
  assert.match(appLayout, /type: 'openWorkspace'/);
  assert.match(mainNav, /workbenchRuntimeStore/);
  assert.doesNotMatch(mainNav, /FlowChatManager|flowChatManager/);
  assert.match(workspaceItem, /WorkbenchSessionsSection/);
  assert.doesNotMatch(workspaceItem, /FlowChatManager|flowChatManager/);
  assert.match(client, /halo_workbench_runtime_snapshot/);
  assert.match(client, /halo_workbench_runtime_submit_intent/);
  assert.match(client, /halo-workbench:\/\/event/);
});
