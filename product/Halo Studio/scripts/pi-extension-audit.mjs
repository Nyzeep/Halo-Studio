import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, realpathSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

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

function normalizeRelativePath(value) {
  return String(value).replaceAll("\\", "/");
}

function isAbsolutePath(value) {
  return /^(?:[A-Za-z]:[\\/]|[\\/]{1,2})/.test(String(value));
}

function isExternalEvidenceReference(value) {
  return isAbsolutePath(value) || READ_ONLY_EVIDENCE_REFERENCE_PATTERN.test(String(value));
}

function safePathLabel(value) {
  if (typeof value !== "string" || value.trim() === "") return "<missing-path>";
  if (isAbsolutePath(value)) return "<external-path>";
  if (READ_ONLY_EVIDENCE_REFERENCE_PATTERN.test(value)) return "<read-only-evidence>";
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

function addFinding(findings, code, message, evidence = {}) {
  findings.push({ code, message, evidence: sanitizeEvidence(evidence) });
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
  const notice = evidenceContents.find(({ path: evidencePath }) => /(^|\/)THIRD_PARTY_NOTICES\.md$/i.test(normalizeRelativePath(evidencePath)));
  if (!notice) {
    addFinding(findings, "license-notice-evidence-missing", `${extension.id} must cite product/THIRD_PARTY_NOTICES.md as notice evidence`);
  } else {
    for (const claim of [extension.id, extension.sourcePath, extension.sourceCommit, extension.gitHashObject, extension.sha256]) {
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
    if (!artifactPath || !isRegularFile(artifactPath)) {
      addFinding(findings, "release-artifact-file-missing", `${extension.id} release artifact evidence file is missing: ${safePathLabel(releaseArtifact.path)}`);
    } else if (!SHA256_PATTERN.test(releaseArtifact.sha256 ?? "") || hashFile(artifactPath, "sha256") !== releaseArtifact.sha256.toLowerCase()) {
      addFinding(findings, "release-artifact-hash-mismatch", `${extension.id} release artifact evidence does not match its recorded SHA-256`);
    }
    if (artifactPath && isRegularFile(artifactPath)) {
      checkFileFingerprint(artifactPath, releaseArtifact, "release-artifact", `${extension.id} release artifact`, findings);
    }
    if (!Array.isArray(releaseArtifact.requiredText) || releaseArtifact.requiredText.length === 0) {
      addFinding(findings, "release-artifact-text-claims-missing", `${extension.id} release artifact evidence must declare exact text claims`);
    }
    for (const requiredText of releaseArtifact.requiredText ?? []) {
      if (typeof requiredText !== "string" || !artifactPath || !isRegularFile(artifactPath) || !readFileSync(artifactPath, "utf8").includes(requiredText)) {
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
  const exactHostTag = typeof host?.sourceTag === "string" && host.sourceTag.trim() !== "" && !/^(?:latest|main|master|next)$/i.test(host.sourceTag.trim());
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
    const hostLicenseEvidencePath = resolveRepoPath(repoRoot, hostLicense.evidencePath);
    if (!hostLicenseEvidencePath || !isRegularFile(hostLicenseEvidencePath)) {
      addFinding(findings, "host-license-evidence-file-missing", `${extension.id} host license evidence must point to a repository-local file`);
    } else {
      checkRequiredTextClaims(hostLicenseEvidencePath, hostLicense.requiredText, "host-license-evidence", `${extension.id} host license evidence`, findings);
    }
    for (const releaseFile of hostLicense.releaseFiles) {
      const descriptor = typeof releaseFile === "string" ? { path: releaseFile } : releaseFile;
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
    const closureEvidencePath = resolveRepoPath(repoRoot, closure.evidencePath);
    if (!closureEvidencePath || !isRegularFile(closureEvidencePath)) {
      addFinding(findings, "host-dependency-closure-evidence-missing", `${extension.id} host dependency closure must point to a repository-local evidence file`);
    } else {
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
    /\b(?:globalThis\.)?fetch\s*\(/i,
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
  if (!/\binherit(?:s|ed)?\b/i.test(hostPermissions) || !/(?:not|no)\s+(?:a\s+)?sandbox\b/i.test(hostPermissions)) {
    addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must describe inherited host permissions and sandbox status`);
    if (hostPermissions !== "") {
      addFinding(findings, "extension-host-permission-claim-invalid", `${extension.id} must state that it inherits Pi process permissions and is not a sandbox`);
    }
  }
  if (typeof extension.load?.pathPolicy !== "string" || extension.load.pathPolicy.trim() === "") {
    addFinding(findings, "extension-contract-metadata-incomplete", `${extension.id} must declare its adapter-owned extension path policy`);
  }
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
  const hasRuntimePathFlow = /pi_rpc_args\s*\(\s*extension_path\s*,/.test(adapterSource)
    && [
      "let mut extension = match self.install_first_party_extension()",
      "let extension_path = extension.path.clone()",
      "state.extension_path = Some(extension_path)",
    ].every((token) => adapterSource.includes(token));
  if (!hasEmbeddedInstall || !hasHashBoundEmbeddedSource) {
    addFinding(findings, "adapter-extension-source-not-hash-bound", `${extension.id} adapter does not prove that the embedded source is copied under its fixed digest`);
  }
  if (!hasHashBoundRuntimePath || !hasRuntimePathFlow) {
    addFinding(findings, "adapter-runtime-load-path-unproven", `${extension.id} adapter does not prove that pi_rpc_args receives the adapter-owned hashed extension path`);
  }
}

function checkSourceProvenance(repoRoot, extension, findings) {
  if (!COMMIT_PATTERN.test(extension.sourceCommit ?? "")) return;
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
    /\bfetch\s*\(/i,
    /\bhttps?\.request\s*\(/i,
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
    if (networkCapability.some((pattern) => pattern.test(contents)) && !isAllowlisted(relativePath, "runtime-network-capability", file)) {
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
    if (!COMMIT_PATTERN.test(builtIn?.sourceCommit ?? "") && !(typeof builtIn?.sourceTag === "string" && builtIn.sourceTag.trim() !== "")) {
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
    : "HALO_BITFUN_REFERENCE_ROOT";
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
  if (!actualStatus.ok || actualStatus.stdout.split(/\r?\n/).some((line) => line.trim() !== "" && !line.startsWith("#"))) {
    addFinding(findings, "upstream-reference-tree-dirty", "The upstream candidate reference tree is not clean");
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

  if (candidate.parentEvidence && typeof candidate.parentEvidence.exitCode === "number") {
    const parent = git(["rev-parse", "HEAD^"]);
    if (candidate.parentEvidence.exitCode === 128 && parent.ok) {
      addFinding(findings, "upstream-parent-evidence-mismatch", "Candidate parent evidence says HEAD^ is unavailable, but it is locally resolvable");
    } else if (candidate.parentEvidence.exitCode !== (parent.ok ? 0 : parent.status)) {
      addFinding(findings, "upstream-parent-evidence-mismatch", "Candidate parent evidence does not match a fresh local HEAD^ check", {
        expectedExitCode: candidate.parentEvidence.exitCode,
        actualExitCode: parent.ok ? 0 : parent.status,
      });
    }
  }

  const initialImportManifest = resolveRepoPath(repoRoot, candidateEvidence.base?.initialImportManifest);
  if (!initialImportManifest || !isRegularFile(initialImportManifest)) {
    addFinding(findings, "upstream-initial-import-manifest-missing", `Initial import manifest is missing: ${candidateEvidence.base?.initialImportManifest}`);
    return;
  }
  const initialManifest = readJson(initialImportManifest, findings);
  if (!initialManifest || initialManifest.upstream?.commit !== evidence.baseCommit || !Array.isArray(initialManifest.entries)) {
    addFinding(findings, "upstream-initial-import-evidence-invalid", "Initial import manifest does not prove the pinned upstream base and entries");
    return;
  }
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
        addFinding(findings, "upstream-representative-path-missing", `Representative changed path is not present in the computed diff: ${relativePath}`);
      }
    }
  }
}

function checkWorkspaceBoundary(repoRoot, boundary, findings) {
  if (!boundary) return;
  const manifestPath = resolveRepoPath(repoRoot, boundary.workspaceManifest);
  if (!manifestPath || !existsSync(manifestPath)) {
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

export function auditInventory({ manifestPath = DEFAULT_MANIFEST_PATH, repoRoot = DEFAULT_REPO_ROOT } = {}) {
  const findings = [];
  const absoluteManifestPath = path.resolve(manifestPath);
  const manifest = readJson(absoluteManifestPath, findings);
  if (!manifest) return { status: "blocked", findings, manifestPath: "<manifest>" };

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
  checkUpstreamEvidence(repoRoot, manifest.upstreamCandidateEvidence, findings);
  checkWorkspaceBoundary(repoRoot, manifest.dependencyBoundary, findings);
  checkRuntimeScan(repoRoot, manifest.runtime, findings);
  checkBuiltInExtensionBoundary(manifest.runtime, findings);
  checkManifestPathRoles(manifest, findings);

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

  return {
    status: findings.length === 0 ? "passed" : "blocked",
    findings,
    manifestPath: "<manifest>",
    extensionCount: manifest.extensions?.length ?? 0,
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
  const report = auditInventory(options);
  if (options.json) console.log(JSON.stringify(report, null, 2));
  else {
    console.log(`Pi extension audit: ${report.status}`);
    for (const finding of report.findings) console.log(`- [${finding.code}] ${finding.message}`);
  }
  return report.status === "passed" ? 0 : 1;
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try {
    process.exitCode = main();
  } catch (error) {
    console.error("Pi extension audit failed");
    process.exitCode = 1;
  }
}
