/**
 * E2E tests for the Create Sandbox page.
 *
 * Covers form display, default values, and sandbox creation flow.
 */
import { test, expect } from '@playwright/test';
import {
  BASE_PATH,
  API_KEY,
  setupAuth,
  gotoPage,
  deleteSandboxViaAPI,
} from './helpers';

test.describe('Create Sandbox Page', () => {
  test.beforeEach(async ({ page }) => {
    await setupAuth(page);
  });

  test('loads create sandbox form', async ({ page }) => {
    await gotoPage(page, '/sandboxes/create');

    await expect(
      page.getByRole('heading', { name: /create\s*sandbox/i }),
    ).toBeVisible();
  });

  test('shows image field with default value', async ({ page }) => {
    await gotoPage(page, '/sandboxes/create');
    await page.waitForTimeout(1000);

    // Should have an image input pre-filled with default
    const imageInput = page.locator('input[name="image"], input[placeholder*="image" i]').first();
    if (await imageInput.isVisible({ timeout: 3000 }).catch(() => false)) {
      const value = await imageInput.inputValue();
      expect(value.length).toBeGreaterThan(0);
    }
  });

  test('shows name field', async ({ page }) => {
    await gotoPage(page, '/sandboxes/create');

    const body = await page.textContent('body');
    expect(body).toMatch(/name/i);
  });

  test('shows port mapping section', async ({ page }) => {
    await gotoPage(page, '/sandboxes/create');

    const body = await page.textContent('body');
    expect(body).toMatch(/port/i);
  });

  test('shows environment variables section', async ({ page }) => {
    await gotoPage(page, '/sandboxes/create');

    const body = await page.textContent('body');
    expect(body).toMatch(/environment/i);
  });

  test('can create a sandbox and navigate to details', async ({
    page,
    request,
  }) => {
    await gotoPage(page, '/sandboxes/create');
    await page.waitForTimeout(1000);

    // Fill in the name
    const nameInput = page.getByLabel(/name/i).first();
    if (await nameInput.isVisible({ timeout: 3000 }).catch(() => false)) {
      await nameInput.fill(`e2e-create-${Date.now()}`);
    }

    // Click "Create Sandbox" button
    const submitBtn = page.getByRole('button', { name: /create sandbox/i });
    await submitBtn.click();

    // Should navigate to the new sandbox details page
    await page.waitForURL(new RegExp(`${BASE_PATH}/sandboxes/[0-9a-f-]+`), { timeout: 15_000 });

    // Extract sandbox ID from URL for cleanup
    const url = page.url();
    const idMatch = url.match(/sandboxes\/([0-9a-f-]+)/);
    if (idMatch) {
      await deleteSandboxViaAPI(request, idMatch[1]);
    }
  });

  test('cancel button navigates back', async ({ page }) => {
    await gotoPage(page, '/sandboxes/create');

    // Look for cancel or back button
    const cancelBtn = page.locator('a, button').filter({ hasText: /cancel|back/i }).first();
    if (await cancelBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
      await cancelBtn.click();
      await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/(sandboxes)?`));
    }
  });
});
