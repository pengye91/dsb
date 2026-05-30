import { defineConfig } from '@playwright/test';

const E2E_BASE_PATH = process.env.E2E_BASE_PATH || '/dsb';
const E2E_BASE_URL = process.env.E2E_BASE_URL || `http://localhost:13001`;

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  retries: 0,
  timeout: 90_000,
  expect: { timeout: 10_000 },
  reporter: 'list',
  use: {
    baseURL: E2E_BASE_URL,
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: { browserName: 'chromium' },
    },
  ],
});
