import { existsSync, readFileSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
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
  'desktop:build:arm64',
  'desktop:build:x86_64',
  'desktop:build:linux',
  'desktop:build:linux:deb',
  'desktop:build:linux:rpm',
  'desktop:build:linux:appimage',
]);
const EXPECTED_SCENES = [
  'welcome',
  'session',
  'file-viewer',
  'git',
  'terminal',
  'shell',
  'settings',
];
const PRUNED_SCENE_IDS = [
  'profile',
  'agents',
  'skills',
  'miniapps',
  'pages',
  'browser',
  'assistant',
  'insights',
  'panel-view',
];
const PRUNED_SETTING_TABS = [
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
];

function fail(message) {
  throw new Error(`HALO_SCOPE_INVALID: ${message}`);
}

function readText(path) {
  try {
    return readFileSync(path, 'utf8');
  } catch (error) {
    fail(`cannot read ${relative(ROOT, path)}: ${error.message}`);
  }
}

function readJson(path) {
  try {
    return JSON.parse(readText(path));
  } catch (error) {
    fail(`cannot parse ${relative(ROOT, path)}: ${error.message}`);
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

function requireContains(source, needle, label) {
  if (!source.includes(needle)) fail(`${label} must contain ${needle}`);
}

function requireNotContains(source, needle, label) {
  if (source.includes(needle)) fail(`${label} must not contain ${needle}`);
}

function requireMatch(source, pattern, label, message = String(pattern)) {
  if (!pattern.test(source)) fail(`${label} must match ${message}`);
}

function requireNoMatch(source, pattern, label, message = String(pattern)) {
  if (pattern.test(source)) fail(`${label} must not match ${message}`);
}

function checkFormalScript(key, value) {
  const expectedScript = key === 'desktop:preview:debug'
    ? 'scripts/halo-web-ui-preview.mjs'
    : 'scripts/halo-tauri.mjs';
  if (typeof value !== 'string' || !value.includes(expectedScript)) {
    fail(`formal script ${key} must use ${expectedScript}`);
  }
  requireNotContains(value, EXTERNAL_REFERENCE, `formal script ${key}`);
  requireNotContains(value, 'src/halo-workbench', `formal script ${key}`);
  requireNotContains(value, 'halo-workbench', `formal script ${key}`);
}

function checkNoExternalOrWorkbenchReferences(rootDir, paths) {
  for (const path of paths) {
    const label = relative(rootDir, path);
    const source = readText(path);
    requireNotContains(source, EXTERNAL_REFERENCE, label);
    requireNotContains(source, 'D:/BitFun-main', label);
    requireNotContains(source, 'src/halo-workbench', label);
  }
}

function checkSourceAssembly(rootDir, scope) {
  const files = {
    app: requireFile(rootDir, `${scope.frontendRoot}/src/app/App.tsx`),
    appLayout: requireFile(rootDir, `${scope.frontendRoot}/src/app/layout/AppLayout.tsx`),
    registry: requireFile(rootDir, `${scope.frontendRoot}/src/app/scenes/registry.ts`),
    sceneViewport: requireFile(rootDir, `${scope.frontendRoot}/src/app/scenes/SceneViewport.tsx`),
    useSceneManager: requireFile(rootDir, `${scope.frontendRoot}/src/app/hooks/useSceneManager.ts`),
    sceneStore: requireFile(rootDir, `${scope.frontendRoot}/src/app/stores/sceneStore.ts`),
    navPanel: requireFile(rootDir, `${scope.frontendRoot}/src/app/components/NavPanel/NavPanel.tsx`),
    mainNav: requireFile(rootDir, `${scope.frontendRoot}/src/app/components/NavPanel/MainNav.tsx`),
    footerActions: requireFile(rootDir, `${scope.frontendRoot}/src/app/components/NavPanel/components/PersistentFooterActions.tsx`),
    searchDialog: requireFile(rootDir, `${scope.frontendRoot}/src/app/components/NavPanel/NavSearchDialog.tsx`),
    workspaceList: requireFile(rootDir, `${scope.frontendRoot}/src/app/components/NavPanel/sections/workspaces/WorkspaceListSection.tsx`),
    workspaceItem: requireFile(rootDir, `${scope.frontendRoot}/src/app/components/NavPanel/sections/workspaces/WorkspaceItem.tsx`),
    workbenchSessions: requireFile(rootDir, `${scope.frontendRoot}/src/app/components/NavPanel/sections/sessions/WorkbenchSessionsSection.tsx`),
    welcomeScene: requireFile(rootDir, `${scope.frontendRoot}/src/app/scenes/welcome/WelcomeScene.tsx`),
    settingsConfig: requireFile(rootDir, `${scope.frontendRoot}/src/app/scenes/settings/settingsConfig.ts`),
    settingsContentRegistry: requireFile(rootDir, `${scope.frontendRoot}/src/app/scenes/settings/settingsContentRegistry.ts`),
    settingsScene: requireFile(rootDir, `${scope.frontendRoot}/src/app/scenes/settings/SettingsScene.tsx`),
    settingsSearchContent: requireFile(rootDir, `${scope.frontendRoot}/src/app/scenes/settings/settingsTabSearchContent.ts`),
    dispatchTargetPicker: requireFile(rootDir, `${scope.frontendRoot}/src/features/dispatch/DispatchTargetPicker.tsx`),
    runtimeEnvironment: requireFile(rootDir, `${scope.frontendRoot}/src/infrastructure/runtime/environment.ts`),
    dailyUpdateGate: requireFile(rootDir, `${scope.frontendRoot}/src/infrastructure/update/DailyAppUpdateGate.tsx`),
    aiExperienceConfigService: requireFile(rootDir, `${scope.frontendRoot}/src/infrastructure/config/services/AIExperienceConfigService.ts`),
    agentCompanionWindowService: requireFile(rootDir, `${scope.frontendRoot}/src/infrastructure/config/services/AgentCompanionWindowService.ts`),
    deferredStartupSystems: requireFile(rootDir, `${scope.frontendRoot}/src/app/startup/deferredStartupSystems.ts`),
    permissionRequestNotify: requireFile(rootDir, `${scope.frontendRoot}/src/app/hooks/usePermissionRequestNotify.ts`),
    chatInput: requireFile(rootDir, `${scope.frontendRoot}/src/flow_chat/components/ChatInput.tsx`),
    workbenchClient: requireFile(rootDir, `${scope.frontendRoot}/src/infrastructure/workbench-runtime/client.ts`),
    workbenchStore: requireFile(rootDir, `${scope.frontendRoot}/src/infrastructure/workbench-runtime/store.ts`),
    workbenchTypes: requireFile(rootDir, `${scope.frontendRoot}/src/infrastructure/workbench-runtime/types.ts`),
  };

  const appSource = readText(files.app);
  requireContains(appSource, 'isHaloLocalCodingScope() || !interactiveShellReady', 'App.tsx');
  requireContains(appSource, 'if (isHaloLocalCodingScope()) {', 'App.tsx');
  requireContains(appSource, 'isHaloLocalCodingScope() && isTauriRuntime()', 'App.tsx');
  requireContains(appSource, 'workbenchRuntimeStore.getState().start()', 'App.tsx');
  requireContains(appSource, 'includeAgentExtensions: !isHaloLocalCodingScope()', 'App.tsx');
  requireNoMatch(appSource, /SSHRemoteProvider|RemoteWorkspaceProvider/, 'App.tsx');

  const appLayoutSource = readText(files.appLayout);
  requireContains(appLayoutSource, 'recentWorkspaces.filter(workspace => !isRemoteWorkspace(workspace))', 'AppLayout.tsx');
  requireContains(appLayoutSource, 'if (isRemoteWorkspace(currentWorkspace)) return;', 'AppLayout.tsx');
  requireContains(appLayoutSource, 'if (isHaloLocalCodingScope()) return;', 'AppLayout.tsx');
  requireContains(appLayoutSource, "type: 'openWorkspace'", 'AppLayout.tsx');
  requireNoMatch(appLayoutSource, /import\s+\{\s*FlowChatManager\s*\}/, 'AppLayout.tsx');
  requireNoMatch(appLayoutSource, /FloatingMiniChat|MCPInteractionDialog|SSHContext|WorkspaceKind|bitfun:create-acp-session|bitfun:acp-session-creation|createAcpChatSession|ensureAssistantBootstrap|agent-companion|MiniApp/, 'AppLayout.tsx');

  const registrySource = readText(files.registry);
  const registeredScenes = Array.from(registrySource.matchAll(/id:\s*'([^']+)'/g), match => match[1]);
  if (JSON.stringify(registeredScenes) !== JSON.stringify(EXPECTED_SCENES)) {
    fail(`scene registry must expose ${EXPECTED_SCENES.join(', ')}; got ${registeredScenes.join(', ')}`);
  }
  for (const id of PRUNED_SCENE_IDS) {
    requireNoMatch(registrySource, new RegExp(`id:\\s*'${id}'`), 'scene registry', id);
  }
  requireNoMatch(registrySource, /miniapp:\$\{string\}|miniAppStore|DYNAMIC_MINIAPP/, 'scene registry');

  const sceneViewportSource = readText(files.sceneViewport);
  for (const id of PRUNED_SCENE_IDS) {
    requireNotContains(sceneViewportSource, `case '${id}'`, 'SceneViewport.tsx');
    requireNoMatch(sceneViewportSource, new RegExp(`scenes/${id}|\\./${id}`), 'SceneViewport.tsx', id);
  }
  requireNoMatch(sceneViewportSource, /miniapp|MiniApp/, 'SceneViewport.tsx');

  const sceneManagerSource = readText(files.useSceneManager);
  requireNoMatch(sceneManagerSource, /miniApp|miniapp|DYNAMIC_MINIAPP/, 'useSceneManager.ts');

  const sceneStoreSource = readText(files.sceneStore);
  requireNoMatch(sceneStoreSource, /miniApp|miniapp|DYNAMIC_MINIAPP/, 'sceneStore.ts');

  const navPanelSource = readText(files.navPanel);
  requireNoMatch(navPanelSource, /PeerRemoteBadge|RemoteConnectDialog/, 'NavPanel.tsx');

  const mainNavSource = readText(files.mainNav);
  requireContains(mainNavSource, "openScene('file-viewer')", 'MainNav.tsx');
  requireContains(mainNavSource, "openScene('git')", 'MainNav.tsx');
  requireContains(mainNavSource, 'data-testid="nav-new-code-session-btn"', 'MainNav.tsx');
  requireContains(mainNavSource, 'data-testid="nav-file-viewer-btn"', 'MainNav.tsx');
  requireContains(mainNavSource, 'data-testid="nav-git-btn"', 'MainNav.tsx');
  requireContains(mainNavSource, 'WorkspaceListSection variant="projects"', 'MainNav.tsx');
  requireContains(mainNavSource, 'workbenchRuntimeStore', 'MainNav.tsx');
  requireContains(mainNavSource, "intent: { type: 'createSession', mode: 'standard' }", 'MainNav.tsx');
  requireNoMatch(mainNavSource, /FlowChatManager|flowChatManager|resolveAgentTypeForSessionCreation/, 'MainNav.tsx');
  requireNoMatch(mainNavSource, /MiniAppEntry|Cowork|SSHRemote|RemoteWorkspaceDialog|openScene\('assistant'\)|openScene\('browser'\)|openScene\('insights'\)/, 'MainNav.tsx');

  const footerSource = readText(files.footerActions);
  requireContains(footerSource, "openScene('settings')", 'PersistentFooterActions.tsx');
  requireContains(footerSource, "openNavScene('shell')", 'PersistentFooterActions.tsx');
  requireContains(footerSource, 'NotificationButton', 'PersistentFooterActions.tsx');
  requireNoMatch(footerSource, /RemoteConnectDialog|remoteControl|showRemoteConnect|ToolbarModeContext|PictureInPicture|Smartphone|Globe|BarChart3|openScene\('browser'\)|openScene\('insights'\)/, 'PersistentFooterActions.tsx');

  const searchSource = readText(files.searchDialog);
  requireContains(searchSource, 'WorkspaceKind.Assistant', 'NavSearchDialog.tsx');
  requireContains(searchSource, 'isRemoteWorkspace', 'NavSearchDialog.tsx');
  requireNoMatch(searchSource, /assistantWorkspacesList|groupAssistants|openScene\('assistant'\)|useMyAgentStore|useNurseryStore/, 'NavSearchDialog.tsx');

  const workspaceListSource = readText(files.workspaceList);
  requireContains(workspaceListSource, 'normalWorkspacesList.filter(workspace => !isRemoteWorkspace(workspace))', 'WorkspaceListSection.tsx');
  requireNoMatch(workspaceListSource, /assistantWorkspacesList|emptyAssistants|variant:\s*'assistants'/, 'WorkspaceListSection.tsx');

  const workspaceItemSource = readText(files.workspaceItem);
  requireContains(workspaceItemSource, 'WorkbenchSessionsSection', 'WorkspaceItem.tsx');
  requireContains(workspaceItemSource, 'workbenchRuntimeStore', 'WorkspaceItem.tsx');
  requireNoMatch(
    workspaceItemSource,
    /FlowChatManager|flowChatManager|from ['"]\.\.\/sessions\/SessionsSection|<SessionsSection/,
    'WorkspaceItem.tsx'
  );
  requireNoMatch(workspaceItemSource, /Cowork|cowork|Acp|acp|ScheduledJobs|RemoteConnectDialog|WorkspaceKind|isRemoteWorkspace|assistant|MiniApp|MCP|openScene\('agents'\)|openScene\('skills'\)/i, 'WorkspaceItem.tsx');

  const workbenchSessionsSource = readText(files.workbenchSessions);
  requireContains(workbenchSessionsSource, 'selectWorkbenchRuntimeSessionsForWorkspace', 'WorkbenchSessionsSection.tsx');
  requireNoMatch(workbenchSessionsSource, /FlowChatManager|flowChatManager|agentAPI|ACPClientAPI|MCPAPI/, 'WorkbenchSessionsSection.tsx');

  const welcomeSceneSource = readText(files.welcomeScene);
  requireContains(welcomeSceneSource, 'filter(ws => !isRemoteWorkspace(ws))', 'WelcomeScene.tsx');

  const settingsConfigSource = readText(files.settingsConfig);
  for (const id of PRUNED_SETTING_TABS) {
    requireNoMatch(settingsConfigSource, new RegExp(`id:\\s*'${id}'`), 'settingsConfig.ts', id);
  }

  const settingsContentRegistrySource = readText(files.settingsContentRegistry);
  const settingsSceneSource = readText(files.settingsScene);
  const settingsSearchContentSource = readText(files.settingsSearchContent);
  for (const id of PRUNED_SETTING_TABS) {
    requireNoMatch(settingsContentRegistrySource, new RegExp(`['"]${id}['"]`), 'settingsContentRegistry.ts', id);
    requireNoMatch(settingsSceneSource, new RegExp(`case '${id}'`), 'SettingsScene.tsx', id);
    requireNoMatch(settingsSearchContentSource, new RegExp(`['"]${id}['"]`), 'settingsTabSearchContent.ts', id);
  }
  requireNoMatch(settingsContentRegistrySource, /AIModelConfig|WorktreesConfig|SessionConfig|McpToolsConfig|ExternalSourcesConfig|AcpAgentsConfig|ReviewConfig|VoiceInputConfig|MemoriesConfig|QuickActionsConfig|HooksConfig/, 'settingsContentRegistry.ts');

  const dispatchTargetPickerSource = readText(files.dispatchTargetPicker);
  requireContains(dispatchTargetPickerSource, 'data-dispatch-kind="local"', 'DispatchTargetPicker.tsx');
  requireNoMatch(
    dispatchTargetPickerSource,
    /SSHConnectionDialog|DispatchInstallDialog|useDispatchTargets|sshDialogOpen|configureTarget|chatInput\.dispatch\.sshSection|chatInput\.dispatch\.addSsh/,
    'DispatchTargetPicker.tsx'
  );

  const runtimeEnvironmentSource = readText(files.runtimeEnvironment);
  requireContains(runtimeEnvironmentSource, 'document.documentElement.dataset.haloScope === \'local-coding\'', 'environment.ts');

  const dailyUpdateGateSource = readText(files.dailyUpdateGate);
  requireContains(dailyUpdateGateSource, 'const haloLocalCodingScope = isHaloLocalCodingScope();', 'DailyAppUpdateGate.tsx');
  requireContains(dailyUpdateGateSource, 'haloLocalCodingScope || !isTauriRuntime()', 'DailyAppUpdateGate.tsx');

  const aiExperienceConfigSource = readText(files.aiExperienceConfigService);
  requireContains(aiExperienceConfigSource, 'merged.enable_agent_companion = false;', 'AIExperienceConfigService.ts');
  requireContains(aiExperienceConfigSource, 'merged.agent_companion_display_mode = \'input\';', 'AIExperienceConfigService.ts');

  const agentCompanionWindowServiceSource = readText(files.agentCompanionWindowService);
  requireContains(agentCompanionWindowServiceSource, 'if (isHaloLocalCodingScope()) return;', 'AgentCompanionWindowService.ts');

  const deferredStartupSource = readText(files.deferredStartupSystems);
  requireContains(deferredStartupSource, 'if (includeAgentExtensions)', 'deferredStartupSystems.ts');

  const permissionRequestNotifySource = readText(files.permissionRequestNotify);
  requireContains(permissionRequestNotifySource, 'if (isHaloLocalCodingScope()) return;', 'usePermissionRequestNotify.ts');

  const chatInputSource = readText(files.chatInput);
  requireContains(chatInputSource, 'const legacyExecutionEnabled = !isHaloLocalCodingScope();', 'ChatInput.tsx');
  requireContains(chatInputSource, 'disabled={!legacyExecutionEnabled}', 'ChatInput.tsx');

  const workbenchClientSource = readText(files.workbenchClient);
  requireContains(workbenchClientSource, "'halo_workbench_runtime_snapshot'", 'workbench-runtime/client.ts');
  requireContains(workbenchClientSource, "'halo_workbench_runtime_submit_intent'", 'workbench-runtime/client.ts');
  requireContains(workbenchClientSource, "'halo-workbench://event'", 'workbench-runtime/client.ts');
  requireNoMatch(workbenchClientSource, /Authorization|Bearer\s|EventSource|WebSocket|fetch\(|opencode serve|opencode acp|127\.0\.0\.1|localhost:|sidecar/i, 'workbench-runtime/client.ts');

  const workbenchStoreSource = readText(files.workbenchStore);
  requireContains(workbenchStoreSource, 'PI_RPC_ADAPTER_IDENTITY', 'workbench-runtime/store.ts');
  requireContains(workbenchStoreSource, 'client.subscribe', 'workbench-runtime/store.ts');
  requireContains(workbenchStoreSource, 'client.readSnapshot', 'workbench-runtime/store.ts');

  const workbenchTypesSource = readText(files.workbenchTypes);
  requireContains(workbenchTypesSource, "'pi-rpc-p0'", 'workbench-runtime/types.ts');
  requireNoMatch(
    workbenchTypesSource,
    /authorization|credential|accessToken|remoteSession|remoteMessage|\bport\b|https?:\/\/|serverUrl/i,
    'workbench-runtime/types.ts'
  );
}

export function verifyHaloScope(rootDir = ROOT) {
  const scopePath = join(rootDir, 'halo-scope.json');
  const scope = readJson(scopePath);
  const packageJsonPath = join(rootDir, 'package.json');
  const packageJson = readJson(packageJsonPath);
  const configPath = requireFile(rootDir, scope.tauriConfig);
  const config = readJson(configPath);
  const desktopCargoToml = requireFile(rootDir, `${scope.desktopRoot}/Cargo.toml`);
  const desktopMain = requireFile(rootDir, `${scope.desktopRoot}/src/main.rs`);
  const webIndexPath = requireFile(rootDir, `${scope.frontendRoot}/index.html`);
  const webMainPath = requireFile(rootDir, `${scope.frontendRoot}/src/main.tsx`);
  const webVitePath = requireFile(rootDir, `${scope.frontendRoot}/vite.config.ts`);
  const webPackagePath = requireFile(rootDir, `${scope.frontendRoot}/package.json`);
  const webPackageJson = readJson(webPackagePath);

  if (scope.schemaVersion !== 1) fail('schemaVersion must be 1');
  if (scope.productId !== 'halo-studio') fail('productId must be halo-studio');
  if (scope.displayName !== 'Halo Studio') fail('displayName must be Halo Studio');
  if (scope.bundleIdentifier !== 'com.halostudio.desktop') fail('bundle identifier changed without a product decision');
  if (scope.desktopRoot !== 'src/apps/halo-desktop') fail('desktopRoot must stay on the Halo Tauri shell');
  if (scope.frontendRoot !== 'src/web-ui') fail('frontendRoot must be the BitFun Web UI source root');
  if (scope.developmentEntry !== 'node scripts/halo-tauri.mjs dev') fail('developmentEntry is not the Halo Tauri wrapper');
  if (scope.packagingEntry !== 'node scripts/halo-tauri.mjs build') fail('packagingEntry is not the Halo Tauri wrapper');
  if (!Array.isArray(scope.excludedRoutes) || scope.excludedRoutes.length === 0) fail('excludedRoutes must be declared');
  if (!scope.runtimePolicy || Object.values(scope.runtimePolicy).some(value => value !== false && value !== 'local-only')) {
    fail('Halo runtime policy must keep all out-of-scope capabilities disabled');
  }
  if (JSON.stringify(scope.includedModules) !== JSON.stringify([
    'local-workspaces',
    'coding-sessions',
    'file-explorer',
    'editor',
    'git',
    'terminal',
  ])) {
    fail('includedModules must describe the local coding workbench');
  }

  for (const key of FORMAL_SCRIPT_KEYS) {
    if (packageJson.scripts?.[key] !== undefined) checkFormalScript(key, packageJson.scripts[key]);
  }
  for (const key of scope.excludedRoutes) {
    if (packageJson.scripts?.[key] !== undefined) fail(`out-of-scope route ${key} must not be exposed by the Halo package`);
  }
  if (packageJson.scripts?.dev !== scope.developmentEntry) fail('package dev script diverges from developmentEntry');
  if (packageJson.scripts?.build !== scope.packagingEntry) fail('package build script diverges from packagingEntry');
  if (packageJson.scripts?.['desktop:preview:debug'] !== 'node scripts/halo-web-ui-preview.mjs') {
    fail('desktop preview must use the Halo Web UI preview wrapper');
  }
  if (packageJson.scripts?.['type-check:web'] !== 'pnpm --dir src/web-ui run type-check') {
    fail('type-check:web must target the BitFun Web UI package');
  }

  if (config.productName !== scope.displayName) fail('Tauri productName diverges from scope');
  if (config.identifier !== scope.bundleIdentifier) fail('Tauri identifier diverges from scope');
  if (config.build?.beforeDevCommand !== 'node ../../scripts/halo-web-ui-dev-server.mjs') fail('Tauri dev command is not the Halo Web UI wrapper');
  if (config.build?.devUrl !== 'http://localhost:1422') fail('Tauri devUrl must match BitFun Web UI Vite');
  if (config.build?.beforeBuildCommand !== 'node ../../scripts/halo-web-ui-build.mjs') fail('Tauri build command is not the Halo Web UI wrapper');
  if (config.build?.frontendDist !== '../../../dist') fail('Tauri frontendDist must load the BitFun Web UI build output');
  if (config.app?.security?.csp !== null) fail('BitFun Web UI requires the inherited desktop CSP compatibility setting');
  if (config.app?.withGlobalTauri !== true) fail('BitFun Web UI requires withGlobalTauri');
  if (config.app?.windows?.[0]?.visible !== true) fail('Halo main window must be visible by default');
  if (!String(config.app?.windows?.[0]?.title ?? '').includes('Halo Studio')) fail('Halo window title must use Halo Studio');
  if (!Array.isArray(config.bundle?.icon) || !config.bundle.icon.includes('icons/halo-icon.ico')) fail('Tauri bundle must use the Halo desktop icon');
  for (const icon of config.bundle.icon) requireFile(rootDir, `${scope.desktopRoot}/${icon}`);
  const desktopCargoSource = readText(desktopCargoToml);
  requireContains(desktopCargoSource, 'halo-local-coding', 'src/apps/halo-desktop/Cargo.toml');
  for (const command of [config.build.beforeDevCommand, config.build.beforeBuildCommand]) {
    const [, scriptPath] = String(command).split(/\s+/, 2);
    if (!scriptPath) fail(`Tauri hook command is missing a script path: ${command}`);
    const resolvedHook = resolve(rootDir, 'src/apps', scriptPath);
    if (!existsSync(resolvedHook)) {
      fail(`Tauri hook command does not resolve from Tauri hook cwd: ${command}`);
    }
  }

  const indexHtml = readText(webIndexPath);
  requireContains(indexHtml, 'lang="zh-CN"', 'src/web-ui/index.html');
  requireContains(indexHtml, 'data-halo-scope="local-coding"', 'src/web-ui/index.html');
  requireContains(indexHtml, 'data-product-id="halo-studio"', 'src/web-ui/index.html');
  requireContains(indexHtml, '<script type="module" src="/src/main.tsx"></script>', 'src/web-ui/index.html');
  requireContains(indexHtml, '/halo-icon.svg', 'src/web-ui/index.html');
  requireContains(indexHtml, 'Halo Studio', 'src/web-ui/index.html');
  requireFile(rootDir, `${scope.frontendRoot}/public/halo-icon.svg`);

  const webMain = readText(webMainPath);
  requireContains(webMain, 'createRoot', 'src/web-ui/src/main.tsx');
  requireContains(webMain, '<App />', 'src/web-ui/src/main.tsx');

  const haloDesktopMain = readText(desktopMain);
  requireContains(
    haloDesktopMain,
    'bitfun_desktop_lib::run_with_context_and_options',
    'src/apps/halo-desktop/src/main.rs',
  );
  requireContains(
    haloDesktopMain,
    'bitfun_desktop_lib::DesktopRunOptions::with_logs_root',
    'src/apps/halo-desktop/src/main.rs',
  );
  requireNoMatch(haloDesktopMain, /tauri::Builder::default\(\)\s*\.run/, 'src/apps/halo-desktop/src/main.rs');

  const viteConfig = readText(webVitePath);
  requireMatch(viteConfig, /port:\s*1422/, 'src/web-ui/vite.config.ts', 'dev server port 1422');
  requireMatch(viteConfig, /strictPort:\s*true/, 'src/web-ui/vite.config.ts', 'strictPort true');
  requireMatch(viteConfig, /port:\s*1421/, 'src/web-ui/vite.config.ts', 'HMR port 1421');
  requireContains(viteConfig, "outDir: '../../dist'", 'src/web-ui/vite.config.ts');

  if (webPackageJson.scripts?.dev !== 'vite') fail('BitFun Web UI dev script must be vite');
  if (webPackageJson.scripts?.['build:desktop'] !== 'vite build --mode desktop') fail('BitFun Web UI desktop build script changed');
  if (webPackageJson.scripts?.['type-check'] !== 'tsc --noEmit') fail('BitFun Web UI type-check script changed');
  if (webPackageJson.scripts?.['test:run'] !== 'vitest run') fail('BitFun Web UI test script changed');

  const wrapperFiles = [
    packageJsonPath,
    configPath,
    webIndexPath,
    webMainPath,
    webVitePath,
    webPackagePath,
    desktopCargoToml,
    desktopMain,
    requireFile(rootDir, 'scripts/halo-tauri.mjs'),
    requireFile(rootDir, 'scripts/halo-web-ui-dev-server.mjs'),
    requireFile(rootDir, 'scripts/halo-web-ui-build.mjs'),
    requireFile(rootDir, 'scripts/halo-web-ui-preview.mjs'),
  ];
  checkNoExternalOrWorkbenchReferences(rootDir, wrapperFiles);
  checkSourceAssembly(rootDir, scope);

  return {
    scopePath,
    configPath,
    desktopRoot: relative(rootDir, desktopCargoToml),
    frontendRoot: relative(rootDir, webIndexPath),
    frontendEntry: relative(rootDir, webMainPath),
    viteConfig: relative(rootDir, webVitePath),
    buildOutDir: 'dist',
    devUrl: config.build.devUrl,
    frontendDist: config.build.frontendDist,
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
