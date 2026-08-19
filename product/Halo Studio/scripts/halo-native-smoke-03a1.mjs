import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const productRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(productRoot, '..', '..');
const artifactRoot = path.join(
  repoRoot,
  'docs',
  'requirements',
  'halo-tauri-product-migration',
  'artifacts',
);
const exePath = path.join(productRoot, 'target', 'release-fast', 'halo-studio.exe');

const stamp = new Date().toISOString()
  .replace(/[-:]/g, '')
  .replace(/\..+$/, '')
  .replace('T', '-');
const baseName = `03a1-native-smoke-${stamp}`;
const appDataDir = path.join(os.tmpdir(), `${baseName}-appdata`);
const e2eUserRoot = path.join(os.tmpdir(), `${baseName}-user-root`);
const e2eHomeRoot = path.join(os.tmpdir(), `${baseName}-home`);
const smokeWorkspace = path.join(os.tmpdir(), `${baseName}-workspace`);
const logDir = path.join(artifactRoot, `${baseName}-logs`);
const haloLogRoot = path.join(appDataDir, 'Halo Studio', 'logs');
const haloLogRoot = path.join(appDataDir, 'halo', 'logs');
const keepFailureArtifacts = process.env.HALO_SMOKE_KEEP_FAILURE_ARTIFACTS === '1';

fs.mkdirSync(artifactRoot, { recursive: true });
fs.rmSync(appDataDir, { recursive: true, force: true });
fs.rmSync(e2eUserRoot, { recursive: true, force: true });
fs.rmSync(e2eHomeRoot, { recursive: true, force: true });
fs.rmSync(smokeWorkspace, { recursive: true, force: true });
fs.rmSync(logDir, { recursive: true, force: true });
fs.mkdirSync(path.join(smokeWorkspace, 'src'), { recursive: true });
fs.mkdirSync(e2eUserRoot, { recursive: true });
fs.mkdirSync(e2eHomeRoot, { recursive: true });
fs.mkdirSync(logDir, { recursive: true });
fs.writeFileSync(path.join(smokeWorkspace, 'README.md'), '# Halo 03A1 smoke\n', 'utf8');
fs.writeFileSync(
  path.join(smokeWorkspace, 'main.ts'),
  'export const smoke = "halo-03a1";\n',
  'utf8',
);
fs.writeFileSync(
  path.join(smokeWorkspace, 'src', 'main.ts'),
  'export const smoke = "halo-03a1";\n',
  'utf8',
);
fs.writeFileSync(
  path.join(smokeWorkspace, 'package.json'),
  '{"name":"halo-03a1-smoke","private":true}\n',
  'utf8',
);

if (!fs.existsSync(exePath)) {
  throw new Error(`Missing release executable: ${exePath}`);
}

const port = await getFreePort();
const child = spawn(exePath, [], {
  cwd: productRoot,
  env: {
    ...process.env,
    APPDATA: appDataDir,
    LOCALAPPDATA: appDataDir,
    HALO_E2E_STORAGE_GUARD: '1',
    HALO_USER_ROOT: e2eUserRoot,
    HALO_HOME: e2eHomeRoot,
    HALO_E2E_LOG_DIR: logDir,
    HALO_LOG_DIR: haloLogRoot,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
  windowsHide: false,
});
let childExit = null;
child.once('exit', (code, signal) => {
  childExit = { code, signal };
});

let cdp;
let finalSummary;
let smokeStage = 'launch';
let cdpTargetDiagnostics = null;
try {
  smokeStage = 'wait-for-page';
  const target = await waitForCdpTarget(port, 60000);
  cdp = await connectCdp(target.webSocketDebuggerUrl);
  await cdp.send('Runtime.enable');
  await cdp.send('Page.enable');

  smokeStage = 'wait-for-app-layout';
  await waitForEval(cdp, `
    document.readyState === 'complete'
      && !!document.querySelector('[data-testid="app-layout"]')
      && document.documentElement.dataset.haloScope === 'local-coding'
  `, 60000);

  smokeStage = 'capture-before';
  const wideBeforePath = path.join(artifactRoot, `${baseName}-wide-before.png`);
  const wideBefore = captureWindow(child.pid, wideBeforePath, 1440, 900);

  smokeStage = 'initial-dom';
  const initialDom = await evalValue(cdp, domProbeExpression());

  smokeStage = 'window-controls';
  const windowInteraction = await evalValue(cdp, `
    (async () => {
      const invoke = window.__TAURI__?.core?.invoke;
      if (typeof invoke !== 'function') {
        return { ok: false, stage: 'resolve-invoke', error: 'Tauri core invoke is unavailable' };
      }
      const invokeWithTimeout = async (command, timeoutMs = 5000) => {
        let timer;
        try {
          return await Promise.race([
            invoke(command),
            new Promise((_, reject) => {
              timer = setTimeout(() => reject(new Error('Timed out invoking ' + command)), timeoutMs);
            }),
          ]);
        } finally {
          clearTimeout(timer);
        }
      };
      const state = async () => invokeWithTimeout('plugin:window|is_maximized');
      try {
        const before = await state();
        if (before) await invokeWithTimeout('plugin:window|unmaximize');
        await invokeWithTimeout('plugin:window|maximize');
        await new Promise((resolve) => setTimeout(resolve, 350));
        const afterMaximize = await state();
        await invokeWithTimeout('plugin:window|unmaximize');
        await new Promise((resolve) => setTimeout(resolve, 350));
        const afterRestore = await state();
        return { ok: true, before, afterMaximize, afterRestore };
      } catch (error) {
        return {
          ok: false,
          stage: 'window-command',
          error: String(error && error.message ? error.message : error),
        };
      }
    })()
  `);

  smokeStage = 'open-workspace';
  const workspaceResult = await evalValue(cdp, `
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const waitFor = async (predicate, timeoutMs = 30000) => {
        const started = Date.now();
        while (Date.now() - started < timeoutMs) {
          const value = predicate();
          if (value) return value;
          await sleep(250);
        }
        return false;
      };

      const internals = window.__TAURI_INTERNALS__;
      const originalInvoke = internals?.invoke;
      if (!internals || typeof originalInvoke !== 'function') {
        return { ok: false, stage: 'resolve-tauri-invoke', error: 'Tauri internals invoke is unavailable' };
      }

      let dialogRequested = false;
      let workspaceCommandSucceeded = false;
      internals.invoke = async function smokeInvoke(command, args, options) {
        if (command === 'plugin:dialog|open') {
          dialogRequested = true;
          return ${JSON.stringify(smokeWorkspace)};
        }

        const result = await originalInvoke.call(this, command, args, options);
        if (command === 'open_workspace') {
          workspaceCommandSucceeded = true;
        }
        return result;
      };

      try {
        const workspaceButton = document.querySelector('[data-testid="nav-workspace-add-btn"]');
        if (!workspaceButton) {
          return { ok: false, stage: 'find-workspace-menu-button', error: 'Workspace menu button is unavailable' };
        }

        workspaceButton.click();
        const menu = await waitFor(() => document.querySelector('.halo-nav-panel__workspace-menu'), 5000);
        if (!menu) {
          return { ok: false, stage: 'open-workspace-menu', error: 'Workspace menu did not open' };
        }

        const openProjectItem = [...menu.querySelectorAll('.halo-nav-panel__workspace-menu-item')]
          .find((element) => /open|鎵撳紑|椤圭洰/i.test(element.textContent || ''));
        if (!openProjectItem) {
          return { ok: false, stage: 'find-open-project-item', error: 'Open project menu item is unavailable' };
        }

        openProjectItem.click();
        const workspaceVisible = await waitFor(() =>
          workspaceCommandSucceeded
            && document.body.innerText.includes('${path.basename(smokeWorkspace)}'),
          30000,
        );

        return {
          ok: Boolean(dialogRequested && workspaceCommandSucceeded && workspaceVisible),
          dialogRequested,
          workspaceCommandSucceeded,
          workspaceVisible: Boolean(workspaceVisible),
        };
      } catch (error) {
        return { ok: false, stage: 'open-workspace-through-ui', error: String(error && error.message ? error.message : error) };
      } finally {
        internals.invoke = originalInvoke;
      }
    })()
  `);

  smokeStage = 'wait-for-workspace';
  await waitForEval(cdp, `
    !!document.querySelector('[data-testid="nav-panel"]')
      && document.body.innerText.includes('${path.basename(smokeWorkspace)}')
  `, 30000).catch(() => {});

  smokeStage = 'workspace-dom';
  const workspaceDom = await evalValue(cdp, domProbeExpression());

  smokeStage = 'file-interaction';
  const fileInteraction = await clickAndProbe(cdp, '[data-testid="nav-file-viewer-btn"]', `
    (async () => {
      const waitFor = async (predicate, timeoutMs = 15000) => {
        const started = Date.now();
        while (Date.now() - started < timeoutMs) {
          const value = predicate();
          if (value) return value;
          await sleep(250);
        }
        return false;
      };
      await waitFor(() => document.querySelector('.halo-file-explorer__tree'), 5000);
      const fileNodes = () => [
        ...document.querySelectorAll(
          '.halo-file-explorer__node-content[data-file="true"][data-is-directory="false"]',
        ),
      ];
      const fileNode = fileNodes().find((node) =>
        /(^|[\\\\/])main\\.ts$/i.test(node.getAttribute('data-file-path') || '')
      ) ?? fileNodes().find((node) => /README\\.md|main\\.ts/.test(node.textContent || ''));
      const clickedFile = fileNode ? {
        text: (fileNode.textContent || '').trim(),
        dataFile: fileNode.getAttribute('data-file'),
        dataIsDirectory: fileNode.getAttribute('data-is-directory'),
        pathSuffix: (fileNode.getAttribute('data-file-path') || '').split(/[\\\\/]/).slice(-2).join('/'),
      } : null;
      if (fileNode) {
        fileNode.scrollIntoView({ block: 'center' });
        fileNode.focus();
        fileNode.click();
      }
      await waitFor(() =>
        document.querySelector('.canvas-tab[data-tab-title="main.ts"][data-tab-type="code-editor"]')
          || document.querySelector('.code-editor-tool[data-monaco-editor="true"][data-file-path$="main.ts"]')
          || document.querySelector('.canvas-tab[data-tab-title="README.md"][data-tab-type="markdown-editor"]')
          || document.querySelector('.halo-markdown-editor'),
        6000,
      );
      await waitFor(() => {
        const codeEditor = document.querySelector('.code-editor-tool[data-monaco-editor="true"][data-file-path$="main.ts"]');
        if (!codeEditor) return false;
        return !codeEditor.classList.contains('is-loading') && !codeEditor.classList.contains('is-error');
      }, 6000);
      await waitFor(() => {
        const monacoText = [...document.querySelectorAll('.monaco-editor .view-line')]
          .map((line) => line.textContent || '')
          .join('\\n');
        const modelTextVisible = Boolean(
          globalThis.monaco?.editor?.getModels?.().some((model) => {
            const modelText = model.getValue?.() || '';
            return modelText.includes('export const smoke') || modelText.includes('halo-03a1');
          }),
        );
        return monacoText.includes('export const smoke')
          || monacoText.includes('halo-03a1')
          || modelTextVisible;
      }, 6000);
      const tabs = [...document.querySelectorAll('.canvas-tab')].map((tab) => ({
        title: tab.getAttribute('data-tab-title'),
        type: tab.getAttribute('data-tab-type'),
        active: tab.getAttribute('data-active'),
        pathSuffix: (tab.getAttribute('data-file-path') || '').split(/[\\\\/]/).slice(-2).join('/'),
      }));
      const activeFileTab = tabs.find((tab) =>
        tab.active === 'true'
          && (tab.title === 'main.ts' || tab.title === 'README.md')
          && /(?:^|\\/)main\\.ts$|(?:^|\\/)README\\.md$/i.test(tab.pathSuffix)
      );
      const codeEditor = document.querySelector('.code-editor-tool[data-monaco-editor="true"][data-file-path$="main.ts"]');
      const markdownEditor = document.querySelector('.halo-markdown-editor');
      const visibleText = document.body.innerText || '';
      const monacoText = [...document.querySelectorAll('.monaco-editor .view-line')]
        .map((line) => line.textContent || '')
        .join('\\n');
      const monacoModelTextVisible = Boolean(
        globalThis.monaco?.editor?.getModels?.().some((model) => {
          const modelText = model.getValue?.() || '';
          return modelText.includes('export const smoke') || modelText.includes('halo-03a1');
        }),
      );
      const monacoModels = globalThis.monaco?.editor?.getModels?.() || [];
      return {
        activeScene: activeSceneId(),
        fileViewerScene: !!document.querySelector('.halo-file-viewer-scene'),
        fileExplorer: !!document.querySelector('.halo-file-explorer'),
        fileTree: !!document.querySelector('.halo-file-explorer__tree'),
        fileNodeClicked: !!fileNode,
        clickedFile,
        tabs,
        fileTabOpened: !!activeFileTab,
        codeEditor: !!codeEditor,
        markdownEditor: !!markdownEditor,
        editor: !!codeEditor || !!markdownEditor,
        editorReady:
          !!codeEditor
          && !codeEditor.classList.contains('is-loading')
          && !codeEditor.classList.contains('is-error'),
        openedFilePathMatches:
          !!activeFileTab
          && (activeFileTab.pathSuffix === 'main.ts' || activeFileTab.pathSuffix.endsWith('/main.ts')
            || activeFileTab.pathSuffix === 'README.md' || activeFileTab.pathSuffix.endsWith('/README.md')),
        openedFileTextVisible:
          visibleText.includes('export const smoke')
          || visibleText.includes('halo-03a1')
          || visibleText.includes('Halo 03A1 smoke')
          || monacoText.includes('export const smoke')
          || monacoText.includes('halo-03a1')
          || monacoModelTextVisible,
        monacoModelCount: monacoModels.length,
        monacoTextLength: monacoText.length,
        monacoTextSample: monacoText.slice(0, 120),
      };
    })()
  `);

  smokeStage = 'git-interaction';
  const gitInteraction = await clickAndProbe(cdp, '[data-testid="nav-git-btn"]', `
    (async () => {
      await sleep(1000);
      return {
        activeScene: activeSceneId(),
        gitScene: !!document.querySelector('.halo-git-scene'),
        gitInitOrStatus:
          !!document.querySelector('.halo-git-scene__init-card, [data-shortcut-scope="git"], .git-status, .git-panel'),
      };
    })()
  `);

  smokeStage = 'terminal-interaction';
  const terminalInteraction = await clickAndProbe(cdp, '[data-testid="shell-panel-entry"]', `
    (async () => {
      await sleep(1000);
      return {
        shellPanel: !!document.querySelector('[data-testid="shell-panel"]'),
        shellList: !!document.querySelector('[data-testid="shell-command-list"]'),
        shellTitle: !!document.querySelector('[data-testid="shell-panel-title"]'),
      };
    })()
  `);

  smokeStage = 'session-interaction';
  const sessionInteraction = await clickAndProbe(cdp, '[data-testid="nav-new-code-session-btn"]', `
    (async () => {
      await sleep(1200);
      return {
        codeSessionButton: !!document.querySelector('[data-testid="nav-new-code-session-btn"]'),
        sessionScene: !!document.querySelector('[data-scene-id="session"]'),
        notificationVisible:
          !!document.querySelector('[role="alert"], .notification, .halo-notification, .toast'),
      };
    })()
  `);

  smokeStage = 'workspace-menu-interaction';
  const workspaceMenuInteraction = await clickAndProbe(cdp, '[data-testid="nav-workspace-add-btn"]', `
    (async () => {
      await sleep(500);
      return {
        workspaceAddButton: !!document.querySelector('[data-testid="nav-workspace-add-btn"]'),
        workspaceMenu: !!document.querySelector('.halo-nav-panel__workspace-menu'),
        openProjectMenuItem:
          !![...document.querySelectorAll('.halo-nav-panel__workspace-menu-item')]
            .find((el) => /open|打开|项目/.test(el.textContent || '')),
      };
    })()
  `);

  smokeStage = 'screenshots-and-logs';
  const afterDom = await evalValue(cdp, domProbeExpression());
  const cdpScreenshotPath = path.join(artifactRoot, `${baseName}-cdp-after.png`);
  const cdpScreenshot = await cdp.send('Page.captureScreenshot', {
    format: 'png',
    captureBeyondViewport: false,
  });
  fs.writeFileSync(cdpScreenshotPath, Buffer.from(cdpScreenshot.data, 'base64'));

  const wideAfterPath = path.join(artifactRoot, `${baseName}-wide-after.png`);
  const wideAfter = captureWindow(child.pid, wideAfterPath, 1440, 900);
  const narrowAfterPath = path.join(artifactRoot, `${baseName}-narrow-after.png`);
  const narrowAfter = captureWindow(child.pid, narrowAfterPath, 1120, 720);
  await sleep(500);
  const haloLogFiles = listLogFiles(haloLogRoot);
  const haloLogFiles = listLogFiles(haloLogRoot);
  const forbiddenFallbackLogFiles = listLogFiles(logDir);

  finalSummary = {
    status: 'passed',
    launchedProcess: {
      pid: child.pid,
      exePath: redactPath(exePath),
      windowBoundToPid: Boolean(wideBefore.hwnd && wideBefore.pid === child.pid),
      title: wideBefore.title,
      exit: childExit,
    },
    frontendProof: {
      url: afterDom.url,
      title: afterDom.title,
      lang: afterDom.lang,
      haloScope: afterDom.haloScope,
      productId: afterDom.productId,
      haloSelectorsPresent: afterDom.haloSelectorsPresent,
      oldHaloWorkbenchAbsent: afterDom.oldHaloWorkbenchAbsent,
      visibleBrandLeaks: afterDom.visibleBrandLeaks,
      scriptSourcesContainHaloWorkbench: afterDom.scriptSourcesContainHaloWorkbench,
    },
    interactions: {
      initial: pickDomBooleans(initialDom),
      window: windowInteraction,
      workspace: {
        openWorkspaceCommandOk: Boolean(workspaceResult.ok),
        openWorkspaceError: workspaceResult.ok ? null : redactText(workspaceResult.error),
        redactedWorkspace: '[temp]/halo-03a1-smoke-workspace',
        workspaceVisible: workspaceDom.workspaceVisible,
      },
      workspaceEntry: workspaceMenuInteraction,
      codeSessionEntry: sessionInteraction,
      fileTreeAndEditor: fileInteraction,
      gitEntry: gitInteraction,
      terminalEntry: terminalInteraction,
    },
    screenshots: {
      nativeWideBefore: redactPath(wideBeforePath),
      nativeWideAfter: redactPath(wideAfterPath),
      nativeNarrowAfter: redactPath(narrowAfterPath),
      cdpAfter: redactPath(cdpScreenshotPath),
    },
    nativeWindow: {
      wideBefore: redactCapture(wideBefore),
      wideAfter: redactCapture(wideAfter),
      narrowAfter: redactCapture(narrowAfter),
    },
    redaction: {
      omitted: ['remote debugging port', 'CDP websocket URL', 'session ids', 'message ids', 'authorization headers'],
      fullWorkspacePathRecorded: false,
    },
    diagnostics: {
      haloLogRoot: redactPath(haloLogRoot),
      haloLogFiles: haloLogFiles.map(redactPath),
      haloLogRoot: redactPath(haloLogRoot),
      haloLogFiles: haloLogFiles.map(redactPath),
      forbiddenFallbackLogDir: redactPath(logDir),
      forbiddenFallbackLogFiles: forbiddenFallbackLogFiles.map(redactPath),
    },
  };

  const checks = [
    finalSummary.launchedProcess.windowBoundToPid,
    wideBefore.visible && wideBefore.nonEmpty,
    wideAfter.visible && wideAfter.nonEmpty,
    narrowAfter.visible && narrowAfter.nonEmpty,
    /Halo Studio/.test(wideBefore.title),
    !/Halo|Agent Companion/.test(wideBefore.title),
    afterDom.haloScope === 'local-coding',
    afterDom.productId === 'halo-studio',
    afterDom.haloSelectorsPresent.navPanel,
    afterDom.haloSelectorsPresent.sceneViewport,
    afterDom.haloSelectorsPresent.appLayout,
    afterDom.oldHaloWorkbenchAbsent,
    !afterDom.visibleBrandLeaks.haloText,
    !afterDom.visibleBrandLeaks.rawI18nKeys,
    !afterDom.visibleBrandLeaks.updateDialog,
    windowInteraction.ok
      && windowInteraction.afterMaximize === true
      && windowInteraction.afterRestore === false,
    haloLogFiles.length > 0,
    haloLogFiles.length === 0,
    forbiddenFallbackLogFiles.length === 0,
    Boolean(workspaceResult.ok),
    workspaceDom.workspaceVisible,
    workspaceMenuInteraction.workspaceAddButton,
    sessionInteraction.codeSessionButton,
    fileInteraction.fileViewerScene
      && fileInteraction.fileExplorer
      && fileInteraction.fileTree
      && fileInteraction.fileNodeClicked
      && fileInteraction.fileTabOpened
      && fileInteraction.openedFilePathMatches
      && fileInteraction.editor
      && (fileInteraction.editorReady || fileInteraction.markdownEditor)
      && fileInteraction.openedFileTextVisible,
    gitInteraction.gitScene && gitInteraction.gitInitOrStatus,
    terminalInteraction.shellPanel && terminalInteraction.shellTitle,
  ];

  if (!checks.every(Boolean)) {
    finalSummary.status = 'failed';
    process.exitCode = 1;
  }
} catch (error) {
  finalSummary = {
    status: 'failed',
    stage: smokeStage,
    error: String(error && error.stack ? error.stack : error),
    launchedProcess: {
      pid: child.pid,
      exePath: redactPath(exePath),
      exit: childExit,
    },
    redaction: {
      omitted: ['remote debugging port', 'CDP websocket URL', 'session ids', 'message ids', 'authorization headers'],
      fullWorkspacePathRecorded: false,
    },
    diagnostics: {
      haloLogRoot: redactPath(haloLogRoot),
      haloLogFiles: listLogFiles(haloLogRoot).map(redactPath),
      haloLogRoot: redactPath(haloLogRoot),
      haloLogFiles: listLogFiles(haloLogRoot).map(redactPath),
      forbiddenFallbackLogDir: redactPath(logDir),
      forbiddenFallbackLogFiles: listLogFiles(logDir).map(redactPath),
      cdpTarget: cdpTargetDiagnostics,
    },
  };
  process.exitCode = 1;
} finally {
  try {
    if (cdp) cdp.close();
  } catch {}
  try {
    if (!child.killed) child.kill();
  } catch {}
  try {
    execFileSync('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {}
  if (keepFailureArtifacts && finalSummary?.status === 'failed' && fs.existsSync(appDataDir)) {
    try {
      fs.cpSync(
        appDataDir,
        path.join(artifactRoot, `${baseName}-failure-appdata`),
        { recursive: true, force: true },
      );
    } catch {}
  }
  if (!keepFailureArtifacts || finalSummary?.status !== 'failed') {
    fs.rmSync(appDataDir, { recursive: true, force: true });
  }
  fs.rmSync(e2eUserRoot, { recursive: true, force: true });
  fs.rmSync(e2eHomeRoot, { recursive: true, force: true });
  fs.rmSync(smokeWorkspace, { recursive: true, force: true });
}

const summaryPath = path.join(artifactRoot, `${baseName}-summary.json`);
fs.writeFileSync(summaryPath, `${JSON.stringify(finalSummary, null, 2)}\n`, 'utf8');
console.log(JSON.stringify({
  status: finalSummary.status,
  summaryPath: redactPath(summaryPath),
  screenshots: finalSummary.screenshots ?? {},
}, null, 2));

function domProbeExpression() {
  return `
    (() => {
      const scriptSources = [...document.scripts].map((script) => script.src || '');
      const active = document.querySelector('[data-testid="scene-viewport-scene"][data-scene-active="true"]');
      const visibleText = document.body.innerText || '';
      return {
        url: location.href,
        title: document.title,
        lang: document.documentElement.lang,
        haloScope: document.documentElement.dataset.haloScope || null,
        productId: document.documentElement.dataset.productId || null,
        activeScene: active?.getAttribute('data-scene-id') || null,
        haloSelectorsPresent: {
          appLayout: !!document.querySelector('[data-testid="app-layout"]'),
          workspaceBody: !!document.querySelector('.halo-workspace-body'),
          navPanel: !!document.querySelector('[data-testid="nav-panel"].halo-nav-panel, .halo-nav-panel'),
          sceneViewport: !!document.querySelector('[data-testid="scene-viewport"].halo-scene-viewport, .halo-scene-viewport'),
          sceneBar: !!document.querySelector('.halo-scene-bar'),
          fileViewer: !!document.querySelector('.halo-file-viewer-scene'),
          gitScene: !!document.querySelector('.halo-git-scene'),
          shellPanel: !!document.querySelector('[data-testid="shell-panel"]'),
        },
        oldHaloWorkbenchAbsent:
          !document.querySelector('.halo-workbench, #halo-workbench, [data-testid="halo-workbench"]')
          && !/HALO WORKBENCH|halo-workbench/i.test(document.body.innerText || ''),
        scriptSourcesContainHaloWorkbench: scriptSources.some((src) => /halo-workbench/i.test(src)),
        visibleBrandLeaks: {
          haloText: /欢迎使用 Halo|Halo Workspace|Starting Halo|正在启动 Halo|Halo Agent Companion/.test(visibleText),
          rawI18nKeys: /features\\.files|nav\\.sections\\.sessions/.test(visibleText),
          updateDialog: /发现新版本|下载并安装|跳过此版本/.test(visibleText),
        },
        workspaceVisible: document.body.innerText.includes('${path.basename(smokeWorkspace)}'),
        navEntries: {
          workspace: !!document.querySelector('[data-testid="nav-workspace-add-btn"]'),
          codeSession: !!document.querySelector('[data-testid="nav-new-code-session-btn"]'),
          fileViewer: !!document.querySelector('[data-testid="nav-file-viewer-btn"]'),
          git: !!document.querySelector('[data-testid="nav-git-btn"]'),
          terminal: !!document.querySelector('[data-testid="shell-panel-entry"]'),
        },
      };
    })()
  `;
}

async function clickAndProbe(cdpClient, selector, probeExpression) {
  return evalValue(cdpClient, `
    (async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      const activeSceneId = () => document
        .querySelector('[data-testid="scene-viewport-scene"][data-scene-active="true"]')
        ?.getAttribute('data-scene-id') || null;
      const target = document.querySelector(${JSON.stringify(selector)});
      if (!target) return { clicked: false, selector: ${JSON.stringify(selector)} };
      target.scrollIntoView({ block: 'center', inline: 'center' });
      target.click();
      await sleep(250);
      const probe = await (${probeExpression});
      return { clicked: true, selector: ${JSON.stringify(selector)}, ...probe };
    })()
  `);
}

function pickDomBooleans(dom) {
  return {
    activeScene: dom.activeScene,
    navEntries: dom.navEntries,
    haloSelectorsPresent: dom.haloSelectorsPresent,
    oldHaloWorkbenchAbsent: dom.oldHaloWorkbenchAbsent,
    visibleBrandLeaks: dom.visibleBrandLeaks,
  };
}

function redactCapture(capture) {
  return {
    pid: capture.pid,
    hwnd: capture.hwnd,
    title: capture.title,
    rect: capture.rect,
    visible: capture.visible,
    nonEmpty: capture.nonEmpty,
    sampleVariance: capture.sampleVariance,
    screenshotPath: redactPath(capture.screenshotPath),
  };
}

function redactPath(value) {
  if (!value) return value;
  let text = String(value);
  const home = os.homedir();
  const tmp = os.tmpdir();
  for (const [prefix, label] of [
    [productRoot, '[product/Halo Studio]'],
    [repoRoot, '[repo]'],
    [home, '[home]'],
    [tmp, '[temp]'],
  ]) {
    text = text.replaceAll(prefix, label);
  }
  return text.replaceAll('\\', '/');
}

function listLogFiles(root) {
  if (!fs.existsSync(root)) return [];
  const results = [];
  const stack = [root];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const absolute = path.join(current, entry.name);
      if (entry.isDirectory()) {
        stack.push(absolute);
      } else {
        results.push(absolute);
      }
    }
  }
  return results.sort();
}

function redactText(value) {
  if (!value) return value;
  return redactPath(String(value));
}

async function getFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      const chosen = address.port;
      server.close(() => resolve(chosen));
    });
  });
}

async function waitForCdpTarget(cdpPort, timeoutMs) {
  const started = Date.now();
  let lastTargets = [];
  let lastError = null;
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(`http://127.0.0.1:${cdpPort}/json/list`);
      const targets = await response.json();
      lastTargets = targets.map(({ id, type, title, url, webSocketDebuggerUrl }) => ({
        id,
        type,
        title,
        url,
        hasWebSocketDebuggerUrl: Boolean(webSocketDebuggerUrl),
      }));
      const page = targets.find((target) =>
        target.type === 'page'
          && target.webSocketDebuggerUrl
          && typeof target.url === 'string'
          && target.url.length > 0
          && target.url !== 'about:blank'
      );
      if (page) {
        cdpTargetDiagnostics = {
          selected: {
            id: page.id,
            type: page.type,
            title: page.title,
            url: page.url,
          },
          targets: lastTargets,
        };
        return page;
      }
    } catch (error) {
      lastError = String(error?.message || error);
    }
    await sleep(300);
  }
  cdpTargetDiagnostics = { targets: lastTargets, lastError };
  throw new Error(
    `Timed out waiting for Tauri WebView CDP target: ${JSON.stringify(cdpTargetDiagnostics)}`,
  );
}

async function connectCdp(webSocketDebuggerUrl) {
  const ws = new WebSocket(webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Timed out opening CDP websocket')), 15000);
    ws.addEventListener('open', () => {
      clearTimeout(timer);
      resolve();
    }, { once: true });
    ws.addEventListener('error', (event) => {
      clearTimeout(timer);
      reject(new Error(`CDP websocket error: ${event.message || 'unknown'}`));
    }, { once: true });
  });

  let id = 0;
  const pending = new Map();
  const rejectPending = (error) => {
    for (const { reject } of pending.values()) reject(error);
    pending.clear();
  };
  ws.addEventListener('message', (event) => {
    const message = JSON.parse(event.data);
    if (!message.id || !pending.has(message.id)) return;
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) reject(new Error(JSON.stringify(message.error)));
    else resolve(message.result ?? {});
  });
  ws.addEventListener('error', () => rejectPending(new Error('CDP websocket error')));
  ws.addEventListener('close', () => rejectPending(new Error('CDP websocket closed')));

  return {
    send(method, params = {}) {
      const messageId = ++id;
      const response = new Promise((resolve, reject) => {
        pending.set(messageId, { resolve, reject });
        setTimeout(() => {
          if (!pending.has(messageId)) return;
          pending.delete(messageId);
          reject(new Error(`Timed out waiting for CDP method ${method}`));
        }, 30000);
      });
      try {
        ws.send(JSON.stringify({ id: messageId, method, params }));
      } catch (error) {
        pending.delete(messageId);
        throw error;
      }
      return response;
    },
    close() {
      ws.close();
    },
  };
}

async function waitForEval(cdpClient, expression, timeoutMs) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const result = await evalValue(cdpClient, expression).catch(() => false);
    if (result) return result;
    await sleep(300);
  }
  throw new Error(`Timed out waiting for expression: ${expression}`);
}

async function evalValue(cdpClient, expression) {
  const result = await cdpClient.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(`Runtime.evaluate failed: ${JSON.stringify(result.exceptionDetails)}`);
  }
  return result.result?.value;
}

function captureWindow(pid, screenshotPath, width, height) {
  const ps = `
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class HaloSmokeWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

  [StructLayout(LayoutKind.Sequential)]
  public struct RECT {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc enumProc, IntPtr lParam);

  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out int processId);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);

  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);

  [DllImport("user32.dll")]
  public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int width, int height, bool repaint);

  [DllImport("user32.dll")]
  public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

  public static IntPtr FindWindowForPid(int pid) {
    IntPtr found = IntPtr.Zero;
    long bestScore = long.MinValue;
    EnumWindows(delegate (IntPtr hWnd, IntPtr lParam) {
      int windowPid;
      GetWindowThreadProcessId(hWnd, out windowPid);
      if (windowPid == pid && IsWindowVisible(hWnd)) {
        RECT rect;
        GetWindowRect(hWnd, out rect);
        var title = GetTitle(hWnd);
        long width = Math.Max(0, rect.Right - rect.Left);
        long height = Math.Max(0, rect.Bottom - rect.Top);
        long area = width * height;
        long score = area;
        if (title.IndexOf("Halo Studio", StringComparison.OrdinalIgnoreCase) >= 0) {
          score += 1000000000000;
        }
        if (title.IndexOf("Agent Companion", StringComparison.OrdinalIgnoreCase) >= 0) {
          score -= 1000000000000;
        }
        if (score > bestScore) {
          bestScore = score;
          found = hWnd;
        }
      }
      return true;
    }, IntPtr.Zero);
    return found;
  }

  public static string GetTitle(IntPtr hWnd) {
    var builder = new StringBuilder(512);
    GetWindowText(hWnd, builder, builder.Capacity);
    return builder.ToString();
  }
}
"@
$targetPid = ${pid}
$screenshotPath = '${psSingle(screenshotPath)}'
$targetWidth = ${width}
$targetHeight = ${height}
$hwnd = [HaloSmokeWin32]::FindWindowForPid($targetPid)
if ($hwnd -eq [IntPtr]::Zero) {
  throw "No visible window found for PID $targetPid"
}
[HaloSmokeWin32]::ShowWindow($hwnd, 9) | Out-Null
[HaloSmokeWin32]::MoveWindow($hwnd, 80, 80, $targetWidth, $targetHeight, $true) | Out-Null
[HaloSmokeWin32]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep -Milliseconds 700
$rect = New-Object HaloSmokeWin32+RECT
[HaloSmokeWin32]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
$w = [Math]::Max(1, $rect.Right - $rect.Left)
$h = [Math]::Max(1, $rect.Bottom - $rect.Top)
$bitmap = New-Object Drawing.Bitmap $w, $h
$graphics = [Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
$colors = New-Object System.Collections.Generic.HashSet[string]
for ($x = 0; $x -lt $w; $x += [Math]::Max(1, [Math]::Floor($w / 12))) {
  for ($y = 0; $y -lt $h; $y += [Math]::Max(1, [Math]::Floor($h / 12))) {
    $pixel = $bitmap.GetPixel($x, $y)
    [void]$colors.Add("$($pixel.R),$($pixel.G),$($pixel.B)")
  }
}
$dir = Split-Path -Parent $screenshotPath
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$bitmap.Save($screenshotPath, [Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
$processIdFromWindow = 0
[HaloSmokeWin32]::GetWindowThreadProcessId($hwnd, [ref]$processIdFromWindow) | Out-Null
[PSCustomObject]@{
  pid = $processIdFromWindow
  hwnd = ('0x{0:X}' -f $hwnd.ToInt64())
  title = [HaloSmokeWin32]::GetTitle($hwnd)
  rect = [PSCustomObject]@{
    left = $rect.Left
    top = $rect.Top
    right = $rect.Right
    bottom = $rect.Bottom
    width = $w
    height = $h
  }
  visible = $true
  nonEmpty = ($colors.Count -gt 4)
  sampleVariance = $colors.Count
  screenshotPath = $screenshotPath
} | ConvertTo-Json -Depth 5
`;
  const encoded = Buffer.from(ps, 'utf16le').toString('base64');
  const output = execFileSync('powershell.exe', [
    '-NoProfile',
    '-ExecutionPolicy',
    'Bypass',
    '-EncodedCommand',
    encoded,
  ], {
    cwd: productRoot,
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  });
  return JSON.parse(output);
}

function psSingle(value) {
  return String(value).replaceAll("'", "''");
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
