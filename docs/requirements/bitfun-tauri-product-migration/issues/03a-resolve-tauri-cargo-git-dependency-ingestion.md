# 03a - Resolve Tauri Cargo Git Dependency Ingestion

**What to decide and prove:** Restore a reproducible Cargo dependency-ingestion path for the existing Tauri and Tao Git pins, then refresh `Cargo.lock` only through Cargo's normal resolver. This is an inserted unblocker for Ticket 03; it does not implement the Halo runtime contract or start Ticket 04.

**Blocked by:** 02 is complete. This ticket needs a one-time user authorization before any public dependency download, proxy/network change, local vendor import, or dependency-source change.

**Blocks:** 03 - Launch Halo-branded Tauri workbench. Ticket 04 remains transitively blocked by 03.

**Status:** ready-for-review

**2026-07-30 recommended-path A execution:** In the VS x64 environment, one authorized, process-local Git CLI attempt ran rustc -vV, where link, and cargo metadata --format-version 1 -vv with CARGO_NET_GIT_FETCH_WITH_CLI=true and GIT_TERMINAL_PROMPT=0. The Rust host was x86_64-pc-windows-msvc and the first linker was Visual Studio Hostx64\x64\link.exe. Cargo remained at Updating git repository https://github.com/tauri-apps/tauri.git until the single controlled 900-second limit expired (900.415 s, exit 124). No Cargo.lock diff or halo-tauri-desktop entry was produced, and no cargo or git process remained after cleanup. This is further evidence of the Cargo Git transport/cache-path block; do not repeat the same command. Tickets 03a and 03 remain blocked pending the user's explicit choice of an actually different approved network/proxy context, B, or C.

**2026-07-30 selected-path B planning:** The user selected the audited vendor strategy. Before the external artifacts were returned, this worktree had no `product/Halo Studio/.cargo/config.toml` and no Cargo vendor directory. A short offline lock-graph probe, `cargo metadata --locked --offline --format-version 1`, exited `1` before validation because the local cache was incomplete (`objc2-core-foundation` missing from the crates.io index). Together with the 900-second online Cargo timeout at `Updating git repository https://github.com/tauri-apps/tauri.git`, this proved the current machine could not generate the missing `Cargo.lock` update or vendor tree through Cargo's normal flow. No artifact was fabricated.

**2026-07-30 selected-path B result:** External Cargo-generated artifacts were returned and audited in this worktree: `Cargo.lock`, `.cargo/config.toml`, and `vendor/cargo/`. `Cargo.lock` only adds the local `halo-tauri-desktop v0.2.14` package block; the Tauri and Tao Git pins remain unchanged. `.cargo/config.toml` uses only workspace-local `vendor/cargo` source replacement, with no external absolute path. The vendor tree has 1091 crate directories, 1091 `.cargo-checksum.json` files, 56768 listed files with zero SHA-256 mismatches, and 1777 license/copyright entries by the external audit pattern. Local VS x64 offline locked `metadata`, `tree`, `check`, `build`, and `git diff --check` all pass after adding the required Halo `tauri.conf.json` `app.macOSPrivateApi` declaration to match the shared Tauri feature set. A clean worktree-local temporary `CARGO_HOME` metadata probe also passed and was removed. See [03a Cargo vendor audit](../03a-cargo-vendor-audit.md).

## Established Facts

- `tauri-runtime`, `tauri-runtime-wry`, and `tauri-utils` are pinned to `https://github.com/tauri-apps/tauri.git` at `ce3860e84b79af0d5ee628b304399499a87328b1`; `tao` is pinned to `https://github.com/tauri-apps/tao.git` at `c704261c519c58cfdd0bc2d58ba24e06a0b71c92`. No Git dependency specifies a branch or tag.
- A temporary bare Git repository fetched the exact Tauri SHA and `git cat-file -e <rev>^{commit}` exited `0`. The pin is therefore retrievable and must not be replaced merely to work around Cargo transport behavior.
- Cargo `metadata --format-version 1 -vv` timed out at `Updating git repository https://github.com/tauri-apps/tauri.git` with `CARGO_NET_GIT_FETCH_WITH_CLI=true` (120.324 s), `false` (120.413 s), and an isolated temporary `CARGO_HOME` (120.296 s). All timed-out process trees and the temporary home were cleaned up.
- `halo-tauri-desktop` is a workspace member and now has a local package entry in `Cargo.lock`. Locked offline metadata/tree/check/build pass locally. `desktop:build` and native-window evidence belong to Ticket 03 and are tracked separately.

## Hard Constraints

- Do not modify `D:\BitFun-main`, system PATH, the registry, global Cargo configuration, or system environment variables. Do not install software.
- Do not hand-edit `Cargo.lock`, inject a Git object into Cargo's cache, or replace/remove a Git pin without explicit user authorization.
- Do not use a broad `cargo update`, and do not treat HTTP smoke as native Tauri acceptance.
- Keep all configuration process-local unless the user explicitly approves a tracked, workspace-local vendor policy. Do not start Ticket 04.

## Options

| Option | Benefits | Risks and prerequisites | Decision |
| --- | --- | --- | --- |
| A. Use a known-good network or approved proxy and rerun Cargo with the current pins | Smallest behavioral and diff surface; retains the audited SHA and existing Tauri/Windows focus fixes; allows Cargo itself to refresh the missing lock entry. | Requires one-time public network/proxy access and Cargo-cache writes. Direct Git success does not prove Cargo will complete, so the run needs bounded timeouts and process cleanup. | Closed for the current network path after the 900-second timeout. |
| B. Establish an audited local/vendor strategy | Can make subsequent builds independent of the failing Cargo Git path and preserve the current pins. | `cargo vendor` itself requires a successfully resolved exact lock graph on a trusted, network-capable environment. It adds a large tracked artifact and workspace-local source-replacement policy, with provenance, license, checksum, update, and stale-vendor risks. | **Selected and verified; ready for review.** |
| C. Audit and switch to a reliably resolved Tauri source/version | May remove the Git transport dependency permanently. | Changes product behavior and the upstream Windows focus fixes that motivated the current patches; can cause a broad lockfile and compatibility change. The current pin is demonstrably retrievable, so transport failure alone does not justify this option. | Do not start without explicit user authorization and a separate compatibility audit. |

## Recommended Path: A

Before any network operation, request one-time authorization that states: public Git and registry dependencies recorded in `Cargo.toml` will be accessed, and only the current user's Cargo cache plus this worktree's `Cargo.lock` may be written. No global configuration, PATH, registry, or installed software will change.

Use the VS x64 environment and an approved network/proxy context. Proxy values, if required, must be provided to the child process only and must not be logged. First run the narrow resolver command with a bounded timeout:

```text
cmd.exe /d /s /c """D:\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cd /d "D:\Halo Studio\.worktrees\issue-03-tauri-workbench\product\Halo Studio" && set CARGO_NET_GIT_FETCH_WITH_CLI=true && cargo metadata --format-version 1 -vv"
```

If Cargo reaches resolution successfully, inspect `git diff -- Cargo.lock` before any build. The only acceptable changes are `halo-tauri-desktop` and dependencies proved necessary for the existing locked graph; the Git URLs and exact revisions must remain unchanged. Do not use `cargo update` to make the diff appear.

## Selected Path: B

Goal: make Ticket 03's Cargo validation independent of the failing Cargo Git transport/cache path while preserving the current exact Tauri and Tao pins.

Scope: only the `product/Halo Studio` Cargo workspace. Do not vendor or configure the repository root, `D:\BitFun-main`, the Halo main workspace, global Cargo config, system PATH, registry, or installed software.

Tracked artifacts expected after successful generation:

- `product/Halo Studio/Cargo.lock`, generated by Cargo and containing the local `halo-tauri-desktop` package entry.
- `product/Halo Studio/vendor/cargo/`, generated by `cargo vendor --locked vendor/cargo`.
- `product/Halo Studio/.cargo/config.toml`, generated from the `cargo vendor` source-replacement output and scoped to this workspace.
- `docs/requirements/bitfun-tauri-product-migration/03a-cargo-vendor-audit.md`, recording command provenance, exact Git URLs/revs, toolchain/Cargo versions, lockfile diff review, per-crate checksum source (`.cargo-checksum.json` plus `Cargo.lock` checksums), and license inventory status.

Generation must happen through Cargo in a trusted environment that can resolve the existing pins. Do not manually seed Cargo's Git cache, hand-edit `Cargo.lock`, hand-copy dependency source trees, replace a Git pin, or run a broad `cargo update`.

External generation commands, from a copy of the current worktree that includes the untracked Halo files:

```text
Set-Location -LiteralPath "<external-copy>\product\Halo Studio"
cargo metadata --format-version 1
rg -n "^name = \"halo-tauri-desktop\"$" Cargo.lock
New-Item -ItemType Directory -Force .cargo
cargo vendor --locked vendor/cargo > .cargo\config.toml
cargo metadata --locked --format-version 1
cargo tree --locked -p halo-tauri-desktop
git diff -- Cargo.lock .cargo/config.toml
git status --short Cargo.lock .cargo/config.toml vendor/cargo
Get-ChildItem -Recurse -Force vendor/cargo -Filter .cargo-checksum.json | Measure-Object
Get-ChildItem -Recurse -Force vendor/cargo -Include LICENSE*,COPYING*,NOTICE*,COPYRIGHT* | Select-Object FullName
```

Returned artifacts were imported as generated `Cargo.lock`, `.cargo/config.toml`, `vendor/cargo/`, and the minimal public command summary needed for the audit. After import, this machine verified the artifacts with a clean, worktree-local temporary `CARGO_HOME` and `--locked`; the temporary home was removed. Ticket 03 desktop acceptance may now continue.

## Fallback Gate: C

Select C only after explicit authorization to change dependency source/version. The audit must identify the exact released or maintained source that contains the Tauri and Tao behavior fixes currently supplied by the patches, compare Rust/MSVC and Tauri API compatibility, review the complete lockfile diff, and rerun Ticket 03's native-window acceptance. Record the rationale in an ADR or an update to the applicable dependency decision before changing manifests.

## Acceptance Commands

After A or approved B succeeds, run these commands in the VS x64 environment. `rustc -vV` must report `x86_64-pc-windows-msvc`, and the first `where link` result must be Visual Studio `Hostx64\x64\link.exe`.

```text
cargo metadata --locked --format-version 1
cargo tree --locked -p halo-tauri-desktop
cargo check --locked -p halo-tauri-desktop
cargo build --locked -p halo-tauri-desktop
git diff --check
```

Success for 03a means the current pins are fully ingested by Cargo, `Cargo.lock` contains `halo-tauri-desktop`, locked metadata/tree/check/build pass, and the lock diff has been reviewed. It does not include `pnpm run desktop:build`, `desktop:dev`, or native-window smoke; those remain Ticket 03 acceptance work.

## Stop Conditions and Ticket Impact

- If the approved A run still times out or fails at Cargo's Git transport stage, record the URL, timeout location, error class, and exit code. Do not retry an unchanged long command; stop for the user's choice between B and C.
- If B cannot be generated by Cargo from a trusted environment, it is not a valid bypass. Keep 03 blocked.
- If C has not received explicit authorization and completed its compatibility audit, keep the current pins unchanged and keep 03 blocked.
- Closing 03a only unblocks Ticket 03's build and native acceptance sequence. It does not unblock or implement Ticket 04 independently.

**Evidence:** [Ticket 03 execution evidence](../03-tauri-workbench-execution-evidence.md) and [03a Cargo vendor audit](../03a-cargo-vendor-audit.md).
