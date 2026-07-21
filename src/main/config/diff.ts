export function createUnifiedDiff(oldContent: string, nextContent: string, label = "config") {
  const oldLines = oldContent.split(/\r?\n/);
  const nextLines = nextContent.split(/\r?\n/);
  const maxLength = Math.max(oldLines.length, nextLines.length);
  const lines = [`--- ${label}:current`, `+++ ${label}:next`];

  for (let index = 0; index < maxLength; index += 1) {
    const oldLine = oldLines[index];
    const nextLine = nextLines[index];

    if (oldLine === nextLine) {
      if (oldLine !== undefined && oldLine.length > 0) {
        lines.push(` ${oldLine}`);
      }
      continue;
    }

    if (oldLine !== undefined && oldLine.length > 0) {
      lines.push(`-${oldLine}`);
    }
    if (nextLine !== undefined && nextLine.length > 0) {
      lines.push(`+${nextLine}`);
    }
  }

  return `${lines.join("\n")}\n`;
}
