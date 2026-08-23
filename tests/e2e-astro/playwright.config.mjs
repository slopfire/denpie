import { defineConfig, devices } from '@playwright/test';

// Browser verification of the served Astro dist through the isolated :3027
// agent-server harness (never :3017).
export default defineConfig({
  testDir: '.',
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: process.env.CI ? 'dot' : 'list',
  use: {
    baseURL: 'http://127.0.0.1:3027',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'just agent-server --keep-data',
    url: 'http://127.0.0.1:3027',
    reuseExistingServer: true,
    timeout: 180_000,
  },
});
