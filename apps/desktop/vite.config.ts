import react from "@vitejs/plugin-react";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

/**
 * KaTeX lists each font as woff2, woff and ttf. Both webviews the app runs in read woff2, so
 * the other two are dropped and never copied into the bundle (about a megabyte saved).
 */
function katexWoff2Only(): Plugin {
  return {
    name: "katex-woff2-only",
    enforce: "pre",
    transform(code, id) {
      if (!id.includes("katex") || !id.endsWith(".css")) return null;
      return code
        .replace(/,url\([^)]*\.woff\) format\("woff"\)/g, "")
        .replace(/,url\([^)]*\.ttf\) format\("truetype"\)/g, "");
    },
  };
}

// Tauri expects a fixed dev port and hides the Vite banner behind its own output.
export default defineConfig({
  plugins: [katexWoff2Only(), react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
    sourcemap: false,
    // The CSP allows fonts and images from 'self' only, so nothing may be inlined as a data URL.
    assetsInlineLimit: 0,
    // KaTeX and the icon set make the one bundle about 700 kB. It loads from disk, not a network.
    chunkSizeWarningLimit: 1000,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    css: false,
  },
});
