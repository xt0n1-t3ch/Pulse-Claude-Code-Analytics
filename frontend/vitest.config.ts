import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import path from "node:path";

export default defineConfig({
  plugins: [svelte({ hot: false }), svelteTesting()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "src"),
      "@testing-library/svelte": path.resolve(__dirname, "node_modules/@testing-library/svelte"),
      // The suites live outside frontend/, so bare Tauri plugin specifiers do
      // not resolve from their location. Pin them to this package's deps so
      // dynamic imports inside components can be mocked.
      "@tauri-apps/plugin-updater": path.resolve(__dirname, "node_modules/@tauri-apps/plugin-updater"),
      "@tauri-apps/plugin-process": path.resolve(__dirname, "node_modules/@tauri-apps/plugin-process"),
    },
  },
  server: {
    fs: {
      allow: [path.resolve(__dirname, "..")],
    },
  },
  test: {
    environment: "happy-dom",
    globals: true,
    setupFiles: ["../tests/setup.ts"],
    include: [
      "../tests/unit/**/*.test.ts",
      "../tests/integration/**/*.test.ts",
      "../tests/components/**/*.test.ts",
    ],
  },
});
