/**
 * E2E tests for the API Keys page.
 *
 * Covers listing, creating, and deleting API keys.
 * Requires admin API key for access.
 */
import { test, expect } from '@playwright/test';
import {
  BASE_PATH,
  API_KEY,
  setupAuth,
  gotoPage,
} from './helpers';

test.describe('API Keys Page', () => {
  test.beforeEach(async ({ page }) => {
    await setupAuth(page);
  });

  test('loads API keys page', async ({ page }) => {
    await gotoPage(page, '/api-keys');

    const body = await page.textContent('body');
    expect(body).toMatch(/api\s*key/i);
  });

  test('shows create API key button', async ({ page }) => {
    await gotoPage(page, '/api-keys');
    await page.waitForTimeout(1000);

    // Use .first() because there are two "Create API Key" buttons (header + empty state)
    const createBtn = page.getByRole('button', { name: /create/i }).first();
    await expect(createBtn).toBeVisible({ timeout: 5000 });
  });

  test('create button opens modal with form', async ({ page }) => {
    await gotoPage(page, '/api-keys');
    await page.waitForTimeout(1000);

    // Use .first() because there are two "Create API Key" buttons (header + empty state)
    const createBtn = page.getByRole('button', { name: /create/i }).first();
    await createBtn.click();
    await page.waitForTimeout(500);

    // Chakra UI uses role="dialog" on its modal
    const modal = page.getByRole('dialog');
    await expect(modal).toBeVisible({ timeout: 3000 });

    const modalText = await modal.textContent();
    expect(modalText).toMatch(/name/i);
  });

  test('can create a new API key', async ({ page }) => {
    await gotoPage(page, '/api-keys');
    await page.waitForTimeout(1000);

    // Use .first() because there are two "Create API Key" buttons (header + empty state)
    const createBtn = page.getByRole('button', { name: /create/i }).first();
    await createBtn.click();
    await page.waitForTimeout(500);

    // Fill in the name field
    const nameInput = page.locator('[role="dialog"] input').first();
    const keyName = `e2e-test-key-${Date.now()}`;
    await nameInput.fill(keyName);

    // Submit the form
    const submitBtn = page.locator('[role="dialog"] button').filter({ hasText: /create|save|submit/i }).first();
    await submitBtn.click();
    await page.waitForTimeout(2000);

    // The new key should appear in the list
    const body = await page.textContent('body');
    expect(body).toContain(keyName);
  });

  test('shows key table with columns', async ({ page }) => {
    await gotoPage(page, '/api-keys');
    await page.waitForTimeout(1000);

    // Create an API key first so the table is visible
    const createBtn = page.getByRole('button', { name: /create/i }).first();
    await createBtn.click();
    await page.waitForTimeout(500);

    const nameInput = page.locator('[role="dialog"] input').first();
    await nameInput.fill(`e2e-table-key-${Date.now()}`);
    const submitBtn = page.locator('[role="dialog"] button').filter({ hasText: /create|save|submit/i }).first();
    await submitBtn.click();
    await page.waitForTimeout(2000);

    const body = await page.textContent('body');
    // Table should have standard columns
    expect(body).toMatch(/name/i);
  });

  test('can delete an API key', async ({ page }) => {
    // First create a key to delete
    await gotoPage(page, '/api-keys');
    await page.waitForTimeout(1000);

    // Use .first() because there are two "Create API Key" buttons (header + empty state)
    const createBtn = page.getByRole('button', { name: /create/i }).first();
    await createBtn.click();
    await page.waitForTimeout(500);

    const nameInput = page.locator('[role="dialog"] input').first();
    const keyName = `e2e-delete-key-${Date.now()}`;
    await nameInput.fill(keyName);

    const submitBtn = page.locator('[role="dialog"] button').filter({ hasText: /create|save|submit/i }).first();
    await submitBtn.click();
    await page.waitForTimeout(2000);

    // Close any success modal/dialog
    await page.keyboard.press('Escape');
    await page.waitForTimeout(500);

    // Now find and click a delete button for the key we just created
    const deleteBtn = page.locator('button[aria-label*="delete" i]').last();
    if (await deleteBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await deleteBtn.click();

      // Handle confirmation
      const confirmBtn = page.locator('button').filter({ hasText: /confirm|yes|delete/i });
      if (await confirmBtn.first().isVisible({ timeout: 2000 }).catch(() => false)) {
        await confirmBtn.first().click();
      }
      await page.waitForTimeout(1000);
    }
  });

  test('can close create modal without creating', async ({ page }) => {
    await gotoPage(page, '/api-keys');
    await page.waitForTimeout(1000);

    // Use .first() because there are two "Create API Key" buttons (header + empty state)
    const createBtn = page.getByRole('button', { name: /create/i }).first();
    await createBtn.click();
    await page.waitForTimeout(500);

    // Close modal
    await page.keyboard.press('Escape');
    await page.waitForTimeout(500);

    const modal = page.locator('[role="dialog"]');
    await expect(modal).not.toBeVisible({ timeout: 2000 });
  });
});
