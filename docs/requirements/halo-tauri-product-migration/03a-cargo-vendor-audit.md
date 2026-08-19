# 03a Cargo Vendor Audit

**Status:** ready-for-review
**Date:** 2026-07-30
**Worktree:** `D:\Halo Studio\.worktrees\issue-03-tauri-workbench`

## Returned Artifacts

- `product/Halo Studio/Cargo.lock`
- `product/Halo Studio/.cargo/config.toml`
- `product/Halo Studio/vendor/cargo/`

External public summary:

- Cargo: `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`
- Source repo: `git@github.com:Nyzeep/Halo-Studio.git`
- Branch: `codex/issue-03-halo-tauri-workbench`
- Rev: `58dd8fcdcf0fe97ee7b367751326000e95bb068d`
- Tauri pin retained: `https://github.com/tauri-apps/tauri.git` rev `ce3860e84b79af0d5ee628b304399499a87328b1`
- Tao pin retained: `https://github.com/tauri-apps/tao.git` rev `c704261c519c58cfdd0bc2d58ba24e06a0b71c92`
- External audit command exit codes: `0`
- No commit, push, lockfile hand-edit, or pin replacement.

## Local Audit

- `product/Halo Studio/.cargo/config.toml` exists and replaces crates.io plus the exact Tauri/Tao Git sources with `source.vendored-sources`.
- `source.vendored-sources.directory` is `vendor/cargo`; no external absolute path, home path, or `file://` source is present.
- `Cargo.lock` contains `halo-tauri-desktop v0.2.14`.
- `git diff -- product/Halo Studio/Cargo.lock` only adds the `halo-tauri-desktop` package block with dependencies on `tauri` and `tauri-build`.
- Tauri lock sources still use `ce3860e84b79af0d5ee628b304399499a87328b1`.
- Tao lock sources still use `c704261c519c58cfdd0bc2d58ba24e06a0b71c92`.

Vendor inventory:

- Crate directories: `1091`
- `.cargo-checksum.json` files: `1091`
- Files verified against vendor checksums: `56768`
- Missing checksum files: `0`
- Missing listed files: `0`
- SHA-256 mismatches: `0`
- Extra unlisted files: `0`
- License/copyright entries by external audit pattern: `1777` (`1762` files and `15` directories)
- Every vendor crate has either a Cargo `license`/`license-file` field or a top-level license-like file.

## Local Fix

`product/Halo Studio/src/apps/halo-desktop/tauri.conf.json` now declares `app.macOSPrivateApi: true`. This is required because the workspace-level `tauri` dependency enables the `macos-private-api` feature, and the existing Halo Studio desktop config already declares the same Tauri allowlist flag.

## Verification

All commands ran in the VS x64 environment. `rustc -vV` reported host `x86_64-pc-windows-msvc`, and the first `where link` result was Visual Studio `Hostx64\x64\link.exe`.

| Command | Result |
| --- | --- |
| `cargo metadata --locked --offline --format-version 1` | Passed |
| `cargo tree --locked --offline -p halo-tauri-desktop` | Passed; tree includes `tauri-runtime`, `tauri-runtime-wry`, and `tauri-utils` from the retained Tauri SHA and `tao` from the retained Tao SHA |
| `cargo check --locked --offline -p halo-tauri-desktop` | Passed after adding `app.macOSPrivateApi` to Halo Tauri config |
| `cargo build --locked --offline -p halo-tauri-desktop` | Passed |
| `git diff --check HEAD -- .` | Passed |

## Result

Ticket 03a is ready for review. This only unblocks Ticket 03's desktop build and native Tauri acceptance sequence. It does not start or unblock Ticket 04 independently.
