import { existsSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyHaloScope } from './halo-scope.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const DEV_SERVER = join(ROOT, 'scripts', 'halo-workbench-dev-server.mjs');
const BINARY_NAME = process.platform === 'win32' ? 'halo-studio.exe' : 'halo-studio';
const BINARY_PATH = join(ROOT, 'target', 'debug', BINARY_NAME);
const PORT = Number(process.env.HALO_TAURI_DEV_PORT || 1432);

function fail(message, code = 1) {
  console.error(`[halo-preview] ${message}`);
  process.exit(code);
}

function usage() {
  console.log('usage: node scripts/halo-workbench-preview.mjs [--force-rebuild]');
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

function waitForServer(server) {
  return new Promise((resolveReady, rejectReady) => {
    let stderr = '';
    const timeout = setTimeout(() => {
      rejectReady(new Error(`Halo dev server did not become ready on http://127.0.0.1:${PORT}`));
    }, 10_000);

    server.stdout.setEncoding('utf8');
    server.stdout.on('data', output => {
      process.stdout.write(output);
      if (output.includes(`[halo-workbench] serving`) && output.includes(`http://127.0.0.1:${PORT}`)) {
        clearTimeout(timeout);
        resolveReady();
      }
    });
    server.stderr.setEncoding('utf8');
    server.stderr.on('data', output => {
      stderr += output;
      process.stderr.write(output);
    });
    server.once('error', error => {
      clearTimeout(timeout);
      rejectReady(error);
    });
    server.once('exit', code => {
      clearTimeout(timeout);
      rejectReady(new Error(`Halo dev server exited before readiness (code=${code ?? 'null'}): ${stderr.trim()}`));
    });
  });
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
    HALO_TAURI_DEV_PORT: String(PORT),
    TAURI_DEV_HOST: '127.0.0.1',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
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
  console.log('[halo-preview] frontend edits use the Halo dev server; Rust changes require --force-rebuild or a manual cargo build');
} catch (error) {
  console.error(`[halo-preview] ${error.message || String(error)}`);
  await shutdown(1);
}
