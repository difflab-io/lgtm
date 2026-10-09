import { defineConfig, devices } from '@playwright/test'

// Each test spawns its own backend + web dev server via helpers in
// tests/env.ts, so the Playwright config stays minimal.
export default defineConfig({
  testDir: './tests',
  fullyParallel: false, // tests allocate ports and share a tmp root
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [['list']],
  use: {
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
})
