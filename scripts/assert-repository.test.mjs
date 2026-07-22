import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);

function runGit(args, env) {
  const result = spawnSync("git", args, {
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

test("detects forbidden root paths when launched from the scripts directory", (t) => {
  const temporaryDirectory = mkdtempSync(join(tmpdir(), "halo-repository-check-"));
  t.after(() => rmSync(temporaryDirectory, { recursive: true, force: true }));

  const env = {
    ...process.env,
    GIT_INDEX_FILE: join(temporaryDirectory, "index"),
  };

  runGit(["read-tree", "HEAD"], env);
  const blob = runGit(["rev-parse", "HEAD:scripts/assert-repository.mjs"], env);
  runGit(
    ["update-index", "--add", "--cacheinfo", `100644,${blob},crates/forbidden-probe`],
    env,
  );

  const result = spawnSync(process.execPath, ["assert-repository.mjs"], {
    cwd: scriptsDirectory,
    encoding: "utf8",
    env,
  });

  assert.equal(result.status, 1, `stdout:\n${result.stdout}\nstderr:\n${result.stderr}`);
  assert.match(result.stderr, /crates\/forbidden-probe/);
});
