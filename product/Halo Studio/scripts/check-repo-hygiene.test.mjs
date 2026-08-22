import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

const scriptPath = fileURLToPath(new URL('./check-repo-hygiene.mjs', import.meta.url));

test('fails closed when Git repository state cannot be read', (t) => {
  const directory = mkdtempSync(join(tmpdir(), 'halo-repo-hygiene-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));

  const result = spawnSync(process.execPath, [scriptPath], {
    cwd: directory,
    encoding: 'utf8',
  });

  assert.notEqual(result.status, 0, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  assert.match(result.stderr, /Repository hygiene check failed: git ls-files failed:/u);
});

test('allows certificate fixture names only inside vendored dependencies', (t) => {
  const directory = mkdtempSync(join(tmpdir(), 'halo-repo-hygiene-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));

  const vendoredFixture = join(directory, 'vendor', 'cargo', 'fixture', 'tests', 'examples');
  mkdirSync(vendoredFixture, { recursive: true });
  writeFileSync(join(vendoredFixture, 'id_rsa.pem'), 'fixture\n', 'utf8');
  writeFileSync(join(directory, 'src-private.pem'), 'fixture\n', 'utf8');

  for (const args of [
    ['init', '-q'],
    ['add', '--', '.'],
    ['-c', 'user.name=Halo Test', '-c', 'user.email=halo-test@example.invalid', 'commit', '-qm', 'fixture'],
  ]) {
    const git = spawnSync('git', args, { cwd: directory, encoding: 'utf8' });
    assert.equal(git.status, 0, `git ${args.join(' ')} failed:\n${git.stderr}`);
  }

  writeFileSync(join(vendoredFixture, 'id_rsa-untracked.pem'), 'fixture\n', 'utf8');

  const result = spawnSync(process.execPath, [scriptPath], {
    cwd: directory,
    encoding: 'utf8',
  });

  assert.equal(result.status, 1, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  assert.match(result.stderr, /src-private\.pem looks like a private key/u);
  assert.match(
    result.stderr,
    /vendor[\\/]cargo[\\/]fixture[\\/]tests[\\/]examples[\\/]id_rsa-untracked\.pem looks like a private key/u,
  );
  assert.doesNotMatch(result.stderr, /vendor[\\/]cargo[\\/]fixture[\\/]tests[\\/]examples[\\/]id_rsa\.pem/u);
});
