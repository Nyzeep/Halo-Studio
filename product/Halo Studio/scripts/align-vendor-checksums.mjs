import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_VENDOR_ROOT = path.resolve(SCRIPT_DIR, "..", "vendor", "cargo");
const SHA256_PATTERN = /^[0-9a-f]{64}$/i;

function walkFiles(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === ".cargo-checksum.json") continue;
    const candidate = path.join(dir, entry.name);
    if (entry.isDirectory()) walkFiles(candidate, out);
    else if (entry.isFile()) out.push(candidate);
  }
  return out;
}

function sha256Hex(filePath) {
  return createHash("sha256").update(readFileSync(filePath)).digest("hex");
}

/**
 * Align each vendor crate's .cargo-checksum.json `files` map with the files
 * actually on disk (recomputed SHA-256), dropping entries whose file is absent.
 * The `package` field and any other top-level fields are preserved verbatim;
 * vendor source files are never modified.
 */
export function alignVendorChecksums({ vendorRoot = DEFAULT_VENDOR_ROOT, dryRun = false } = {}) {
  const errors = [];
  let crates = 0;
  let cratesUpdated = 0;
  let cratesUnchanged = 0;
  let filesUpdated = 0;
  let entriesDropped = 0;
  let filesAdded = 0;

  if (!existsSync(vendorRoot) || !readdirSync(vendorRoot, { withFileTypes: true }).some((entry) => entry.isDirectory())) {
    throw new Error(`Vendor root does not exist or contains no crate directories: ${vendorRoot}`);
  }

  for (const crateEntry of readdirSync(vendorRoot, { withFileTypes: true })) {
    if (!crateEntry.isDirectory()) continue;
    const crateRoot = path.join(vendorRoot, crateEntry.name);
    const checksumPath = path.join(crateRoot, ".cargo-checksum.json");
    if (!existsSync(checksumPath)) continue;
    crates += 1;

    let checksum;
    try {
      checksum = JSON.parse(readFileSync(checksumPath, "utf8"));
    } catch (error) {
      errors.push({ crate: crateEntry.name, error: `invalid .cargo-checksum.json: ${error.message}` });
      continue;
    }
    if (!checksum || typeof checksum !== "object" || Array.isArray(checksum)
      || typeof checksum.files !== "object" || checksum.files === null || Array.isArray(checksum.files)) {
      errors.push({ crate: crateEntry.name, error: ".cargo-checksum.json must contain a files object" });
      continue;
    }
    const packageValid = checksum.package === null
      || (typeof checksum.package === "string" && SHA256_PATTERN.test(checksum.package));
    if (!packageValid) {
      errors.push({ crate: crateEntry.name, error: ".cargo-checksum.json package field must be a SHA-256 hex string or null" });
      continue;
    }

    const newFiles = Object.fromEntries(
      walkFiles(crateRoot)
        .map((filePath) => [
          path.relative(crateRoot, filePath).replaceAll("\\", "/"),
          sha256Hex(filePath),
        ])
        .sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0),
    );

    let crateChanged = false;
    for (const declaredPath of Object.keys(checksum.files)) {
      if (!(declaredPath in newFiles)) {
        entriesDropped += 1;
        crateChanged = true;
      } else if (newFiles[declaredPath] !== checksum.files[declaredPath]) {
        filesUpdated += 1;
        crateChanged = true;
      }
    }
    for (const filePath of Object.keys(newFiles)) {
      if (!(filePath in checksum.files)) {
        filesAdded += 1;
        crateChanged = true;
      }
    }

    if (!crateChanged) {
      cratesUnchanged += 1;
      continue;
    }
    cratesUpdated += 1;
    if (dryRun) continue;

    const regenerated = { ...checksum, files: newFiles };
    writeFileSync(checksumPath, JSON.stringify(regenerated));
  }

  return { crates, cratesUpdated, cratesUnchanged, filesUpdated, entriesDropped, filesAdded, errors, dryRun };
}

export function main(argv = process.argv.slice(2)) {
  let vendorRoot = DEFAULT_VENDOR_ROOT;
  let dryRun = false;
  let json = false;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--root") vendorRoot = path.resolve(argv[++index]);
    else if (argument === "--dry-run") dryRun = true;
    else if (argument === "--json") json = true;
    else if (argument === "--help" || argument === "-h") {
      console.log("Usage: node scripts/align-vendor-checksums.mjs [--root <vendor/cargo>] [--dry-run] [--json]");
      return 0;
    } else throw new Error(`Unknown argument: ${argument}`);
  }
  try {
    const result = alignVendorChecksums({ vendorRoot, dryRun });
    const summary = {
      vendorRoot,
      ...result,
    };
    if (json) console.log(JSON.stringify(summary, null, 2));
    else {
      console.log(`Vendor checksum alignment: ${result.errors.length === 0 ? "ok" : "errors"}`);
      console.log(`crates=${result.crates} updated=${result.cratesUpdated} unchanged=${result.cratesUnchanged}`);
      console.log(`filesUpdated=${result.filesUpdated} entriesDropped=${result.entriesDropped} filesAdded=${result.filesAdded}`);
      for (const error of result.errors) console.error(`- [${error.crate}] ${error.error}`);
    }
    return result.errors.length === 0 ? 0 : 1;
  } catch (error) {
    console.error(error.message);
    return 1;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  try {
    process.exitCode = main();
  } catch (error) {
    console.error(error.message || String(error));
    process.exitCode = 1;
  }
}



