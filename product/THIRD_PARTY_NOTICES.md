# BitFun third-party attribution index

Halo Studio carries the pinned BitFun source tree under `product/Halo Studio/`. This
index keeps the upstream license evidence discoverable without changing the
upstream source files.

## BitFun

- MIT license and the upstream copyright notice: `product/Halo Studio/LICENSE`
- Upstream repository: `https://github.com/GCWing/BitFun.git`
- Pinned source commit: recorded in `docs/requirements/halo-tauri-product-migration/upstream-manifest.json`（历史清单，记录去品牌化前导入快照）

## Retained license files

The imported tree preserves its applicable nested license files, including the
licenses for bundled skills, the JSON repair utility, and bundled fonts:

- `product/Halo Studio/src/crates/assembly/core/builtin_skills/docx/LICENSE.txt`
- `product/Halo Studio/src/crates/assembly/core/builtin_skills/pdf/LICENSE.txt`
- `product/Halo Studio/src/crates/assembly/core/builtin_skills/ppt-design/LICENSE.txt`
- `product/Halo Studio/src/crates/assembly/core/builtin_skills/pptx/LICENSE.txt`
- `product/Halo Studio/src/crates/assembly/core/builtin_skills/xlsx/LICENSE.txt`
- `product/Halo Studio/src/crates/execution/tool-call-jsonrepair/LICENSE`
- `product/Halo Studio/src/web-ui/public/fonts/FiraCode/LICENSE.txt`
- `product/Halo Studio/src/web-ui/public/fonts/Noto_Sans_SC/variable/LICENSE.txt`

Dependency manifests remain with the source tree in `package.json`,
`pnpm-lock.yaml`, `Cargo.toml`, and `Cargo.lock`. The pinned upstream tree does
not contain a separate central third-party notice file; the nested license
files above are therefore the applicable declarations currently retained.

The presence of source-only BitFun modules under `product/Halo Studio/` does not
claim that Halo supports or assembles those modules. Product scope is a later
migration decision and is unchanged by issue 02.

## Halo first-party Pi extension

`halo-workbench-permission-gate` is Halo-owned source embedded by the Pi RPC
adapter. It is not downloaded from npm or Git and is not discovered from a
project `.pi` directory or the user-wide Pi extension directory.

- Source: `product/Halo Studio/src/crates/adapters/pi-rpc-adapter/src/halo_permission_gate.ts`
- Fixed version: `1.0.0`
- Source commit: `e8c445d6a81d90851ac03d6aac7a4f11b6b749a3`
- Source commit tree: `f50918b6bdebc6067f409f248cc9182ff5bcdec3`
- Git object hash: `15d6908cc30e45f8812a87c591e58799d2f7ae69`
- SHA-256: `A6F704110E56BE3C1C0754DADDE1BE2B27F65C76EE03F2C19A1E43CD06848C0B`
- License and copyright evidence: `product/Halo Studio/LICENSE`, MIT License,
  `Copyright (c) 2026 CWing`
- Extension-specific notice: this file and
  `docs/architecture/pi-first-party-extension-inventory.json`

The extension has no runtime dependency or transitive dependency of its own;
its `@earendil-works/pi-coding-agent` import is type-only and supplied by the
user-installed Pi host. Pi's executable, provider packages, and host package
closure are not bundled by Halo P0. Their licenses and source provenance are
therefore not inferred from this entry; an exact host source commit/tag,
complete dependency closure, or future bundled distribution keeps the issue 13
release gate blocked until separately audited.

## Pi host, Provider, Core, Session, and built-in boundary

The Halo extension MIT notice above applies only to the Halo-owned source file.
It is not a license or provenance declaration for the user-installed Pi host,
Pi Provider packages, Pi Core/Agent implementation, Pi Session/runtime files,
or Pi's inline `llama.cpp` built-in. Those surfaces remain separate release
inputs and require their own exact source commit/tag, complete direct and
transitive dependency closure, license texts/notices, and an exact release
artifact before they can become release-eligible.

No exact desktop distribution artifact is currently recorded for that check.
The absence of a bundled host artifact is a blocked evidence state, not proof
that the host or its built-ins are covered by Halo's license notice.

## Pi host built-in capability excluded from Halo release

The locally observed Pi `0.83.0` host registers an inline `llama.cpp` built-in
even when Halo passes `--no-extensions`. It is not a Halo-owned extension and
is not release-eligible. The read-only source references are
`<PI_REFERENCE_ROOT>/packages/coding-agent/src/extensions/index.ts`,
`main.ts`, `resource-loader.ts`, and the `llama` source files; that tree has no
Git commit/tag or Halo distribution notice.

The built-in can reach configured llama.cpp and Hugging Face endpoints, read
`LLAMA_API_KEY`, `LLAMA_BASE_URL`, `HF_TOKEN` and token files, and persist model
state. These observations are host capability evidence only. They do not
replace an actual Halo LICENSE, lockfile, complete dependency notice, or exact
release artifact, so the issue 13 release gate remains blocked.
