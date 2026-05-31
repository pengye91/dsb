/**
 * E2E tests for the Settings page.
 *
 * Covers API key configuration, persistence, and navigation guards.
 */
import { test, expect } from '@playwright/test';
import { BASE_PATH, API_KEY, setupAuth, gotoPage } from './helpers';

test.describe('Settings Page', () => {
  test('shows settings page when no API key is configured', async ({ page }) => {
    // Clear any stored keys
    await page.goto(`${BASE_PATH}/settings`);
    await page.evaluate(() => {
      localStorage.removeItem('dsb_api_key');
      localStorage.removeItem('dsb_admin_api_key');
    });
    await page.reload();
    await page.waitForSelector('#root');

    const body = await page.textContent('body');
    expect(body).toContain('Settings');
    expect(body).toContain('API Key');
  });

  test('redirects to settings when visiting protected page without API key', async ({
    page,
  }) => {
    await page.goto(`${BASE_PATH}/sandboxes`);
    await page.evaluate(() => {
      localStorage.removeItem('dsb_api_key');
      localStorage.removeItem('dsb_admin_api_key');
    });
    await page.reload();
    await page.waitForSelector('#root');

    // Should redirect to settings
    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/(settings)?$`));
  });

  test('can save API key and navigate to dashboard', async ({ page }) => {
    await page.goto(`${BASE_PATH}/settings`);
    await page.evaluate(() => {
      localStorage.removeItem('dsb_api_key');
      localStorage.removeItem('dsb_admin_api_key');
    });
    await page.reload();
    await page.waitForSelector('#root');

    // Find the user API key input and fill it
    const apiKeyInput = page.locator('input[type="password"], input[placeholder*="API"], input[placeholder*="key"], input[placeholder*="Key"]').first();
    await apiKeyInput.fill(API_KEY);

    // Click save button
    const saveButton = page.locator('button').filter({ hasText: /save|apply|confirm/i }).first();
    await saveButton.click();

    // Should be able to navigate to dashboard now
    await page.waitForTimeout(500);
    await page.goto(`${BASE_PATH}/`);
    await page.waitForSelector('#root');

    const body = await page.textContent('body');
    expect(body).toContain('Dashboard');
  });

  test('persists API key across page reloads', async ({ page }) => {
    await setupAuth(page);
    await gotoPage(page, '/');

    // Verify we're on the dashboard
    const body = await page.textContent('body');
    expect(body).toContain('Dashboard');

    // Reload
    await page.reload();
    await page.waitForSelector('#root');

    // Should still be on dashboard (not redirected to settings)
    const bodyAfterReload = await page.textContent('body');
    expect(bodyAfterReload).toContain('Dashboard');
  });

  test('settings page shows admin API key section', async ({ page }) => {
    await setupAuth(page);
    await gotoPage(page, '/settings');

    const body = await page.textContent('body');
    expect(body).toContain('Settings');
    // Should mention admin key somewhere
    expect(body).toMatch(/admin/i);
  });
});
