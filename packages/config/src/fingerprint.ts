import { createHash } from "node:crypto";
export function fingerprint(value: string): string { return createHash("sha256").update(value, "utf8").digest("hex"); }
export const sha256Fingerprint = fingerprint;
