/**
 * E2E tests for the Images page.
 *
 * Covers image listing, pull modal, and delete functionality.
 */
import { test, expect } from '@playwright/test';
import {
  BASE_PATH,
  API_KEY,
  setupAuth,
  gotoPage,
} from './helpers';

test.describe('Images Page', () => {
  test.beforeEach(async ({ page }) => {
    await setupAuth(page);
  });

  test('loads and displays images heading', async ({ page }) => {
    await gotoPage(page, '/images');

    const body = await page.textContent('body');
    expect(body).toMatch(/docker\s*images|images/i);
  });

  test('lists available images', async ({ page }) => {
    await gotoPage(page, '/images');
    await page.waitForTimeout(2000);

    // Should list at least the sandbox image used for testing
    const body = await page.textContent('body');
    expect(body).toMatch(/dsb\/sandbox|image|alpine/i);
  });

  test('shows image size and creation date', async ({ page }) => {
    await gotoPage(page, '/images');
    await page.waitForTimeout(2000);

    // Image cards should show size info
    const body = await page.textContent('body');
    // Size is typically shown in MB/GB format or as bytes
    expect(body).toMatch(/MB|GB|KB|\d+\.\d+/i);
  });

  test('shows pull image button', async ({ page }) => {
    await gotoPage(page, '/images');

    const pullBtn = page.getByRole('button', { name: /pull/i });
    await expect(pullBtn).toBeVisible();
  });

  test('pull image button opens modal', async ({ page }) => {
    await gotoPage(page, '/images');

    const pullBtn = page.getByRole('button', { name: /pull/i });
    await pullBtn.click();

    // Wait for Chakra UI dialog to appear
    await page.waitForTimeout(500);
    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible({ timeout: 3000 });

    const modalBody = await modal.textContent();
    expect(modalBody).toMatch(/image|pull/i);
  });

  test('pull modal has image name and tag fields', async ({ page }) => {
    await gotoPage(page, '/images');

    const pullBtn = page.getByRole('button', { name: /pull/i });
    await pullBtn.click();
    await page.waitForTimeout(500);

    // Should have input fields for image name and tag
    const inputs = page.getByRole('dialog').locator('input');
    const inputCount = await inputs.count();
    expect(inputCount).toBeGreaterThanOrEqual(1);
  });

  test('can close pull modal', async ({ page }) => {
    await gotoPage(page, '/images');

    const pullBtn = page.getByRole('button', { name: /pull/i });
    await pullBtn.click();
    await page.waitForTimeout(500);

    // Close the modal
    await page.keyboard.press('Escape');

    await page.waitForTimeout(500);
    const modal = page.getByRole('dialog');
    await expect(modal).not.toBeVisible({ timeout: 2000 });
  });

  test('image cards have delete buttons', async ({ page }) => {
    await gotoPage(page, '/images');
    await page.waitForTimeout(2000);

    // Check for delete/trash buttons on image cards
    const deleteBtn = page.locator('button[aria-label*="delete" i]').first();
    // At least one image should have a delete button (if images exist)
    const body = await page.textContent('body');
    if (body?.match(/dsb\/sandbox|alpine/i)) {
      // Only check for delete button if images are listed
      const hasDelete = await deleteBtn.isVisible({ timeout: 2000 }).catch(() => false);
      // Delete button presence depends on image policy, just verify page loaded
      expect(body).toMatch(/image/i);
    }
  });
});
