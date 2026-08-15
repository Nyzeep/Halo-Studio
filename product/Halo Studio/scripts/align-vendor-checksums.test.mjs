import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { alignVendorChecksums } from "./align-vendor-checksums.mjs";
const ALIGN_SCRIPT = fileURLToPath(new URL("./align-vendor-checksums.mjs", import.meta.url));

function sha256Hex(text) {
  return createHash("sha256").update(text).digest("hex");
}

function makeFixture() {
  const root = mkdtempSync(path.join(tmpdir(), "halo-align-vendor-checksums-"));
  const vendorRoot = path.join(root, "vendor", "cargo");
  const crateA = path.join(vendorRoot, "allocator-api2");
  const crateB = path.join(vendorRoot, "stable-crate");
  mkdirSync(path.join(crateA, "src"), { recursive: true });
  mkdirSync(path.join(crateB, "src"), { recursive: true });
  writeFileSync(path.join(crateA, "src", "lib.rs"), "pub fn a() {}\n");
  writeFileSync(path.join(crateA, "Cargo.toml"), "[package]\nname = \"allocator-api2\"\n");
  writeFileSync(path.join(crateA, "CHANGELOG.md"), "changelog\n");
  writeFileSync(path.join(crateA, ".cargo-checksum.json"), JSON.stringify({
    files: {
      "CHANGELOG.md": "0000000000000000000000000000000000000000000000000000000000000000",
      "Cargo.toml": "1111111111111111111111111111111111111111111111111111111111111111",
      "src/lib.rs": "2222222222222222222222222222222222222222222222222222222222222222",
      "xz-5.2/windows/vs2013/xz_win.sln": "3333333333333333333333333333333333333333333333333333333333333333",
    },
    package: "683d7910e743518b0e34f1186f92494becacb047c7b6bf616c96772180fef923",
  }));
  writeFileSync(path.join(crateB, "src", "lib.rs"), "pub fn b() {}\n");
  writeFileSync(path.join(crateB, ".cargo-checksum.json"), JSON.stringify({
    files: {
      "src/lib.rs": "6f6f32196b128ed1e78075f3ffbef9037092f42e05d5d0b14db5966961e682eb",
    },
    package: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  }));
  return { root, vendorRoot, crateA, crateB };
}

test("alignVendorChecksums rewrites mismatched hashes, drops missing entries, and preserves package", () => {
  const fixture = makeFixture();
  const result = alignVendorChecksums({ vendorRoot: fixture.vendorRoot });
  assert.equal(result.crates, 2);
  assert.equal(result.cratesUpdated, 1);
  assert.equal(result.cratesUnchanged, 1);
  assert.equal(result.filesUpdated, 3);
  assert.equal(result.entriesDropped, 1);
  assert.equal(result.filesAdded, 0);
  assert.deepEqual(result.errors, []);

  const updated = JSON.parse(readFileSync(path.join(fixture.crateA, ".cargo-checksum.json"), "utf8"));
  assert.equal(updated.package, "683d7910e743518b0e34f1186f92494becacb047c7b6bf616c96772180fef923");
  assert.deepEqual(Object.keys(updated.files), ["CHANGELOG.md", "Cargo.toml", "src/lib.rs"]);
  assert.ok(!("xz-5.2/windows/vs2013/xz_win.sln" in updated.files));
  assert.equal(updated.files["src/lib.rs"], sha256Hex("pub fn a() {}\n"));
  assert.equal(updated.files["Cargo.toml"], sha256Hex("[package]\nname = \"allocator-api2\"\n"));
  assert.equal(updated.files["CHANGELOG.md"], sha256Hex("changelog\n"));

  const raw = readFileSync(path.join(fixture.crateA, ".cargo-checksum.json"), "utf8");
  assert.ok(!raw.endsWith("\n"));
  assert.ok(!raw.includes("\n  "));
});

test("alignVendorChecksums leaves correct crates byte-for-byte untouched", () => {
  const fixture = makeFixture();
  const before = readFileSync(path.join(fixture.crateB, ".cargo-checksum.json"));
  alignVendorChecksums({ vendorRoot: fixture.vendorRoot });
  const after = readFileSync(path.join(fixture.crateB, ".cargo-checksum.json"));
  assert.ok(before.equals(after));
});

test("alignVendorChecksums dry-run does not write", () => {
  const fixture = makeFixture();
  const beforeA = readFileSync(path.join(fixture.crateA, ".cargo-checksum.json"));
  const result = alignVendorChecksums({ vendorRoot: fixture.vendorRoot, dryRun: true });
  assert.equal(result.cratesUpdated, 1);
  assert.equal(result.entriesDropped, 1);
  const afterA = readFileSync(path.join(fixture.crateA, ".cargo-checksum.json"));
  assert.ok(beforeA.equals(afterA));
});

test("alignVendorChecksums reports malformed checksum files without writing them", () => {
  const fixture = makeFixture();
  writeFileSync(path.join(fixture.crateB, ".cargo-checksum.json"), "{not json");
  const result = alignVendorChecksums({ vendorRoot: fixture.vendorRoot });
  assert.equal(result.errors.length, 1);
  assert.equal(result.errors[0].crate, "stable-crate");
  const stillBroken = readFileSync(path.join(fixture.crateB, ".cargo-checksum.json"), "utf8");
  assert.equal(stillBroken, "{not json");
  assert.equal(result.cratesUpdated, 1);
});

test("CLI --help exits 0 with usage", () => {
  const result = spawnSync(process.execPath, [ALIGN_SCRIPT, "--help"], { encoding: "utf8" });
  assert.equal(result.status, 0);
  assert.match(result.stdout, /Usage:/);
});

test("CLI unknown argument fails controlled without a raw stack trace", () => {
  const result = spawnSync(process.execPath, [ALIGN_SCRIPT, "--definitely-not-an-option"], { encoding: "utf8" });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Unknown argument/);
  assert.ok(!result.stderr.includes("at "));
});