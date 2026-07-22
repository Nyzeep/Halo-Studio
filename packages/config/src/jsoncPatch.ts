import { applyEdits, modify, parse } from "jsonc-parser";
import type { ParseError, FormattingOptions } from "jsonc-parser";
import type { ConfigOperation } from "@halo-studio/contracts";

export class ConfigParseError extends Error { readonly code = "ProtocolViolation" as const; constructor() { super("Invalid configuration document"); this.name = "ConfigParseError"; } }
export class ConfigPatchError extends Error { readonly code = "ProtocolViolation" as const; constructor() { super("Invalid configuration operation"); this.name = "ConfigPatchError"; } }
const formatting: FormattingOptions = { insertSpaces: true, tabSize: 2, eol: "\n" };
const unsafe = new Set(["__proto__", "prototype", "constructor"]);

export function parseJsonc(text: string): unknown {
  const errors: ParseError[] = [];
  const value = parse(text, errors, { allowTrailingComma: true, disallowComments: false });
  if (errors.length > 0 || value === undefined || value === null || typeof value !== "object") throw new ConfigParseError();
  return value;
}

export function applyJsoncPatch(text: string, operations: readonly ConfigOperation[]): string {
  parseJsonc(text);
  let output = text;
  try {
    for (const operation of operations) {
      if (!Array.isArray(operation.path) || operation.path.length === 0 || operation.path.some((part) => typeof part === "string" && unsafe.has(part))) throw new ConfigPatchError();
      const edits = modify(output, operation.path as (string | number)[], operation.op === "remove" ? undefined : operation.value, { formattingOptions: formatting, isArrayInsertion: false });
      output = applyEdits(output, edits);
    }
    parseJsonc(output);
    return output;
  } catch (error) { if (error instanceof ConfigParseError || error instanceof ConfigPatchError) throw error; throw new ConfigParseError(); }
}
