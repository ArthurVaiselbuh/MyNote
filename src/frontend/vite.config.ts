/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  test: {
    environment: "happy-dom",
  },
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    target: "chrome110",
    outDir: "dist",
  },
});
