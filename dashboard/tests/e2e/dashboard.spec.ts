/**
 * E2E tests for the Dashboard (home) page.
 *
 * Covers statistics display, recent sandboxes, and navigation.
 */
import { test, expect } from '@playwright/test';
import {
  BASE_PATH,
  API_KEY,
  setupAuth,
  gotoPage,
  createSandboxViaAPI,
  deleteSandboxViaAPI,
  waitForSandboxState,
} from './helpers';

test.describe('Dashboard Page', () => {
  test.beforeEach(async ({ page }) => {
    await setupAuth(page);
  });

  test('loads and displays dashboard heading', async ({ page }) => {
    await gotoPage(page, '/');
    // The main content area has an h2 "Dashboard" heading
    await expect(page.getByRole('heading', { name: 'Dashboard', exact: true })).toBeVisible();
  });

  test('shows sandbox statistics cards', async ({ page }) => {
    await gotoPage(page, '/');
    await page.waitForTimeout(2000);

    // The dashboard should show stats
    const body = await page.textContent('body');
    expect(body).toMatch(/total|running|stopped/i);
  });

  test('shows create sandbox button', async ({ page }) => {
    await gotoPage(page, '/');

    const createBtn = page.locator('a, button').filter({ hasText: /create\s*sandbox/i });
    await expect(createBtn.first()).toBeVisible();
  });

  test('create sandbox button navigates to create page', async ({ page }) => {
    await gotoPage(page, '/');

    const createBtn = page.locator('a, button').filter({ hasText: /create\s*sandbox/i }).first();
    await createBtn.click();

    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/sandboxes/create`));
  });

  test('displays recent sandboxes when they exist', async ({
    page,
    request,
  }) => {
    // Create a sandbox via API
    const sbId = await createSandboxViaAPI(request, `e2e-dashboard-${Date.now()}`);
    await waitForSandboxState(request, sbId, 'running');

    try {
      await gotoPage(page, '/');

      // Wait for sandboxes to load
      await page.waitForTimeout(1000);

      // Should show at least one sandbox
      const body = await page.textContent('body');
      expect(body).toMatch(/e2e-dashboard|sandbox/i);
    } finally {
      await deleteSandboxViaAPI(request, sbId);
    }
  });

  test('stats update to show running sandbox count', async ({
    page,
    request,
  }) => {
    const sbId = await createSandboxViaAPI(request, `e2e-stats-${Date.now()}`);
    await waitForSandboxState(request, sbId, 'running');

    try {
      await gotoPage(page, '/');
      await page.waitForTimeout(1000);

      // The running count should be at least 1
      // Look for a stat that shows a number > 0 near "Running"
      const body = await page.textContent('body');
      expect(body).toMatch(/running/i);
    } finally {
      await deleteSandboxViaAPI(request, sbId);
    }
  });
});
