import { resolve } from "node:path";
import { defineConfig } from "vite";

const externalMainDependencies = [
  "electron",
  "@halo-studio/agent-opencode",
  "@halo-studio/agent-pi",
  "@halo-studio/config",
  "@halo-studio/contracts",
  "@halo-studio/core",
  "@halo-studio/storage",
];

export default defineConfig(({ mode }) => {
  if (mode === "main") {
    return {
      build: {
        emptyOutDir: false,
        outDir: "dist-electron/main",
        ssr: resolve(__dirname, "src/main/main.ts"),
        rollupOptions: {
          external: [
            /^node:/u,
            ...externalMainDependencies,
          ],
          output: {
            entryFileNames: "main.js",
            format: "es",
          },
        },
      },
    };
  }

  if (mode === "preload") {
    return {
      build: {
        emptyOutDir: false,
        outDir: "dist-electron/preload",
        lib: {
          entry: resolve(__dirname, "src/preload/entry.ts"),
          formats: ["cjs"],
          fileName: () => "preload.cjs",
        },
        rollupOptions: { external: ["electron"] },
      },
    };
  }

  return {
    base: "./",
    build: {
      emptyOutDir: false,
      outDir: "dist-electron/renderer",
    },
  };
});
