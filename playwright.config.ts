import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : [['list'], ['html']],
  use: {
    baseURL: process.env.E2E_BASE_URL ?? 'http://127.0.0.1:4173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure'
  },
  webServer: {
    // Phase 4 Stage 1 — `PUBLIC_E2E=1` is read by Vite at build time;
    // it gates the in-app mock provider in `src/lib/e2e/mockBootstrap.ts`
    // and `src/lib/panel/ChatPanel.svelte`. We rebuild before preview so
    // the constant flips on for this run; production builds (without the
    // env var) drop the mock paths entirely via dead-code elimination.
    command: 'PUBLIC_E2E=1 pnpm build && PUBLIC_E2E=1 pnpm preview --host 127.0.0.1 --port 4173',
    url: 'http://127.0.0.1:4173',
    timeout: 120_000,
    reuseExistingServer: !process.env.CI
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] }
    }
  ]
});
