import { createTwoFilesPatch as diffPatch } from "diff";

const secretKey = /(?:apiKey|token|secret|password|authorization)/iu;
function redactLine(line: string): string {
  if (!secretKey.test(line) || !line.includes(":")) return line;
  const colon = line.indexOf(":");
  const prefix = line.slice(0, colon + 1);
  const suffix = line.trimEnd().endsWith(",") ? "," : "";
  return `${prefix} \"[REDACTED]\"${suffix}`;
}
export function redactSecrets(text: string): string { return text.split("\n").map(redactLine).join("\n"); }
export function createTwoFilesPatch(oldName: string, newName: string, oldText: string, newText: string): string {
  return diffPatch(oldName, newName, redactSecrets(oldText), redactSecrets(newText), "", "");
}
