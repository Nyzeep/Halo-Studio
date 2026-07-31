# BitFun third-party attribution index

Halo Studio carries the pinned BitFun source tree under `product/Halo Studio/`. This
index keeps the upstream license evidence discoverable without changing the
upstream source files.

## BitFun

- MIT license and the upstream copyright notice: `product/Halo Studio/LICENSE`
- Upstream repository: `https://github.com/GCWing/BitFun.git`
- Pinned source commit: recorded in `docs/requirements/bitfun-tauri-product-migration/bitfun-upstream-manifest.json`

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
