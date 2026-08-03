import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { auditInventory, auditReleaseGate } from "./pi-extension-audit.mjs";

const AUDIT_SCRIPT = fileURLToPath(new URL("./pi-extension-audit.mjs", import.meta.url));

const EXTENSION_ID = "halo-workbench-permission-gate";
const EXTENSION_SOURCE = `import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
export default function haloPermissionGate(pi: ExtensionAPI) {
  pi.on("tool_call", async (_event, ctx) => {
    await ctx.ui.confirm("Halo permission decision", "one-shot");
  });
}
`;
const ADAPTER_SOURCE = `pub const HALO_PI_EXTENSION_VERSION: &str = "1.0.0";
const source = include_str!("halo_permission_gate.ts");
const args = vec!["--no-extensions", "--extension", extension_path];
fn install_first_party_extension() { install_embedded_extension(); }
fn install_embedded_extension() {
  let _ = stable_digest(HALO_PERMISSION_EXTENSION_SOURCE);
  let _ = HALO_PI_EXTENSION_ID;
}
fn spawn_session_process(extension_path: &Path) { pi_rpc_args(extension_path, mode); }
fn pi_rpc_args(extension_path: &Path) {
  args.extend(["--extension".to_string(), extension_path.to_string_lossy().into_owned()]);
}
let mut extension = match self.install_first_party_extension() { _ => todo!() };
let extension_path = extension.path.clone();
state.extension_path = Some(extension_path);
let config_dir = "adapter-owned";
`;

function createFixture(overrides = {}) {
  const root = mkdtempSync(path.join(tmpdir(), "halo-pi-extension-audit-"));
  const extensionPath = "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts";
  const adapterPath = "product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/lib.rs";
  const licensePath = "product/Halo Studio/LICENSE";
  const hostLicensePath = "product/Halo Studio/pi-host-LICENSE.txt";
  const noticePath = "product/THIRD_PARTY_NOTICES.md";
  const releaseArtifactPath = "product/Halo Studio/release-license-bundle.txt";
  const lockPaths = [
    "product/Halo Studio/pnpm-lock.yaml",
    "product/Halo Studio/package-lock.json",
    "product/Halo Studio/Cargo.lock",
  ];

  for (const file of [extensionPath, adapterPath, licensePath, hostLicensePath, noticePath, releaseArtifactPath, ...lockPaths]) {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  }
  writeFileSync(path.join(root, extensionPath), EXTENSION_SOURCE);
  writeFileSync(path.join(root, adapterPath), ADAPTER_SOURCE);
  writeFileSync(
    path.join(root, licensePath),
    "MIT License\\nCopyright (c) 2026 CWing\\nPermission is hereby granted, free of charge, to any person obtaining a copy\\n",
  );
  writeFileSync(
    path.join(root, hostLicensePath),
    "MIT License\\nCopyright (c) Pi fixture\\nPermission is hereby granted, free of charge, to any person obtaining a copy\\n"
      + `${EXTENSION_ID}\\n@earendil-works/pi-agent-core\\n@earendil-works/pi-ai\\n`,
  );
  writeFileSync(
    path.join(root, noticePath),
    `${EXTENSION_ID}\\nMIT License\\n${extensionPath}\\n`,
  );
  writeFileSync(path.join(root, releaseArtifactPath), `${EXTENSION_ID}\\nMIT License\\n`);
  for (const lockPath of lockPaths) writeFileSync(path.join(root, lockPath), "lockfile\\n");

  execFileSync("git", ["init", "--quiet"], { cwd: root, stdio: "ignore" });
  execFileSync("git", ["add", "--", extensionPath, adapterPath, licensePath, hostLicensePath, noticePath, releaseArtifactPath, ...lockPaths], { cwd: root, stdio: "ignore" });
  execFileSync(
    "git",
    ["-c", "user.name=Halo fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture"],
    { cwd: root, stdio: "ignore" },
  );
  const sourceCommit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim();
  const sourceTree = execFileSync("git", ["rev-parse", "HEAD^{tree}"], { cwd: root, encoding: "utf8" }).trim();
  const gitHashObject = execFileSync("git", ["hash-object", "--", extensionPath], { cwd: root, encoding: "utf8" }).trim();
  const sha256 = createHash("sha256").update(readFileSync(path.join(root, extensionPath))).digest("hex");
  writeFileSync(
    path.join(root, noticePath),
    `${EXTENSION_ID}\\nMIT License\\n${extensionPath}\\n${sourceCommit}\\n${sourceTree}\\n${gitHashObject}\\n${sha256.toUpperCase()}\\n`,
  );
  const releaseArtifactContents = `${EXTENSION_ID}\\nMIT License\\n${sourceCommit}\\n${sha256}\\n`;
  const hostClosureReleaseArtifactContents = `${releaseArtifactContents}@earendil-works/pi-agent-core 0.83.0 MIT \\n` + `npm:@earendil-works/pi-agent-core@0.83.0 \\n@earendil-works/pi-ai 0.83.0 MIT \\n` + `npm:@earendil-works/pi-ai@0.83.0 \\n`;
  writeFileSync(path.join(root, releaseArtifactPath), hostClosureReleaseArtifactContents);
  const releaseArtifactSha256 = createHash("sha256").update(hostClosureReleaseArtifactContents).digest("hex");

  const fileFingerprint = (relativePath) => ({
    path: relativePath,
    sha256: createHash("sha256").update(readFileSync(path.join(root, relativePath))).digest("hex"),
    size: readFileSync(path.join(root, relativePath)).length,
  });

  const hostDirectDependency = {
    name: "@earendil-works/pi-agent-core",
    version: "0.83.0",
    source: "npm:@earendil-works/pi-agent-core@0.83.0",
    license: "MIT",
  };
  const hostTransitiveDependency = {
    name: "@earendil-works/pi-ai",
    version: "0.83.0",
    source: "npm:@earendil-works/pi-ai@0.83.0",
    license: "MIT",
  };

  const manifest = {
    schemaVersion: 1,
    scope: {
      productRoot: "product/Halo Studio",
      runtimeAdapter: adapterPath,
      auditScript: "product/Halo Studio/scripts/pi-extension-audit.mjs",
    },
    releaseGate: { status: "passed", blockingReasons: [] },
    upstreamCandidateEvidence: {
      path: "docs/issue-13-upstream-sync-candidate.json",
      baseCommit: "a".repeat(40),
      candidateCommit: "b".repeat(40),
      diffRecorded: true,
      automatedMerge: false,
      upstreamWrite: false,
    },
    runtime: {
      adapterPath,
      scanPaths: [adapterPath],
      builtInExtensions: [
        {
          id: "fixture-built-in",
          sourceCommit: null,
          sourceTag: "fixture-v1",
          releaseEligible: false,
          capabilities: {
            tools: [],
            events: [],
            network: [],
            files: [],
            credentials: [],
            process: "none",
          },
        },
      ],
    },
    extensions: [
      {
        id: EXTENSION_ID,
        fixedVersion: "1.0.0",
        sourcePath: extensionPath,
        sourceCommit,
        sourceTree,
        sourceTag: null,
        gitHashObject,
        sha256,
        load: {
          arguments: [
            "--mode",
            "rpc",
            "--no-extensions",
            "--extension",
            `<adapter-owned-temp>/${EXTENSION_ID}-<sha256>.ts`,
          ],
          noExtensions: true,
          projectAutoDiscovery: false,
          userAutoDiscovery: false,
          runtimeDownload: false,
          inlineBuiltInsPolicy: "audited-host-built-ins",
          pathPolicy: "Only the adapter-owned temporary copy of the embedded, hash-verified source may be passed to --extension.",
        },
        capabilities: {
          tools: [],
          events: ["tool_call"],
          ui: ["ctx.ui.confirm", "extension_ui_request", "extension_ui_response"],
          cleanup: ["stop", "abort", "eof", "failure", "application-exit"],
        },
        impact: {
          files: "none",
          network: "none",
          process: "none",
          credentials: "none",
          git: "none",
          renderer: "none",
        },
        hostPermissions: "inherits Pi process user permissions; no sandbox",
        dependencies: {
          runtime: { direct: [], transitive: [] },
          typeOnly: ["@earendil-works/pi-coding-agent"],
          host: {
            package: "@earendil-works/pi-coding-agent",
            version: "0.83.0",
            sourceCommit,
            sourceTag: null,
            bundled: false,
            dependencyClosure: {
              status: "complete",
              direct: [hostDirectDependency],
              transitive: [hostTransitiveDependency],
              evidencePath: releaseArtifactPath,
            },
            licenseEvidence: {
              observedSpdx: "MIT",
              evidencePath: hostLicensePath,
              copyright: "Copyright (c) Pi fixture",
              requiredText: ["MIT License", "Permission is hereby granted", "Copyright (c) Pi fixture"],
              releaseStatus: "included",
              releaseFiles: [{
                ...fileFingerprint(hostLicensePath),
                requiredText: [EXTENSION_ID, "MIT License", hostDirectDependency.name, hostTransitiveDependency.name],
              }],
            },
          },
          lockfiles: lockPaths,
        },
        license: {
          spdx: "MIT",
          copyright: "Copyright (c) 2026 CWing",
          evidence: [
            { ...fileFingerprint(licensePath), requiredText: ["MIT License", "Copyright (c) 2026 CWing"] },
            { ...fileFingerprint(noticePath), requiredText: [EXTENSION_ID, "MIT License"] },
          ],
          distributionFiles: [
            { ...fileFingerprint(licensePath), requiredText: ["MIT License", "Copyright (c) 2026 CWing"] },
            { ...fileFingerprint(noticePath), requiredText: [EXTENSION_ID, "MIT License"] },
          ],
          lockfileEvidence: lockPaths.map(fileFingerprint),
            releaseArtifactEvidence: {
            path: releaseArtifactPath,
            sha256: releaseArtifactSha256,
            size: Buffer.byteLength(hostClosureReleaseArtifactContents),
            requiredText: [EXTENSION_ID, "MIT License"],
          },
        },
        updateResponsibility: "Halo Studio maintainers",
      },
    ],
    ...overrides,
  };
  const manifestPath = path.join(root, "docs", "pi-extension-inventory.json");
  mkdirSync(path.dirname(manifestPath), { recursive: true });
  writeFileSync(manifestPath, JSON.stringify(manifest, null, 2));
  return { root, manifestPath, manifest, extensionPath, adapterPath, releaseArtifactPath };
}

function createUpstreamFixture({ invalidRecord = true } = {}) {
  const fixture = createFixture();
  const referenceRoot = mkdtempSync(path.join(tmpdir(), "halo-pi-reference-"));
  writeFileSync(path.join(referenceRoot, "candidate.txt"), "candidate\n");
  execFileSync("git", ["init", "--quiet"], { cwd: referenceRoot, stdio: "ignore" });
  execFileSync("git", ["add", "--", "candidate.txt"], { cwd: referenceRoot, stdio: "ignore" });
  execFileSync(
    "git",
    ["-c", "user.name=Halo fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "base"],
    { cwd: referenceRoot, stdio: "ignore" },
  );
  execFileSync(
    "git",
    ["-c", "user.name=Halo fixture", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "candidate"],
    { cwd: referenceRoot, stdio: "ignore" },
  );
  execFileSync("git", ["branch", "-M", "main"], { cwd: referenceRoot, stdio: "ignore" });
  const candidateCommit = execFileSync("git", ["rev-parse", "HEAD"], { cwd: referenceRoot, encoding: "utf8" }).trim();
  const candidateTree = execFileSync("git", ["rev-parse", "HEAD^{tree}"], { cwd: referenceRoot, encoding: "utf8" }).trim();
  const candidateBlob = execFileSync("git", ["hash-object", "--", "candidate.txt"], { cwd: referenceRoot, encoding: "utf8" }).trim();
  const candidateSize = readFileSync(path.join(referenceRoot, "candidate.txt")).length;
  const baseCommit = execFileSync("git", ["rev-parse", "HEAD^"], { cwd: referenceRoot, encoding: "utf8" }).trim();
  const initialImportTree = execFileSync("git", ["rev-parse", `${baseCommit}^{tree}`], { cwd: referenceRoot, encoding: "utf8" }).trim();
  const initialManifestPath = "docs/initial-import.json";
  const evidencePath = "docs/issue-13-upstream-sync-candidate.json";
  const recordsPath = "docs/issue-13-upstream-sync-diff.json";
  mkdirSync(path.join(fixture.root, "docs"), { recursive: true });
  writeFileSync(
    path.join(fixture.root, initialManifestPath),
    JSON.stringify({ upstream: { commit: baseCommit }, entries: [{ path: "candidate.txt", mode: "100644", type: "blob", sha: candidateBlob, size: candidateSize }] }, null, 2),
  );
  writeFileSync(
    path.join(fixture.root, recordsPath),
    JSON.stringify({ schemaVersion: 1, baseCommit, candidateCommit, records: invalidRecord ? [{ path: "ghost.txt", status: "added", base: null, candidate: { mode: "100644", type: "blob", blob: "z".repeat(40), size: 0 } }] : [] }, null, 2),
  );
  writeFileSync(
    path.join(fixture.root, evidencePath),
    JSON.stringify({
      operation: "incremental_sync",
      base: {
        commit: baseCommit,
        initialImportManifest: initialManifestPath,
        initialImportTree,
        manifestDerivedTree: initialImportTree,
        treeBindingStatus: "matched",
        resolution: { status: "resolved", resolvedCommit: baseCommit, objectType: "commit", exitCode: 0 },
      },
      candidate: {
        referenceRoot,
        referenceRootRole: "read-only-evidence",
        branch: "main",
        commit: candidateCommit,
        tree: candidateTree,
        resolution: { status: "resolved", resolvedCommit: candidateCommit, objectType: "commit", exitCode: 0 },
        repositoryState: { isShallow: false, shallowFilePresent: false, graftsFilePresent: false, replaceRefs: [] },
        parentEvidence: { command: "git rev-parse HEAD^", exitCode: 0, result: baseCommit },
        statusEvidence: {
          command: "git status --porcelain=v2 --branch",
          exitCode: 0,
          clean: true,
          result: "clean; branch main is synchronized with origin/main",
        },
      },
      ancestry: { status: "proven", exitCode: 0 },
      comparison: {
        baseEntries: 1,
        candidateEntries: 1,
        canonicalUtf8Counts: { identical: 1, modified: 0, added: 0, removed: 0, changedEntries: 0 },
        recordsPath,
      },
      conflictsAndDecisions: {
        mergeAttempted: false,
        automaticMerge: false,
        conflictMarkers: "not applicable: no merge was attempted",
        manualReviewRequired: ["candidate-only source changes require manual review"],
        retainedHaloBrand: ["product/Halo Studio/"],
        retainedWorkbenchRuntime: ["Workbench Runtime public contract"],
        retainedPiRpcPort: ["PiRpcPort and Pi RPC adapter seam"],
        prohibitedActions: [
          "No automatic merge or cherry-pick was performed.",
          "No commit, push, fetch, or file write was performed in the reference tree.",
          "No Pi source was copied and no npm/Git dependency was added.",
        ],
      },
      representativeChangedPaths: {},
      releaseGate: { status: "passed", evidenceGaps: [] },
    }, null, 2),
  );
  fixture.manifest.upstreamCandidateEvidence = {
    path: evidencePath,
    baseCommit,
    candidateCommit,
    diffRecorded: true,
    automatedMerge: false,
    upstreamWrite: false,
  };
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));
  return fixture;
}

function runAuditCli(fixture, args = []) {
  return spawnSync(
    process.execPath,
    [AUDIT_SCRIPT, "--manifest", fixture.manifestPath, "--root", fixture.root, ...args],
    { cwd: fixture.root, encoding: "utf8", windowsHide: true },
  );
}

test("release-gate seam returns a canonical blocked decision with reasons and safe evidence locators", () => {
  const fixture = createFixture({
    releaseGate: { status: "blocked", blockingReasons: ["candidate has not passed the release matrix"] },
  });

  const report = auditReleaseGate({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "blocked");
  assert.ok(report.blockingReasons.includes("candidate has not passed the release matrix"));
  assert.ok(report.evidenceLocators.some((locator) => (
    locator.pointer === "manifest.upstreamCandidateEvidence.path"
      && locator.locator === fixture.manifest.upstreamCandidateEvidence.path
  )));
  assert.ok(report.findings.every((finding) => typeof finding.locator === "string" && finding.locator.length > 0));
  assert.ok(!JSON.stringify(report).includes(fixture.root));
});

test("release-gate seam returns eligible only when every evidence check passes", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });

  const report = auditReleaseGate({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "eligible", JSON.stringify(report.findings, null, 2));
  assert.deepEqual(report.blockingReasons, []);
  assert.deepEqual(report.findings, []);
});

test("release-gate findings redact absolute evidence paths", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const candidateEvidencePath = path.join(fixture.root, fixture.manifest.upstreamCandidateEvidence.path);
  const candidateEvidence = JSON.parse(readFileSync(candidateEvidencePath, "utf8"));
  candidateEvidence.base.initialImportManifest = path.join(fixture.root, "private-evidence.json");
  writeFileSync(candidateEvidencePath, JSON.stringify(candidateEvidence, null, 2));

  const report = auditReleaseGate({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "blocked");
  const missingManifestFinding = report.findings.find((finding) => finding.code === "upstream-initial-import-manifest-missing");
  assert.ok(missingManifestFinding);
  assert.equal(missingManifestFinding.message, "Initial import manifest is missing: <external-path>");
  assert.ok(!JSON.stringify(report).includes(fixture.root));
});

test("a declared blocker cannot be hidden behind a passing inventory status", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  fixture.manifest.releaseGate = { status: "passed", blockingReasons: ["release artifact is not validated"] };
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditReleaseGate({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "blocked");
  assert.ok(report.findings.some((finding) => finding.code === "release-gate-reasons-on-passed"));
  assert.ok(report.blockingReasons.includes("release artifact is not validated"));
});

test("a blocked upstream candidate cannot make the release gate eligible", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const candidateEvidencePath = path.join(fixture.root, fixture.manifest.upstreamCandidateEvidence.path);
  const candidateEvidence = JSON.parse(readFileSync(candidateEvidencePath, "utf8"));
  candidateEvidence.releaseGate = { status: "blocked", evidenceGaps: ["candidate validation is incomplete"] };
  writeFileSync(candidateEvidencePath, JSON.stringify(candidateEvidence, null, 2));

  const report = auditReleaseGate({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "blocked");
  assert.ok(report.findings.some((finding) => finding.code === "upstream-candidate-release-gate-blocked"));
});

test("audit exceptions return a structured blocked release-gate result", () => {
  const fixture = createFixture();

  const report = auditReleaseGate({ manifestPath: fixture.manifestPath, repoRoot: null });

  assert.equal(report.status, "blocked");
  assert.ok(report.findings.some((finding) => finding.code === "audit-exception"));
  assert.ok(report.findings.every((finding) => typeof finding.locator === "string"));
  assert.ok(report.blockingReasons.length > 0);
});

test("CLI emits a structured blocked JSON result for argument exceptions", () => {
  const result = spawnSync(process.execPath, [AUDIT_SCRIPT, "--json", "--root"], {
    encoding: "utf8",
    windowsHide: true,
  });

  assert.equal(result.status, 1);
  const report = JSON.parse(result.stdout);
  assert.equal(report.status, "blocked");
  assert.ok(report.findings.some((finding) => finding.code === "audit-exception"));
});

test("local evidence is audited while missing host provenance blocks release", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.host.sourceCommit = null;
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence = null;
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));
  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "blocked");
  assert.ok(report.findings.some((finding) => finding.code === "host-source-provenance-missing"));
  assert.ok(report.findings.some((finding) => finding.code === "host-license-evidence-missing"));
});

test("the declared source commit must exist and contain the audited source blob", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].sourceCommit = "c".repeat(40);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "source-commit-provenance-unavailable"));
});

test("source provenance rejects a Git tree object in the sourceCommit field", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].sourceCommit = execFileSync("git", ["rev-parse", "HEAD^{tree}"], {
    cwd: fixture.root,
    encoding: "utf8",
  }).trim();
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "source-commit-provenance-unavailable"));
});

test("a source mutation is caught by both content hashes", () => {
  const fixture = createFixture();
  writeFileSync(path.join(fixture.root, fixture.extensionPath), `${EXTENSION_SOURCE}// changed\\n`);

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "git-hash-object-mismatch"));
  assert.ok(report.findings.some((finding) => finding.code === "sha256-mismatch"));
});

test("an unreviewed explicit extension path and auto-discovery are blocked", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].load.arguments[4] = "./.pi/extensions/unreviewed.ts";
  fixture.manifest.extensions[0].load.projectAutoDiscovery = true;
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "unreviewed-extension-path"));
  assert.ok(report.findings.some((finding) => finding.code === "project-extension-discovery-enabled"));
});

test("duplicate or aliased extension arguments are rejected", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].load.arguments.push("--extension", "another.ts");
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-argument-shape-invalid"));
});

test("the short extension alias is rejected even without a duplicate long flag", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].load.arguments = [
    "--mode",
    "rpc",
    "--no-extensions",
    "-e",
    `<adapter-owned-temp>/${EXTENSION_ID}-<sha256>.ts`,
  ];
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-argument-shape-invalid"));
});

test("the runtime adapter load path must remain embedded, copied, and hash-bound", () => {
  const fixture = createFixture();
  const unsafeAdapter = ADAPTER_SOURCE
    .replace("stable_digest(HALO_PERMISSION_EXTENSION_SOURCE)", "extension_path")
    .replace("pi_rpc_args(extension_path, mode)", "pi_rpc_args(caller_path, mode)");
  writeFileSync(path.join(fixture.root, fixture.adapterPath), unsafeAdapter);

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "adapter-extension-source-not-hash-bound"));
  assert.ok(report.findings.some((finding) => finding.code === "adapter-runtime-load-path-unproven"));
});

test("runtime download commands and forbidden absolute paths fail closed", () => {
  const fixture = createFixture();
  const externalReference = [String.fromCharCode(68) + ":", "pi-main"].join("\\");
  writeFileSync(
    path.join(fixture.root, fixture.adapterPath),
    `${ADAPTER_SOURCE}execFileSync("npm", ["install"]); const reference = "${externalReference}";\\n`,
  );

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-download-capability"));
  assert.ok(report.findings.some((finding) => finding.code === "forbidden-absolute-path"));
});

test("CLI help, blocked JSON, and passing JSON have explicit exit contracts", () => {
  const help = spawnSync(process.execPath, [AUDIT_SCRIPT, "--help"], { encoding: "utf8", windowsHide: true });
  assert.equal(help.status, 0, help.stderr);
  assert.match(help.stdout, /Usage: node scripts\/pi-extension-audit\.mjs/);

  const blockedFixture = createFixture({
    releaseGate: { status: "blocked", blockingReasons: ["fixture blocker"] },
  });
  const blocked = runAuditCli(blockedFixture, ["--json"]);
  assert.equal(blocked.status, 1, blocked.stderr);
  assert.equal(JSON.parse(blocked.stdout).status, "blocked");

  const passingFixture = createUpstreamFixture({ invalidRecord: false });
  const passed = runAuditCli(passingFixture, ["--json"]);
  assert.equal(passed.status, 0, `${passed.stderr}\n${passed.stdout}`);
  assert.equal(JSON.parse(passed.stdout).status, "eligible");
});

test("CLI rejects unknown arguments and missing option values", () => {
  const fixture = createFixture();
  const unknown = runAuditCli(fixture, ["--unknown"]);
  assert.equal(unknown.status, 1);

  const missingValue = spawnSync(process.execPath, [AUDIT_SCRIPT, "--manifest"], {
    cwd: fixture.root,
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(missingValue.status, 1);
});

test("runtime download invocations in JavaScript build inputs are blocked", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/runtime-danger.js";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), 'execFileSync("npm", ["ci"]);\n');
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-download-capability"));
});

test("runtime scanning catches network calls in extensionless text inputs", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/runtime-network-check";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), 'globalThis.fetch("https://example.invalid");\n');
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-network-capability"));
});

test("runtime scanning catches bare fetch calls", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/runtime-bare-fetch";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), 'fetch("https://example.invalid");\n');
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-network-capability"));
});

test("runtime scanning catches http get calls", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/runtime-http-get";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), 'http.get("https://example.invalid");\n');
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-network-capability"));
});

test("runtime scanning catches computed global fetch calls", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/runtime-computed-fetch";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), 'globalThis["fetch"]("https://example.invalid");\n');
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-network-capability"));
});

test("runtime scanning rejects variable and template computed global capabilities", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/runtime-computed-global";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), 'const networkMethod = `fetch`; globalThis[networkMethod]("https://example.invalid");\\n');
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-network-capability"));
});

test("computed global capability aliases and optional calls fail closed in extension and runtime scans", () => {
  const fixture = createFixture();
  const computedCapabilitySource = `${EXTENSION_SOURCE}const f = globalThis["fetch"]; f("https://example.invalid"); globalThis?.[name]("https://example.invalid");\\n`;
  writeFileSync(path.join(fixture.root, fixture.extensionPath), computedCapabilitySource);
  const runtimePath = "product/Halo Studio/scripts/runtime-computed-global-alias";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), computedCapabilitySource);
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-host-capability"));
  assert.ok(report.findings.some((finding) => finding.code === "runtime-network-capability"));
});

test("aliases of globalThis and window fail closed before computed capability calls", () => {
  const fixture = createFixture();
  const aliasedCapabilitySource = `${EXTENSION_SOURCE}const g = globalThis; g["fetch"]?.("https://example.invalid");\\n`;
  writeFileSync(path.join(fixture.root, fixture.extensionPath), aliasedCapabilitySource);
  const runtimePath = "product/Halo Studio/scripts/runtime-aliased-global";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), aliasedCapabilitySource);
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-host-capability"));
  assert.ok(report.findings.some((finding) => finding.code === "runtime-network-capability"));
});

test("shell download variants and script module inputs are blocked", () => {
  const fixture = createFixture();
  const runtimeDirectory = "product/Halo Studio/scripts/runtime-inputs";
  const files = {
    "download.bat": "npm install attacker-package\n",
    "module.psm1": "Invoke-WebRequest https://example.invalid/a.tgz\n",
    "manifest.psd1": "git -C repo fetch origin\n",
    "download.sh": "curl -fsSL https://example.invalid/a.tgz -o out\n",
  };
  mkdirSync(path.join(fixture.root, runtimeDirectory), { recursive: true });
  for (const [name, contents] of Object.entries(files)) writeFileSync(path.join(fixture.root, runtimeDirectory, name), contents);
  fixture.manifest.runtime.scanPaths.push(runtimeDirectory);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.findings.filter((finding) => finding.code === "runtime-download-capability").length, 4, JSON.stringify(report.findings, null, 2));
});

test("diagnostic and documentation strings do not count as shell downloads", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/diagnostic-text.js";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(
    path.join(fixture.root, runtimePath),
    'const messages = ["Command: git pull", "curl https://example.invalid/a.tgz", "npm install package", "Invoke-WebRequest https://example.invalid"];\n',
  );
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(!report.findings.some((finding) => finding.code === "runtime-download-capability" && finding.message.includes("diagnostic-text.js")));
});

test("generic external absolute paths in runtime inputs are blocked", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/external-path.js";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  const genericDrivePath = [String.fromCharCode(67) + ":", "unreviewed-reference", "source"].join("\\");
  writeFileSync(path.join(fixture.root, runtimePath), `const external = "${genericDrivePath}";\n`);
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "forbidden-absolute-path" && finding.message.includes("external-path.js")));
});

test("rooted Windows paths are both blocked and redacted from audit evidence", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/rooted-path.js";
  const rootedWindowsPath = String.raw`\rooted\source`;
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), `const external = "${rootedWindowsPath}";\n`);
  fixture.manifest.extensions[0].load.arguments[4] = rootedWindowsPath;
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "forbidden-absolute-path"));
  assert.ok(!JSON.stringify(report).includes(rootedWindowsPath));
});

test("dynamic extension imports and host capabilities are rejected", () => {
  const fixture = createFixture();
  writeFileSync(
    path.join(fixture.root, fixture.extensionPath),
    `${EXTENSION_SOURCE}const fs = require("node:fs");\nawait import("node:net");\nglobalThis.fetch("https://example.invalid");\n`,
  );

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-runtime-import-present"));
  assert.ok(report.findings.some((finding) => finding.code === "extension-host-capability"));
});

test("side-effect and unresolved dynamic extension imports fail closed", () => {
  const fixture = createFixture();
  writeFileSync(
    path.join(fixture.root, fixture.extensionPath),
    `${EXTENSION_SOURCE}import "@unreviewed/side-effect";\nexport * from "@unreviewed/re-export";\nexport * as ns from "@unreviewed/re-export-namespace";\nconst packageName = "@unreviewed/computed";\nrequire(packageName);\n` + "import(`@unreviewed/template/${packageName}`);\n",
  );

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-runtime-import-present"));
  assert.ok(report.findings.some((finding) => finding.code === "extension-runtime-import-unresolved"));
});

test("legal dollar-prefixed namespace re-exports are audited as runtime imports", () => {
  const fixture = createFixture();
  writeFileSync(
    path.join(fixture.root, fixture.extensionPath),
    `${EXTENSION_SOURCE}export * as $ns from "@unreviewed/re-export-namespace";\\n`,
  );

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-runtime-import-present" && finding.evidence.imports.includes("@unreviewed/re-export-namespace")));
});

test("extension metadata must describe the fail-closed permission seam", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].capabilities = { tools: [], events: [], ui: [], cleanup: [] };
  fixture.manifest.extensions[0].impact = { files: "none" };
  fixture.manifest.extensions[0].hostPermissions = "";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-contract-metadata-incomplete"));
});

test("extension metadata rejects custom tools and sandbox claims", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].capabilities.tools = ["shell"];
  fixture.manifest.extensions[0].hostPermissions = "does not inherit Pi process permissions; not a sandbox";
  fixture.manifest.extensions[0].load.pathPolicy = "Any extension path is accepted";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-custom-tool-present"));
  assert.ok(report.findings.some((finding) => finding.code === "extension-host-permission-claim-invalid"));
  assert.ok(report.findings.some((finding) => finding.code === "extension-path-policy-claim-invalid"));
});

test("audit reports never expose the caller's absolute workspace path", () => {
  const fixture = createFixture();

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.manifestPath, "<manifest>");
  assert.ok(!JSON.stringify(report).includes(fixture.root));
});

test("POSIX, UNC, and rooted Windows absolute paths are blocked", () => {
  const fixture = createFixture();
  const runtimeDirectory = "product/Halo Studio/scripts/path-matrix";
  const files = {
    "posix.js": String.raw`const external = "/srv/external/source";\n`,
    "unc.js": String.raw`const external = "\\server\share\source";\n`,
    "rooted-windows.js": String.raw`const external = "\rooted\source";\n`,
  };
  mkdirSync(path.join(fixture.root, runtimeDirectory), { recursive: true });
  for (const [name, contents] of Object.entries(files)) writeFileSync(path.join(fixture.root, runtimeDirectory, name), contents);
  fixture.manifest.runtime.scanPaths.push(runtimeDirectory);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  for (const name of Object.keys(files)) {
    assert.ok(report.findings.some((finding) => finding.code === "forbidden-absolute-path" && finding.message.includes(name)));
  }
});

test("external provenance paths require an explicit read-only evidence role", () => {
  const fixture = createFixture();
  fixture.manifest.runtime.builtInExtensions[0].sourcePath = "readonly-evidence://pi-main/packages/coding-agent";
  fixture.manifest.runtime.builtInExtensions[0].sourcePathRole = "runtime-input";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "external-path-role-invalid"));
});

test("a changed allowlisted file is not silently exempted from runtime scanning", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/download-diagnostic.js";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  const original = 'const message = "Run pnpm install before continuing";\n';
  writeFileSync(path.join(fixture.root, runtimePath), original);
  const originalHash = createHash("sha256").update(original).digest("hex");
  fixture.manifest.runtime.scanPaths.push(runtimePath);
  fixture.manifest.runtime.allowlistedFindings = [{
    path: runtimePath,
    code: "runtime-download-capability",
    sha256: originalHash,
    reason: "Only a diagnostic message in this exact file revision.",
  }];
  writeFileSync(path.join(fixture.root, runtimePath), `${original}execFileSync("pnpm", ["install"]);\n`);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-allowlist-stale"));
  assert.ok(report.findings.some((finding) => finding.code === "runtime-download-capability"));
});

test("diagnostic install instructions do not count as runtime downloads", () => {
  const fixture = createFixture();
  const runtimePath = "product/Halo Studio/scripts/diagnostic.js";
  const sentinelPath = "product/Halo Studio/scripts/download-sentinel.js";
  mkdirSync(path.dirname(path.join(fixture.root, runtimePath)), { recursive: true });
  writeFileSync(path.join(fixture.root, runtimePath), 'const message = "Run pnpm install before continuing";\n');
  writeFileSync(path.join(fixture.root, sentinelPath), 'execFileSync("pnpm", ["install"]);\n');
  fixture.manifest.runtime.scanPaths.push(runtimePath, sentinelPath);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-download-capability" && finding.message.includes("download-sentinel.js")));
  assert.ok(!report.findings.some((finding) => finding.code === "runtime-download-capability" && finding.message.includes("diagnostic.js")));
});

test("license evidence is read from files and never inferred from a package name", () => {
  const fixture = createFixture();
  writeFileSync(path.join(fixture.root, "product/Halo Studio/LICENSE"), "Copyright only\\n");
  fixture.manifest.extensions[0].license.spdx = "MIT";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "license-evidence-text-missing"));
});

test("license evidence must bind declared SPDX and copyright to actual files", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].license.evidence[0].requiredText = ["Copyright only"];
  writeFileSync(path.join(fixture.root, "product/Halo Studio/LICENSE"), "Copyright only\n");
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "license-spdx-evidence-missing"));
  assert.ok(report.findings.some((finding) => finding.code === "license-copyright-evidence-missing"));
});

test("license evidence requires a reproducible file fingerprint", () => {
  const fixture = createFixture();
  delete fixture.manifest.extensions[0].license.evidence[0].sha256;
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "license-evidence-fingerprint-missing"));
});

test("an unsupported SPDX identifier remains blocked without an evidence rule", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].license.spdx = "BSD-3-Clause";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "license-spdx-evidence-missing"));
});

test("a release gate needs a regular, recorded distribution artifact", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].license.releaseArtifactEvidence = null;
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "release-artifact-evidence-missing"));
});

test("a release artifact without exact text claims is blocked", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].license.releaseArtifactEvidence.requiredText = [];
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "release-artifact-text-claims-missing"));
});

test("distribution files require exact license and notice claims", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].license.distributionFiles[0].requiredText = ["missing distribution claim"];
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "distribution-license-text-missing"));
});

test("runtime and transitive dependency declarations are not admitted", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.runtime.direct = ["npm:unreviewed-extension@latest"];
  fixture.manifest.extensions[0].dependencies.runtime.transitive = ["transitive-unreviewed-package"];
  writeFileSync(
    path.join(fixture.root, "product/Halo Studio/pnpm-lock.yaml"),
    "@earendil-works/pi-coding-agent: 0.83.0\\n",
  );
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "runtime-dependency-present"));
  assert.ok(report.findings.some((finding) => finding.code === "transitive-dependency-present"));
  assert.ok(report.findings.some((finding) => finding.code === "runtime-dependency-in-lockfile"));
});

test("complete host closure and included host license claims require release evidence files", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.host.dependencyClosure.evidencePath = "missing-host-closure.txt";
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.releaseFiles = [{ path: "missing-host-license.txt" }];
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "host-dependency-closure-evidence-missing"));
  assert.ok(report.findings.some((finding) => finding.code === "host-license-release-file-missing"));
});

test("host closure entries and license claims cannot be empty or ungrounded", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.host.dependencyClosure.direct = [];
  fixture.manifest.extensions[0].dependencies.host.dependencyClosure.transitive = [];
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.requiredText = [];
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.releaseFiles[0].requiredText = [];
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "host-dependency-closure-empty"));
  assert.ok(report.findings.some((finding) => finding.code === "host-license-evidence-claims-missing"));
  assert.ok(report.findings.some((finding) => finding.code === "host-license-release-claims-missing"));
});

test("host license claims require SPDX markers and attribution text", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.observedSpdx = "Apache-2.0";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "host-license-spdx-text-missing"));
});

test("host license evidence cannot reuse Halo extension license files", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.evidencePath = "product/Halo Studio/LICENSE";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "host-license-evidence-misclassified"));
});

test("host license evidence path aliases are compared after repository resolution", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.evidencePath = "product/Halo Studio/./nested/../LICENSE";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "host-license-evidence-misclassified"));
});

test("host license release files cannot reuse Halo extension license files through path aliases", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.releaseFiles = [{
    path: "product/Halo Studio/./LICENSE",
    sha256: fixture.manifest.extensions[0].license.evidence[0].sha256,
    size: fixture.manifest.extensions[0].license.evidence[0].size,
    requiredText: ["MIT License", "Copyright (c) 2026 CWing"],
  }];
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "host-license-release-file-misclassified"));
});

test("host license evidence cannot reuse extension distribution or release artifact files", () => {
  const fixture = createFixture();
  const distributionPath = "product/Halo Studio/extension-distribution-notices.txt";
  const distributionContents = "MIT License\\nCopyright (c) Pi fixture\\nPermission is hereby granted, free of charge, to any person obtaining a copy\\n";
  writeFileSync(path.join(fixture.root, distributionPath), distributionContents);
  const distributionSha256 = createHash("sha256").update(distributionContents).digest("hex");
  fixture.manifest.extensions[0].license.distributionFiles = [{
    path: distributionPath,
    sha256: distributionSha256,
    size: Buffer.byteLength(distributionContents),
    requiredText: ["MIT License", "Copyright (c) Pi fixture"],
  }];
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.evidencePath = distributionPath;
  fixture.manifest.extensions[0].dependencies.host.licenseEvidence.releaseFiles = [
    fixture.manifest.extensions[0].license.releaseArtifactEvidence,
  ];
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "host-license-evidence-misclassified"));
  assert.ok(report.findings.some((finding) => finding.code === "host-license-release-file-misclassified"));
});

test("floating version and incomplete provenance are blocked", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].fixedVersion = "latest";
  fixture.manifest.extensions[0].sourceCommit = "e8c445d6";
  fixture.manifest.extensions[0].gitHashObject = "";
  fixture.manifest.extensions[0].sha256 = "not-a-sha";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-version-unpinned"));
  assert.ok(report.findings.some((finding) => finding.code === "extension-source-commit-unpinned"));
  assert.ok(report.findings.some((finding) => finding.code === "git-hash-object-unpinned"));
  assert.ok(report.findings.some((finding) => finding.code === "sha256-unpinned"));
});

test("source provenance binds the declared commit tree", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].sourceTree = "f".repeat(40);
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "source-commit-tree-mismatch"));
});

test("the extension version is bound to the adapter identity", () => {
  const fixture = createFixture();
  fixture.manifest.extensions[0].fixedVersion = "1.0.1";
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "extension-version-adapter-mismatch"));
});

test("an upstream candidate must be verifiable from its read-only reference tree", () => {
  const fixture = createFixture();
  const evidencePath = path.join(fixture.root, "docs", "issue-13-upstream-sync-candidate.json");
  mkdirSync(path.dirname(evidencePath), { recursive: true });
  const evidence = {
    base: { commit: "a".repeat(40) },
    candidate: {
      referenceRoot: path.join(fixture.root, "missing-reference-tree"),
      commit: "b".repeat(40),
      tree: "c".repeat(40),
      branch: "main",
    },
    comparison: { baseEntries: 1, candidateEntries: 1, identicalBlobOrModeEntries: 1, modifiedEntries: 0, addedEntries: 0, removedEntries: 0, changedEntries: 0 },
    conflictsAndDecisions: { mergeAttempted: false, automaticMerge: false, upstreamWrite: false },
    releaseGate: { status: "blocked", evidenceGaps: ["missing reference"] },
  };
  writeFileSync(evidencePath, JSON.stringify(evidence, null, 2));
  fixture.manifest.upstreamCandidateEvidence.path = "docs/issue-13-upstream-sync-candidate.json";
  fixture.manifest.upstreamCandidateEvidence.candidateCommit = evidence.candidate.commit;
  fixture.manifest.upstreamCandidateEvidence.baseCommit = evidence.base.commit;
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-reference-tree-unavailable"));
});

test("upstream evidence must record HEAD^ and clean-status results", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const candidateEvidencePath = path.join(fixture.root, fixture.manifest.upstreamCandidateEvidence.path);
  const candidateEvidence = JSON.parse(readFileSync(candidateEvidencePath, "utf8"));
  delete candidateEvidence.candidate.parentEvidence;
  delete candidateEvidence.candidate.statusEvidence;
  writeFileSync(candidateEvidencePath, JSON.stringify(candidateEvidence, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-parent-evidence-missing"));
  assert.ok(report.findings.some((finding) => finding.code === "upstream-status-evidence-missing"));
});

test("upstream status evidence must retain a non-empty command result", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const candidateEvidencePath = path.join(fixture.root, fixture.manifest.upstreamCandidateEvidence.path);
  const candidateEvidence = JSON.parse(readFileSync(candidateEvidencePath, "utf8"));
  delete candidateEvidence.candidate.statusEvidence.result;
  writeFileSync(candidateEvidencePath, JSON.stringify(candidateEvidence, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-status-evidence-missing"));
});

test("upstream status evidence result must match the clean flag", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const candidateEvidencePath = path.join(fixture.root, fixture.manifest.upstreamCandidateEvidence.path);
  const candidateEvidence = JSON.parse(readFileSync(candidateEvidencePath, "utf8"));
  candidateEvidence.candidate.statusEvidence.result = "dirty; stale result";
  writeFileSync(candidateEvidencePath, JSON.stringify(candidateEvidence, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-status-evidence-result-mismatch"));
});

test("upstream parent evidence result must match a resolved HEAD^ check", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const candidateEvidencePath = path.join(fixture.root, fixture.manifest.upstreamCandidateEvidence.path);
  const candidateEvidence = JSON.parse(readFileSync(candidateEvidencePath, "utf8"));
  candidateEvidence.candidate.parentEvidence.result = "stale result";
  writeFileSync(candidateEvidencePath, JSON.stringify(candidateEvidence, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-parent-evidence-result-mismatch"));
});

test("upstream evidence must validate Halo retention and conflict decisions", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const evidencePath = path.join(fixture.root, "docs", "issue-13-upstream-sync-candidate.json");
  const evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  evidence.conflictsAndDecisions.retainedPiRpcPort = [];
  writeFileSync(evidencePath, JSON.stringify(evidence, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-retention-decision-missing"));
});

test("upstream evidence must enumerate each prohibited action category", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const evidencePath = path.join(fixture.root, "docs", "issue-13-upstream-sync-candidate.json");
  const evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  evidence.conflictsAndDecisions.prohibitedActions = ["No automatic merge was performed."];
  writeFileSync(evidencePath, JSON.stringify(evidence, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-prohibited-action-detail-missing"));
});

test("an unresolved upstream base cannot pass an incremental-sync audit", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const evidencePath = path.join(fixture.root, "docs", "issue-13-upstream-sync-candidate.json");
  const evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  const unresolvedBase = "a".repeat(40);
  evidence.base.commit = unresolvedBase;
  evidence.base.resolution = {
    status: "unresolved",
    resolvedCommit: null,
    objectType: null,
    exitCode: 128,
  };
  evidence.ancestry = { status: "unproven", exitCode: 128 };
  writeFileSync(evidencePath, JSON.stringify(evidence, null, 2));
  const initialImportPath = path.join(fixture.root, "docs", "initial-import.json");
  const initialImport = JSON.parse(readFileSync(initialImportPath, "utf8"));
  initialImport.upstream.commit = unresolvedBase;
  writeFileSync(initialImportPath, JSON.stringify(initialImport, null, 2));
  fixture.manifest.upstreamCandidateEvidence.baseCommit = unresolvedBase;
  writeFileSync(fixture.manifestPath, JSON.stringify(fixture.manifest, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-base-commit-unresolved"));
  assert.ok(report.findings.some((finding) => finding.code === "upstream-ancestry-unproven"));
});

test("the initial-import tree is bound to the recorded file manifest", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const evidencePath = path.join(fixture.root, "docs", "issue-13-upstream-sync-candidate.json");
  const evidence = JSON.parse(readFileSync(evidencePath, "utf8"));
  evidence.base.initialImportTree = "b".repeat(40);
  writeFileSync(evidencePath, JSON.stringify(evidence, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-initial-import-tree-mismatch"));
});

test("upstream path records reject invalid tree entry evidence", () => {
  const fixture = createUpstreamFixture();

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-diff-record-entry-invalid"));
});

test("upstream path records must match the fresh base and candidate tree entries", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });
  const recordsPath = path.join(fixture.root, "docs", "issue-13-upstream-sync-diff.json");
  const records = JSON.parse(readFileSync(recordsPath, "utf8"));
  records.records = [{
    path: "candidate.txt",
    status: "modified",
    base: { mode: "100644", type: "blob", blob: "a".repeat(40), size: 0 },
    candidate: { mode: "100644", type: "blob", blob: "b".repeat(40), size: 0 },
  }];
  writeFileSync(recordsPath, JSON.stringify(records, null, 2));

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.ok(report.findings.some((finding) => finding.code === "upstream-diff-record-entry-mismatch"));
});

test("a complete local evidence fixture passes the audit baseline", () => {
  const fixture = createUpstreamFixture({ invalidRecord: false });

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "passed", JSON.stringify(report.findings, null, 2));
  assert.deepEqual(report.findings, []);
});

test("a declared blocked release gate remains blocked even when static evidence is complete", () => {
  const fixture = createFixture({
    releaseGate: {
      status: "blocked",
      blockingReasons: ["candidate has not passed the release matrix"],
    },
  });

  const report = auditInventory({ manifestPath: fixture.manifestPath, repoRoot: fixture.root });

  assert.equal(report.status, "blocked");
  assert.ok(report.findings.some((finding) => finding.code === "release-gate-declared-blocked"));
});
