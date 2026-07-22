import { createTwoFilesPatch as diffPatch } from "diff";
import { getNodeValue, parseTree } from "jsonc-parser";
import type { Node, ParseError } from "jsonc-parser";

const secretKey = /(?:apiKey|token|secret|password|authorization)/iu;

function secretValueRanges(root: Node): Array<{ offset: number; length: number }> {
  const ranges: Array<{ offset: number; length: number }> = [];
  const pending = [root];
  while (pending.length > 0) {
    const node = pending.pop()!;
    if (node.type === "property" && node.children?.length === 2) {
      const key = getNodeValue(node.children[0]!);
      const value = node.children[1]!;
      if (typeof key === "string" && secretKey.test(key)) {
        ranges.push({ offset: value.offset, length: value.length });
        continue;
      }
    }
    for (const child of node.children ?? []) pending.push(child);
  }
  return ranges.sort((left, right) => right.offset - left.offset);
}

export function redactSecrets(text: string): string {
  const errors: ParseError[] = [];
  const root = parseTree(text, errors, {
    allowTrailingComma: true,
    disallowComments: false,
  });
  if (root === undefined || errors.length > 0) return "{}\n";
  let output = text;
  for (const range of secretValueRanges(root)) {
    output = `${output.slice(0, range.offset)}"[REDACTED]"${output.slice(range.offset + range.length)}`;
  }
  return output;
}

export function createTwoFilesPatch(
  oldName: string,
  newName: string,
  oldText: string,
  newText: string,
): string {
  return diffPatch(
    oldName,
    newName,
    redactSecrets(oldText),
    redactSecrets(newText),
    "",
    "",
  );
}
