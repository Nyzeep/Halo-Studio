# Config Hardening Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining configuration read, replace, durability, lifecycle, registration, and public-contract findings without weakening existing conflict and unsafe-path errors.

**Architecture:** TargetRegistry owns bounded handle-based reads and immutable target registration snapshots. Atomic writes expose explicit stages and compare parent/temp identities around rename; ConfigTransaction consumes those stages, maintains bounded encrypted backup history, and restores missing originals by guarded deletion. A package security contract records the pure-Node rename residual risk and directory ownership prerequisite.

**Tech Stack:** TypeScript 5.7, Node.js fs handles, Vitest 2.1, jsonc-parser, npm workspaces.

---

### Task 1: Handle-bound reads

**Files:**
- Modify: `packages/config/src/targetRegistry.ts`
- Modify: `packages/config/src/pathPolicy.test.ts`

- [ ] Add RED tests that replace the target and parent between handle open, read, and path revalidation, plus an oversized-file test.
- [ ] Run `npx vitest run src/pathPolicy.test.ts` and confirm identity/size assertions fail.
- [ ] Implement bounded handle reads using `open`, `FileHandle.stat`, path `stat`, target/parent realpath revalidation, and post-read identity checks.
- [ ] Re-run the path tests and confirm GREEN.

### Task 2: Atomic replace stages and durability

**Files:**
- Modify: `packages/config/src/atomicWrite.ts`
- Modify: `packages/config/src/configTransaction.ts`
- Modify: `packages/config/src/configTransaction.test.ts`

- [ ] Add RED tests for parent/temp identity replacement, rename-success plus directory-sync failure, and mode preservation.
- [ ] Run focused atomic/transaction tests and confirm stage assertions fail.
- [ ] Implement parent/temp identity checks, explicit `replaced` and `durability-failed` errors, guarded cleanup, and transaction recovery based on the current fingerprint.
- [ ] Re-run focused tests and confirm GREEN.

### Task 3: Missing rollback and bounded lifecycle

**Files:**
- Modify: `packages/config/src/configTransaction.ts`
- Modify: `packages/config/src/configTransaction.test.ts`

- [ ] Add RED tests for rollback-to-absence, rollback conflict, 1 MiB read/preview limits, fake-timer expiry, dispose, backup eviction/deletion, and audit immutability/bounds.
- [ ] Run focused transaction tests and confirm failures.
- [ ] Add timers with `unref`, retained-byte accounting, bounded per-target backups/audit, vault deletion on eviction, and guarded deletion for originally absent targets.
- [ ] Re-run focused tests and confirm GREEN.

### Task 4: Registration snapshots and independent roots

**Files:**
- Modify: `packages/config/src/targetRegistry.ts`
- Modify: `packages/config/src/pathPolicy.test.ts`

- [ ] Add RED proxy/getter mutation tests and four-target independent-root tests across CJK/space paths.
- [ ] Run focused path tests and confirm failures.
- [ ] Snapshot each input field once inside one safe boundary, freeze stored/returned records, and accept one allowed root per default target.
- [ ] Re-run focused tests and confirm GREEN.

### Task 5: Contract, dependency cleanup, and verification

**Files:**
- Create: `packages/config/SECURITY.md`
- Modify: `packages/config/package.json`
- Modify: `package-lock.json`
- Modify: `packages/config/src/index.ts`

- [ ] Document the exclusive-write parent-directory prerequisite, remaining pure-Node rename window, POSIX mode behavior, and Windows ACL limitation.
- [ ] Remove unused `write-file-atomic` and narrow exports that Main does not need.
- [ ] Run `npm test --workspace @halo-studio/config`, config typecheck/build, root verify, audit, and `git diff --check`.
- [ ] Commit all reviewed changes with a Chinese commit message and do not push.
