import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  validateOldSixBehaviorEquivalence,
  renderOldSixBehaviorEquivalence,
  validateNativeUiStatusArtifact,
  verifyOldSixBehaviorEquivalence,
} from './verify-old-six-behavior-equivalence.mjs';

const matrixPath = fileURLToPath(
  new URL('../../../docs/verification/issue-12-old-six-behavior-equivalence.json', import.meta.url),
);
const reportPath = fileURLToPath(
  new URL('../../../docs/verification/issue-12-old-six-behavior-equivalence.md', import.meta.url),
);
const nativeUiStatusArtifactPath = fileURLToPath(
  new URL('../../../docs/verification/issue-12-real-native-ui-acceptance-status.json', import.meta.url),
);

function readMatrix() {
  return JSON.parse(readFileSync(matrixPath, 'utf8'));
}

test('the Issue 12 matrix maps each historical issue once and blocks unrun native acceptance', () => {
  const summary = verifyOldSixBehaviorEquivalence();

  assert.deepEqual(summary.legacyIssues, [9, 10, 11, 12, 13, 14]);
  assert.deepEqual(summary.p0Issues, ['04', '05', '06', '07', '08', '09', '10', '11']);
  assert.equal(summary.releaseStatus, 'blocked');
  assert.equal(summary.realNativeUiStatus, 'not-run');
});

test('the matrix rejects duplicate legacy issues or missing P0 coverage', () => {
  const duplicateLegacyIssue = readMatrix();
  duplicateLegacyIssue.entries[1].legacyIssue = 9;
  assert.throws(
    () => validateOldSixBehaviorEquivalence(duplicateLegacyIssue),
    /legacy issues/,
  );

  const missingP0Issue = readMatrix();
  missingP0Issue.entries[5].p0Issues = ['10'];
  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingP0Issue),
    /P0 issue coverage/,
  );
});

test('the matrix rejects a historical issue remapped to the wrong P0 work', () => {
  const remappedP0Work = readMatrix();
  remappedP0Work.entries[0].p0Issues = ['08'];
  remappedP0Work.entries[2].p0Issues = ['04', '05'];

  assert.throws(
    () => validateOldSixBehaviorEquivalence(remappedP0Work),
    /GitHub #9 P0 mapping/,
  );
});

test('the matrix binds every historical issue to its canonical GitHub evidence', () => {
  const mismatchedHistoricalEvidence = readMatrix();
  mismatchedHistoricalEvidence.entries[0].legacyEvidence[0].locator
    = 'https://github.com/Nyzeep/Halo-Studio/issues/10';

  assert.throws(
    () => validateOldSixBehaviorEquivalence(mismatchedHistoricalEvidence),
    /canonical GitHub #9 locator/,
  );
});

test('the matrix rejects a missing excluded evidence authority', () => {
  const incompleteExclusions = readMatrix();
  incompleteExclusions.excludedEvidenceAuthorities = incompleteExclusions.excludedEvidenceAuthorities
    .filter((authority) => authority !== 'pi-internal-source');

  assert.throws(
    () => validateOldSixBehaviorEquivalence(incompleteExclusions),
    /excluded evidence authorities/,
  );
});

test('the matrix keeps legacy Pi transport alternatives out of the P0 proof', () => {
  const incompleteExclusions = readMatrix();
  incompleteExclusions.excludedEvidenceAuthorities = incompleteExclusions.excludedEvidenceAuthorities
    .filter((authority) => authority !== 'pi-tui');

  assert.throws(
    () => validateOldSixBehaviorEquivalence(incompleteExclusions),
    /excluded evidence authorities/,
  );
});

test('the matrix requires a native Tauri desktop path for every legacy issue', () => {
  const missingNativeDesktopPath = readMatrix();
  missingNativeDesktopPath.entries[2].desktopPathEvidence = missingNativeDesktopPath.entries[2]
    .desktopPathEvidence
    .filter((evidence) => evidence.kind !== 'tauri-command-event-contract');

  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingNativeDesktopPath),
    /native Tauri desktop path/,
  );
});

test('the matrix rejects an unclassified non-passing evidence result', () => {
  const unclassifiedFailure = readMatrix();
  delete unclassifiedFailure.entries[0].desktopPathEvidence[0].classification;

  assert.throws(
    () => validateOldSixBehaviorEquivalence(unclassifiedFailure),
    /classification/,
  );
});

test('the matrix rejects unknown current evidence statuses', () => {
  const unknownStatus = readMatrix();
  unknownStatus.entries[0].desktopPathEvidence[0].status = 'inconclusive';

  assert.throws(
    () => validateOldSixBehaviorEquivalence(unknownStatus),
    /known execution status/,
  );
});

test('the matrix rejects current evidence that uses excluded authorities', () => {
  const excludedEvidence = readMatrix();
  excludedEvidence.entries[0].piRpcAdapterEvidence[0].kind = 'legacy-sidecar-jsonl';

  assert.throws(
    () => validateOldSixBehaviorEquivalence(excludedEvidence),
    /accepted current evidence kind/,
  );
});

test('the matrix binds current evidence to recorded verification outcomes', () => {
  const unrecordedCommand = readMatrix();
  unrecordedCommand.entries[0].piRpcAdapterEvidence[0].command
    = 'cargo test --manifest-path "product/Halo Studio/Cargo.toml" -p unknown';
  assert.throws(
    () => validateOldSixBehaviorEquivalence(unrecordedCommand),
    /verification provenance/,
  );

  const mismatchedStatus = readMatrix();
  mismatchedStatus.entries[0].piRpcAdapterEvidence[0].status = 'blocked';
  mismatchedStatus.entries[0].piRpcAdapterEvidence[0].classification = 'fabricated-status';
  assert.throws(
    () => validateOldSixBehaviorEquivalence(mismatchedStatus),
    /verification provenance/,
  );
});

test('the matrix binds each evidence locator to a file exercised by its exact command', () => {
  const mismatchedLocatorCommand = readMatrix();
  mismatchedLocatorCommand.entries[0].piRpcAdapterEvidence[0].locator
    = 'product/Halo Studio/src/crates/adapters/pi-rpc-adapter/tests/pi_rpc_contract.rs::version_probe_uses_private_config_and_cleans_it_on_success_or_failure';

  assert.throws(
    () => validateOldSixBehaviorEquivalence(mismatchedLocatorCommand),
    /exercised by its exact command/,
  );
});

test('the matrix rejects current evidence outside the Halo product tree', () => {
  const legacyLocator = readMatrix();
  legacyLocator.entries[0].piRpcAdapterEvidence[0].locator
    = 'docs/archive/legacy-pyside-sidecar-baseline/requirements.json::fake';

  assert.throws(
    () => validateOldSixBehaviorEquivalence(legacyLocator),
    /Halo product locator/,
  );
});

test('the matrix requires current evidence locators to resolve to a real test anchor', () => {
  const missingFile = readMatrix();
  missingFile.entries[0].piRpcAdapterEvidence[0].locator
    = 'product/Halo Studio/src/crates/does-not-exist.rs::missing_test';
  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingFile),
    /locator file/,
  );

  const missingAnchor = readMatrix();
  missingAnchor.entries[0].piRpcAdapterEvidence[0].locator
    = 'product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::missing_test';
  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingAnchor),
    /test anchor/,
  );

  const emptyAnchor = readMatrix();
  emptyAnchor.entries[0].piRpcAdapterEvidence[0].locator
    = 'product/Halo Studio/src/crates/execution/agent-runtime/tests/workbench_runtime_contracts.rs::';
  assert.throws(
    () => validateOldSixBehaviorEquivalence(emptyAnchor),
    /test anchor/,
  );
});

test('the matrix requires classified execution provenance', () => {
  const missingVerification = readMatrix();
  delete missingVerification.verification;
  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingVerification),
    /verification/,
  );
});

test('the matrix requires every Issue 12 verification command', () => {
  const missingRequiredCommand = readMatrix();
  missingRequiredCommand.verification = missingRequiredCommand.verification
    .filter((result) => result.command !== 'git diff --check');

  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingRequiredCommand),
    /verification commands/,
  );
});

test('the matrix rejects a zero exit code for blocked verification', () => {
  const inconsistentBlockedResult = readMatrix();
  const desktopResult = inconsistentBlockedResult.verification.find((result) => (
    result.command.includes('bitfun-desktop')
  ));
  desktopResult.exitCode = 0;

  assert.throws(
    () => validateOldSixBehaviorEquivalence(inconsistentBlockedResult),
    /exitCode/,
  );
});

test('the matrix requires evidence locators for release conclusions', () => {
  const missingNativeAcceptanceEvidence = readMatrix();
  delete missingNativeAcceptanceEvidence.realNativeUiAcceptance.evidence;
  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingNativeAcceptanceEvidence),
    /realNativeUiAcceptance\.evidence/,
  );

  const missingEntryConclusionEvidence = readMatrix();
  delete missingEntryConclusionEvidence.entries[0].conclusion.evidence;
  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingEntryConclusionEvidence),
    /GitHub #9\.conclusion\.evidence/,
  );
});

test('the matrix binds release conclusions to the deidentified Issue 14 status artifact', () => {
  const wrongNativeAcceptanceArtifact = readMatrix();
  wrongNativeAcceptanceArtifact.realNativeUiAcceptance.evidence[0].locator
    = 'docs/verification/other-status.json';
  assert.throws(
    () => validateOldSixBehaviorEquivalence(wrongNativeAcceptanceArtifact),
    /realNativeUiAcceptance\.evidence/,
  );

  const wrongEntryConclusionArtifact = readMatrix();
  wrongEntryConclusionArtifact.entries[0].conclusion.evidence[0].locator
    = 'docs/verification/other-status.json';
  assert.throws(
    () => validateOldSixBehaviorEquivalence(wrongEntryConclusionArtifact),
    /GitHub #9\.conclusion\.evidence/,
  );
});

test('the matrix rejects sensitive native acceptance reason text', () => {
  for (const reason of [
    'Authorization=sk-test-secret',
    'Bearer secret-value',
    'Authorization Bearer abc123',
    'token abc123',
  ]) {
    const sensitiveReason = readMatrix();
    sensitiveReason.realNativeUiAcceptance.reason = reason;

    assert.throws(
      () => validateOldSixBehaviorEquivalence(sensitiveReason),
      /sensitive/,
    );
  }
});

test('the native acceptance artifact requires canonical Issue 14 provenance', () => {
  const artifact = JSON.parse(readFileSync(nativeUiStatusArtifactPath, 'utf8'));

  const wrongOwner = { ...artifact, ownerIssue: '12' };
  assert.throws(() => validateNativeUiStatusArtifact(wrongOwner), /owned by Issue 14/);

  const wrongSource = { ...artifact, sourceLocator: 'docs/verification/other-status.json' };
  assert.throws(() => validateNativeUiStatusArtifact(wrongSource), /canonical Issue 14 source/);
});

test('the readable report is generated from the checked matrix without drift', () => {
  const matrix = readMatrix();
  const report = readFileSync(reportPath, 'utf8');

  assert.equal(report, renderOldSixBehaviorEquivalence(matrix));
  assert.match(report, /GitHub #9/);
  assert.match(report, /GitHub #14/);
  assert.match(report, /real-native-ui-not-run/);
});
