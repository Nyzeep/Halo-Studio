import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/renderer/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        halo: {
          bg: "#0b0f14",
          panel: "#111820",
          panelSoft: "#151f2a",
          line: "#263241",
          cyan: "#22d3ee",
          amber: "#f59e0b",
          green: "#22c55e",
          red: "#ef4444"
        }
      }
    }
  },
  plugins: []
} satisfies Config;
