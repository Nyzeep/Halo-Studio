# Halo Studio 2.0 Rewrite — Implementation Handoff (2026-09-05)

> Audience: future agents picking up this repository. Chinese edition (for the owner): [halo-2-handoff-20260905.md](halo-2-handoff-20260905.md).
> Status: M1–M5 complete and merged to `main` (merge commit `b800cad32`, lockfile follow-up `672a73991`); M6 gated behind human acceptance.
> This document is the **factual handoff**: what was built, where it lives, how to verify, what must not change, and what remains.

## 1. What this effort was (four locked premises)

1. **Execution base**: Halo Workbench Runtime stays the single Rust authority; a new `halo-dsh-adapter` (DSH acp main channel) joins `halo-pi-rpc-adapter` as a **same-tier managed adapter**, both behind a unified `ManagedExecutorPort`; executor selection and task-level handover are restored (ADR-0078, supersedes ADR-0072).
2. **Frontend**: full rewrite onto the niri strip spatial model + DMS (DankMaterialShell) MD3 token language — workspace rail + task strip + overview + P0 gesture set; stack retained on React 18 + Vite + zustand + Tauri v2 (ADR-0076/0077, supersede ADR-0027/0018).
3. **Fact model**: the "never fabricate history" principle (ADR-0080, amends 0075) — attempts recorded separately, cancellation lands the delivered prefix + `interrupted`, committed-granularity facts only, a single redaction gate.
4. **Migration**: the legacy P0 acceptance chain was frozen as tag `migration-baseline-20260905`; the BitFun upstream tree is read-only (guard-enforced) until the new base passes its first real acceptance, then deleted wholesale (ADR-0079, issue #58 still open).

Decision trail: decision map [#32](https://github.com/Nyzeep/Halo-Studio/issues/32) (14/14 decision tickets closed; "Decisions so far" is the index of every resolution) → spec [#52](https://github.com/Nyzeep/Halo-Studio/issues/52) (`docs/requirements/halo-2-rewrite/00-spec.md`, testable acceptance for M1–M6) → implementation tickets #53–#58 (#53–#57 closed, #58 gated) → [PR #59](https://github.com/Nyzeep/Halo-Studio/pull/59) merged.

Research inputs (primary sources with citations — **read these before touching code**):
- `docs/architecture/niri-interaction-research-20260905.md` (niri interaction model and transposition table; **niri is GPL-3.0 — borrow paradigms, never copy code**)
- `docs/architecture/dms-design-language-research-20260905.md` (DMS design language; **MIT, portable with attribution**)
- `docs/architecture/dsh-upstream-state-research-20260905.md` (DSH architecture, candidates A–E)
- `docs/architecture/dsh-adapter-protocol-research-20260905.md` (ACP/sdk protocol details with DSH source file:line citations)
- `docs/architecture/pi-upstream-capabilities-research-20260905.md` (full pi RPC surface and extraction points)

## 2. Where things live (read before editing)

### Rust base (`product/Halo Studio/src/crates/`)

| Location | Contents |
|---|---|
| `contracts/runtime-ports/src/managed_executor.rs` | **`ManagedExecutorPort`** trait (prompt/follow_up/abort/get_entries/approval flow/event projection/subscribe) + `ManagedExecutorCapabilityProfile` (honest flags: steer, queue_events, approval_channel, entry_read…) + closed approval enum (`allowed-once|rejected|cancelled|unavailable`, default unavailable = fail-closed) + sandbox contract layer (mode enum + `SandboxEnforcement: full\|partial` honest reporting) + `normalize_managed_event_summary` **single redaction gate** + `ManagedExecutorKind {PiRpc, Dsh}` closed set |
| `execution/agent-runtime/src/managed_event_facts.rs` | Fact model v2 (`MANAGED_EVENT_FACT_SCHEMA_VERSION = 2`, v1 facts remain readable): three core kinds (user-message summary / agent-reply summary / tool activity) + new `AttemptFailed` (independently counted, excluded from model-visible rebuild) + `TaskInterrupted` (cancel lands delivered prefix); streaming frames never become facts |
| `adapters/pi-rpc-adapter/src/managed_executor.rs` | `PiRpcManagedExecutor` thin wrapper implementing the unified port; capability profile derived from `PiRpcPort::readiness()` facts (all false when unprobed) |
| `adapters/pi-rpc-adapter/src/lib.rs` | `SUPPORTED_PI_RPC_PROFILES` includes **0.85.0**; `steer` (only on 0.85 with a running turn); `queue_update` projection; `PiRpcInstallSource` pinning (`@earendil-works` accepted / `@mariozechner` rejected); `PI_RPC_CONSUMED_COMMAND_TYPES` + single-exit chokepoint (`bash`/`abort_bash` structurally cannot reach stdin) |
| `adapters/dsh-adapter/` (new crate) | `DshAdapter` (one controlled process per task, Windows Job Object, cancel ladder) + `acp.rs` (JSON-RPC client, requestPermission → unified decision mapping, unknown-update filtering) + `profile.rs` (anchored to 0.1.3-alpha.1, acp+sdk channels) + `credentials.rs` (CredentialRef env injection, `DSH_HOME` isolation, `.env` never a channel) + `managed_executor.rs` (implements the port; sdk canary degradation keeps `approval_channel=false` with an unbroken event surface) |
| `execution/agent-runtime/src/halo_workbench.rs` | `dispatch_managed_executor_action()`: managed-session prompt/follow_up/abort go through the unified port; `create_session(executor_override)` binds once into session + task baseline (serde default, backwards compatible); `install_managed_executor()` / `available_managed_executors()` / workspace default executor |
| `assembly/core/src/halo_workbench.rs` + `Cargo.toml` | Composition root assembles `PiRpcManagedExecutor`; `halo-dsh-adapter` is wired via the optional **`dsh-executor`** feature (off by default) |

### Frontend (`product/Halo Studio/src/web-ui/src/`)

| Location | Contents |
|---|---|
| `tokens/` | **The only visual entry point**: `tokens.css` (MD3-named CSS custom properties: 20 color roles × `data-theme` dual themes, 3 radii / 5 spacings / 4 font sizes × `--font-scale`, 3 motion durations/easings) + `theme.ts`/`themeStore.ts` |
| `workbench/state/` | Strict dual-store split: `workbenchRuntimeStore` (fact-event projection, sequence ring, stale-seq guard) vs `workbenchUiStore` (focus/overview/gesture transients); `workbenchUiBoundary.ts` triple boundary assertion |
| `workbench/components/` | `WorkbenchShell` (full keyboard set ←→/n/o/1..9/Esc + Ctrl/⌘+K), `WorkspaceRail` (always-present "new" slot), `TaskStrip` (fixed column width, new column inserts right of focus with **zero re-layout**), `TaskColumn` (session flow + activity chips + unified operation-request card + delivery review), `Overview` (grouped + paginated), `CommandPalette` (executor choice only at creation), `WorkbenchSurfaces` (Git/settings container placeholders) |
| `workbench/workbenchGate.ts` | Feature gate: strip workbench by default; `sessionStorage['halo:workbench-view']='legacy'` rolls back |
| `app/layout/AppLayout.tsx` | Conditional mount (`isHaloLocalCodingScope() && isStripWorkbenchEnabled()`), lazy import |
| `scripts/check-style-tokens.mjs` | No-bare-value check (colors/spacings/radii must use tokens; 342 legacy files exempted, shrink-only) |

### Governance and guards

- `scripts/check-repo-hygiene.mjs`: **frozen-path guard** — any diff/untracked file under `product/Halo Studio/vendor/`, `halo-scope.json`, `MiniApp/`, `BitFun-latest/` turns it red (ADR-0079)
- `scripts/core-boundaries/`: public-API allowlist regime (new pub symbols must be registered in `public-api-rules.mjs`), dependency layering (apps may compose apps; vendor/installer are not layer subjects), forbidden-import rules (adapter contract-test dirs are in allowPaths)
- Migration baseline tag `migration-baseline-20260905`: the behavioral-equivalence reference — **never rewrite or delete**

## 3. How to verify (the standard loop for any change)

```powershell
# Rust (from product/Halo Studio)
cargo test -p halo-runtime-ports -p halo-agent-runtime -p halo-services-core -p halo-pi-rpc-adapter -p halo-dsh-adapter
node scripts/check-core-boundaries.mjs      # public API/layering; register new pub symbols first
node scripts/check-repo-hygiene.mjs         # frozen-path guard + hygiene

# Frontend (from product/Halo Studio/src/web-ui)
npm run test:run        # 2433 tests
npm run type-check
npm run lint            # 0 errors (2 pre-existing warnings in legacy files); runs check-style-tokens

# Strip workbench preview
npm run dev             # http://localhost:1422 (mock-driver driven; new view unless the legacy sessionStorage key is set)
```

Baseline numbers: Rust ≥826 (four crates at M1) + dsh 26 + 695 across three crates after M3; frontend 2433. **Any drop is a regression.**

## 4. Hard lines (agent red lines)

1. **Frozen paths**: `vendor/`, `halo-scope.json`, `MiniApp/`, `BitFun-latest/` — the guard goes red; deleting them is exclusively M6 (#58).
2. **The migration baseline tag** must not be rewritten; historical evidence files (`docs/verification/`) are never edited retroactively.
3. **Baseline contract tests must not regress**: pi_configuration_contract 11, pi_rpc_contract 51, managed_executor_contracts 12 (pi) + 15 (dsh), workbench_runtime_contracts 58.
4. **Public API discipline**: new pub symbols in crates require a matching entry in `public-api-rules.mjs` (with owner/consumer/verification metadata) — otherwise check-core-boundaries fails.
5. **No bare style values**: new styles must use tokens; legacy `.scss` shrinks only (zero before M6).
6. **Glossary authority**: `CONTEXT.md` terms 主执行器/受管执行器/执行器交接 were rewritten per the dual-adapter decision — new terms go through the `/domain-modeling` flow; implementation details never enter CONTEXT.md.
7. **One-time decisions are never relaxed** (ADR-0012); both executors render as the same "Agent operation request" card.
8. **niri is GPL-3.0**: paradigms yes, code no; DMS is MIT, keep attribution (ADR-0052).

## 5. What remains (by priority)

1. **M6 (#58, human gate)**: after a real dual-adapter managed-task main chain passes (create → decision → delivery review → accept/reject, entirely in the new UI) → delete `vendor/`, `halo-scope.json`, `MiniApp/`, `BitFun-latest/` wholesale → remove the sass dependency → converge the guard into a regression assertion. **Do not start before the human gate opens.**
2. **UI visual redesign**: the owner will run a visual pass with Gemini Studio. Handoff in one line: **only edit `tokens/` and component CSS Modules; never touch `.tsx` logic or the `workbench/` structure**; the 2433 tests + lint are the regression net.
3. **M5 leftovers** (issue #57 report): real Git panel/settings content (containers are placeholders), in-session command execution chain (ADR-0030), real Tauri driver replacing the mock (`workbenchRuntimeStoreDriver.ts` is the seam), real dsh binary acceptance (the `$/cancelRequest` parameter shape needs correction), strip virtualization (explicitly deferred in the spec).
4. **P1 extraction points** (#42 resolution): in-run model/thinking-level switching, `fork`/`new_session(parentSession)` attribution chains, compaction, session_stats/export (must pass the redaction gate), image attachments.
5. **Known pre-existing issues**: Halo-Installer dependency resolution sits outside the product vendor flow (excluded from the layering check; its build flow is an M6 follow-up); 2 pre-existing web-ui lint warnings (`pi-configuration/client.ts`, `infrastructure/workbench-runtime/store.ts`).

## 6. Working-style advice for future agents

- **Read the research docs before the code** — they are cited primary-source summaries and save large amounts of exploration tokens (one exploratory agent in this effort burned 21M tokens).
- Run agents serially with an explicit tool-call budget; wide parallel runs collide with the user's quota limit.
- The implementation-slice template lives in ticket bodies #53–#57 (must-read list / hard constraints / acceptance / "do not commit" — the main session reviews and commits).
- For decision-shaped questions use the decision-map pattern (ticket + resolution comment + map index); hard-to-reverse outcomes become ADRs.
