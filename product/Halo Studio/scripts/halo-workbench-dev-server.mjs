import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const FRONTEND_ROOT = join(ROOT, 'src', 'halo-workbench');
const ICON_PATH = join(ROOT, 'src', 'apps', 'halo-desktop', 'icons', 'halo-icon.svg');
const HOST = process.env.TAURI_DEV_HOST || '127.0.0.1';
const PORT = Number(process.env.HALO_TAURI_DEV_PORT || 1432);
const MIME_TYPES = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.svg': 'image/svg+xml',
};

function safeFile(urlPath) {
  const decoded = decodeURIComponent(urlPath.split('?')[0]);
  const requested = decoded === '/' ? '/index.html' : decoded;
  const candidate = resolve(FRONTEND_ROOT, `.${requested}`);
  return candidate === FRONTEND_ROOT || candidate.startsWith(FRONTEND_ROOT + sep) ? candidate : null;
}

const server = createServer((request, response) => {
  try {
    const file = request.url?.split('?')[0] === '/halo-icon.svg' ? ICON_PATH : safeFile(request.url || '/');
    if (!file || !existsSync(file) || !statSync(file).isFile()) {
      response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
      response.end('Not found');
      return;
    }
    response.writeHead(200, {
      'cache-control': 'no-store',
      'content-type': MIME_TYPES[extname(file)] || 'application/octet-stream',
    });
    createReadStream(file).pipe(response);
  } catch (error) {
    response.writeHead(500, { 'content-type': 'text/plain; charset=utf-8' });
    response.end(error.message || String(error));
  }
});

server.listen(PORT, HOST, () => {
  console.log(`[halo-workbench] serving ${relative(ROOT, FRONTEND_ROOT)} at http://${HOST}:${PORT}`);
});

function shutdown() {
  server.close(() => process.exit(0));
}
process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);
