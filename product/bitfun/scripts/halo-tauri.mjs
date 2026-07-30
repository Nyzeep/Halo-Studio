import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyHaloScope } from './halo-scope.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const DESKTOP_ROOT = join(ROOT, 'src', 'apps', 'halo-desktop');

function fail(message, code = 1) {
  console.error(`[halo-tauri] ${message}`);
  process.exit(code);
}

const mode = process.argv[2];
if (!['dev', 'build'].includes(mode)) {
  fail('usage: node scripts/halo-tauri.mjs <dev|build> [tauri args]', 2);
}

let scope;
try {
  scope = verifyHaloScope();
} catch (error) {
  fail(error.message || String(error));
}

const forwarded = process.argv.slice(3).filter(argument => argument !== '--');
if (forwarded.some(argument => argument === '--config' || argument.startsWith('--config=') || argument === '-c' || argument.startsWith('-c='))) {
  fail('Tauri configuration overrides are disabled for the Halo product entry');
}
const tauriArgs = [];
const cargoArgs = [];
for (let index = 0; index < forwarded.length; index += 1) {
  const argument = forwarded[index];
  if (argument === '--profile' || argument.startsWith('--profile=')) {
    const profile = argument.startsWith('--profile=') ? argument.slice('--profile='.length) : forwarded[++index];
    if (!profile || profile.startsWith('-')) fail('--profile requires a Cargo profile name');
    cargoArgs.push('--profile', profile);
    continue;
  }
  tauriArgs.push(argument);
}
const tauriBinaryName = process.platform === 'win32' ? 'tauri.cmd' : 'tauri';
const tauriBinary = join(ROOT, 'node_modules', '.bin', tauriBinaryName);
if (!existsSync(tauriBinary)) {
  fail(`Tauri CLI is missing at ${tauriBinary}. Run pnpm install in product/bitfun first.`, 2);
}

console.log(`[halo-tauri] ${mode} ${scope.frontendRoot} -> ${scope.desktopRoot}`);
const command = process.platform === 'win32' ? `"${tauriBinary}"` : tauriBinary;
const devtoolsArgs = mode === 'dev' && !tauriArgs.some(argument => argument === '--features' || argument.startsWith('--features='))
  ? ['--features', 'devtools']
  : [];
const result = spawnSync(command, [mode, '--config', 'tauri.conf.json', ...devtoolsArgs, ...tauriArgs, ...(cargoArgs.length > 0 ? ['--', ...cargoArgs] : [])], {
  cwd: DESKTOP_ROOT,
  env: {
    ...process.env,
    HALO_PRODUCT_SCOPE: 'local-coding',
  },
  stdio: 'inherit',
  shell: process.platform === 'win32',
});

if (result.error) {
  fail(result.error.message);
}
process.exit(result.status ?? 1);
