import { defineConfig } from "@playwright/test";
import base from "./playwright.config";

// Config for the README GIF recorder (record/record.spec.ts) — not part of the
// regression suite. Run with: npx playwright test -c record.config.ts
export default defineConfig({
  ...base,
  testDir: "./record",
  timeout: 300_000,
  retries: 0,
  reporter: [["list"]],
});
