import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyHaloScope } from './halo-scope.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const SOURCE = join(ROOT, 'src', 'halo-workbench');
const DIST = join(SOURCE, 'dist');

verifyHaloScope();
if (!existsSync(SOURCE)) {
  throw new Error('HALO_BUILD_INVALID: Halo frontend source is missing');
}

rmSync(DIST, { force: true, recursive: true });
mkdirSync(DIST, { recursive: true });
for (const file of ['index.html', 'app.js', 'styles.css']) {
  cpSync(join(SOURCE, file), join(DIST, file));
}
cpSync(join(ROOT, 'src', 'apps', 'halo-desktop', 'icons', 'halo-icon.svg'), join(DIST, 'halo-icon.svg'));
console.log(`[halo-workbench] built ${DIST}`);
