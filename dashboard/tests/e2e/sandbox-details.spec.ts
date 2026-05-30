/**
 * E2E tests for the Sandbox Details page.
 *
 * Covers info display, tabs, stop/delete actions, and real-time features.
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

test.describe('Sandbox Details Page', () => {
  test.describe.configure({ mode: 'serial' });
  let sandboxId: string;

  test.beforeEach(async ({ page, request }) => {
    await setupAuth(page);
    sandboxId = await createSandboxViaAPI(request, `e2e-detail-${Date.now()}`);
    await waitForSandboxState(request, sandboxId, 'running');
  });

  test.afterEach(async ({ request }) => {
    if (sandboxId) {
      await deleteSandboxViaAPI(request, sandboxId);
    }
  });

  test('displays sandbox details', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    const body = await page.textContent('body');
    // Should show sandbox info
    expect(body).toMatch(/running|sandbox|image/i);
  });

  test('shows sandbox state badge', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    // Should have a "running" badge
    const badge = page.locator('span, div').filter({ hasText: /^running$/i }).first();
    await expect(badge).toBeVisible({ timeout: 5000 });
  });

  test('shows info tab with configuration', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    // Click Info tab if not already selected
    const infoTab = page.locator('button, [role="tab"]').filter({ hasText: /info/i }).first();
    if (await infoTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await infoTab.click();
    }

    const body = await page.textContent('body');
    // Should show image info
    expect(body).toMatch(/dsb\/sandbox|image/i);
  });

  test('shows stats tab', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    const statsTab = page.locator('button, [role="tab"]').filter({ hasText: /stats/i }).first();
    if (await statsTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await statsTab.click();
      await page.waitForTimeout(2000);

      const body = await page.textContent('body');
      // Stats tab should show CPU or memory related text
      expect(body).toMatch(/cpu|memory|network|stats/i);
    }
  });

  test('shows activities tab', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    const activitiesTab = page.locator('button, [role="tab"]').filter({ hasText: /activit/i }).first();
    if (await activitiesTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await activitiesTab.click();
      await page.waitForTimeout(1000);

      // Should show activity log or "no activities" message
      const body = await page.textContent('body');
      expect(body).toMatch(/activit|create|log/i);
    }
  });

  test('shows terminal tab', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    const terminalTab = page.locator('button, [role="tab"]').filter({ hasText: /terminal/i }).first();
    if (await terminalTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await terminalTab.click();
      await page.waitForTimeout(2000);

      // Terminal should render an xterm container
      const terminal = page.locator('.xterm, [class*="terminal"], .xterm-screen');
      await expect(terminal.first()).toBeVisible({ timeout: 5000 });
    }
  });

  test('shows VNC tab', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    const vncTab = page.locator('button, [role="tab"]').filter({ hasText: /vnc/i }).first();
    if (await vncTab.isVisible({ timeout: 2000 }).catch(() => false)) {
      await vncTab.click();
      await page.waitForTimeout(2000);

      // VNC tab should have a canvas or iframe
      const vncElement = page.locator('canvas, iframe, [class*="vnc"]');
      // VNC may or may not connect depending on sandbox config
      const body = await page.textContent('body');
      expect(body).toMatch(/vnc|connect|display/i);
    }
  });

  test('VNC tab requests a session token before opening the websocket', async ({ page }) => {
    const tokenRequestPromise = page.waitForRequest(
      (request) =>
        request.method() === 'POST' &&
        request.url().includes(`${BASE_PATH}/api/session-tokens`),
    );
    await gotoPage(page, `/sandboxes/${sandboxId}`);

    const tokenRequest = await tokenRequestPromise;
    expect(tokenRequest.postDataJSON()).toMatchObject({
      sandbox_id: sandboxId,
      service: 'vnc',
    });

    await expect
      .poll(
        () =>
          page.evaluate(
            (id) => window.sessionStorage.getItem(`vnc_token_${id}`),
            sandboxId,
          ),
      )
      .toBeTruthy();
  });

  test('standalone VNC viewer creates its own session token', async ({ page }) => {
    const tokenRequestPromise = page.waitForRequest(
      (request) =>
        request.method() === 'POST' &&
        request.url().includes(`${BASE_PATH}/api/session-tokens`),
    );

    await gotoPage(page, `/vnc-viewer/${sandboxId}`);

    const tokenRequest = await tokenRequestPromise;
    expect(tokenRequest.postDataJSON()).toMatchObject({
      sandbox_id: sandboxId,
      service: 'vnc',
    });

    await expect
      .poll(
        () =>
          page.evaluate(
            (id) => window.sessionStorage.getItem(`vnc_token_${id}`),
            sandboxId,
          ),
      )
      .toBeTruthy();
  });

  test('can stop sandbox from details page', async ({ page, request }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(2000);

    // The frontend uses native window.confirm() — accept it automatically
    page.on('dialog', (dialog) => dialog.accept());

    const stopBtn = page.getByRole('button', { name: 'Stop', exact: true });
    await expect(stopBtn).toBeVisible({ timeout: 5000 });
    await stopBtn.click();
    await page.waitForTimeout(1000);

    // Poll API for state change (stop can take a few seconds)
    await waitForSandboxState(request, sandboxId, 'stopped', 20_000);
  });

  test('back button navigates to sandboxes list', async ({ page }) => {
    await gotoPage(page, `/sandboxes/${sandboxId}`);
    await page.waitForTimeout(1000);

    // Click back button (usually an arrow icon)
    const backBtn = page.locator('button[aria-label*="back" i], a[aria-label*="back" i]').first();
    if (await backBtn.isVisible({ timeout: 2000 }).catch(() => false)) {
      await backBtn.click();
      await expect(page).toHaveURL(new RegExp(`${BASE_PATH}/sandboxes`));
    }
  });
});
