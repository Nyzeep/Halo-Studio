import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);

function runGit(args, env) {
  const result = spawnSync("git", ["-c", `safe.directory=${repositoryRoot}`, ...args], {
    cwd: repositoryRoot,
    encoding: "utf8",
    env,
  });

  assert.equal(
    result.status,
    0,
    `git ${args.join(" ")} failed:\n${result.stderr}`,
  );
  return result.stdout.trim();
}

function createTemporaryIndex(t) {
  const temporaryDirectory = mkdtempSync(join(tmpdir(), "halo-repository-check-"));
  t.after(() => rmSync(temporaryDirectory, { recursive: true, force: true }));
  const env = {
    ...process.env,
    GIT_INDEX_FILE: join(temporaryDirectory, "index"),
  };
  runGit(["read-tree", "HEAD"], env);
  return env;
}

function runRepositoryCheck(env) {
  return spawnSync(process.execPath, ["assert-repository.mjs"], {
    cwd: scriptsDirectory,
    encoding: "utf8",
    env,
  });
}

test("detects forbidden paths case-insensitively in staged content", (t) => {
  const env = createTemporaryIndex(t);
  const blob = runGit(["rev-parse", "HEAD:scripts/assert-repository.mjs"], env);
  runGit(
    ["update-index", "--add", "--cacheinfo", `100644,${blob},CrAtEs/forbidden-probe.mjs`],
    env,
  );
  runGit(
    ["update-index", "--add", "--cacheinfo", `100644,${blob},用于参考的几个项目的代码/probe.mjs`],
    env,
  );

  const result = runRepositoryCheck(env);
  assert.equal(result.status, 1, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  assert.match(result.stderr, /CrAtEs\/forbidden-probe\.mjs/i);
  assert.match(result.stderr, /用于参考的几个项目的代码\/probe\.mjs/u);
});

test("detects legacy integration markers in staged runtime source", (t) => {
  const env = createTemporaryIndex(t);
  const blob = runGit(["rev-parse", "HEAD:packages/contracts/src/contracts.test.ts"], env);
  runGit(
    ["update-index", "--add", "--cacheinfo", `100644,${blob},packages/core/src/deprecated-runtime-probe.ts`],
    env,
  );

  const result = runRepositoryCheck(env);
  assert.equal(result.status, 1, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  assert.match(result.stderr, /staged content packages\/core\/src\/deprecated-runtime-probe\.ts/u);
  assert.match(result.stderr, /legacy Claude integration|legacy Codex integration/u);
  assert.match(result.stderr, /legacy web fallback runtime/u);
});

test("detects an unstaged runtime file without scanning legacy documentation", (t) => {
  const runtimeProbe = join(scriptsDirectory, "repository-check-working-tree-probe.mjs");
  const documentationProbe = join(repositoryRoot, "docs", "repository-check-legacy-trace-probe.md");
  writeFileSync(runtimeProbe, "export const legacy = 'cLaUdE_Code';\n", "utf8");
  writeFileSync(documentationProbe, "Historical Codex and MCP trace notes are permitted here.\n", "utf8");
  t.after(() => {
    rmSync(runtimeProbe, { force: true });
    rmSync(documentationProbe, { force: true });
  });

  const result = runRepositoryCheck(process.env);
  assert.equal(result.status, 1, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  assert.match(result.stderr, /working tree scripts\/repository-check-working-tree-probe\.mjs/u);
  assert.match(result.stderr, /legacy Claude integration/u);
  assert.doesNotMatch(result.stderr, /repository-check-legacy-trace-probe/u);
});

test("detects hardcoded local paths in the working tree", (t) => {
  const runtimeProbe = join(scriptsDirectory, "repository-check-local-path-probe.mjs");
  writeFileSync(runtimeProbe, "export const workspace = 'd:\\\\Halo Studio';\n", "utf8");
  t.after(() => rmSync(runtimeProbe, { force: true }));

  const result = runRepositoryCheck(process.env);
  assert.equal(result.status, 1, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  assert.match(result.stderr, /working tree scripts\/repository-check-local-path-probe\.mjs/u);
  assert.match(result.stderr, /hardcoded Windows local path/u);
});
