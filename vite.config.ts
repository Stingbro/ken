import { createRequire } from "node:module";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// package.json is the single source of truth for the version (sync-version.sh
// mirrors it into the Rust/Tauri manifests). Bake it in so the What's New
// dialog knows which release it is showing.
const pkg = createRequire(import.meta.url)("./package.json") as { version: string };

export default defineConfig({
  plugins: [svelte()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  // Tauri expects a fixed dev port and no screen clearing
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"],
    },
  },
  test: {
    environment: "happy-dom",
    include: ["src/**/*.test.ts"],
  },
});
