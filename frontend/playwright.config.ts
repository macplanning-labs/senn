import { defineConfig, devices } from "@playwright/test";

/**
 * SENN — Playwright E2E テスト設定
 *
 * 前提: Django API (localhost:8000) + Vite dev server (localhost:5173) が起動済み
 * 実行: cd frontend && npx playwright test
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  retries: 1,
  workers: 1,
  reporter: [
    ["html", { open: "never" }],
    ["list"],
  ],
  use: {
    baseURL: "http://localhost:5173",
    trace: "on-first-retry",
    screenshot: "on",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  /* Django API + Vite を手動起動する前提（CI では webServer で自動起動） */
});
