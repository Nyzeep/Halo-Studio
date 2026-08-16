export function retireHaloWorkbenchEntry(entry) {
  console.error(
    `[halo-workbench] ${entry} is retired because it only serves the obsolete static demo. Use pnpm run desktop:dev instead.`,
  );
  process.exitCode = 2;
}
