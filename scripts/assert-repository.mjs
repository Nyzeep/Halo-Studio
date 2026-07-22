import { spawnSync } from "node:child_process";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);

const forbiddenPrefixes = [
  "用于参考的几个项目的代码/",
  "crates/",
  "plugins/agents/",
  "apps/desktop/halo_desktop/",
];

const result = spawnSync("git", ["ls-files", "-z"], {
  cwd: repositoryRoot,
  encoding: "utf8",
});

if (result.error || result.status !== 0) {
  console.error("Repository check failed: unable to list tracked files.");
  if (result.error) {
    console.error(result.error.message);
  }
  if (result.stderr) {
    console.error(result.stderr.trim());
  }
  process.exit(1);
}

const trackedFiles = result.stdout.split("\0").filter(Boolean);
const violations = trackedFiles.filter((file) =>
  forbiddenPrefixes.some((prefix) => file.startsWith(prefix)),
);

if (violations.length > 0) {
  console.error("Repository check failed: forbidden tracked files found:");
  for (const file of violations) {
    console.error(`- ${file}`);
  }
  process.exit(1);
}

console.log("Repository check passed.");
