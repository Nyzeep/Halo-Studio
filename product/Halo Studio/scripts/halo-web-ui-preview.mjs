import { existsSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { verifyHaloScope } from './halo-scope.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const DEV_SERVER = join(ROOT, 'scripts', 'halo-web-ui-dev-server.mjs');
const BINARY_NAME = process.platform === 'win32' ? 'halo-studio.exe' : 'halo-studio';
const BINARY_PATH = join(ROOT, 'target', 'debug', BINARY_NAME);
const DEV_URL = 'http://localhost:1422';

function fail(message, code = 1) {
  console.error(`[halo-preview] ${message}`);
  process.exit(code);
}

function usage() {
  console.log('usage: node scripts/halo-web-ui-preview.mjs [--force-rebuild]');
}

function stop(child) {
  if (child && child.exitCode === null) child.kill();
}

function run(command, args, options) {
  return new Promise((resolveRun, rejectRun) => {
    const child = spawn(command, args, options);
    child.once('error', rejectRun);
    child.once('exit', code => {
      if (code === 0) {
        resolveRun();
      } else {
        rejectRun(new Error(`${command} exited with code ${code ?? 'null'}`));
      }
    });
  });
}

async function waitForServer(server) {
  const startedAt = Date.now();
  let lastError = '';
  while (Date.now() - startedAt < 30_000) {
    if (server.exitCode !== null) {
      throw new Error(`Halo Web UI dev server exited before readiness (code=${server.exitCode})`);
    }
    try {
      const response = await fetch(DEV_URL);
      if (response.ok) return;
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error.message || String(error);
    }
    await new Promise(resolveWait => setTimeout(resolveWait, 250));
  }
  throw new Error(`Halo Web UI dev server did not become ready at ${DEV_URL}: ${lastError}`);
}

async function pruneTarget() {
  try {
    const { runGcBestEffort } = await import('./cargo-target-gc.mjs');
    runGcBestEffort({
      rootDir: ROOT,
      profile: 'debug',
      logger: {
        info: message => console.log(`[halo-preview] ${message}`),
        warn: message => console.warn(`[halo-preview] ${message}`),
      },
    });
  } catch (error) {
    console.warn(`[halo-preview] target cleanup skipped: ${error.message || String(error)}`);
  }
}

const args = new Set(process.argv.slice(2));
if (args.delete('--help') || args.delete('-h')) {
  usage();
  process.exit(0);
}
const forceRebuild = args.delete('--force-rebuild');
if (args.size > 0) fail(`unsupported preview arguments: ${[...args].join(', ')}`, 2);

try {
  verifyHaloScope();
} catch (error) {
  fail(error.message || String(error));
}

if (forceRebuild) {
  console.log('[halo-preview] rebuilding the Halo debug binary');
  try {
    await run(process.platform === 'win32' ? 'cargo.exe' : 'cargo', ['build', '--locked', '-p', 'halo-tauri-desktop'], {
      cwd: ROOT,
      stdio: 'inherit',
    });
  } catch (error) {
    fail(error.message || String(error));
  }
}

if (!existsSync(BINARY_PATH)) {
  fail(`debug binary is missing: ${BINARY_PATH}. Run cargo build --locked -p halo-tauri-desktop or retry with --force-rebuild.`);
}

const server = spawn(process.execPath, [DEV_SERVER], {
  cwd: ROOT,
  env: {
    ...process.env,
    HALO_PRODUCT_SCOPE: 'local-coding',
    HALO_PRODUCT_NAME: 'Halo Studio',
    TAURI_DEV_HOST: 'localhost',
  },
  stdio: ['ignore', 'inherit', 'inherit'],
});

let app;
let shuttingDown = false;
async function shutdown(code = 0) {
  if (shuttingDown) return;
  shuttingDown = true;
  stop(app);
  stop(server);
  await pruneTarget();
  process.exit(code);
}

process.once('SIGINT', () => void shutdown(0));
process.once('SIGTERM', () => void shutdown(0));

try {
  await waitForServer(server);
  console.log(`[halo-preview] launching ${BINARY_PATH}`);
  app = spawn(BINARY_PATH, [], {
    cwd: ROOT,
    env: process.env,
    stdio: 'inherit',
  });
  app.once('error', error => {
    console.error(`[halo-preview] native app failed to start: ${error.message || String(error)}`);
    void shutdown(1);
  });
  app.once('exit', code => void shutdown(code ?? 0));
  console.log('[halo-preview] frontend edits use the Halo Web UI dev server; Rust changes require --force-rebuild or a manual cargo build');
} catch (error) {
  console.error(`[halo-preview] ${error.message || String(error)}`);
  await shutdown(1);
}
