import { spawnSync } from "node:child_process";
import { existsSync, lstatSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = dirname(scriptsDirectory);

const forbiddenPathPatterns = [
  {
    label: "reference source directory",
    pattern: /(?:^|\/)用于参考的几个项目的代码(?:\/|$)/iu,
  },
  { label: "legacy Rust workspace", pattern: /^crates(?:\/|$)/iu },
  { label: "legacy plugin agent directory", pattern: /^plugins\/agents(?:\/|$)/iu },
  {
    label: "legacy Python desktop runtime",
    pattern: /^apps\/desktop\/halo_desktop(?:\/|$)/iu,
  },
  {
    label: "legacy Claude or Codex agent directory",
    pattern: /(?:^|\/)(?:agent[-_](?:claude(?:[-_]code)?|codex(?:[-_]cli)?)|(?:claude(?:[-_]code)?|codex(?:[-_]cli)?)[-_]agent)(?:\/|$)/iu,
  },
  {
    label: "legacy web fallback entry point",
    pattern: /^(?:server|web[-_]?server|backend)\.(?:[cm]?[jt]s|tsx)$/iu,
  },
];

const forbiddenRuntimeMarkers = [
  {
    label: "legacy Claude integration",
    pattern: /\bclaude(?:[\s_-]*code)?\b/iu,
  },
  {
    label: "legacy Codex integration",
    pattern: /\bcodex(?:[\s_-]*(?:cli|agent))?\b/iu,
  },
  {
    label: "legacy MCP configuration",
    pattern: /(?:\bmcp\.json\b|\bmcp[\s_-]?(?:config|server|servers|client|transport)\b|@modelcontextprotocol)/iu,
  },
  {
    label: "mock PTY runtime",
    pattern: /\b(?:mock[\s_-]*pty|node[\s_-]*pty)\b/iu,
  },
  {
    label: "legacy web fallback runtime",
    pattern: /\b(?:express|web[\s_-]*socket|socket\.io|http-server)\b/iu,
  },
  {
    label: "hardcoded Windows local path",
    pattern: /(?:^|[^a-z0-9_])(?:[a-z]:[\\/])/iu,
  },
  {
    label: "hardcoded POSIX local path",
    pattern: /(?:^|["'`(=,:\s])\/(?:Users|home|mnt\/c\/Users)\/[^/\s"'`]+/u,
  },
  {
    label: "hardcoded UNC local path",
    pattern: /(?:^|["'`(=,:\s])\\\\[^\\\s]+\\[^\\\s]+/u,
  },
];

function git(args, options = {}) {
  return spawnSync("git", ["-c", `safe.directory=${repositoryRoot}`, ...args], {
    cwd: repositoryRoot,
    encoding: "utf8",
    maxBuffer: 128 * 1024 * 1024,
    ...options,
  });
}

function listGitPaths(args) {
  const result = git(args);
  if (result.error || result.status !== 0) {
    console.error("Repository check failed: unable to list repository files.");
    if (result.error) console.error(result.error.message);
    if (result.stderr) console.error(result.stderr.trim());
    process.exit(1);
  }
  return result.stdout.split("\0").filter(Boolean);
}

function normalizeRepositoryPath(file) {
  return file.replaceAll("\\", "/").replace(/^\.\//u, "");
}

function findForbiddenPath(file) {
  const normalized = normalizeRepositoryPath(file);
  return forbiddenPathPatterns.find(({ pattern }) => pattern.test(normalized));
}

function isTestOrDocumentationPath(file) {
  return (
    file.startsWith("docs/")
    || /(?:^|\/)(?:test|tests|fixtures)(?:\/|$)/iu.test(file)
    || /\.(?:test|spec)\.[cm]?[jt]sx?$/iu.test(file)
  );
}

function isRuntimeSource(file) {
  if (
    file === "scripts/assert-repository.mjs"
    || file === "scripts/verify-bitfun-import.mjs"
    || isTestOrDocumentationPath(file)
  ) {
    return false;
  }

  if (
    file === "package.json"
    || /^(?:apps|packages)\/[^/]+\/package\.json$/u.test(file)
  ) {
    return true;
  }

  if (!/\.(?:[cm]?[jt]s|tsx|json)$/u.test(file)) return false;
  return (
    file.startsWith("apps/desktop/src/")
    || file.startsWith("packages/") && file.includes("/src/")
    || file.startsWith("scripts/")
    || /^apps\/desktop\/(?:vite|vitest)\.config\.ts$/u.test(file)
  );
}

function findRuntimeMarkers(source) {
  return forbiddenRuntimeMarkers.filter(({ pattern }) => pattern.test(source));
}

function readWorkingTreeSource(file) {
  const absolutePath = resolve(repositoryRoot, file);
  const pathFromRoot = relative(repositoryRoot, absolutePath);
  if (pathFromRoot === "" || pathFromRoot.startsWith("..") || isAbsolute(pathFromRoot)) {
    throw new Error(`unsafe repository path ${file}`);
  }
  if (!existsSync(absolutePath)) return undefined;
  if (lstatSync(absolutePath).isSymbolicLink()) return undefined;
  return readFileSync(absolutePath, "utf8");
}

function inspectIndexSources(files) {
  if (files.length === 0) return new Map();
  const result = git(["cat-file", "--batch"], {
    encoding: null,
    input: files.map((file) => `:${file}\n`).join(""),
  });
  if (result.error || result.status !== 0 || !Buffer.isBuffer(result.stdout)) {
    throw new Error("unable to inspect staged runtime source");
  }

  const sources = new Map();
  let offset = 0;
  for (const file of files) {
    const headerEnd = result.stdout.indexOf(0x0a, offset);
    if (headerEnd === -1) throw new Error(`unable to inspect staged source ${file}`);
    const header = result.stdout.subarray(offset, headerEnd).toString("utf8");
    const match = /\sblob\s(\d+)$/u.exec(header);
    if (match?.[1] === undefined) throw new Error(`unable to inspect staged source ${file}`);
    const size = Number.parseInt(match[1], 10);
    const sourceStart = headerEnd + 1;
    const sourceEnd = sourceStart + size;
    if (sourceEnd > result.stdout.length || result.stdout[sourceEnd] !== 0x0a) {
      throw new Error(`unable to inspect staged source ${file}`);
    }
    sources.set(file, result.stdout.subarray(sourceStart, sourceEnd).toString("utf8"));
    offset = sourceEnd + 1;
  }
  return sources;
}

function formatViolation(snapshot, file, label) {
  return `- ${snapshot} ${file}: ${label}`;
}

const indexedFiles = listGitPaths(["ls-files", "-z"]);
const untrackedFiles = listGitPaths(["ls-files", "--others", "--exclude-standard", "-z"]);
const candidateFiles = [...new Set([...indexedFiles, ...untrackedFiles])];

const pathViolations = candidateFiles
  .map((file) => ({ file, forbidden: findForbiddenPath(file) }))
  .filter(({ forbidden }) => forbidden !== undefined);

if (pathViolations.length > 0) {
  console.error("Repository check failed: forbidden repository paths found:");
  for (const { file, forbidden } of pathViolations) {
    console.error(formatViolation("path", file, forbidden.label));
  }
  process.exit(1);
}

const runtimeViolations = [];
const runtimeFiles = candidateFiles.filter(isRuntimeSource);
const indexedRuntimeFiles = runtimeFiles.filter((file) => indexedFiles.includes(file));
let indexedSources;
try {
  indexedSources = inspectIndexSources(indexedRuntimeFiles);
} catch (error) {
  console.error(`Repository check failed: ${error instanceof Error ? error.message : String(error)}.`);
  process.exit(1);
}

for (const file of runtimeFiles) {
  try {
    const source = readWorkingTreeSource(file);
    if (source !== undefined) {
      for (const { label } of findRuntimeMarkers(source)) {
        runtimeViolations.push({ snapshot: "working tree", file, label });
      }
    }

    const indexedSource = indexedSources.get(file);
    if (indexedSource !== undefined) {
      for (const { label } of findRuntimeMarkers(indexedSource)) {
        runtimeViolations.push({ snapshot: "staged content", file, label });
      }
    }
  } catch (error) {
    console.error(`Repository check failed: ${error instanceof Error ? error.message : String(error)}.`);
    process.exit(1);
  }
}

if (runtimeViolations.length > 0) {
  console.error("Repository check failed: forbidden runtime markers found:");
  for (const { snapshot, file, label } of runtimeViolations) {
    console.error(formatViolation(snapshot, file, label));
  }
  process.exit(1);
}

console.log("Repository check passed.");
