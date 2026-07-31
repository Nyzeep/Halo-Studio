import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const WEB_UI_ROOT = join(ROOT, 'src', 'web-ui');

function fail(message, code = 1) {
  console.error(`[halo-web-ui] ${message}`);
  process.exit(code);
}

if (!existsSync(join(WEB_UI_ROOT, 'index.html')) || !existsSync(join(WEB_UI_ROOT, 'src', 'main.tsx'))) {
  fail('Halo Studio Web UI entry is missing under src/web-ui');
}

const pnpm = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const child = spawn(pnpm, ['--dir', 'src/web-ui', 'run', 'dev'], {
  cwd: ROOT,
  env: {
    ...process.env,
    HALO_PRODUCT_SCOPE: 'local-coding',
    HALO_PRODUCT_NAME: 'Halo Studio',
    TAURI_DEV_HOST: process.env.TAURI_DEV_HOST || 'localhost',
  },
  stdio: 'inherit',
  shell: process.platform === 'win32',
});

child.once('error', error => fail(error.message));
child.once('exit', code => process.exit(code ?? 1));
