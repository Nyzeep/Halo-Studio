import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { request } from 'node:http';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const serverScript = resolve(ROOT, 'scripts', 'halo-workbench-dev-server.mjs');
const port = Number(process.env.HALO_TAURI_DEV_PORT || 1432);
const child = spawn(process.execPath, [serverScript], {
  cwd: ROOT,
  env: { ...process.env, HALO_TAURI_DEV_PORT: String(port), TAURI_DEV_HOST: '127.0.0.1' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let childError;
let childExit;
let serverReady = false;
let childStderr = '';
child.stdout.setEncoding('utf8');
child.stdout.on('data', output => {
  serverReady ||= output.includes(`[halo-workbench] serving`) && output.includes(`http://127.0.0.1:${port}`);
});
child.stderr.setEncoding('utf8');
child.stderr.on('data', output => { childStderr += output; });
child.once('error', error => { childError = error; });
child.once('exit', (code, signal) => { childExit = { code, signal }; });

function stop() {
  if (!child.killed) child.kill();
}

function fetchPage(path = '/') {
  return new Promise((resolveRequest, reject) => {
    const req = request({ host: '127.0.0.1', port, path, method: 'GET' }, response => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', chunk => { body += chunk; });
      response.on('end', () => resolveRequest({ statusCode: response.statusCode, body }));
    });
    req.on('error', reject);
    req.end();
  });
}

try {
  const startedAt = Date.now();
  let page;
  while (Date.now() - startedAt < 10_000) {
    if (childError) throw childError;
    if (childExit) {
      throw new Error(`Halo dev server exited before readiness (code=${childExit.code}, signal=${childExit.signal}): ${childStderr.trim()}`);
    }
    if (!serverReady) {
      await new Promise(resolveWait => setTimeout(resolveWait, 100));
      continue;
    }
    try {
      page = await fetchPage();
      break;
    } catch {
      await new Promise(resolveWait => setTimeout(resolveWait, 100));
    }
  }
  assert.ok(page, 'Halo dev server did not become ready');
  assert.equal(childExit, undefined, 'Halo dev server exited after readiness');
  assert.equal(page.statusCode, 200);
  assert.match(page.body, /data-halo-scope="local-coding"/);
  assert.match(page.body, /Halo Studio/);
  const app = await fetchPage('/app.js');
  assert.equal(app.statusCode, 200);
  for (const marker of ['class="shell"', 'class="sidebar sidebar--left"', 'class="main-panel"', 'class="sidebar sidebar--right"', '工作区', '编码会话', '版本控制', '终端']) {
    assert.match(app.body, new RegExp(marker));
  }
  console.log(JSON.stringify({ ok: true, smoke: 'halo-workbench-http', tauriWindow: false, statusCode: page.statusCode, scope: 'local-coding' }));
} finally {
  stop();
}
