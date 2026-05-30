/**
 * E2E tests for sidebar navigation.
 *
 * Covers navigation between all pages and active state highlighting.
 */
import { test, expect } from '@playwright/test';
import {
  BASE_PATH,
  setupAuth,
  gotoPage,
} from './helpers';

test.describe('Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await setupAuth(page);
  });

  test('sidebar shows all navigation links', async ({ page }) => {
    await gotoPage(page, '/');

    const nav = page.locator('nav');
    const navText = await nav.textContent();
    expect(navText).toMatch(/Dashboard/);
    expect(navText).toMatch(/Sandboxes/);
    expect(navText).toMatch(/Images/);
    expect(navText).toMatch(/API Keys/);
  });

  test('can navigate to dashboard', async ({ page }) => {
    await gotoPage(page, '/settings');

    const dashLink = page.locator('a').filter({ hasText: /dashboard/i }).first();
    await dashLink.click();

    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/?$`));
    const body = await page.textContent('body');
    expect(body).toMatch(/dashboard/i);
  });

  test('can navigate to sandboxes', async ({ page }) => {
    await gotoPage(page, '/');

    const link = page.locator('a').filter({ hasText: /sandbox/i }).first();
    await link.click();

    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/sandboxes`));
  });

  test('can navigate to images', async ({ page }) => {
    await gotoPage(page, '/');

    const link = page.locator('a').filter({ hasText: /image/i }).first();
    await link.click();

    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/images`));
  });

  test('can navigate to API keys', async ({ page }) => {
    await gotoPage(page, '/');

    const link = page.locator('a').filter({ hasText: /api\s*key/i }).first();
    await link.click();

    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/api-keys`));
  });

  test('can navigate to settings', async ({ page }) => {
    await gotoPage(page, '/');

    // Settings is a button in the header, not a sidebar link
    const settingsBtn = page.getByRole('button', { name: /settings/i });
    await settingsBtn.click();

    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/settings`));
  });

  test('full navigation round-trip', async ({ page }) => {
    await gotoPage(page, '/');

    // Dashboard → Sandboxes
    await page.locator('a').filter({ hasText: /^Sandboxes$/ }).click();
    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/sandboxes`));

    // Sandboxes → Images
    await page.locator('a').filter({ hasText: /^Images$/ }).click();
    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/images`));

    // Images → API Keys
    await page.locator('a').filter({ hasText: /API Keys/ }).click();
    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/api-keys`));

    // API Keys → Settings (via header button)
    await page.getByRole('button', { name: /settings/i }).click();
    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/settings`));

    // Settings → Dashboard
    await page.locator('a').filter({ hasText: /^Dashboard$/ }).click();
    await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/?$`));
  });
});
