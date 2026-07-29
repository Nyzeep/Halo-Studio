import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    include: ["tests/**/*.test.ts", "src/renderer/**/*.test.tsx"],
    environment: "node",
    environmentMatchGlobs: [["src/renderer/**/*.test.tsx", "jsdom"]],
    setupFiles: ["./src/renderer/testSetup.ts"],
  },
});
