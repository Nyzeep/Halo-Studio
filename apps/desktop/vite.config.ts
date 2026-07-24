import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig, type Plugin } from "vite";

const externalMainDependencies = [
  "electron",
  "@halo-studio/agent-opencode",
  "@halo-studio/agent-pi",
  "@halo-studio/config",
  "@halo-studio/contracts",
  "@halo-studio/core",
  "@halo-studio/storage",
];

function developmentCspPlugin(): Plugin {
  return {
    name: "halo-development-csp",
    transformIndexHtml(html, context) {
      if (context.server === undefined) return html;
      return html.replace("style-src 'self'", "style-src 'self' 'unsafe-inline'");
    },
  };
}

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
    plugins: [react(), developmentCspPlugin()],
    base: "./",
    build: {
      emptyOutDir: false,
      outDir: "dist-electron/renderer",
    },
  };
});
