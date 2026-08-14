import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  validateOldSixBehaviorEquivalence,
  renderOldSixBehaviorEquivalence,
  verifyOldSixBehaviorEquivalence,
} from './verify-old-six-behavior-equivalence.mjs';

const matrixPath = fileURLToPath(
  new URL('../../../docs/verification/issue-12-old-six-behavior-equivalence.json', import.meta.url),
);
const reportPath = fileURLToPath(
  new URL('../../../docs/verification/issue-12-old-six-behavior-equivalence.md', import.meta.url),
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

test('the matrix requires classified execution provenance', () => {
  const missingVerification = readMatrix();
  delete missingVerification.verification;
  assert.throws(
    () => validateOldSixBehaviorEquivalence(missingVerification),
    /verification/,
  );
});

test('the readable report is generated from the checked matrix without drift', () => {
  const matrix = readMatrix();
  const report = readFileSync(reportPath, 'utf8');

  assert.equal(report, renderOldSixBehaviorEquivalence(matrix));
  assert.match(report, /GitHub #9/);
  assert.match(report, /GitHub #14/);
  assert.match(report, /real-native-ui-not-run/);
});
