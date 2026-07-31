import { copyFileSync, existsSync, mkdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { verifyHaloScope } from './halo-scope.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const WEB_UI_ROOT = join(ROOT, 'src', 'web-ui');
const WEB_UI_PUBLIC = join(WEB_UI_ROOT, 'public');
const HALO_ICON_SOURCE = join(ROOT, 'src', 'apps', 'halo-desktop', 'icons', 'halo-icon.svg');
const HALO_ICON_TARGET = join(WEB_UI_PUBLIC, 'halo-icon.svg');

function fail(message, code = 1) {
  console.error(`[halo-web-ui] ${message}`);
  process.exit(code);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: ROOT,
    env: {
      ...process.env,
      HALO_PRODUCT_SCOPE: 'local-coding',
      HALO_PRODUCT_NAME: 'Halo Studio',
      NODE_ENV: options.nodeEnv || process.env.NODE_ENV || 'production',
    },
    stdio: 'inherit',
    shell: process.platform === 'win32',
  });
  if (result.error) fail(result.error.message);
  if (result.status !== 0) {
    fail(`${command} ${args.join(' ')} exited with code ${result.status ?? 'null'}`, result.status ?? 1);
  }
}

verifyHaloScope();

if (!existsSync(join(WEB_UI_ROOT, 'index.html')) || !existsSync(join(WEB_UI_ROOT, 'src', 'main.tsx'))) {
  fail('Halo Studio Web UI entry is missing under src/web-ui');
}
if (!existsSync(HALO_ICON_SOURCE)) {
  fail('Halo desktop icon source is missing');
}

mkdirSync(WEB_UI_PUBLIC, { recursive: true });
copyFileSync(HALO_ICON_SOURCE, HALO_ICON_TARGET);

const pnpm = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
run(pnpm, ['run', 'copy-monaco']);
run(pnpm, ['run', 'generate-version'], { nodeEnv: 'production' });
run(pnpm, ['--dir', 'src/web-ui', 'run', 'build:desktop'], { nodeEnv: 'production' });

console.log(`[halo-web-ui] built ${join(ROOT, 'dist')}`);
