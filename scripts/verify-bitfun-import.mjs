import { createHash } from "node:crypto";
import { access, readdir, readFile, stat } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = join(
  repoRoot,
  "docs",
  "requirements",
  "bitfun-tauri-product-migration",
  "bitfun-upstream-manifest.json",
);
const sourceRoot = join(repoRoot, "product", "bitfun");
const expectedUpstream = Object.freeze({
  repository: "https://github.com/GCWing/BitFun.git",
  ref: "refs/heads/main",
  commit: "ca56631e38f36db675583288df2bd44c540d250a",
});
const externalPathNeedle = Buffer.from("D:\\BitFun-main", "utf8");
const formalScanRoots = [
  "package.json",
  "package-lock.json",
  "scripts",
  "sidecar",
  "product",
];

function comparePaths(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function canonicalManifestLines(entries) {
  return [...entries]
    .sort((left, right) => comparePaths(left.path, right.path))
    .map((entry) => `${entry.path}\t${entry.mode}\t${entry.sha}\t${entry.size}`)
    .join("\n") + "\n";
}

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function gitBlobSha(bytes) {
  return createHash("sha1")
    .update(`blob ${bytes.length}\0`)
    .update(bytes)
    .digest("hex");
}

async function collectFiles(directory, prefix = "") {
  const files = [];
  const entries = await readdir(directory, { withFileTypes: true });

  for (const entry of entries) {
    const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name;
    const absolutePath = join(directory, entry.name);

    if (entry.isSymbolicLink()) {
      throw new Error(`symlink is not allowed in imported source: ${relativePath}`);
    }

    if (entry.isDirectory()) {
      if (entry.name === ".git") {
        throw new Error(`Git metadata directory is not allowed: ${relativePath}`);
      }
      files.push(...(await collectFiles(absolutePath, relativePath)));
      continue;
    }

    if (!entry.isFile()) {
      throw new Error(`unsupported filesystem entry in imported source: ${relativePath}`);
    }

    if (entry.name === ".git" || entry.name === ".gitmodules") {
      throw new Error(`Git metadata file is not allowed: ${relativePath}`);
    }

    files.push({ absolutePath, relativePath });
  }

  return files;
}

async function collectFormalFiles() {
  const files = [];

  for (const relativeRoot of formalScanRoots) {
    const absoluteRoot = join(repoRoot, relativeRoot);
    try {
      await access(absoluteRoot);
    } catch {
      continue;
    }

    if ((await stat(absoluteRoot)).isFile()) {
      files.push({ absolutePath: absoluteRoot, relativePath: relativeRoot });
      continue;
    }

    files.push(...(await collectFiles(absoluteRoot, relativeRoot)));
  }

  return files;
}

function normalizedModeCounts(entries) {
  const counts = {};
  for (const entry of entries) {
    counts[entry.mode] = (counts[entry.mode] ?? 0) + 1;
  }
  return Object.fromEntries(
    Object.entries(counts).sort(([left], [right]) => comparePaths(left, right)),
  );
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

async function verify() {
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const expectedEntries = manifest.entries ?? [];
  const expected = new Map(expectedEntries.map((entry) => [entry.path, entry]));
  const errors = [];

  if (
    manifest.upstream?.repository !== expectedUpstream.repository ||
    manifest.upstream?.ref !== expectedUpstream.ref ||
    manifest.upstream?.commit !== expectedUpstream.commit
  ) {
    errors.push("manifest upstream identity does not match the pinned baseline");
  }

  const expectedRemoteOutput = [
    `${expectedUpstream.commit}\tHEAD`,
    `${expectedUpstream.commit}\t${expectedUpstream.ref}`,
  ];
  if (
    manifest.upstream?.remote_verification?.exit_code !== 0 ||
    !sameJson(manifest.upstream?.remote_verification?.output, expectedRemoteOutput)
  ) {
    errors.push("manifest upstream remote verification is not the pinned result");
  }

  if (manifest.import?.scope_root !== "product/bitfun") {
    errors.push("manifest scope_root must be product/bitfun");
  }

  if (
    manifest.source?.file_count !== expectedEntries.length ||
    manifest.import?.file_count !== expectedEntries.length
  ) {
    errors.push("manifest file counts are internally inconsistent");
  }

  if (manifest.source?.submodule_count !== 0) {
    errors.push("manifest records an unexpected upstream submodule");
  }

  for (let index = 0; index < expectedEntries.length; index += 1) {
    const entry = expectedEntries[index];
    if (entry.type !== "blob") {
      errors.push(`manifest entry is not a blob: ${entry.path}`);
    }
    if (!/^(100644|100755)$/.test(entry.mode)) {
      errors.push(`manifest entry has an unsupported Git mode: ${entry.path}`);
    }
    if (!/^[0-9a-f]{40}$/.test(entry.sha) || !Number.isInteger(entry.size)) {
      errors.push(`manifest entry has invalid blob metadata: ${entry.path}`);
    }
    if (index > 0 && comparePaths(expectedEntries[index - 1].path, entry.path) > 0) {
      errors.push("manifest entries are not sorted by ordinal path order");
      break;
    }
  }

  if (!sameJson(normalizedModeCounts(expectedEntries), manifest.source?.mode_counts)) {
    errors.push("manifest mode_counts do not match the file entries");
  }

  const calculatedListDigest = sha256(canonicalManifestLines(expectedEntries));
  if (calculatedListDigest !== manifest.verification?.file_list_sha256) {
    errors.push(
      `manifest file-list digest mismatch: expected ${manifest.verification?.file_list_sha256}, calculated ${calculatedListDigest}`,
    );
  }

  const actualFiles = await collectFiles(sourceRoot);
  const actualPaths = new Set(actualFiles.map((file) => file.relativePath));
  const missing = expectedEntries
    .filter((entry) => !actualPaths.has(entry.path))
    .map((entry) => entry.path);
  const extra = actualFiles
    .filter((file) => !expected.has(file.relativePath))
    .map((file) => file.relativePath);

  if (missing.length > 0) {
    errors.push(`missing imported files: ${missing.slice(0, 20).join(", ")}`);
  }
  if (extra.length > 0) {
    errors.push(`unexpected imported files: ${extra.slice(0, 20).join(", ")}`);
  }

  const mismatches = [];
  for (const file of actualFiles) {
    const entry = expected.get(file.relativePath);
    if (!entry) {
      continue;
    }

    const bytes = await readFile(file.absolutePath);
    const actualSha = gitBlobSha(bytes);
    if (bytes.length !== entry.size || actualSha !== entry.sha) {
      mismatches.push({
        path: file.relativePath,
        expectedSize: entry.size,
        actualSize: bytes.length,
        expectedSha: entry.sha,
        actualSha,
      });
    }
  }

  if (mismatches.length > 0) {
    errors.push(`imported blob mismatches: ${JSON.stringify(mismatches.slice(0, 20))}`);
  }

  const formalFiles = await collectFormalFiles();
  const externalPathMatches = [];
  for (const file of formalFiles) {
    if (file.relativePath === "scripts/verify-bitfun-import.mjs") {
      continue;
    }
    const bytes = await readFile(file.absolutePath);
    if (bytes.includes(externalPathNeedle)) {
      externalPathMatches.push(file.relativePath);
    }
  }
  if (externalPathMatches.length > 0) {
    errors.push(
      `external reference path found in formal files: ${externalPathMatches.join(", ")}`,
    );
  }

  return {
    ok: errors.length === 0,
    upstreamCommit: manifest.upstream?.commit,
    expectedFiles: expectedEntries.length,
    actualFiles: actualFiles.length,
    missingFiles: missing.length,
    extraFiles: extra.length,
    mismatchedFiles: mismatches.length,
    externalPathMatches,
    errors,
  };
}

try {
  const result = await verify();
  console.log(JSON.stringify(result, null, 2));
  if (!result.ok) {
    process.exitCode = 1;
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
