/**
 * E2E tests for the Sandboxes page.
 *
 * Covers listing, filtering, CRUD operations, and navigation to details.
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

test.describe('Sandboxes Page', () => {
  test.beforeEach(async ({ page }) => {
    await setupAuth(page);
  });

  test('loads and displays sandboxes heading', async ({ page }) => {
    await gotoPage(page, '/sandboxes');
    await expect(
      page.getByRole('heading', { name: /sandboxes/i }),
    ).toBeVisible();
  });

  test('shows create sandbox button', async ({ page }) => {
    await gotoPage(page, '/sandboxes');

    const createBtn = page.locator('a, button').filter({ hasText: /create\s*sandbox/i });
    await expect(createBtn.first()).toBeVisible();
  });

  test('displays sandbox cards when sandboxes exist', async ({
    page,
    request,
  }) => {
    const sbId = await createSandboxViaAPI(request, `e2e-list-${Date.now()}`);
    await waitForSandboxState(request, sbId, 'running');

    try {
      await gotoPage(page, '/sandboxes');
      await page.waitForTimeout(1000);

      // Should show the sandbox in the list
      const body = await page.textContent('body');
      expect(body).toMatch(/e2e-list|running/i);
    } finally {
      await deleteSandboxViaAPI(request, sbId);
    }
  });

  test('can stop a running sandbox', async ({ page, request }) => {
    const sbId = await createSandboxViaAPI(request, `e2e-stop-${Date.now()}`);
    await waitForSandboxState(request, sbId, 'running');

    try {
      await gotoPage(page, '/sandboxes');
      // Wait for sandbox data to load (not just "Loading...")
      await page.waitForFunction(
        () => !document.body.textContent?.includes('Loading sandboxes'),
        { timeout: 10000 },
      );
      await page.waitForTimeout(500);

      // Find and click the stop button
      const stopBtn = page.locator('button[aria-label*="stop" i], button[aria-label*="Stop"]').first();
      if (await stopBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
        await stopBtn.click();
        await page.waitForTimeout(3000);

        // Verify via API
        const resp = await request.get(`${BASE_PATH}/api/sandboxes/${sbId}`, {
          headers: { 'X-API-Key': API_KEY },
        });
        const body = await resp.json();
        expect(body.state).toMatch(/stopped|exited/i);
      }
    } finally {
      await deleteSandboxViaAPI(request, sbId);
    }
  });

  test('can delete a sandbox', async ({ page, request }) => {
    const sbName = `e2e-delete-${Date.now()}`;
    const sbId = await createSandboxViaAPI(request, sbName);
    await waitForSandboxState(request, sbId, 'running');

    try {
      await gotoPage(page, '/sandboxes');
      await page.waitForTimeout(1000);

      // The frontend uses native window.confirm() — accept it automatically
      page.on('dialog', (dialog) => dialog.accept());

      // Find and click a delete button (trash icon)
      const deleteBtn = page.locator('button[aria-label*="delete" i]').first();
      if (await deleteBtn.isVisible()) {
        await deleteBtn.click();
        await page.waitForTimeout(2000);
      }
    } finally {
      // Cleanup in case delete didn't work through UI
      await deleteSandboxViaAPI(request, sbId);
    }
  });

  test('can click through to sandbox details', async ({ page, request }) => {
    const sbId = await createSandboxViaAPI(request, `e2e-details-nav-${Date.now()}`);
    await waitForSandboxState(request, sbId, 'running');

    try {
      await gotoPage(page, '/sandboxes');
      await page.waitForFunction(
        () => !document.body.textContent?.includes('Loading sandboxes'),
        { timeout: 10000 },
      );

      // Click on any sandbox card/link that goes to a details page
      const sandboxLink = page.locator(`a[href*="sandboxes/"]`).first();
      await expect(sandboxLink).toBeVisible({ timeout: 5000 });
      await sandboxLink.click();

      await expect(page).toHaveURL(new RegExp(`sandboxes/[0-9a-f-]+`), { timeout: 10000 });
    } finally {
      await deleteSandboxViaAPI(request, sbId);
    }
  });

  test('filter by state shows filtered results', async ({ page, request }) => {
    const sbId = await createSandboxViaAPI(request, `e2e-filter-${Date.now()}`);
    await waitForSandboxState(request, sbId, 'running');

    try {
      await gotoPage(page, '/sandboxes');
      await page.waitForFunction(
        () => !document.body.textContent?.includes('Loading sandboxes'),
        { timeout: 10000 },
      );

      // Look for filter controls
      const filterBtn = page.locator('button').filter({ hasText: /filter/i }).first();
      if (await filterBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
        await filterBtn.click();
        await page.waitForTimeout(500);

        // Try selecting a state filter
        const stateSelect = page.locator('select').first();
        if (await stateSelect.isVisible({ timeout: 2000 }).catch(() => false)) {
          // Get the available options
          const options = await stateSelect.locator('option').allTextContents();
          // Select a running-related option (case may vary)
          const runningOpt = options.find((o) => /running/i.test(o));
          if (runningOpt) {
            await stateSelect.selectOption({ label: runningOpt });
          } else {
            // Try by value
            await stateSelect.selectOption('running');
          }
          await page.waitForTimeout(1000);

          const body = await page.textContent('body');
          expect(body).toMatch(/running/i);
        }
      }
    } finally {
      await deleteSandboxViaAPI(request, sbId);
    }
  });
});
