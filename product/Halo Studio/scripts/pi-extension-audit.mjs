import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, realpathSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { inflateRawSync } from "node:zlib";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..", "..");
const DEFAULT_MANIFEST_PATH = path.join(
  DEFAULT_REPO_ROOT,
  "docs",
  "architecture",
  "pi-first-party-extension-inventory.json",
);
const SHA256_PATTERN = /^[0-9a-f]{64}$/i;
const GIT_OBJECT_PATTERN = /^[0-9a-f]{40}$/i;
const COMMIT_PATTERN = /^[0-9a-f]{40}$/i;
const VERSION_PATTERN = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
const READ_ONLY_EVIDENCE_REFERENCE_PATTERN = /^readonly-evidence:\/\/[A-Za-z0-9._-]+(?:\/[^\s]*)?$/;
const DIRECT_GLOBAL_CAPABILITY_PATTERN = /\b(?:globalThis|window)\s*(?:\?\.\s*|\.\s*)[$_\p{ID_Start}][$_\p{ID_Continue}]*/u;
const COMPUTED_GLOBAL_CAPABILITY_PATTERN = /\b(?:globalThis|window)\s*(?:\?\.)?\s*\[[^\]]+\]/i;
const DESKTOP_PAYLOAD_FORMATS = {
  "windows-pe": {
    path: /\.exe$/i,
    magic: (contents) => contents.subarray(0, 2).equals(Buffer.from("MZ", "ascii")),
  },
  "windows-msi": {
    path: /\.msi$/i,
    magic: (contents) => contents.subarray(0, 8).equals(Buffer.from("D0CF11E0A1B11AE1", "hex")),
  },
};
const TEXT_EXTENSIONS = new Set([
  ".cjs",
  ".bat",
  ".cmd",
  ".json",
  ".js",
  ".jsx",
  ".lock",
  ".mjs",
  ".md",
  ".ps1",
  ".psd1",
  ".psm1",
  ".py",
  ".rs",
  ".sh",
  ".toml",
  ".ts",
  ".tsx",
  ".txt",
  ".yaml",
  ".yml",
]);

const READ_ONLY_GIT_ARGS = [
  "--no-optional-locks",
  "-c",
  "core.fsmonitor=false",
  "-c",
  "core.untrackedCache=false",
];
const REQUIRED_UNIQUE_WORKSPACE_MEMBERS = ["src/crates/adapters/pi-rpc-adapter"];
const EXPECTED_RUNTIME_ADAPTER_PATH = "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/lib.rs";
const EXPECTED_AUDIT_SCRIPT_PATH = "product/Halo Studio/scripts/pi-extension-audit.mjs";

function normalizeRelativePath(value) {
  return String(value).replaceAll("\\", "/");
}

function normalizeArchiveEntryPath(value) {
  if (typeof value !== "string") return null;
  const normalized = normalizeRelativePath(value);
  const parts = normalized.split("/");
  if (normalized.trim() === "" || isAbsolutePath(normalized) || parts.some((part) => part === "" || part === "." || part === "..")) return null;
  return normalized;
}

function isAbsolutePath(value) {
  return /^(?:[A-Za-z]:[\\/]|[\\/]{1,2})/.test(String(value));
}

function isExternalEvidenceReference(value) {
  return isAbsolutePath(value) || READ_ONLY_EVIDENCE_REFERENCE_PATTERN.test(String(value));
}

function isFixedVersionTag(tag, version) {
  if (typeof version !== "string" || !VERSION_PATTERN.test(version.trim())) return false;
  if (typeof tag !== "string" || !/^v?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag.trim())) return false;
  const normalizedTag = tag.trim().replace(/^v/, "");
  return normalizedTag === version.trim();
}

function safePathLabel(value) {
  if (typeof value !== "string" || value.trim() === "") return "<missing-path>";
  if (isAbsolutePath(value)) return "<external-path>";
  if (READ_ONLY_EVIDENCE_REFERENCE_PATTERN.test(value)) return "<read-only-evidence>";
  return normalizeRelativePath(value);
}

function safeEvidenceLocator(value) {
  if (typeof value !== "string" || value.trim() === "") return null;
  if (isAbsolutePath(value)) return "<external-path>";
  return normalizeRelativePath(value);
}

function sanitizeEvidence(value) {
  if (Array.isArray(value)) return value.map((entry) => sanitizeEvidence(entry));
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, sanitizeEvidence(entry)]));
  }
  if (typeof value === "string" && isAbsolutePath(value)) return "<external-path>";
  if (typeof value === "string" && READ_ONLY_EVIDENCE_REFERENCE_PATTERN.test(value)) return "<read-only-evidence>";
  return value;
}

function inside(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative === "" || (relative !== ".." && !relative.startsWith(`..${path.sep}`));
}

function resolveRepoPath(repoRoot, relativePath) {
  if (typeof relativePath !== "string" || isAbsolutePath(relativePath)) return null;
  const resolved = path.resolve(repoRoot, ...normalizeRelativePath(relativePath).split("/"));
  if (!inside(repoRoot, resolved)) return null;
  if (existsSync(resolved)) {
    const realRoot = realpathSync(repoRoot);
    const realCandidate = realpathSync(resolved);
    if (!inside(realRoot, realCandidate)) return null;
  }
  return resolved;
}

function canonicalRepoRelativePath(repoRoot, relativePath) {
  const resolved = resolveRepoPath(repoRoot, relativePath);
  if (!resolved) return null;
  const realRoot = realpathSync(repoRoot);
  const realCandidate = existsSync(resolved) ? realpathSync(resolved) : resolved;
  if (!inside(realRoot, realCandidate)) return null;
  return normalizeRelativePath(path.relative(realRoot, realCandidate)).toLowerCase();
}

function addFinding(findings, code, message, evidence = {}) {
  const locator = safeEvidenceLocator(
    evidence?.locator
      ?? evidence?.path
      ?? evidence?.manifest
      ?? evidence?.workspaceManifest
      ?? evidence?.referenceRoot,
  ) ?? `audit://finding/${code}`;
  findings.push({ code, message, locator, evidence: sanitizeEvidence(evidence) });
}

function collectEvidenceLocators(manifest) {
  const locators = [];
  const add = (pointer, value, kind) => {
    const locator = safeEvidenceLocator(value);
    if (!locator) return;
    locators.push({ kind, pointer, locator });
  };

  add("manifest", "<manifest>", "inventory");
  add("manifest.scope.auditScript", manifest?.scope?.auditScript, "audit-script");
  add("manifest.upstreamCandidateEvidence.path", manifest?.upstreamCandidateEvidence?.path, "candidate-evidence");
  add("manifest.dependencyBoundary.workspaceManifest", manifest?.dependencyBoundary?.workspaceManifest, "dependency-boundary");
  add("manifest.releasePolicy.upstreamCandidate.policySource", manifest?.releasePolicy?.upstreamCandidate?.policySource, "release-policy");
  add("manifest.releasePolicy.hostPackage.policySource", manifest?.releasePolicy?.hostPackage?.policySource, "release-policy");
  add("manifest.runtime.adapterPath", manifest?.runtime?.adapterPath, "runtime-adapter");
  for (const [index, scanPath] of (manifest?.runtime?.scanPaths ?? []).entries()) {
    add(`manifest.runtime.scanPaths[${index}]`, scanPath, "runtime-input");
  }
  for (const [index, builtIn] of (manifest?.runtime?.builtInExtensions ?? []).entries()) {
    add(`manifest.runtime.builtInExtensions[${index}].sourcePath`, builtIn?.sourcePath, "host-evidence");
    for (const [evidenceIndex, evidencePath] of (builtIn?.capabilities?.evidence ?? []).entries()) {
      add(`manifest.runtime.builtInExtensions[${index}].capabilities.evidence[${evidenceIndex}]`, evidencePath, "host-evidence");
    }
  }
  for (const [index, extension] of (manifest?.extensions ?? []).entries()) {
    const prefix = `manifest.extensions[${index}]`;
    add(`${prefix}.sourcePath`, extension?.sourcePath, "extension-source");
    for (const [evidenceIndex, item] of (extension?.license?.evidence ?? []).entries()) {
      add(`${prefix}.license.evidence[${evidenceIndex}].path`, typeof item === "string" ? item : item?.path, "license-evidence");
    }
    for (const [lockfileIndex, item] of (extension?.license?.lockfileEvidence ?? []).entries()) {
      add(`${prefix}.license.lockfileEvidence[${lockfileIndex}].path`, typeof item === "string" ? item : item?.path, "lockfile-evidence");
    }
    for (const [distributionIndex, item] of (extension?.license?.distributionFiles ?? []).entries()) {
      add(`${prefix}.license.distributionFiles[${distributionIndex}].path`, typeof item === "string" ? item : item?.path, "distribution-evidence");
    }
    add(`${prefix}.license.releaseArtifactEvidence.path`, extension?.license?.releaseArtifactEvidence?.path, "release-artifact");
    add(`${prefix}.dependencies.host.dependencyClosure.evidencePath`, extension?.dependencies?.host?.dependencyClosure?.evidencePath, "host-dependency-evidence");
    add(`${prefix}.dependencies.host.licenseEvidence.evidencePath`, extension?.dependencies?.host?.licenseEvidence?.evidencePath, "host-license-evidence");
    for (const [releaseIndex, item] of (extension?.dependencies?.host?.licenseEvidence?.releaseFiles ?? []).entries()) {
      add(`${prefix}.dependencies.host.licenseEvidence.releaseFiles[${releaseIndex}].path`, typeof item === "string" ? item : item?.path, "host-license-release-evidence");
    }
  }

  const seen = new Set();
  return locators.filter((entry) => {
    const key = `${entry.kind}:${entry.pointer}:${entry.locator}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function regexEscape(value) {
  return String(value).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function hasAliasedComputedGlobalCapability(sourceContents) {
  const aliases = [...sourceContents.matchAll(/\b(?:const|let|var)\s+([$_\p{ID_Start}][$_\p{ID_Continue}]*)\s*=\s*(?:globalThis|window)\b(?!\s*(?:\?\.|\.|\[))/gu)]
    .map((match) => match[1]);
  return aliases.some((alias) => new RegExp(
    `(?:^|[^$_\\p{ID_Continue}])${regexEscape(alias)}\\s*(?:\\?\\.)?\\s*\\[[^\\]]+\\]`,
    "u",
  ).test(sourceContents));
}

function hasAliasedGlobalCapability(sourceContents) {
  if (hasAliasedComputedGlobalCapability(sourceContents)) return true;
  const aliases = new Set(
    [...sourceContents.matchAll(/\b(?:const|let|var)\s+([$_\p{ID_Start}][$_\p{ID_Continue}]*)\s*=\s*(?:globalThis|window)\b(?!\s*(?:\?\.|\.|\[))/gu)]
      .map((match) => match[1]),
  );
  let changed = true;
  while (changed) {
    changed = false;
    for (const match of sourceContents.matchAll(/\b(?:const|let|var)\s+([$_\p{ID_Start}][$_\p{ID_Continue}]*)\s*=\s*([$_\p{ID_Start}][$_\p{ID_Continue}]*)\b/gu)) {
      if (aliases.has(match[2]) && !aliases.has(match[1])) {
        aliases.add(match[1]);
        changed = true;
      }
    }
  }
  return [...aliases].some((alias) => new RegExp(
    "(?:^|[^$_\\p{ID_Continue}])"
      + regexEscape(alias)
      + "\\s*(?:\\?\\.)?\\s*(?:\\[[^\\]]+\\]|\\.\\s*[$_\\p{ID_Start}][$_\\p{ID_Continue}]*)",
    "u",
  ).test(sourceContents));
}

function readJson(filePath, findings) {
  try {
    return JSON.parse(readFileSync(filePath, "utf8"));
  } catch (error) {
    addFinding(findings, "manifest-invalid-json", "Cannot parse JSON evidence");
    return null;
  }
}

function hashFile(filePath, algorithm) {
  return createHash(algorithm).update(readFileSync(filePath)).digest("hex");
}

function gitHashObject(repoRoot, relativePath) {
  const result = runGit(repoRoot, ["hash-object", "--", normalizeRelativePath(relativePath)]);
  return result.ok ? result.stdout.trim() : null;
}

function runGit(cwd, args) {
  try {
    return {
      ok: true,
      status: 0,
      stdout: execFileSync("git", [...READ_ONLY_GIT_ARGS, ...args], {
        cwd,
        encoding: "utf8",
        env: { ...process.env, GIT_OPTIONAL_LOCKS: "0" },
        stdio: ["ignore", "pipe", "pipe"],
      }),
      stderr: "",
    };
  } catch (error) {
    return {
      ok: false,
      status: typeof error.status === "number" ? error.status : null,
      stdout: String(error.stdout ?? ""),
      stderr: String(error.stderr ?? ""),
    };
  }
}

function gitBlobAtCommit(repoRoot, commit, relativePath) {
  const resolvedCommit = runGit(repoRoot, ["rev-parse", "--verify", `${commit}^{commit}`]);
  if (!resolvedCommit.ok || resolvedCommit.stdout.trim().toLowerCase() !== String(commit).toLowerCase()) return null;
  const result = runGit(repoRoot, ["rev-parse", "--verify", `${commit}:${normalizeRelativePath(relativePath)}`]);
  if (!result.ok) return null;
  const blob = result.stdout.trim();
  const type = runGit(repoRoot, ["cat-file", "-t", blob]);
  return type.ok && type.stdout.trim() === "blob" ? blob : null;
}

function gitTreeAtCommit(repoRoot, commit) {
  const resolvedCommit = runGit(repoRoot, ["rev-parse", "--verify", `${commit}^{commit}`]);
  if (!resolvedCommit.ok || resolvedCommit.stdout.trim().toLowerCase() !== String(commit).toLowerCase()) return null;
  const result = runGit(repoRoot, ["rev-parse", "--verify", `${commit}^{tree}`]);
  if (!result.ok) return null;
  const tree = result.stdout.trim();
  const type = runGit(repoRoot, ["cat-file", "-t", tree]);
  return type.ok && type.stdout.trim() === "tree" ? tree : null;
}

function isRegularFile(filePath) {
  try {
    return statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function isTextLikeFile(filePath) {
  try {
    return !readFileSync(filePath).subarray(0, 8192).includes(0);
  } catch {
    return false;
  }
}

function collectTextFiles(repoRoot, relativePath, findings) {
  const absolutePath = resolveRepoPath(repoRoot, relativePath);
  if (!absolutePath || !existsSync(absolutePath)) {
    addFinding(findings, "runtime-scan-path-missing", `Runtime scan path is missing: ${safePathLabel(relativePath)}`);
    return [];
  }

  const stat = statSync(absolutePath);
  if (stat.isFile()) return [absolutePath];
  if (!stat.isDirectory()) return [];

  const files = [];
  for (const entry of readdirSync(absolutePath, { withFileTypes: true })) {
    const entryPath = path.join(absolutePath, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectTextFiles(repoRoot, path.relative(repoRoot, entryPath), findings));
      continue;
    }
    if (entry.isFile() && (TEXT_EXTENSIONS.has(path.extname(entry.name).toLowerCase()) || path.extname(entry.name) === "") && isTextLikeFile(entryPath)) {
      files.push(entryPath);
    }
  }
  return files;
}

function checkFileFingerprint(filePath, descriptor, codePrefix, label, findings) {
  const expectedHash = descriptor?.sha256;
  const expectedSize = descriptor?.size;
  if (!SHA256_PATTERN.test(expectedHash ?? "") || !Number.isInteger(expectedSize) || expectedSize < 0) {
    addFinding(findings, `${codePrefix}-fingerprint-missing`, `${label} must declare a SHA-256 and byte size`);
    return;
  }
  const actualHash = hashFile(filePath, "sha256");
  const actualSize = statSync(filePath).size;
  if (actualHash.toLowerCase() !== expectedHash.toLowerCase()) {
    addFinding(findings, `${codePrefix}-hash-mismatch`, `${label} SHA-256 does not match the audited file`);
  }
  if (actualSize !== expectedSize) {
    addFinding(findings, `${codePrefix}-size-mismatch`, `${label} byte size does not match the audited file`);
  }
}

function checkBufferFingerprint(contents, descriptor, codePrefix, label, findings) {
  const expectedHash = descriptor?.sha256;
  const expectedSize = descriptor?.size;
  if (!SHA256_PATTERN.test(expectedHash ?? "") || !Number.isInteger(expectedSize) || expectedSize < 0) {
    addFinding(findings, `${codePrefix}-fingerprint-missing`, `${label} must declare a SHA-256 and byte size`);
    return;
  }
  const actualHash = createHash("sha256").update(contents).digest("hex");
  if (actualHash.toLowerCase() !== expectedHash.toLowerCase()) {
    addFinding(findings, `${codePrefix}-hash-mismatch`, `${label} SHA-256 does not match the audited bytes`);
  }
  if (contents.length !== expectedSize) {
    addFinding(findings, `${codePrefix}-size-mismatch`, `${label} byte size does not match the audited bytes`);
  }
}

function checkBufferRequiredTextClaims(contents, claims, codePrefix, label, findings) {
  if (!Array.isArray(claims) || claims.length === 0) {
    addFinding(findings, `${codePrefix}-claims-missing`, `${label} must declare exact text claims`);
    return;
  }
  const text = contents.toString("utf8");
  for (const claim of claims) {
    if (typeof claim !== "string" || claim.trim() === "" || !text.includes(claim)) {
      addFinding(findings, `${codePrefix}-text-missing`, `${label} does not contain a declared text claim`);
    }
  }
}

function checkReleaseArtifactArchive(repoRoot, extension, releaseArtifact, artifactPath, findings) {
  if (releaseArtifact.artifactFormat !== "zip") {
    addFinding(findings, "release-artifact-format-missing", `${extension.id} release artifact evidence must declare the supported zip format`);
    return null;
  }
  if (typeof releaseArtifact.path !== "string" || !releaseArtifact.path.toLowerCase().endsWith(".zip")) {
    addFinding(findings, "release-artifact-format-mismatch", `${extension.id} release artifact path must identify a .zip desktop distribution`);
    return null;
  }
  if (!artifactPath || !isRegularFile(artifactPath)) return null;

  let entries;
  try {
    entries = readZipEntries(artifactPath);
  } catch {
    addFinding(findings, "release-artifact-archive-invalid", `${extension.id} release artifact is not a verifiable zip archive`);
    return null;
  }

  const requiredRoles = new Map([
    ["license", "product/Halo Studio/LICENSE"],
    ["third-party-notice", "product/THIRD_PARTY_NOTICES.md"],
  ]);
  const includedFiles = releaseArtifact.includedFiles;
  if (!Array.isArray(includedFiles) || includedFiles.length === 0) {
    addFinding(findings, "release-artifact-inclusion-evidence-missing", `${extension.id} release artifact must enumerate exact included license and notice files`);
    return entries;
  }

  const seenRoles = new Set();
  const seenPaths = new Set();
  for (const descriptor of includedFiles) {
    const role = descriptor?.role;
    const archivePath = normalizeArchiveEntryPath(descriptor?.path);
    if (!descriptor || typeof descriptor !== "object" || !requiredRoles.has(role) || !archivePath) {
      addFinding(findings, "release-artifact-inclusion-invalid", `${extension.id} release artifact inclusion entries require a role and safe archive path`);
      continue;
    }
    if (seenRoles.has(role) || seenPaths.has(archivePath)) {
      addFinding(findings, "release-artifact-inclusion-duplicate", `${extension.id} release artifact inclusion entries must use each required role and archive path once`);
    }
    seenRoles.add(role);
    seenPaths.add(archivePath);

    const expectedSourcePath = requiredRoles.get(role);
    if (descriptor.sourcePath !== expectedSourcePath) {
      addFinding(findings, "release-artifact-source-path-invalid", `${extension.id} release artifact ${role} entry must bind the exact product source file`, {
        expected: expectedSourcePath,
        recorded: descriptor.sourcePath,
      });
    }
    const archiveContents = entries.get(archivePath);
    if (!archiveContents) {
      addFinding(findings, "release-artifact-included-file-missing", `${extension.id} release artifact is missing its declared ${role} file: ${safePathLabel(descriptor.path)}`);
      continue;
    }
    checkBufferFingerprint(archiveContents, descriptor, "release-artifact-entry", `${extension.id} release artifact ${role}`, findings);
    checkBufferRequiredTextClaims(archiveContents, descriptor.requiredText, "release-artifact-entry", `${extension.id} release artifact ${role}`, findings);

    const sourcePath = resolveRepoPath(repoRoot, descriptor.sourcePath);
    if (!sourcePath || !isRegularFile(sourcePath)) {
      addFinding(findings, "release-artifact-source-file-missing", `${extension.id} release artifact source file is missing: ${safePathLabel(descriptor.sourcePath)}`);
    } else {
      checkFileFingerprint(sourcePath, descriptor, "release-artifact-source", `${extension.id} release artifact ${role} source`, findings);
      if (!readFileSync(sourcePath).equals(archiveContents)) {
        addFinding(findings, "release-artifact-entry-source-mismatch", `${extension.id} release artifact ${role} bytes do not match the exact product source file`);
      }
    }
  }
  for (const role of requiredRoles.keys()) {
    if (!seenRoles.has(role)) addFinding(findings, "release-artifact-inclusion-missing", `${extension.id} release artifact must include a ${role} file entry`);
  }
  return entries;
}

function checkReleaseArtifactPayload(extension, releaseArtifact, entries, findings) {
  const payload = releaseArtifact?.payload;
  if (!payload || typeof payload !== "object") {
    addFinding(findings, "release-artifact-payload-evidence-missing", `${extension.id} release artifact must identify a verifiable desktop payload`);
    return;
  }
  const format = DESKTOP_PAYLOAD_FORMATS[payload.format];
  const payloadPath = normalizeArchiveEntryPath(payload.path);
  if (!format || !payloadPath || !format.path.test(payloadPath)) {
    addFinding(findings, "release-artifact-payload-format-invalid", `${extension.id} release artifact payload must use a supported desktop binary format and path`);
    return;
  }
  const payloadContents = entries.get(payloadPath);
  if (!payloadContents) {
    addFinding(findings, "release-artifact-payload-missing", `${extension.id} release artifact is missing its declared desktop payload: ${safePathLabel(payload.path)}`);
    return;
  }
  checkBufferFingerprint(payloadContents, payload, "release-artifact-payload", `${extension.id} release artifact payload`, findings);
  if (!format.magic(payloadContents)) {
    addFinding(findings, "release-artifact-payload-magic-invalid", `${extension.id} release artifact payload does not match its declared desktop binary format`);
  }
}

function crc32(contents) {
  let value = 0xffffffff;
  for (const byte of contents) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
    }
  }
  return (value ^ 0xffffffff) >>> 0;
}

function readZipEntries(filePath) {
  const archive = readFileSync(filePath);
  const minimumEndRecordOffset = Math.max(0, archive.length - 0xffff - 22);
  let endRecordOffset = -1;
  for (let offset = archive.length - 22; offset >= minimumEndRecordOffset; offset -= 1) {
    if (offset >= 0 && archive.readUInt32LE(offset) === 0x06054b50) {
      endRecordOffset = offset;
      break;
    }
  }
  if (endRecordOffset < 0) throw new Error("ZIP end record is missing");

  const diskNumber = archive.readUInt16LE(endRecordOffset + 4);
  const centralDirectoryDisk = archive.readUInt16LE(endRecordOffset + 6);
  const entriesOnDisk = archive.readUInt16LE(endRecordOffset + 8);
  const entryCount = archive.readUInt16LE(endRecordOffset + 10);
  const centralDirectorySize = archive.readUInt32LE(endRecordOffset + 12);
  const centralDirectoryOffset = archive.readUInt32LE(endRecordOffset + 16);
  if (diskNumber !== 0 || centralDirectoryDisk !== 0 || entriesOnDisk !== entryCount
    || centralDirectoryOffset + centralDirectorySize > endRecordOffset) {
    throw new Error("ZIP archive is multi-disk or has an invalid central directory");
  }

  const entries = new Map();
  let cursor = centralDirectoryOffset;
  for (let index = 0; index < entryCount; index += 1) {
    if (archive.readUInt32LE(cursor) !== 0x02014b50) throw new Error("ZIP central directory entry is invalid");
    const compressionMethod = archive.readUInt16LE(cursor + 10);
    const expectedCrc = archive.readUInt32LE(cursor + 16);
    const compressedSize = archive.readUInt32LE(cursor + 20);
    const uncompressedSize = archive.readUInt32LE(cursor + 24);
    const fileNameLength = archive.readUInt16LE(cursor + 28);
    const extraLength = archive.readUInt16LE(cursor + 30);
    const commentLength = archive.readUInt16LE(cursor + 32);
    const localHeaderOffset = archive.readUInt32LE(cursor + 42);
    const nameStart = cursor + 46;
    const name = archive.subarray(nameStart, nameStart + fileNameLength).toString("utf8");
    const normalizedName = normalizeArchiveEntryPath(name);
    if (!normalizedName || entries.has(normalizedName)) throw new Error("ZIP archive contains an invalid or duplicate entry");

    if (archive.readUInt32LE(localHeaderOffset) !== 0x04034b50) throw new Error("ZIP local file header is invalid");
    const localNameLength = archive.readUInt16LE(localHeaderOffset + 26);
    const localExtraLength = archive.readUInt16LE(localHeaderOffset + 28);
    const dataStart = localHeaderOffset + 30 + localNameLength + localExtraLength;
    const compressed = archive.subarray(dataStart, dataStart + compressedSize);
    if (compressed.length !== compressedSize) throw new Error("ZIP entry data is truncated");
    let contents;
    if (compressionMethod === 0) contents = Buffer.from(compressed);
    else if (compressionMethod === 8) contents = inflateRawSync(compressed);
    else throw new Error("ZIP entry uses an unsupported compression method");
    if (contents.length !== uncompressedSize || crc32(contents) !== expectedCrc) {
      throw new Error("ZIP entry content fingerprint is invalid");
    }
    entries.set(normalizedName, contents);
    cursor += 46 + fileNameLength + extraLength + commentLength;
  }
  if (cursor !== centralDirectoryOffset + centralDirectorySize) throw new Error("ZIP central directory size is invalid");
  return entries;
}

function checkRequiredTextClaims(filePath, claims, codePrefix, label, findings) {
  if (!Array.isArray(claims) || claims.length === 0) {
    addFinding(findings, `${codePrefix}-claims-missing`, `${label} must declare exact text claims`);
    return;
  }
  const contents = readFileSync(filePath, "utf8");
  for (const claim of claims) {
    if (typeof claim !== "string" || claim.trim() === "" || !contents.includes(claim)) {
      addFinding(findings, `${codePrefix}-text-missing`, `${label} does not contain a declared text claim`);
    }
  }
}

function checkSpdxTextClaims(contents, spdx, codePrefix, label, findings) {
  const markers = {
    MIT: [/MIT License/i, /Permission is hereby granted/i],
    "Apache-2.0": [/Apache License/i, /Version 2\.0/i],
  }[spdx];
  if (!markers || !markers.every((marker) => marker.test(contents))) {
    addFinding(findings, `${codePrefix}-spdx-text-missing`, `${label} does not contain the complete text for its declared SPDX license`);
  }
}

function checkEvidenceFiles(repoRoot, extension, findings) {
  const license = extension.license;
  const evidence = license?.evidence;
  const evidenceContents = [];
  if (!Array.isArray(evidence) || evidence.length === 0) {
    addFinding(findings, "license-evidence-missing", `${extension.id} has no file-based license evidence`);
  } else {
    for (const item of evidence) {
      const evidencePath = resolveRepoPath(repoRoot, item?.path);
      if (!evidencePath || !isRegularFile(evidencePath)) {
        addFinding(findings, "license-evidence-file-missing", `License evidence file is missing: ${safePathLabel(item?.path)}`);
        continue;
      }
      checkFileFingerprint(evidencePath, item, "license-evidence", `License evidence ${safePathLabel(item.path)}`, findings);
      const contents = readFileSync(evidencePath, "utf8");
      evidenceContents.push({ path: item.path, contents });
      if (!Array.isArray(item.requiredText) || item.requiredText.length === 0) {
        addFinding(findings, "license-evidence-claims-missing", `License evidence ${safePathLabel(item.path)} must declare exact file claims`);
      }
      for (const requiredText of item.requiredText ?? []) {
        if (typeof requiredText !== "string" || !contents.includes(requiredText)) {
          addFinding(
            findings,
            "license-evidence-text-missing",
            `License evidence ${safePathLabel(item.path)} does not contain a declared text claim`,
          );
        }
      }
    }
  }

  const combinedEvidence = evidenceContents.map(({ contents }) => contents).join("\n");
  const spdx = typeof license?.spdx === "string" ? license.spdx.trim() : "";
  const spdxMarkers = {
    MIT: [/MIT License/i, /Permission is hereby granted/i],
    "Apache-2.0": [/Apache License/i, /Version 2\.0/i],
  };
  const markers = spdxMarkers[spdx];
  if (!spdx || !markers || !markers.every((marker) => marker.test(combinedEvidence))) {
    addFinding(findings, "license-spdx-evidence-missing", `${extension.id} declared SPDX ${spdx || "<empty>"} is not evidenced by the actual license files`);
  }
  const copyright = typeof license?.copyright === "string" ? license.copyright.trim() : "";
  if (!copyright || !combinedEvidence.includes(copyright)) {
    addFinding(findings, "license-copyright-evidence-missing", `${extension.id} declared copyright is not present in the actual license evidence`);
  }
  const productNoticePath = canonicalRepoRelativePath(repoRoot, "product/THIRD_PARTY_NOTICES.md");
  const notice = evidenceContents.find(({ path: evidencePath }) => /(^|\/)THIRD_PARTY_NOTICES\.md$/i.test(normalizeRelativePath(evidencePath)));
  if (!notice) {
    addFinding(findings, "license-notice-evidence-missing", `${extension.id} must cite product/THIRD_PARTY_NOTICES.md as notice evidence`);
  } else if (canonicalRepoRelativePath(repoRoot, notice.path) !== productNoticePath) {
    addFinding(findings, "license-notice-path-invalid", extension.id + " license notice evidence must be product/THIRD_PARTY_NOTICES.md");
  } else {
    for (const claim of [extension.id, extension.sourcePath, extension.sourceCommit, extension.sourceTree, extension.gitHashObject, extension.sha256]) {
      if (typeof claim !== "string" || !notice.contents.toLowerCase().includes(claim.toLowerCase())) {
        addFinding(findings, "license-notice-provenance-missing", `${extension.id} notice does not contain an audited provenance claim`);
      }
    }
  }

  const lockfileEvidence = license?.lockfileEvidence;
  if (!Array.isArray(lockfileEvidence) || lockfileEvidence.length === 0) {
    addFinding(findings, "license-lockfile-evidence-missing", `${extension.id} has no lockfile evidence list`);
  } else {
    for (const lockfile of lockfileEvidence) {
      const lockfileDescriptor = typeof lockfile === "string" ? { path: lockfile } : lockfile;
      const lockfilePath = resolveRepoPath(repoRoot, lockfileDescriptor?.path);
      if (!lockfilePath || !isRegularFile(lockfilePath)) {
        addFinding(findings, "license-lockfile-file-missing", `${extension.id} lockfile evidence is missing: ${safePathLabel(lockfileDescriptor?.path)}`);
        continue;
      }
      checkFileFingerprint(lockfilePath, lockfileDescriptor, "license-lockfile", `${extension.id} lockfile ${safePathLabel(lockfileDescriptor.path)}`, findings);
    }
  }

  const distributionFiles = license?.distributionFiles;
  if (!Array.isArray(distributionFiles) || distributionFiles.length === 0) {
    addFinding(findings, "distribution-license-evidence-missing", `${extension.id} has no release-file evidence`);
  } else {
    if (!distributionFiles.some((item) => canonicalRepoRelativePath(repoRoot, typeof item === "string" ? item : item?.path) === productNoticePath)) {
      addFinding(findings, "distribution-license-notice-missing", extension.id + " release evidence must include product/THIRD_PARTY_NOTICES.md");
    }
    for (const distributionFile of distributionFiles) {
      const relativePath = typeof distributionFile === "string" ? distributionFile : distributionFile?.path;
      const distributionPath = resolveRepoPath(repoRoot, relativePath);
      if (!distributionPath || !isRegularFile(distributionPath)) {
        addFinding(findings, "distribution-license-file-missing", `Release license/notice file is missing: ${safePathLabel(relativePath)}`);
        continue;
      }
      checkFileFingerprint(distributionPath, distributionFile, "distribution-license", `Release license/notice file ${safePathLabel(relativePath)}`, findings);
      const requiredText = distributionFile && typeof distributionFile === "object" ? distributionFile.requiredText : null;
      if (!Array.isArray(requiredText) || requiredText.length === 0) {
        addFinding(findings, "distribution-license-text-claims-missing", `Release license/notice file ${safePathLabel(relativePath)} must declare exact text claims`);
      }
      const contents = readFileSync(distributionPath, "utf8");
      for (const claim of requiredText ?? []) {
        if (typeof claim !== "string" || !contents.includes(claim)) {
          addFinding(findings, "distribution-license-text-missing", `Release license/notice file ${safePathLabel(relativePath)} does not contain a declared text claim`);
        }
      }
    }
  }

  const releaseArtifact = license?.releaseArtifactEvidence;
  if (!releaseArtifact || typeof releaseArtifact !== "object") {
    addFinding(findings, "release-artifact-evidence-missing", `${extension.id} has no exact release artifact license/notice evidence`);
  } else {
    const artifactPath = resolveRepoPath(repoRoot, releaseArtifact.path);
    if (releaseArtifact.artifactType !== "desktop-distribution") {
      addFinding(findings, "release-artifact-type-missing", extension.id + " release artifact evidence must identify an exact desktop distribution artifact");
    }
    const artifactLabel = canonicalRepoRelativePath(repoRoot, releaseArtifact.path);
    const evidenceOwnedPaths = new Set([
      ...(license?.evidence ?? []).map((item) => canonicalRepoRelativePath(repoRoot, typeof item === "string" ? item : item?.path)),
      ...(license?.distributionFiles ?? []).map((item) => canonicalRepoRelativePath(repoRoot, typeof item === "string" ? item : item?.path)),
      ...(license?.lockfileEvidence ?? []).map((item) => canonicalRepoRelativePath(repoRoot, typeof item === "string" ? item : item?.path)),
    ].filter(Boolean));
    if (artifactLabel && evidenceOwnedPaths.has(artifactLabel)) {
      addFinding(findings, "release-artifact-reuses-license-file", extension.id + " exact release artifact must be distinct from license, notice, and lockfile evidence");
    }
    if (!artifactPath || !isRegularFile(artifactPath)) {
      addFinding(findings, "release-artifact-file-missing", `${extension.id} release artifact evidence file is missing: ${safePathLabel(releaseArtifact.path)}`);
    } else if (!SHA256_PATTERN.test(releaseArtifact.sha256 ?? "") || hashFile(artifactPath, "sha256") !== releaseArtifact.sha256.toLowerCase()) {
      addFinding(findings, "release-artifact-hash-mismatch", `${extension.id} release artifact evidence does not match its recorded SHA-256`);
    }
    const artifactEntries = checkReleaseArtifactArchive(repoRoot, extension, releaseArtifact, artifactPath, findings);
    if (artifactEntries) checkReleaseArtifactPayload(extension, releaseArtifact, artifactEntries, findings);
    if (artifactPath && isRegularFile(artifactPath)) {
      checkFileFingerprint(artifactPath, releaseArtifact, "release-artifact", `${extension.id} release artifact`, findings);
    }
    if (!Array.isArray(releaseArtifact.requiredText) || releaseArtifact.requiredText.length === 0) {
      addFinding(findings, "release-artifact-text-claims-missing", `${extension.id} release artifact evidence must declare exact text claims`);
    }
    const artifactText = artifactEntries
      ? [...artifactEntries.values()].map((contents) => contents.toString("utf8")).join("\n")
      : "";
    for (const requiredText of releaseArtifact.requiredText ?? []) {
      if (typeof requiredText !== "string" || artifactText === "" || !artifactText.includes(requiredText)) {
        addFinding(findings, "release-artifact-text-missing", `${extension.id} release artifact evidence is missing a declared text claim`);
      }
    }
  }
}

function checkDependencies(repoRoot, extension, findings) {
  const dependencies = extension.dependencies;
  if (!dependencies?.runtime || typeof dependencies.runtime !== "object") {
    addFinding(findings, "dependency-inventory-incomplete", `${extension.id}.dependencies.runtime is incomplete`);
  } else {
    for (const key of ["direct", "transitive"]) {
      if (!Array.isArray(dependencies.runtime[key])) {
        addFinding(findings, "dependency-inventory-incomplete", `${extension.id}.dependencies.runtime.${key} is incomplete`);
      }
    }
  }
  if (!Array.isArray(dependencies?.typeOnly)) addFinding(findings, "dependency-inventory-incomplete", `${extension.id}.dependencies.typeOnly is incomplete`);
  if (!dependencies?.host || typeof dependencies.host !== "object") addFinding(findings, "dependency-inventory-incomplete", `${extension.id}.dependencies.host is incomplete`);
  if (!Array.isArray(dependencies?.lockfiles)) addFinding(findings, "dependency-inventory-incomplete", `${extension.id}.dependencies.lockfiles is incomplete`);
  if (Array.isArray(dependencies?.typeOnly) && dependencies.typeOnly.some((entry) => typeof entry !== "string" || entry.trim() === "")) {
    addFinding(findings, "dependency-inventory-incomplete", `${extension.id}.dependencies.typeOnly must contain package names`);
  }

  if (Array.isArray(dependencies?.runtime?.direct) && dependencies.runtime.direct.length > 0) {
    addFinding(findings, "runtime-dependency-present", `${extension.id} declares runtime extension dependencies`, {
      dependencies: dependencies.runtime.direct,
    });
  }
  if (Array.isArray(dependencies?.runtime?.transitive) && dependencies.runtime.transitive.length > 0) {
    addFinding(findings, "transitive-dependency-present", `${extension.id} declares transitive extension dependencies`, {
      dependencies: dependencies.runtime.transitive,
    });
  }

  for (const lockfile of dependencies?.lockfiles ?? []) {
    const lockfilePath = resolveRepoPath(repoRoot, lockfile);
    if (!lockfilePath || !isRegularFile(lockfilePath)) {
      addFinding(findings, "dependency-lockfile-missing", `Dependency lockfile is missing: ${safePathLabel(lockfile)}`);
      continue;
    }
    const contents = readFileSync(lockfilePath, "utf8");
    if (contents.includes("@earendil-works/pi-coding-agent")) {
      addFinding(
        findings,
        "runtime-dependency-in-lockfile",
        `The Pi host package appears in a Halo lockfile and must not become an extension runtime dependency: ${safePathLabel(lockfile)}`,
      );
    }
  }

  const host = dependencies?.host;
  if (host && (typeof host.package !== "string" || host.package.trim() === "" || !VERSION_PATTERN.test(host.version ?? ""))) {
    addFinding(findings, "host-package-identity-incomplete", `${extension.id} host package must declare a package name and fixed version`);
  }
  const exactHostTag = isFixedVersionTag(host?.sourceTag, host?.version);
  if (!COMMIT_PATTERN.test(host?.sourceCommit ?? "") && !exactHostTag) {
    addFinding(
      findings,
      "host-source-provenance-missing",
      `${extension.id} host package has no exact source commit or tag; do not infer provenance from its package name`,
    );
  }
  const hostLicense = host?.licenseEvidence;
  if (!hostLicense || typeof hostLicense !== "object" || typeof hostLicense.observedSpdx !== "string" || typeof hostLicense.evidencePath !== "string") {
    addFinding(
      findings,
      "host-license-evidence-missing",
      `${extension.id} host package license is not evidenced in the Halo release files`,
    );
  } else if (hostLicense.releaseStatus !== "included" || !Array.isArray(hostLicense.releaseFiles) || hostLicense.releaseFiles.length === 0) {
    addFinding(
      findings,
      "host-license-evidence-not-release",
      `${extension.id} host package license evidence is observed outside Halo release files and cannot pass the release gate`,
      { evidencePath: hostLicense.evidencePath, releaseStatus: hostLicense.releaseStatus ?? null },
    );
  } else {
    const extensionLicenseEvidencePaths = new Set(
      (extension.license?.evidence ?? [])
        .map((item) => canonicalRepoRelativePath(repoRoot, typeof item === "string" ? item : item?.path))
        .filter(Boolean),
    );
    const extensionLicenseDistributionPaths = (extension.license?.distributionFiles ?? [])
      .map((item) => canonicalRepoRelativePath(repoRoot, typeof item === "string" ? item : item?.path))
      .filter(Boolean);
    const extensionLicenseReleaseArtifactPath = canonicalRepoRelativePath(repoRoot, extension.license?.releaseArtifactEvidence?.path);
    const extensionOwnedLicensePaths = new Set([
      ...extensionLicenseEvidencePaths,
      ...extensionLicenseDistributionPaths,
      extensionLicenseReleaseArtifactPath,
    ].filter(Boolean));
    const haloLicensePath = canonicalRepoRelativePath(repoRoot, "product/Halo Studio/LICENSE");
    const hostLicenseEvidenceLabel = canonicalRepoRelativePath(repoRoot, hostLicense.evidencePath);
    if (hostLicenseEvidenceLabel && (extensionOwnedLicensePaths.has(hostLicenseEvidenceLabel) || hostLicenseEvidenceLabel === haloLicensePath)) {
      addFinding(findings, "host-license-evidence-misclassified", `${extension.id} host license evidence must not reuse Halo extension license files`);
    }
    const hostLicenseEvidencePath = resolveRepoPath(repoRoot, hostLicense.evidencePath);
    if (!hostLicenseEvidencePath || !isRegularFile(hostLicenseEvidencePath)) {
      addFinding(findings, "host-license-evidence-file-missing", `${extension.id} host license evidence must point to a repository-local file`);
    } else {
      if (!SHA256_PATTERN.test(hostLicense.evidenceSha256 ?? "") || !Number.isInteger(hostLicense.evidenceSize) || hostLicense.evidenceSize < 0) {
        addFinding(findings, "host-license-evidence-fingerprint-missing", `${extension.id} host license evidence must declare a SHA-256 and byte size`);
      } else {
        checkFileFingerprint(hostLicenseEvidencePath, {
          sha256: hostLicense.evidenceSha256,
          size: hostLicense.evidenceSize,
        }, "host-license-evidence", `${extension.id} host license evidence`, findings);
      }
      const hostLicenseContents = readFileSync(hostLicenseEvidencePath, "utf8");
      checkRequiredTextClaims(hostLicenseEvidencePath, hostLicense.requiredText, "host-license-evidence", `${extension.id} host license evidence`, findings);
      checkSpdxTextClaims(hostLicenseContents, hostLicense.observedSpdx, "host-license", `${extension.id} host license evidence`, findings);
      if (typeof hostLicense.copyright !== "string" || hostLicense.copyright.trim() === "" || !hostLicenseContents.includes(hostLicense.copyright)) {
        addFinding(findings, "host-license-copyright-missing", `${extension.id} host license evidence does not contain its declared attribution text`);
      }
    }
    for (const releaseFile of hostLicense.releaseFiles) {
      const descriptor = typeof releaseFile === "string" ? { path: releaseFile } : releaseFile;
      const releaseLabel = canonicalRepoRelativePath(repoRoot, descriptor?.path);
      if (releaseLabel && (extensionOwnedLicensePaths.has(releaseLabel) || releaseLabel === haloLicensePath)) {
        addFinding(findings, "host-license-release-file-misclassified", `${extension.id} host license release evidence must not reuse Halo extension license files`);
      }
      const releasePath = resolveRepoPath(repoRoot, descriptor?.path);
      if (!releasePath || !isRegularFile(releasePath)) {
        addFinding(findings, "host-license-release-file-missing", `${extension.id} host license release evidence file is missing: ${safePathLabel(descriptor?.path)}`);
      } else {
        checkFileFingerprint(releasePath, descriptor, "host-license-release", `${extension.id} host license release evidence ${safePathLabel(descriptor.path)}`, findings);
        checkRequiredTextClaims(releasePath, descriptor?.requiredText, "host-license-release", `${extension.id} host license release evidence ${safePathLabel(descriptor.path)}`, findings);
      }
    }
  }
  const closure = host?.dependencyClosure;
  if (!closure || closure.status !== "complete" || !Array.isArray(closure.direct) || !Array.isArray(closure.transitive)) {
    addFinding(
      findings,
      "host-dependency-closure-incomplete",
      `${extension.id} host package direct/transitive dependency closure is not complete evidence; do not infer it from the package name`,
    );
  } else {
    const entries = [...closure.direct, ...closure.transitive];
    if (entries.length === 0) {
      addFinding(findings, "host-dependency-closure-empty", `${extension.id} host package dependency closure must enumerate direct and transitive entries`);
    }
    const validEntries = entries.filter((entry) => entry && typeof entry === "object" && ["name", "version", "source", "license"].every(
      (key) => typeof entry[key] === "string" && entry[key].trim() !== "",
    ));
    if (validEntries.length !== entries.length) {
      addFinding(findings, "host-dependency-entry-invalid", `${extension.id} host dependency closure entries must include name, version, source, and license`);
    }
    const entryNames = new Set();
    for (const entry of validEntries) {
      if (entryNames.has(entry.name)) {
        addFinding(findings, "host-dependency-entry-duplicate", `${extension.id} host dependency closure must list each package exactly once`, {
          name: entry.name,
        });
      }
      entryNames.add(entry.name);
    }
    const closureEvidencePath = resolveRepoPath(repoRoot, closure.evidencePath);
    if (!closureEvidencePath || !isRegularFile(closureEvidencePath)) {
      addFinding(findings, "host-dependency-closure-evidence-missing", `${extension.id} host dependency closure must point to a repository-local evidence file`);
    } else {
      if (!SHA256_PATTERN.test(closure.evidenceSha256 ?? "") || !Number.isInteger(closure.evidenceSize) || closure.evidenceSize < 0) {
        addFinding(findings, "host-dependency-closure-fingerprint-missing", `${extension.id} host dependency closure evidence must declare a SHA-256 and byte size`);
      } else {
        checkFileFingerprint(closureEvidencePath, {
          sha256: closure.evidenceSha256,
          size: closure.evidenceSize,
        }, "host-dependency-closure", `${extension.id} host dependency closure`, findings);
      }
      const closureContents = readFileSync(closureEvidencePath, "utf8");
      for (const entry of validEntries) {
        if (![entry.name, entry.version, entry.source, entry.license].every((claim) => closureContents.includes(claim))) {
          addFinding(findings, "host-dependency-closure-evidence-mismatch", `${extension.id} host dependency closure evidence does not contain an exact entry claim`);
        }
      }
    }
  }
}

function checkExtensionSource(extension, sourceContents, findings) {
  if (!/^\s*import\s+type\b[^;]+from\s+["']@earendil-works\/pi-coding-agent["'];/m.test(sourceContents)) {
    addFinding(findings, "runtime-import-not-type-only", `${extension.id} must import the Pi API as a type-only import`);
  }

  const staticImports = [
    ...sourceContents.matchAll(/^\s*import\s+(?!type\b).*?from\s+["']([^"']+)["']/gm),
    ...sourceContents.matchAll(/^\s*import\s+(?!type\b)["']([^"']+)["'];?/gm),
    ...sourceContents.matchAll(/^\s*export\s+(?:\*\s+as\s+[$_\p{ID_Start}][$_\p{ID_Continue}]*|\*|\{[^}]*\})\s+from\s+["']([^"']+)["']/gmu),
  ].map((match) => match[1]);
  const dynamicImports = [];
  const unresolvedDynamicImports = [];
  for (const match of sourceContents.matchAll(/\b(?:require|import)\s*\(([^)]*)\)/g)) {
    const argument = match[1].trim();
    const literal = argument.match(/^(['"`])([\s\S]*)\1$/);
    if (!literal || (literal[1] === "`" && literal[2].includes("${"))) {
      unresolvedDynamicImports.push("dynamic import/require");
      continue;
    }
    dynamicImports.push(literal[2]);
  }
  const externalImports = [...new Set([...staticImports, ...dynamicImports])]
    .filter((specifier) => !specifier.startsWith("."));
  if (externalImports.length > 0) {
    addFinding(findings, "extension-runtime-import-present", `${extension.id} declares runtime package imports; extension runtime dependencies are not admitted`, {
      imports: externalImports,
    });
  }
  if (unresolvedDynamicImports.length > 0) {
    addFinding(findings, "extension-runtime-import-unresolved", `${extension.id} uses a computed or unresolved runtime import; extension dependencies cannot be proven`, {
      count: unresolvedDynamicImports.length,
    });
  }

  const forbiddenExtensionPatterns = [
    /from\s+["']node:(?:fs|fs\/promises|child_process|net|http|https|os)["']/i,
    /\b(?:require|import)\s*\(\s*["'](?:node:)?(?:fs|fs\/promises|child_process|net|http|https|os)["']\s*\)/i,
    DIRECT_GLOBAL_CAPABILITY_PATTERN,
    /\b(?:globalThis\.)?fetch\s*\(/i,
    COMPUTED_GLOBAL_CAPABILITY_PATTERN,
    /\b(?:fetch|exec|spawn|fork|readFile|writeFile|mkdir|unlink|rm|createReadStream|createWriteStream)\s*\(/,
    /\b(?:process\.env|credential|apiKey|authorization)\b/i,
  ];
  for (const pattern of forbiddenExtensionPatterns) {
    if (pattern.test(sourceContents)) {
      addFinding(findings, "extension-host-capability", `${extension.id} uses a forbidden direct host capability`, {
        pattern: String(pattern),
      });
    }
  }
  if (hasAliasedGlobalCapability(sourceContents)) {
    addFinding(findings, "extension-host-capability", `${extension.id} uses a forbidden aliased computed global capability`, {
      pattern: "aliased computed global/window property access",
    });
  }
}

function checkExtensionContractMetadata(extension, findings) {
  const capabilities = extension.capabilities;
  const requiredArrays = ["tools", "events", "ui", "cleanup"];
  if (!capabilities || typeof capabilities !== "object") {
    addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must declare structured capabilities`);
  } else {
    for (const key of requiredArrays) {
      if (!Array.isArray(capabilities[key])) {
        addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id}.capabilities.${key} must be an array`);
      }
    }
    if (Array.isArray(capabilities.events) && !capabilities.events.includes("tool_call")) {
      addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must declare the tool_call pre-execution gate`);
    }
    for (const uiCapability of ["ctx.ui.confirm", "extension_ui_request", "extension_ui_response"]) {
      if (Array.isArray(capabilities.ui) && !capabilities.ui.includes(uiCapability)) {
        addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must declare ${uiCapability}`);
      }
    }
    for (const cleanupEvent of ["stop", "abort", "eof", "failure", "application-exit"]) {
      if (Array.isArray(capabilities.cleanup) && !capabilities.cleanup.includes(cleanupEvent)) {
        addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must declare cleanup for ${cleanupEvent}`);
      }
    }
    if (Array.isArray(capabilities.tools) && capabilities.tools.length > 0) {
      addFinding(findings, "extension-custom-tool-present", `${extension.id} must not declare custom tools on the Halo P0 seam`);
    }
  }

  const impact = extension.impact;
  if (!impact || typeof impact !== "object" || ["files", "network", "process", "credentials", "git", "renderer"].some(
    (key) => typeof impact[key] !== "string" || impact[key].trim() === "",
  )) {
    addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must declare all host-impact fields`);
  }
  const hostPermissions = typeof extension.hostPermissions === "string" ? extension.hostPermissions.trim() : "";
  const hasInheritedPiPermissions = /\binherits\s+(?:(?:the\s+)?launching\s+user(?:'s)?|pi\s+process)\b/i.test(hostPermissions);
  const hasNoSandboxClaim = /\b(?:not|no)\s+(?:a\s+)?sandbox\b/i.test(hostPermissions);
  if (!hasInheritedPiPermissions || !hasNoSandboxClaim) {
    addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must describe inherited host permissions and sandbox status`);
    if (hostPermissions !== "") {
      addFinding(findings, "extension-host-permission-claim-invalid", `${extension.id} must state that it inherits Pi process permissions and is not a sandbox`);
    }
  }
  if (typeof extension.load?.pathPolicy !== "string" || extension.load.pathPolicy.trim() === "") {
    addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must declare its adapter-owned extension path policy`);
  } else if (!["adapter-owned", "temporary", "embedded", "hash-verified", "--extension"].every(
    (fragment) => extension.load.pathPolicy.toLowerCase().includes(fragment),
  )) {
    addFinding(findings, "extension-path-policy-claim-invalid", `${extension.id} path policy must bind an adapter-owned temporary copy of embedded, hash-verified source to --extension`);
  }
}

function extractRustFunctionBody(sourceContents, functionName) {
  const signature = new RegExp("(?:async\\s+)?fn\\s+" + regexEscape(functionName) + "\\s*\\(", "m");
  const match = signature.exec(sourceContents);
  if (!match) return null;
  const openingBrace = sourceContents.indexOf("{", match.index + match[0].length);
  if (openingBrace < 0) return null;
  let depth = 0;
  let quote = null;
  let escaped = false;
  let lineComment = false;
  let blockComment = false;
  for (let index = openingBrace; index < sourceContents.length; index += 1) {
    const character = sourceContents[index];
    const next = sourceContents[index + 1];
    if (lineComment) {
      if (character === "\n") lineComment = false;
      continue;
    }
    if (blockComment) {
      if (character === "*" && next === "/") {
        blockComment = false;
        index += 1;
      }
      continue;
    }
    if (quote) {
      if (escaped) {
        escaped = false;
      } else if (character === "\\") {
        escaped = true;
      } else if (character === quote) {
        quote = null;
      }
      continue;
    }
    if (character === "/" && next === "/") {
      lineComment = true;
      index += 1;
      continue;
    }
    if (character === "/" && next === "*") {
      blockComment = true;
      index += 1;
      continue;
    }
    if (character === "\"" || character === "'") {
      quote = character;
      continue;
    }
    if (character === "{") depth += 1;
    if (character === "}") {
      depth -= 1;
      if (depth === 0) return sourceContents.slice(openingBrace + 1, index);
    }
  }
  return null;
}

function checkLoadBoundary(extension, adapterSource, findings) {
  const load = extension.load;
  if (!Array.isArray(load?.arguments)) {
    addFinding(findings, "load-arguments-missing", `${extension.id} has no explicit load arguments`);
  } else {
    const extensionIndices = load.arguments.flatMap((argument, index) => argument === "--extension" ? [index] : []);
    const noExtensionIndices = load.arguments.flatMap((argument, index) => argument === "--no-extensions" ? [index] : []);
    const hasExtensionAlias = load.arguments.some((argument) => argument === "-e" || String(argument).startsWith("--extension="));
    const expectedPath = `<adapter-owned-temp>/${extension.id}-<sha256>.ts`;
    if (extensionIndices.length !== 1 || noExtensionIndices.length !== 1 || hasExtensionAlias) {
      addFinding(findings, "extension-argument-shape-invalid", `${extension.id} must have exactly one --no-extensions and one --extension argument`, {
        arguments: load.arguments,
      });
    }
    const extensionIndex = extensionIndices[0] ?? -1;
    if (noExtensionIndices.length !== 1) {
      addFinding(findings, "no-extensions-flag-missing", `${extension.id} must disable Pi extension discovery`);
    }
    if (extensionIndex < 0 || load.arguments[extensionIndex + 1] !== expectedPath) {
      addFinding(findings, "unreviewed-extension-path", `${extension.id} --extension must use its adapter-owned hashed path`, {
        expectedPath,
        arguments: load.arguments,
      });
    }
  }
  if (load?.noExtensions !== true) addFinding(findings, "no-extensions-policy-missing", `${extension.id} does not declare no-extensions policy`);
  if (load?.projectAutoDiscovery !== false) addFinding(findings, "project-extension-discovery-enabled", `${extension.id} permits project .pi extension discovery`);
  if (load?.userAutoDiscovery !== false) addFinding(findings, "user-extension-discovery-enabled", `${extension.id} permits user-global extension discovery`);
  if (load?.runtimeDownload !== false) addFinding(findings, "runtime-download-policy-missing", `${extension.id} does not prohibit runtime downloads`);
  if (load?.inlineBuiltInsPolicy !== "audited-host-built-ins") addFinding(findings, "built-in-extension-policy-missing", `${extension.id} does not declare how Pi inline built-in extensions are audited`);

  for (const token of [
    'include_str!("halo_permission_gate.ts")',
    '"--no-extensions"',
    '"--extension"',
    "install_first_party_extension",
    "HALO_PI_EXTENSION_VERSION",
  ]) {
    if (!adapterSource.includes(token)) {
      addFinding(findings, "adapter-load-boundary-missing", `PiRpcAdapter is missing required extension boundary token: ${token}`);
    }
  }

  const hasEmbeddedInstall = /fn\s+install_first_party_extension[\s\S]*?install_embedded_extension\s*\(/.test(adapterSource);
  const hasHashBoundEmbeddedSource = /fn\s+install_embedded_extension[\s\S]*?stable_digest\s*\(\s*HALO_PERMISSION_EXTENSION_SOURCE\s*\)/.test(adapterSource)
    && /fn\s+install_embedded_extension[\s\S]*?HALO_PI_EXTENSION_ID/.test(adapterSource);
  const hasHashBoundRuntimePath = /fn\s+pi_rpc_args[\s\S]*?--extension[\s\S]*?extension_path\.to_string_lossy\s*\(\)/.test(adapterSource);
  const spawnSessionBody = extractRustFunctionBody(adapterSource, "spawn_session_process");
  const createSessionBody = extractRustFunctionBody(adapterSource, "create_session");
  const hasRuntimePathFlow = Boolean(spawnSessionBody
    && /let\s+extension_path\s*=\s*extension\.as_ref\(\)\.map\(\|extension\|\s*extension\.path\.as_path\(\)\)/.test(spawnSessionBody)
    && /pi_rpc_args\s*\(\s*extension_path\s*,/.test(spawnSessionBody)
    && createSessionBody
    && /self\.install_first_party_extension\(\)\?/.test(createSessionBody)
    && /Some\(extension\)/.test(createSessionBody));
  if (!hasEmbeddedInstall || !hasHashBoundEmbeddedSource) {
    addFinding(findings, "adapter-extension-source-not-hash-bound", `${extension.id} adapter does not prove that the embedded source is copied under its fixed digest`);
  }
  if (!hasHashBoundRuntimePath || !hasRuntimePathFlow) {
    addFinding(findings, "adapter-runtime-load-path-unproven", `${extension.id} adapter does not prove that pi_rpc_args receives the adapter-owned hashed extension path`);
  }
}

function checkSourceProvenance(repoRoot, extension, findings) {
  if (!COMMIT_PATTERN.test(extension.sourceCommit ?? "")) return;
  const declaredTree = gitTreeAtCommit(repoRoot, extension.sourceCommit);
  if (!declaredTree) {
    addFinding(findings, "source-commit-tree-unavailable", `${extension.id} source commit tree is not available in the Halo Git object database`, {
      commit: extension.sourceCommit,
    });
  } else if (declaredTree.toLowerCase() !== String(extension.sourceTree).toLowerCase()) {
    addFinding(findings, "source-commit-tree-mismatch", `${extension.id} sourceCommit does not contain the declared Git tree`, {
      commit: extension.sourceCommit,
      expectedTree: extension.sourceTree,
      actualTree: declaredTree,
    });
  }
  const declaredBlob = gitBlobAtCommit(repoRoot, extension.sourceCommit, extension.sourcePath);
  if (!declaredBlob) {
    addFinding(findings, "source-commit-provenance-unavailable", `${extension.id} source commit or path is not available in the Halo Git object database`, {
      commit: extension.sourceCommit,
      path: extension.sourcePath,
    });
  } else if (declaredBlob.toLowerCase() !== String(extension.gitHashObject).toLowerCase()) {
    addFinding(findings, "source-commit-blob-mismatch", `${extension.id} sourceCommit does not contain the declared Git blob`, {
      commit: extension.sourceCommit,
      expectedBlob: extension.gitHashObject,
      actualBlob: declaredBlob,
    });
  }
}

function checkAdapterVersion(extension, adapterSource, findings) {
  const match = adapterSource.match(/HALO_PI_EXTENSION_VERSION\s*:\s*&str\s*=\s*"([^"]+)"/);
  if (match && match[1] !== extension.fixedVersion) {
    addFinding(findings, "extension-version-adapter-mismatch", `${extension.id} inventory version does not match the adapter's fixed extension identity`, {
      inventory: extension.fixedVersion,
      adapter: match[1],
    });
  }
}

function checkRuntimeScan(repoRoot, runtime, findings) {
  if (!Array.isArray(runtime?.scanPaths) || runtime.scanPaths.length === 0) {
    addFinding(findings, "runtime-scan-paths-missing", "The inventory does not declare reproducible runtime/build scan paths");
    return;
  }

  const files = new Set();
  for (const relativePath of runtime.scanPaths) {
    for (const file of collectTextFiles(repoRoot, relativePath, findings)) files.add(file);
  }

  const forbiddenAbsolutePath = /(?:\b[A-Z]:[\\/]|(?:^|["'`=(,\s])(?:\\\\|\\)[A-Za-z0-9._-]+(?:\s+[A-Za-z0-9._-]+)*[\\/]|(?:^|["'`=(,\s])\/(?:[A-Za-z0-9_.-]+\/)+)/im;
  const downloadCapability = [
    /\b(?:exec|execFile|execFileSync|spawn|spawnSync|fork)\s*\(\s*["'`](?:npm|pnpm|yarn|bun|npx|git|curl|wget)(?:\.(?:cmd|exe))?["'`]/i,
    /\bCommand::new\(\s*["'](?:npm|pnpm|yarn|bun|npx|git|curl|wget)(?:\.(?:cmd|exe))?["']/i,
    /(?:^|[\r\n;&|])\s*(?:(?:sudo|command|env)\s+)*(?:npm|pnpm|yarn|bun)(?:\.(?:cmd|exe))?\s+(?:i|install|ci|add|exec|dlx)(?=\s|$)/im,
    /(?:^|[\r\n;&|])\s*(?:(?:sudo|command|env)\s+)*(?:npx|bunx)(?:\.(?:cmd|exe))?\s+\S+/im,
    /(?:^|[\r\n;&|])\s*(?:(?:sudo|command|env)\s+)*(?:git|git\.exe)\s+(?:(?:(?:-C|--git-dir)\s+\S+|--no-pager)\s+)*(?:clone|fetch|pull|remote\s+update|submodule\s+update|lfs\s+(?:fetch|pull))\b/im,
    /(?:^|[\r\n;&|])\s*(?:(?:sudo|command|env)\s+)*(?:curl|wget)(?:\.exe)?\b[^\r\n]*?\bhttps?:\/\/\S+/im,
    /(?:^|[\r\n;&|{}])\s*(?:Invoke-WebRequest|Invoke-RestMethod|iwr|irm|Start-BitsTransfer)\b/im,
    /\b(?:WebClient\.(?:DownloadFile|DownloadString|DownloadData)|DownloadFile\s*\()\b/i,
  ];
  const networkCapability = [
    /\b(?:globalThis|window)\.fetch\s*\(/i,
    /\b(?:globalThis|window)\s*\[\s*["']fetch["']\s*\]\s*\(/i,
    COMPUTED_GLOBAL_CAPABILITY_PATTERN,
    /\bfetch\s*\(/i,
    /\bhttps?\.(?:get|request)\s*\(/i,
  ];
  const projectDiscovery = /(?:\.pi[\\/]extensions|\.pi[\\/]packages|PI_CODING_AGENT_DIR\s*.*(?:workspace|project))/i;
  const isAllowlisted = (relativePath, code, file) => (runtime.allowlistedFindings ?? []).some(
    (entry) => entry?.path === relativePath
      && entry?.code === code
      && !["runtime-download-capability", "runtime-network-capability"].includes(code)
      && typeof entry.reason === "string"
      && entry.reason.trim() !== ""
      && SHA256_PATTERN.test(entry.sha256 ?? "")
      && hashFile(file, "sha256") === entry.sha256.toLowerCase(),
  );

  for (const entry of runtime.allowlistedFindings ?? []) {
    if (["runtime-download-capability", "runtime-network-capability"].includes(entry?.code)) {
      addFinding(findings, "runtime-capability-allowlist-forbidden", "Runtime download and network capabilities cannot be allowlisted; remove the capability or keep the release gate blocked");
    }
    if (!entry?.path || !entry?.code || !entry?.reason || isAbsolutePath(entry.path)) {
      addFinding(findings, "runtime-allowlist-invalid", "Runtime scan allowlist entries require a relative path, code, and reason");
      continue;
    }
    const allowlistedPath = resolveRepoPath(repoRoot, entry.path);
    if (!SHA256_PATTERN.test(entry.sha256 ?? "") || !allowlistedPath || !isRegularFile(allowlistedPath)) {
      addFinding(findings, "runtime-allowlist-hash-missing", `Runtime scan allowlist entry must bind an existing file to a SHA-256: ${entry.path}`);
    } else if (hashFile(allowlistedPath, "sha256") !== entry.sha256.toLowerCase()) {
      addFinding(findings, "runtime-allowlist-stale", `Runtime scan allowlist file hash is stale: ${entry.path}`);
    }
  }

  for (const file of files) {
    const contents = readFileSync(file, "utf8");
    const relativePath = normalizeRelativePath(path.relative(repoRoot, file));
    if (forbiddenAbsolutePath.test(contents) && !isAllowlisted(relativePath, "forbidden-absolute-path", file)) {
      addFinding(findings, "forbidden-absolute-path", `Forbidden external absolute path in runtime/build input: ${relativePath}`);
    }
    if (downloadCapability.some((pattern) => pattern.test(contents)) && !isAllowlisted(relativePath, "runtime-download-capability", file)) {
      addFinding(findings, "runtime-download-capability", `Runtime/build input contains a download or package installation capability: ${relativePath}`);
    }
    if ((networkCapability.some((pattern) => pattern.test(contents)) || hasAliasedGlobalCapability(contents))
      && !isAllowlisted(relativePath, "runtime-network-capability", file)) {
      addFinding(findings, "runtime-network-capability", `Runtime/build input contains a network capability: ${relativePath}`);
    }
    if (projectDiscovery.test(contents)) {
      addFinding(findings, "project-extension-discovery-capability", `Runtime/build input can discover project/user Pi extensions: ${relativePath}`);
    }
  }
}

function checkBuiltInExtensionBoundary(runtime, findings) {
  if (!Array.isArray(runtime?.builtInExtensions)) {
    addFinding(findings, "built-in-extension-audit-missing", "The inventory must record Pi inline built-in extensions separately from Halo first-party extensions");
    return;
  }
  for (const builtIn of runtime.builtInExtensions) {
    if (typeof builtIn?.id !== "string" || builtIn.id.trim() === "") {
      addFinding(findings, "built-in-extension-inventory-incomplete", "Every Pi built-in extension record requires an id");
    }
    if (builtIn?.releaseEligible !== false) {
      addFinding(findings, "built-in-extension-release-eligible", `Pi built-in extension ${builtIn?.id ?? "<unknown>"} is not explicitly excluded from the Halo release gate`);
    }
    if (!COMMIT_PATTERN.test(builtIn?.sourceCommit ?? "") && !isFixedVersionTag(builtIn?.sourceTag, builtIn?.version)) {
      addFinding(findings, "built-in-extension-provenance-missing", `Pi built-in extension ${builtIn?.id ?? "<unknown>"} has no exact source commit or tag`);
    }
    for (const key of ["tools", "events", "network", "files", "credentials", "process"]) {
      if (builtIn?.capabilities?.[key] === undefined) addFinding(findings, "built-in-extension-capability-inventory-incomplete", `Pi built-in extension ${builtIn?.id ?? "<unknown>"} is missing capability field ${key}`);
    }
  }
}

function checkExternalEvidencePathRole(label, value, role, findings) {
  if (typeof value !== "string" || !isExternalEvidenceReference(value)) return;
  if (role !== "read-only-evidence") {
    addFinding(findings, "external-path-role-invalid", `${label} is an external evidence reference but is not explicitly marked read-only-evidence`);
  }
}

function checkManifestPathRoles(manifest, findings) {
  for (const builtIn of manifest.runtime?.builtInExtensions ?? []) {
    checkExternalEvidencePathRole(
      `Pi built-in extension ${builtIn?.id ?? "<unknown>"}.sourcePath`,
      builtIn?.sourcePath,
      builtIn?.sourcePathRole,
      findings,
    );
  }
  for (const extension of manifest.extensions ?? []) {
    const host = extension?.dependencies?.host;
    checkExternalEvidencePathRole(
      `${extension?.id ?? "<unknown>"}.dependencies.host.sourcePath`,
      host?.sourcePath,
      host?.sourcePathRole,
      findings,
    );
    checkExternalEvidencePathRole(
      `${extension?.id ?? "<unknown>"}.dependencies.host.licenseEvidence.evidencePath`,
      host?.licenseEvidence?.evidencePath,
      host?.licenseEvidence?.evidencePathRole,
      findings,
    );
  }
}

function checkManifestScope(manifest, findings) {
  if (manifest.scope?.runtimeAdapter !== EXPECTED_RUNTIME_ADAPTER_PATH
    || manifest.runtime?.adapterPath !== EXPECTED_RUNTIME_ADAPTER_PATH) {
    addFinding(findings, "runtime-adapter-scope-mismatch", "Inventory scope and runtime adapter must bind the fixed Halo PiRpcAdapter source path");
  }
  if (manifest.scope?.auditScript !== EXPECTED_AUDIT_SCRIPT_PATH) {
    addFinding(findings, "audit-script-scope-mismatch", "Inventory scope must bind the fixed Pi extension audit script path");
  }
  if (!Array.isArray(manifest.runtime?.scanPaths) || !manifest.runtime.scanPaths.includes(EXPECTED_RUNTIME_ADAPTER_PATH)) {
    addFinding(findings, "runtime-scan-scope-missing", "Runtime scan paths must include the fixed Halo PiRpcAdapter source path");
  }
}

function checkConflictDecisions(candidateEvidence, findings) {
  const decisions = candidateEvidence?.conflictsAndDecisions;
  if (!decisions || typeof decisions !== "object") {
    addFinding(findings, "upstream-conflict-decisions-missing", "Upstream candidate evidence must record conflict and retention decisions");
    return;
  }
  if (decisions.mergeAttempted !== false || decisions.automaticMerge !== false) {
    addFinding(findings, "upstream-merge-policy-invalid", "Upstream candidate evidence must prove that no merge or automatic conflict resolution was attempted");
  }
  if (typeof decisions.conflictMarkers !== "string" || decisions.conflictMarkers.trim() === "") {
    addFinding(findings, "upstream-conflict-record-missing", "Upstream candidate evidence must record conflict-marker status");
  }
  if (!Array.isArray(decisions.manualReviewRequired) || decisions.manualReviewRequired.length === 0) {
    addFinding(findings, "upstream-manual-review-missing", "Upstream candidate evidence must list unresolved manual review decisions");
  }
  const retainedScopes = [
    ["retainedHaloBrand", /halo|product[\\/]halo/i],
    ["retainedWorkbenchRuntime", /workbench|runtime/i],
    ["retainedPiRpcPort", /pi.?rpc|rpc.?port|adapter/i],
  ];
  for (const [field, marker] of retainedScopes) {
    const values = decisions[field];
    if (!Array.isArray(values) || values.length === 0 || !values.some((value) => typeof value === "string" && marker.test(value))) {
      addFinding(findings, "upstream-retention-decision-missing", `Upstream candidate evidence must record a retained ${field} decision`);
    }
  }
  if (!Array.isArray(decisions.prohibitedActions) || decisions.prohibitedActions.length === 0) {
    addFinding(findings, "upstream-prohibited-actions-missing", "Upstream candidate evidence must record prohibited merge, upstream-write, and source-copy actions");
    return;
  }
  const prohibitedActionCategories = [
    ["merge", /\b(?:merge|cherry[- ]pick)\b/i],
    ["upstream-write", /\b(?:commit|push|fetch|pull|write)\b/i],
    ["source-copy", /\b(?:copy|copied|clone|cloned)\b/i],
  ];
  const prohibitedActions = decisions.prohibitedActions.filter((value) => typeof value === "string");
  for (const [category, marker] of prohibitedActionCategories) {
    if (!prohibitedActions.some((value) => marker.test(value))) {
      addFinding(findings, "upstream-prohibited-action-detail-missing", `Upstream candidate evidence must enumerate the prohibited ${category} action`, { category });
    }
  }
}

function resolveReferenceRoot(candidate) {
  const locator = candidate?.referenceRoot;
  if (isAbsolutePath(locator)) return path.resolve(locator);
  if (!READ_ONLY_EVIDENCE_REFERENCE_PATTERN.test(String(locator))) return null;
  const environmentVariable = typeof candidate?.referenceRootEnvironmentVariable === "string"
    ? candidate.referenceRootEnvironmentVariable
    : "HALO_HALO_REFERENCE_ROOT";
  const configuredRoot = process.env[environmentVariable];
  return isAbsolutePath(configuredRoot) ? path.resolve(configuredRoot) : null;
}

function parseGitTreeEntries(output) {
  const entries = new Map();
  for (const record of output.split("\0")) {
    if (!record) continue;
    const separator = record.indexOf("\t");
    if (separator < 0) continue;
    const [mode, type, blob, size] = record.slice(0, separator).trim().split(/\s+/);
    const relativePath = normalizeRelativePath(record.slice(separator + 1));
    entries.set(relativePath, { mode, type, blob, size: Number(size) });
  }
  return entries;
}

function gitTreeOidFromEntries(entries) {
  const root = { files: [], directories: new Map() };
  for (const entry of entries ?? []) {
    if (!entry || typeof entry.path !== "string" || !GIT_OBJECT_PATTERN.test(entry.sha ?? entry.blob ?? "")) return null;
    const relativePath = normalizeRelativePath(entry.path);
    const parts = relativePath.split("/");
    if (parts.some((part) => part === "" || part === "." || part === "..")) return null;
    let directory = root;
    for (const part of parts.slice(0, -1)) {
      if (!directory.directories.has(part)) directory.directories.set(part, { files: [], directories: new Map() });
      directory = directory.directories.get(part);
    }
    directory.files.push({
      name: parts.at(-1),
      mode: String(entry.mode),
      type: entry.type,
      oid: String(entry.sha ?? entry.blob),
    });
  }

  const serialize = (directory) => {
    const children = [...directory.directories.entries()].map(([name, child]) => ({
      name,
      mode: "40000",
      type: "tree",
      oid: serialize(child),
    }));
    const treeEntries = [...children, ...directory.files].sort((left, right) => {
      const leftName = Buffer.from(`${left.name}${left.type === "tree" ? "/" : ""}`, "utf8");
      const rightName = Buffer.from(`${right.name}${right.type === "tree" ? "/" : ""}`, "utf8");
      return Buffer.compare(leftName, rightName);
    });
    const body = Buffer.concat(treeEntries.map((entry) => Buffer.concat([
      Buffer.from(`${entry.mode} ${entry.name}\0`, "utf8"),
      Buffer.from(entry.oid, "hex"),
    ])));
    return createHash("sha1").update(Buffer.concat([
      Buffer.from(`tree ${body.length}\0`, "utf8"),
      body,
    ])).digest("hex");
  };

  return serialize(root);
}

function compareTreeEntries(baseEntries, candidateEntries) {
  const paths = new Set([...baseEntries.keys(), ...candidateEntries.keys()]);
  const changed = [];
  let identical = 0;
  let modified = 0;
  let added = 0;
  let removed = 0;
  for (const relativePath of [...paths].sort()) {
    const base = baseEntries.get(relativePath) ?? null;
    const candidate = candidateEntries.get(relativePath) ?? null;
    if (!base) {
      added += 1;
      changed.push({ path: relativePath, status: "added", base: null, candidate });
    } else if (!candidate) {
      removed += 1;
      changed.push({ path: relativePath, status: "removed", base, candidate: null });
    } else if (base.blob !== candidate.blob || base.mode !== candidate.mode) {
      modified += 1;
      changed.push({ path: relativePath, status: "modified", base, candidate });
    } else {
      identical += 1;
    }
  }
  return { identical, modified, added, removed, changed, changedEntries: changed.length };
}

function checkInitialImportManifest(initialManifest, expectedCommit, findings) {
  if (initialManifest?.schema_version !== 1) {
    addFinding(findings, "upstream-initial-import-schema-invalid", "Initial import manifest must declare schema_version 1");
  }
  if (!COMMIT_PATTERN.test(initialManifest?.upstream?.commit ?? "")) {
    addFinding(findings, "upstream-initial-import-commit-invalid", "Initial import manifest must declare a full upstream commit SHA");
  }
  const seenPaths = new Set();
  for (const entry of initialManifest?.entries ?? []) {
    const normalizedPath = typeof entry?.path === "string" ? normalizeRelativePath(entry.path) : null;
    const pathParts = normalizedPath?.split("/") ?? [];
    const validPath = normalizedPath !== null
      && normalizedPath.trim() !== ""
      && !isAbsolutePath(normalizedPath)
      && pathParts.every((part) => part !== "" && part !== "." && part !== "..");
    const validEntry = entry && typeof entry === "object"
      && validPath
      && /^(?:100644|100755|120000)$/.test(String(entry.mode))
      && entry.type === "blob"
      && GIT_OBJECT_PATTERN.test(entry.sha ?? entry.blob ?? "")
      && Number.isInteger(entry.size)
      && entry.size >= 0;
    if (!validEntry) {
      addFinding(findings, "upstream-initial-import-entry-invalid", "Initial import manifest contains an invalid path, mode, blob, type, or size", {
        path: safePathLabel(entry?.path),
      });
      continue;
    }
    if (seenPaths.has(normalizedPath)) {
      addFinding(findings, "upstream-initial-import-entry-duplicate", "Initial import manifest must contain each Git path exactly once", {
        path: safePathLabel(normalizedPath),
      });
    }
    seenPaths.add(normalizedPath);
  }
  if (initialManifest?.upstream?.commit !== expectedCommit) {
    addFinding(findings, "upstream-initial-import-commit-mismatch", "Initial import manifest does not bind the pinned upstream base commit");
  }
}

function hasNonEmptyStringEntries(value) {
  return Array.isArray(value) && value.some((entry) => typeof entry === "string" && entry.trim() !== "");
}

function checkCandidateReleaseGate(candidateEvidence, findings) {
  const releaseGate = candidateEvidence?.releaseGate;
  if (!releaseGate || typeof releaseGate !== "object" || Array.isArray(releaseGate)) {
    addFinding(findings, "upstream-release-gate-missing", "Upstream candidate evidence must declare a structured release gate result");
    return;
  }
  if (!["passed", "blocked"].includes(releaseGate.status)) {
    addFinding(findings, "upstream-release-gate-status-invalid", "Upstream candidate evidence release gate must be explicitly passed or blocked");
  }
  for (const field of ["evidenceGaps", "blockingReasons"]) {
    if (releaseGate[field] !== undefined && !Array.isArray(releaseGate[field])) {
      addFinding(findings, "upstream-release-gate-reasons-invalid", "Upstream candidate release gate " + field + " must be an array");
    }
    if (releaseGate.status === "passed" && !Array.isArray(releaseGate[field])) {
      addFinding(findings, "upstream-release-gate-reasons-missing", "A passing upstream candidate must declare empty evidenceGaps and blockingReasons arrays");
    }
  }
  if (releaseGate.status !== "passed") {
    addFinding(findings, "upstream-candidate-release-gate-blocked", "Upstream candidate evidence is not validated for release");
    if (!hasNonEmptyStringEntries(releaseGate.evidenceGaps) && !hasNonEmptyStringEntries(releaseGate.blockingReasons)) {
      addFinding(findings, "upstream-release-gate-reasons-missing", "A blocked upstream candidate must record evidence gaps or blocking reasons");
    }
    return;
  }
  if (Array.isArray(releaseGate.evidenceGaps) && releaseGate.evidenceGaps.length > 0) {
    addFinding(findings, "upstream-release-gate-gaps-on-passed", "A passing upstream candidate cannot retain evidence gaps", {
      evidenceGaps: releaseGate.evidenceGaps,
    });
  }
  if (Array.isArray(releaseGate.blockingReasons) && releaseGate.blockingReasons.length > 0) {
    addFinding(findings, "upstream-release-gate-reasons-on-passed", "A passing upstream candidate cannot retain blocking reasons", {
      blockingReasons: releaseGate.blockingReasons,
    });
  }
}

function checkUpstreamEvidence(repoRoot, evidence, findings) {
  if (!evidence || typeof evidence !== "object") {
    addFinding(findings, "upstream-evidence-missing", "The inventory does not link an upstream candidate evidence record");
    return;
  }
  if (!COMMIT_PATTERN.test(evidence.baseCommit ?? "")) addFinding(findings, "upstream-base-commit-invalid", "Upstream base commit is not a full commit SHA");
  if (!COMMIT_PATTERN.test(evidence.candidateCommit ?? "")) addFinding(findings, "upstream-candidate-commit-invalid", "Upstream candidate commit is not a full commit SHA");
  if (evidence.baseCommit === evidence.candidateCommit) addFinding(findings, "upstream-candidate-not-different", "Upstream candidate must differ from the initial import commit");
  if (evidence.diffRecorded !== true) addFinding(findings, "upstream-diff-missing", "Upstream candidate diff is not recorded");
  if (evidence.automatedMerge !== false) addFinding(findings, "upstream-merge-attempted", "The audit record must not claim an automatic merge");
  if (evidence.upstreamWrite !== false) addFinding(findings, "upstream-write-attempted", "The audit record must not claim an upstream write or push");

  const evidencePath = resolveRepoPath(repoRoot, evidence.path);
  if (!evidencePath || !existsSync(evidencePath)) {
    addFinding(findings, "upstream-evidence-file-missing", `Upstream candidate evidence file is missing: ${safePathLabel(evidence.path)}`);
    return;
  }
  const candidateEvidence = readJson(evidencePath, findings);
  if (!candidateEvidence || candidateEvidence.base?.commit !== evidence.baseCommit || candidateEvidence.candidate?.commit !== evidence.candidateCommit) {
    addFinding(findings, "upstream-evidence-mismatch", "Inventory upstream commit fields do not match the candidate evidence file");
    return;
  }
  checkCandidateReleaseGate(candidateEvidence, findings);
  checkConflictDecisions(candidateEvidence, findings);

  const candidate = candidateEvidence.candidate;
  checkExternalEvidencePathRole(
    "Upstream candidate referenceRoot",
    candidate?.referenceRoot,
    candidate?.referenceRootRole,
    findings,
  );
  const referenceRoot = resolveReferenceRoot(candidate);
  if (!referenceRoot || !existsSync(referenceRoot) || !statSync(referenceRoot).isDirectory()) {
    addFinding(findings, "upstream-reference-tree-unavailable", "Upstream candidate reference tree is unavailable");
    return;
  }
  if (inside(path.resolve(repoRoot), referenceRoot)) {
    addFinding(findings, "upstream-reference-tree-inside-repo", "Upstream candidate reference tree must remain outside the Halo repository root");
    return;
  }

  const git = (args) => runGit(referenceRoot, ["-c", `safe.directory=${referenceRoot}`, ...args]);
  const resolveGitPath = (gitPath) => {
    const result = git(["rev-parse", "--git-path", gitPath]);
    if (!result.ok || result.stdout.trim() === "") return null;
    return path.resolve(referenceRoot, result.stdout.trim());
  };
  const inspectCommit = (commit) => {
    const resolved = git(["rev-parse", "--verify", `${commit}^{commit}`]);
    const resolvedCommit = resolved.ok ? resolved.stdout.trim() : null;
    const type = resolvedCommit ? git(["cat-file", "-t", resolvedCommit]) : null;
    const objectType = type?.ok ? type.stdout.trim() : null;
    return {
      status: resolved.ok && resolvedCommit === commit && objectType === "commit" ? "resolved" : "unresolved",
      resolvedCommit: resolved.ok && resolvedCommit === commit ? resolvedCommit : null,
      objectType,
      exitCode: resolved.ok ? 0 : resolved.status,
    };
  };
  const checkCommitResolution = (label, commit, recorded) => {
    const actual = inspectCommit(commit);
    if (actual.status !== "resolved") {
      addFinding(findings, `upstream-${label}-commit-unresolved`, `Upstream ${label} commit cannot be resolved as a full Git commit in the read-only reference tree`, {
        commit,
        exitCode: actual.exitCode,
        objectType: actual.objectType,
      });
    }
    if (!recorded || typeof recorded !== "object") {
      addFinding(findings, `upstream-${label}-resolution-missing`, `Upstream ${label} commit resolution evidence is missing`);
    } else {
      for (const key of ["status", "resolvedCommit", "objectType", "exitCode"]) {
        if (recorded[key] !== actual[key]) {
          addFinding(findings, `upstream-${label}-resolution-mismatch`, `Upstream ${label} commit resolution evidence does not match a fresh reference-tree check`, {
            key,
            expected: actual[key],
            recorded: recorded[key],
          });
        }
      }
    }
    return actual;
  };
  const actualCommit = git(["rev-parse", "HEAD"]);
  const actualTree = git(["rev-parse", "HEAD^{tree}"]);
  const actualBranch = git(["branch", "--show-current"]);
  const actualStatus = git(["status", "--porcelain=v2", "--branch"]);
  const shallowState = git(["rev-parse", "--is-shallow-repository"]);
  const replaceState = git(["replace", "-l"]);
  const actualRepositoryState = {
    isShallow: shallowState.ok ? shallowState.stdout.trim() === "true" : null,
    shallowFilePresent: (() => {
      const shallowPath = resolveGitPath("shallow");
      return shallowPath ? existsSync(shallowPath) : null;
    })(),
    graftsFilePresent: (() => {
      const graftsPath = resolveGitPath("info/grafts");
      return graftsPath ? existsSync(graftsPath) : null;
    })(),
    replaceRefs: replaceState.ok ? replaceState.stdout.split(/\r?\n/).map((line) => line.trim()).filter(Boolean).sort() : null,
  };
  if (!shallowState.ok || !replaceState.ok || actualRepositoryState.shallowFilePresent === null || actualRepositoryState.graftsFilePresent === null) {
    addFinding(findings, "upstream-repository-state-unavailable", "The read-only reference tree history-boundary state could not be fully inspected");
  }
  if (actualRepositoryState.isShallow === true || actualRepositoryState.graftsFilePresent === true || (actualRepositoryState.replaceRefs?.length ?? 0) > 0) {
    addFinding(findings, "upstream-history-boundary-untrusted", "The candidate reference tree is shallow, grafted, or has replace refs; ancestry cannot be accepted as complete evidence", {
      repositoryState: actualRepositoryState,
    });
  }
  if (!candidate.repositoryState || typeof candidate.repositoryState !== "object") {
    addFinding(findings, "upstream-repository-state-evidence-missing", "Upstream candidate repository history-boundary evidence is missing");
  } else {
    for (const key of ["isShallow", "shallowFilePresent", "graftsFilePresent", "replaceRefs"]) {
      if (JSON.stringify(candidate.repositoryState[key]) !== JSON.stringify(actualRepositoryState[key])) {
        addFinding(findings, "upstream-repository-state-mismatch", "Upstream candidate repository state evidence does not match a fresh reference-tree check", {
          key,
          expected: actualRepositoryState[key],
          recorded: candidate.repositoryState[key],
        });
      }
    }
  }
  if (!actualCommit.ok || actualCommit.stdout.trim() !== candidate.commit) {
    addFinding(findings, "upstream-candidate-commit-mismatch", "Candidate commit does not match the read-only reference tree HEAD", {
      expected: candidate.commit,
      actual: actualCommit.stdout.trim(),
    });
  }
  if (!actualTree.ok || actualTree.stdout.trim() !== candidate.tree) {
    addFinding(findings, "upstream-candidate-tree-mismatch", "Candidate tree does not match the read-only reference tree", {
      expected: candidate.tree,
      actual: actualTree.stdout.trim(),
    });
  }
  if (candidate.branch && (!actualBranch.ok || actualBranch.stdout.trim() !== candidate.branch)) {
    addFinding(findings, "upstream-candidate-branch-mismatch", "Candidate branch does not match the read-only reference tree", {
      expected: candidate.branch,
      actual: actualBranch.stdout.trim(),
    });
  }
  const actualStatusExitCode = actualStatus.ok ? 0 : actualStatus.status;
  const actualClean = actualStatus.ok
    && !actualStatus.stdout.split(/\r?\n/).some((line) => line.trim() !== "" && !line.startsWith("#"));
  if (!actualClean) {
    addFinding(findings, "upstream-reference-tree-dirty", "The upstream candidate reference tree is not clean");
  }
  const statusEvidence = candidate.statusEvidence;
  if (!statusEvidence || typeof statusEvidence !== "object"
    || typeof statusEvidence.command !== "string"
    || !/status\s+--porcelain=v2\s+--branch/.test(statusEvidence.command)
    || typeof statusEvidence.exitCode !== "number"
    || typeof statusEvidence.clean !== "boolean"
    || typeof statusEvidence.result !== "string"
    || statusEvidence.result.trim() === "") {
    addFinding(findings, "upstream-status-evidence-missing", "Upstream candidate evidence must record the clean-status command, exit code, and clean result");
  } else {
    for (const [key, expected] of [["exitCode", actualStatusExitCode], ["clean", actualClean]]) {
      if (statusEvidence[key] !== expected) {
        addFinding(findings, "upstream-status-evidence-mismatch", "Upstream candidate clean-status evidence does not match a fresh reference-tree check", {
          key,
          expected,
          recorded: statusEvidence[key],
        });
      }
    }
    if (/^clean\b/i.test(statusEvidence.result.trim()) !== actualClean) {
      addFinding(findings, "upstream-status-evidence-result-mismatch", "Upstream candidate clean-status result does not match the fresh clean-status check", {
        expectedClean: actualClean,
        recordedResult: statusEvidence.result,
      });
    }
  }

  const baseResolution = checkCommitResolution("base", candidateEvidence.base?.commit, candidateEvidence.base?.resolution);
  const candidateResolution = checkCommitResolution("candidate", candidate.commit, candidate.resolution);
  if (candidateResolution.status !== "resolved") {
    addFinding(findings, "upstream-candidate-resolution-invalid", "The candidate HEAD is not a resolved full Git commit");
  }
  const ancestryResult = git(["merge-base", "--is-ancestor", candidateEvidence.base?.commit, candidate.commit]);
  const actualAncestry = {
    status: ancestryResult.ok ? "proven" : ancestryResult.status === 1 ? "not_ancestor" : "unproven",
    exitCode: ancestryResult.ok ? 0 : ancestryResult.status,
  };
  if (actualAncestry.status !== "proven") {
    addFinding(findings, "upstream-ancestry-unproven", `Upstream base/candidate ancestry is not proven for an incremental sync`, {
      baseCommit: candidateEvidence.base?.commit,
      candidateCommit: candidate.commit,
      status: actualAncestry.status,
      exitCode: actualAncestry.exitCode,
    });
  }
  if (!candidateEvidence.ancestry || typeof candidateEvidence.ancestry !== "object") {
    addFinding(findings, "upstream-ancestry-evidence-missing", "Upstream candidate ancestry evidence is missing");
  } else {
    for (const key of ["status", "exitCode"]) {
      if (candidateEvidence.ancestry[key] !== actualAncestry[key]) {
        addFinding(findings, "upstream-ancestry-evidence-mismatch", "Upstream candidate ancestry evidence does not match a fresh reference-tree check", {
          key,
          expected: actualAncestry[key],
          recorded: candidateEvidence.ancestry[key],
        });
      }
    }
  }
  if (candidateEvidence.operation !== "incremental_sync") {
    addFinding(findings, "upstream-operation-invalid", "Upstream candidate evidence must identify an incremental_sync operation");
  }

  const parent = git(["rev-parse", "HEAD^"]);
  const parentEvidence = candidate.parentEvidence;
  const parentExitCode = parent.ok ? 0 : parent.status;
  if (!parentEvidence || typeof parentEvidence !== "object"
    || typeof parentEvidence.command !== "string"
    || !/rev-parse\s+HEAD\^/.test(parentEvidence.command)
    || typeof parentEvidence.exitCode !== "number"
    || typeof parentEvidence.result !== "string"
    || parentEvidence.result.trim() === "") {
    addFinding(findings, "upstream-parent-evidence-missing", "Upstream candidate evidence must record the HEAD^ command, exit code, and result");
  } else {
    if (parentEvidence.exitCode !== parentExitCode) {
      addFinding(findings, "upstream-parent-evidence-mismatch", "Candidate parent evidence does not match a fresh local HEAD^ check", {
        expectedExitCode: parentExitCode,
        recordedExitCode: parentEvidence.exitCode,
      });
    }
    const parentResult = parentEvidence.result.trim();
    const resultMatches = parent.ok
      ? parentResult === parent.stdout.trim()
      : /(?:unavailable|unresolved|failed|error|not found|cannot)/i.test(parentResult);
    if (!resultMatches) {
      addFinding(findings, "upstream-parent-evidence-result-mismatch", "Candidate parent evidence result does not match a fresh local HEAD^ check", {
        resolved: parent.ok,
        recordedResult: parentEvidence.result,
      });
    }
  }

  const initialImportManifest = resolveRepoPath(repoRoot, candidateEvidence.base?.initialImportManifest);
  if (!initialImportManifest || !isRegularFile(initialImportManifest)) {
    addFinding(findings, "upstream-initial-import-manifest-missing", `Initial import manifest is missing: ${safePathLabel(candidateEvidence.base?.initialImportManifest)}`);
    return;
  }
  const initialManifest = readJson(initialImportManifest, findings);
  if (!initialManifest || initialManifest.upstream?.commit !== evidence.baseCommit || !Array.isArray(initialManifest.entries)) {
    addFinding(findings, "upstream-initial-import-evidence-invalid", "Initial import manifest does not prove the pinned upstream base and entries");
    return;
  }
  checkInitialImportManifest(initialManifest, evidence.baseCommit, findings);
  const declaredInitialImportTree = candidateEvidence.base?.initialImportTree;
  if (!GIT_OBJECT_PATTERN.test(declaredInitialImportTree ?? "")) {
    addFinding(findings, "upstream-initial-import-tree-invalid", "Initial import evidence must declare a full Git tree object hash");
  } else {
    const computedInitialImportTree = gitTreeOidFromEntries(initialManifest.entries);
    const declaredManifestDerivedTree = candidateEvidence.base?.manifestDerivedTree;
    if (!GIT_OBJECT_PATTERN.test(declaredManifestDerivedTree ?? "")) {
      addFinding(findings, "upstream-manifest-derived-tree-invalid", "Initial import evidence must record the Git tree hash derived from its file manifest");
    } else if (computedInitialImportTree !== declaredManifestDerivedTree) {
      addFinding(findings, "upstream-manifest-derived-tree-mismatch", "Initial import manifest entries do not reproduce the recorded manifest-derived tree object", {
        expected: declaredManifestDerivedTree,
        actual: computedInitialImportTree,
      });
    }
    if (!computedInitialImportTree || computedInitialImportTree !== declaredInitialImportTree) {
      addFinding(findings, "upstream-initial-import-tree-mismatch", "Initial import manifest entries do not reproduce the declared base tree object", {
        expected: declaredInitialImportTree,
        actual: computedInitialImportTree,
      });
    }
    const expectedTreeBindingStatus = computedInitialImportTree === declaredInitialImportTree ? "matched" : "mismatch";
    if (candidateEvidence.base?.treeBindingStatus !== expectedTreeBindingStatus) {
      addFinding(findings, "upstream-tree-binding-status-mismatch", "Upstream tree binding status does not match the fresh manifest/tree comparison", {
        expected: expectedTreeBindingStatus,
        recorded: candidateEvidence.base?.treeBindingStatus,
      });
    }
    if (baseResolution.status === "resolved") {
      const actualBaseTree = git(["rev-parse", `${candidateEvidence.base.commit}^{tree}`]);
      if (!actualBaseTree.ok || actualBaseTree.stdout.trim() !== declaredInitialImportTree) {
        addFinding(findings, "upstream-base-tree-mismatch", "Resolved upstream base commit does not match the initial-import tree evidence", {
          expected: declaredInitialImportTree,
          actual: actualBaseTree.stdout.trim(),
        });
      }
    }
  }

  const treeResult = git(["ls-tree", "-r", "-l", "--full-tree", "-z", "HEAD"]);
  if (!treeResult.ok) {
    addFinding(findings, "upstream-candidate-tree-list-unavailable", "Candidate tree entries could not be read from the reference tree");
    return;
  }
  const baseEntries = new Map(initialManifest.entries.map((entry) => [normalizeRelativePath(entry.path), {
    mode: entry.mode,
    type: entry.type,
    blob: entry.sha ?? entry.blob,
    size: Number(entry.size),
  }]));
  const candidateEntries = parseGitTreeEntries(treeResult.stdout);
  const comparison = compareTreeEntries(baseEntries, candidateEntries);
  const recorded = candidateEvidence.comparison ?? {};
  const canonical = recorded.canonicalUtf8Counts ?? {
    identical: recorded.identicalBlobOrModeEntries,
    modified: recorded.modifiedEntries,
    added: recorded.addedEntries,
    removed: recorded.removedEntries,
    changedEntries: recorded.changedEntries,
  };
  if (!recorded.canonicalUtf8Counts) addFinding(findings, "upstream-diff-canonical-counts-missing", "Upstream comparison must record counts using raw UTF-8 Git paths");
  for (const [key, value] of Object.entries({
    baseEntries: baseEntries.size,
    candidateEntries: candidateEntries.size,
    ...(recorded.canonicalUtf8Counts ? {} : {
      identicalBlobOrModeEntries: comparison.identical,
      modifiedEntries: comparison.modified,
      addedEntries: comparison.added,
      removedEntries: comparison.removed,
      changedEntries: comparison.changedEntries,
    }),
  })) {
    if (recorded[key] !== undefined && recorded[key] !== value) {
      addFinding(findings, "upstream-diff-count-mismatch", `Upstream comparison ${key} does not match a fresh path/blob comparison`, {
        expected: value,
        recorded: recorded[key],
      });
    }
  }
  for (const [key, value] of Object.entries({
    identical: comparison.identical,
    modified: comparison.modified,
    added: comparison.added,
    removed: comparison.removed,
    changedEntries: comparison.changedEntries,
  })) {
    if (canonical[key] !== value) addFinding(findings, "upstream-diff-canonical-count-mismatch", `Upstream canonical comparison ${key} does not match a fresh path/blob comparison`, { expected: value, recorded: canonical[key] });
  }
  let pathRecords = recorded.records;
  if (!Array.isArray(pathRecords) && typeof recorded.recordsPath === "string") {
    const recordsPath = resolveRepoPath(repoRoot, recorded.recordsPath);
    if (recordsPath && isRegularFile(recordsPath)) {
      const recordsArtifact = readJson(recordsPath, findings);
      pathRecords = recordsArtifact?.records;
      if (recordsArtifact?.baseCommit !== evidence.baseCommit || recordsArtifact?.candidateCommit !== evidence.candidateCommit) {
        addFinding(findings, "upstream-diff-records-provenance-mismatch", "Upstream path-level diff records do not carry the candidate commit identity");
      }
    }
  }
  if (!Array.isArray(pathRecords)) {
    addFinding(findings, "upstream-diff-records-missing", "Upstream candidate evidence must include path-level diff records or a repository-local records file");
  } else {
    const recordCounts = { modified: 0, added: 0, removed: 0 };
    const recordMap = new Map();
    const validTreeEntry = (entry) => entry && entry.type === "blob" && /^(?:100644|100755|120000)$/.test(String(entry.mode)) && GIT_OBJECT_PATTERN.test(entry.blob ?? "") && Number.isInteger(entry.size) && entry.size >= 0;
    for (const record of pathRecords) {
      if (!record || typeof record.path !== "string" || !["modified", "added", "removed"].includes(record.status)) {
        addFinding(findings, "upstream-diff-record-invalid", "Upstream path-level diff records contain an invalid path or status");
        continue;
      }
      recordCounts[record.status] += 1;
      recordMap.set(record.path, record.status);
      if (record.status === "added" && record.base !== null) addFinding(findings, "upstream-diff-record-invalid", `Added path has unexpected base entry: ${safePathLabel(record.path)}`);
      if (record.status === "removed" && record.candidate !== null) addFinding(findings, "upstream-diff-record-invalid", `Removed path has unexpected candidate entry: ${safePathLabel(record.path)}`);
      if (record.status === "modified" && (!record.base || !record.candidate)) addFinding(findings, "upstream-diff-record-invalid", `Modified path must contain both entries: ${safePathLabel(record.path)}`);
      if ((record.status === "added" && !validTreeEntry(record.candidate)) || (record.status === "removed" && !validTreeEntry(record.base)) || (record.status === "modified" && (!validTreeEntry(record.base) || !validTreeEntry(record.candidate)))) {
        addFinding(findings, "upstream-diff-record-entry-invalid", `Upstream path record has an invalid mode/blob/size entry: ${safePathLabel(record.path)}`);
      }
      const expectedBase = baseEntries.get(normalizeRelativePath(record.path)) ?? null;
      const expectedCandidate = candidateEntries.get(normalizeRelativePath(record.path)) ?? null;
      const sameTreeEntry = (actual, expected) => actual && expected
        && actual.mode === expected.mode
        && actual.type === expected.type
        && actual.blob === expected.blob
        && actual.size === expected.size;
      const entriesMatch = record.status === "added"
        ? record.base === null && sameTreeEntry(record.candidate, expectedCandidate)
        : record.status === "removed"
          ? record.candidate === null && sameTreeEntry(record.base, expectedBase)
          : sameTreeEntry(record.base, expectedBase) && sameTreeEntry(record.candidate, expectedCandidate);
      if (!entriesMatch) {
        addFinding(findings, "upstream-diff-record-entry-mismatch", `Upstream path record does not match the fresh Git tree: ${safePathLabel(record.path)}`);
      }
    }
    for (const [status, count] of Object.entries({ modified: comparison.modified, added: comparison.added, removed: comparison.removed })) {
      if (recordCounts[status] !== count) addFinding(findings, "upstream-diff-record-count-mismatch", `Upstream ${status} path record count is not reproducible`, { expected: count, recorded: recordCounts[status] });
    }
    const computedMap = new Map(comparison.changed.map((record) => [record.path, record.status]));
    if (computedMap.size !== recordMap.size || [...computedMap].some(([relativePath, status]) => recordMap.get(relativePath) !== status)) {
      addFinding(findings, "upstream-diff-record-content-mismatch", "Upstream path-level records do not match the fresh Git tree comparison");
    }
  }
  const topLevel = {};
  for (const entry of comparison.changed) {
    const top = entry.path.split("/")[0];
    topLevel[top] = (topLevel[top] ?? 0) + 1;
  }
  for (const [top, count] of Object.entries(recorded.topLevelChangedEntries ?? {})) {
    if (topLevel[top] !== count) addFinding(findings, "upstream-top-level-count-mismatch", `Top-level changed count for ${top} is not reproducible`, { expected: topLevel[top] ?? 0, recorded: count });
  }
  for (const paths of Object.values(recorded.representativeChangedPaths ?? {})) {
    for (const relativePath of paths ?? []) {
      if (!comparison.changed.some((entry) => entry.path === normalizeRelativePath(relativePath))) {
        addFinding(findings, "upstream-representative-path-missing", `Representative changed path is not present in the computed diff: ${safePathLabel(relativePath)}`);
      }
    }
  }
}

function checkWorkspaceBoundary(repoRoot, boundary, findings) {
  if (!boundary || typeof boundary !== "object" || Array.isArray(boundary)
    || typeof boundary.workspaceManifest !== "string" || !Array.isArray(boundary.uniqueMembers)
    || boundary.uniqueMembers.length === 0) {
    addFinding(findings, "dependency-boundary-incomplete", "The inventory must declare a non-empty unique Cargo dependency boundary");
    return;
  }
  const declaredMembers = new Set(boundary.uniqueMembers);
  if (declaredMembers.size !== boundary.uniqueMembers.length) {
    addFinding(findings, "dependency-boundary-incomplete", "The inventory dependency boundary must not repeat a workspace member");
  }
  for (const requiredMember of REQUIRED_UNIQUE_WORKSPACE_MEMBERS) {
    if (!declaredMembers.has(requiredMember)) {
      addFinding(findings, "dependency-boundary-incomplete", `The inventory dependency boundary must cover ${requiredMember}`);
    }
  }
  const manifestPath = resolveRepoPath(repoRoot, boundary.workspaceManifest);
  if (!manifestPath || !isRegularFile(manifestPath)) {
    addFinding(findings, "workspace-manifest-missing", `Workspace dependency manifest is missing: ${safePathLabel(boundary.workspaceManifest)}`);
    return;
  }
  const contents = readFileSync(manifestPath, "utf8");
  const membersSection = contents.match(/\[workspace\][\s\S]*?(?=\n\[[^\n]+\]|$)/)?.[0] ?? "";
  for (const member of boundary.uniqueMembers ?? []) {
    const count = [...membersSection.matchAll(new RegExp(`^\\s*"${member.replace(/[.*+?^${}()|[\\]\\]/g, "\\$&")}"\\s*,?\\s*$`, "gm"))].length;
    if (count !== 1) {
      addFinding(findings, "workspace-member-duplicate", `Workspace member ${member} appears ${count} times; dependency boundary is not unique`, {
        manifest: boundary.workspaceManifest,
        member,
        count,
      });
    }
  }
}

function manifestPathInsideRepo(repoRoot, manifestPath) {
  if (typeof repoRoot !== "string" || typeof manifestPath !== "string") return false;
  try {
    const realRoot = realpathSync(path.resolve(repoRoot));
    const absoluteManifestPath = path.resolve(manifestPath);
    const realManifestPath = existsSync(absoluteManifestPath)
      ? realpathSync(absoluteManifestPath)
      : absoluteManifestPath;
    return inside(realRoot, realManifestPath);
  } catch {
    return false;
  }
}


const REHEARSAL_ONLY_SCOPE = "rehearsal-only";
const UPSTREAM_CANDIDATE_EXCLUDED_FINDINGS = new Set([
  "upstream-candidate-release-gate-blocked",
  "upstream-history-boundary-untrusted",
  "upstream-base-commit-unresolved",
  "upstream-ancestry-unproven",
]);
const HOST_EXCLUDED_FINDINGS = new Set([
  "host-license-evidence-not-release",
  "host-dependency-closure-incomplete",
]);

function checkReleasePolicy(manifest, findings) {
  const policy = manifest?.releasePolicy;
  const result = { upstreamRehearsalOnly: false, hostExcluded: false };
  if (policy === undefined || policy === null) {
    addFinding(findings, "release-policy-invalid", "Release policy must be declared; absence keeps every exclusion-type finding blocking");
    return result;
  }
  if (typeof policy !== "object" || Array.isArray(policy)) {
    addFinding(findings, "release-policy-invalid", "Release policy must be an object when declared");
    return result;
  }
  const schemaValid = policy.schemaVersion === 1;
  if (!schemaValid) {
    addFinding(findings, "release-policy-invalid", "Release policy must declare schemaVersion 1");
  }
  const upstream = policy.upstreamCandidate;
  if (upstream !== undefined && upstream !== null) {
    const valid = typeof upstream === "object" && !Array.isArray(upstream)
      && upstream.scope === REHEARSAL_ONLY_SCOPE
      && typeof upstream.reason === "string" && upstream.reason.trim() !== ""
      && typeof upstream.policySource === "string" && upstream.policySource.trim() !== "";
    if (!valid) {
      addFinding(findings, "release-policy-invalid", "Rehearsal-only upstream scope requires scope, reason, and policySource");
    } else if (schemaValid) {
      result.upstreamRehearsalOnly = true;
    }
  }
  const host = policy.hostPackage;
  if (host !== undefined && host !== null) {
    const valid = typeof host === "object" && !Array.isArray(host)
      && host.excludedFromRelease === true
      && typeof host.reason === "string" && host.reason.trim() !== ""
      && typeof host.policySource === "string" && host.policySource.trim() !== "";
    if (!valid) {
      addFinding(findings, "release-policy-invalid", "Host exclusion requires excludedFromRelease true, reason, and policySource");
    } else if (schemaValid) {
      result.hostExcluded = true;
    }
  }
  return result;
}

function applyReleasePolicyFindings(findings, policy) {
  for (const finding of findings) {
    if (policy.upstreamRehearsalOnly && UPSTREAM_CANDIDATE_EXCLUDED_FINDINGS.has(finding.code)) {
      finding.blocking = false;
    }
    if (policy.hostExcluded && HOST_EXCLUDED_FINDINGS.has(finding.code)) {
      finding.blocking = false;
    }
  }
}

export function auditInventory({ manifestPath = DEFAULT_MANIFEST_PATH, repoRoot = DEFAULT_REPO_ROOT } = {}) {
  const findings = [];
  if (typeof repoRoot !== "string" || repoRoot.trim() === "") {
    throw new TypeError("repoRoot must be a non-empty path");
  }
  const absoluteManifestPath = path.resolve(manifestPath);
  if (!manifestPathInsideRepo(repoRoot, absoluteManifestPath)) {
    addFinding(findings, "manifest-path-outside-repo", `Audit manifest must be inside the audited repository: ${safePathLabel(manifestPath)}`);
    return {
      status: "blocked",
      findings,
      manifestPath: "<manifest>",
      declaredBlockingReasons: [],
      evidenceLocators: collectEvidenceLocators(null),
    };
  }
  const manifest = readJson(absoluteManifestPath, findings);
  if (!manifest) {
    return {
      status: "blocked",
      findings,
      manifestPath: "<manifest>",
      declaredBlockingReasons: [],
      evidenceLocators: collectEvidenceLocators(null),
    };
  }
  const evidenceLocators = collectEvidenceLocators(manifest);

  if (manifest.schemaVersion !== 1) addFinding(findings, "manifest-schema-unsupported", "The extension inventory schema version is unsupported");
  if (manifest.scope?.productRoot !== "product/Halo Studio") addFinding(findings, "product-root-policy-mismatch", "The inventory product root must remain product/Halo Studio");
  if (!manifest.releaseGate || !["passed", "blocked"].includes(manifest.releaseGate.status)) {
    addFinding(findings, "release-gate-status-missing", "The inventory must explicitly declare a passed or blocked release gate");
  }
  if (manifest.releaseGate?.status === "blocked") {
    addFinding(findings, "release-gate-declared-blocked", "The release gate is explicitly blocked", {
      reasonCount: Array.isArray(manifest.releaseGate.blockingReasons) ? manifest.releaseGate.blockingReasons.length : 0,
    });
  }
  if (manifest.releaseGate?.status === "blocked" && (!Array.isArray(manifest.releaseGate.blockingReasons) || manifest.releaseGate.blockingReasons.length === 0)) {
    addFinding(findings, "release-gate-reasons-missing", "A blocked release gate must record explicit blocking reasons");
  }
  if (manifest.releaseGate?.status === "passed" && Array.isArray(manifest.releaseGate.blockingReasons) && manifest.releaseGate.blockingReasons.length > 0) {
    addFinding(findings, "release-gate-reasons-on-passed", "A passing release gate cannot retain blocking reasons");
  }
  checkUpstreamEvidence(repoRoot, manifest.upstreamCandidateEvidence, findings);
  checkWorkspaceBoundary(repoRoot, manifest.dependencyBoundary, findings);
  checkRuntimeScan(repoRoot, manifest.runtime, findings);
  checkBuiltInExtensionBoundary(manifest.runtime, findings);
  checkManifestPathRoles(manifest, findings);
  checkManifestScope(manifest, findings);

  if (!Array.isArray(manifest.extensions) || manifest.extensions.length === 0) {
    addFinding(findings, "extension-inventory-empty", "The Halo first-party extension inventory is empty");
  }

  const ids = new Set();
  for (const extension of manifest.extensions ?? []) {
    const label = `extensions.${extension?.id ?? "<unknown>"}`;
    for (const key of [
      "id",
      "fixedVersion",
      "sourcePath",
      "sourceCommit",
      "sourceTree",
      "gitHashObject",
      "sha256",
      "load",
      "capabilities",
      "impact",
      "hostPermissions",
      "dependencies",
      "license",
      "updateResponsibility",
    ]) {
      if (extension?.[key] === undefined || extension?.[key] === null) addFinding(findings, "manifest-field-missing", `${label}.${key} is required`);
    }
    if (ids.has(extension?.id)) addFinding(findings, "extension-id-duplicate", `Duplicate extension id: ${extension?.id}`);
    ids.add(extension?.id);
    if (!VERSION_PATTERN.test(extension?.fixedVersion ?? "")) addFinding(findings, "extension-version-unpinned", `${label}.fixedVersion must be a fixed semantic version`);
    if (!COMMIT_PATTERN.test(extension?.sourceCommit ?? "")) addFinding(findings, "extension-source-commit-unpinned", `${label}.sourceCommit must be a full commit SHA`);
    if (!GIT_OBJECT_PATTERN.test(extension?.sourceTree ?? "")) addFinding(findings, "extension-source-tree-unpinned", `${label}.sourceTree must be a full Git tree hash`);
    if (!GIT_OBJECT_PATTERN.test(extension?.gitHashObject ?? "")) addFinding(findings, "git-hash-object-unpinned", `${label}.gitHashObject must be a full Git object hash`);
    if (!SHA256_PATTERN.test(extension?.sha256 ?? "")) addFinding(findings, "sha256-unpinned", `${label}.sha256 must be a full SHA-256`);

    const sourcePath = resolveRepoPath(repoRoot, extension?.sourcePath);
    const productRoot = resolveRepoPath(repoRoot, manifest.scope?.productRoot ?? "product/Halo Studio");
    if (!sourcePath || !productRoot || !inside(productRoot, sourcePath)) {
      addFinding(findings, "source-path-outside-product", `${label}.sourcePath is not inside the Halo product tree`);
      continue;
    }
    if (!existsSync(sourcePath)) {
      addFinding(findings, "extension-source-missing", `Extension source is missing: ${safePathLabel(extension.sourcePath)}`);
      continue;
    }

    const actualGitHash = gitHashObject(repoRoot, extension.sourcePath);
    if (actualGitHash === null) addFinding(findings, "git-hash-object-unavailable", `git hash-object could not inspect ${safePathLabel(extension.sourcePath)}`);
    else if (actualGitHash.toLowerCase() !== extension.gitHashObject.toLowerCase()) addFinding(findings, "git-hash-object-mismatch", `Git object hash mismatch for ${safePathLabel(extension.sourcePath)}`, { expected: extension.gitHashObject, actual: actualGitHash });
    const actualSha256 = hashFile(sourcePath, "sha256");
    if (actualSha256.toLowerCase() !== extension.sha256.toLowerCase()) addFinding(findings, "sha256-mismatch", `SHA-256 mismatch for ${safePathLabel(extension.sourcePath)}`, { expected: extension.sha256, actual: actualSha256 });

    const sourceContents = readFileSync(sourcePath, "utf8");
    checkSourceProvenance(repoRoot, extension, findings);
    checkExtensionSource(extension, sourceContents, findings);
    checkExtensionContractMetadata(extension, findings);
    checkDependencies(repoRoot, extension, findings);
    checkEvidenceFiles(repoRoot, extension, findings);

    const adapterPath = resolveRepoPath(repoRoot, manifest.runtime?.adapterPath);
    if (!adapterPath || !existsSync(adapterPath)) addFinding(findings, "adapter-source-missing", `PiRpcAdapter source is missing: ${safePathLabel(manifest.runtime?.adapterPath)}`);
    else {
      const adapterSource = readFileSync(adapterPath, "utf8");
      checkLoadBoundary(extension, adapterSource, findings);
      checkAdapterVersion(extension, adapterSource, findings);
    }
  }

  const releasePolicy = checkReleasePolicy(manifest, findings);
  applyReleasePolicyFindings(findings, releasePolicy);

  return {
    status: findings.some((finding) => finding.blocking !== false) ? "blocked" : "passed",
    findings,
    manifestPath: "<manifest>",
    extensionCount: manifest.extensions?.length ?? 0,
    declaredBlockingReasons: Array.isArray(manifest.releaseGate?.blockingReasons)
      ? manifest.releaseGate.blockingReasons.filter((reason) => typeof reason === "string" && reason.trim() !== "")
      : [],
    evidenceLocators,
  };
}

export function auditReleaseGate(options = {}) {
  try {
    const inventoryReport = auditInventory(options);
    const generatedReasons = inventoryReport.findings
      .filter((finding) => finding.code !== "release-gate-declared-blocked" && finding.blocking !== false)
      .map((finding) => finding.message)
      .filter((reason) => typeof reason === "string" && reason.trim() !== "");
    const blockingReasons = [...new Set([
      ...(inventoryReport.declaredBlockingReasons ?? []),
      ...generatedReasons,
    ])];

    return {
      status: inventoryReport.status === "passed" ? "eligible" : "blocked",
      findings: inventoryReport.findings,
      blockingReasons,
      evidenceLocators: inventoryReport.evidenceLocators,
      manifestPath: inventoryReport.manifestPath,
      extensionCount: inventoryReport.extensionCount ?? 0,
    };
  } catch {
    return createAuditExceptionReport();
  }
}

function createAuditExceptionReport() {
  const findings = [];
  addFinding(findings, "audit-exception", "Release-gate audit failed closed", { locator: "audit://exception" });
  return {
    status: "blocked",
    findings,
    blockingReasons: findings.map((finding) => finding.message),
    evidenceLocators: collectEvidenceLocators(null),
    manifestPath: "<manifest>",
    extensionCount: 0,
  };
}

function parseArgs(argv) {
  const options = { manifestPath: DEFAULT_MANIFEST_PATH, repoRoot: DEFAULT_REPO_ROOT, json: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--manifest") options.manifestPath = path.resolve(argv[++index]);
    else if (argument === "--root") options.repoRoot = path.resolve(argv[++index]);
    else if (argument === "--json") options.json = true;
    else if (argument === "--help" || argument === "-h") options.help = true;
    else throw new Error("Unknown audit argument");
  }
  return options;
}

export function main(argv = process.argv.slice(2)) {
  const options = parseArgs(argv);
  if (options.help) {
    console.log("Usage: node scripts/pi-extension-audit.mjs [--manifest <path>] [--root <repo>] [--json]");
    return 0;
  }
  const report = auditReleaseGate(options);
  if (options.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`Pi extension audit: ${report.status}`);
    for (const finding of report.findings) console.log(`- [${finding.code}] ${finding.message}`);
  }
  return report.status === "eligible" ? 0 : 1;
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try {
    process.exitCode = main();
  } catch {
    const report = createAuditExceptionReport();
    if (process.argv.includes("--json")) console.log(JSON.stringify(report, null, 2));
    else console.error("Pi extension audit failed");
    process.exitCode = 1;
  }
}
