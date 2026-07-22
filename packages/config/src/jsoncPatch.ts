import { applyEdits, modify, parse } from "jsonc-parser";
import type { ParseError, FormattingOptions } from "jsonc-parser";
import type { ConfigOperation } from "@halo-studio/contracts";

export class ConfigParseError extends Error { readonly code = "ProtocolViolation" as const; constructor() { super("Invalid configuration document"); this.name = "ConfigParseError"; } }
export class ConfigPatchError extends Error { readonly code = "ProtocolViolation" as const; constructor() { super("Invalid configuration operation"); this.name = "ConfigPatchError"; } }
const unsafe = new Set(["__proto__", "prototype", "constructor"]);

function detectFormatting(text: string): FormattingOptions {
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const indents = text
    .split(/\r?\n/u)
    .map((line) => /^(\s+)["}\]]/u.exec(line)?.[1])
    .filter((indent): indent is string => indent !== undefined);
  if (indents.some((indent) => indent.includes("\t"))) {
    return { insertSpaces: false, tabSize: 1, eol };
  }
  const widths = indents.map((indent) => indent.length).filter((width) => width > 0);
  return {
    insertSpaces: true,
    tabSize: widths.length > 0 ? Math.min(...widths) : 2,
    eol,
  };
}

export function parseJsonc(text: string): unknown {
  const errors: ParseError[] = [];
  const value = parse(text, errors, { allowTrailingComma: true, disallowComments: false });
  if (errors.length > 0 || value === undefined || value === null || typeof value !== "object") throw new ConfigParseError();
  return value;
}

export function applyJsoncPatch(text: string, operations: readonly ConfigOperation[]): string {
  parseJsonc(text);
  let output = text;
  const formatting = detectFormatting(text);
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
