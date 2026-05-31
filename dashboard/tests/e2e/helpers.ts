/**
 * Shared helpers for DSB dashboard E2E tests.
 *
 * Provides API key setup, sandbox lifecycle management,
 * and common selectors used across test suites.
 */
import { Page, APIRequestContext } from '@playwright/test';

export const BASE_PATH = process.env.E2E_BASE_PATH || '/dsb';
export const API_KEY = 'test-admin-key-for-testing-only';

/**
 * Inject API keys into localStorage so the dashboard authenticates.
 * Must be called before navigating to any protected page.
 */
export async function setupAuth(page: Page) {
  // Navigate to any page first so we can set localStorage on the right origin
  await page.goto(`${BASE_PATH}/settings`);
  await page.evaluate(
    ({ key }) => {
      localStorage.setItem('dsb_api_key', key);
      localStorage.setItem('dsb_admin_api_key', key);
    },
    { key: API_KEY },
  );
}

/**
 * Navigate to a dashboard page with auth already set up.
 */
export async function gotoPage(page: Page, path: string) {
  await page.goto(`${BASE_PATH}${path}`);
  await page.waitForSelector('#root');
}

/**
 * Create a sandbox via the API and return its ID.
 */
export async function createSandboxViaAPI(
  request: APIRequestContext,
  name?: string,
): Promise<string> {
  const resp = await request.post(`${BASE_PATH}/api/sandboxes`, {
    headers: {
      'Content-Type': 'application/json',
      'X-API-Key': API_KEY,
    },
    data: {
      image: process.env.E2E_SANDBOX_IMAGE || 'dsb/sandbox:latest',
      name: name || `e2e-test-${Date.now()}`,
      command: ['sleep', '300'],
      pull_policy: 'never',
    },
  });
  const body = await resp.json();
  return body.id;
}

/**
 * Delete a sandbox via the API (cleanup).
 * Silently ignores errors (used in afterEach where context may already be closed).
 */
export async function deleteSandboxViaAPI(
  request: APIRequestContext,
  id: string,
) {
  try {
    await request.delete(`${BASE_PATH}/api/sandboxes/${id}`, {
      headers: { 'X-API-Key': API_KEY },
      failOnStatusCode: false,
    });
  } catch {
    // Context may be closed after test timeout — ignore cleanup errors
  }
}

/**
 * Wait for a sandbox to reach a given state.
 */
export async function waitForSandboxState(
  request: APIRequestContext,
  id: string,
  state: string,
  timeoutMs = 60_000,
) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const resp = await request.get(`${BASE_PATH}/api/sandboxes/${id}`, {
      headers: { 'X-API-Key': API_KEY },
      failOnStatusCode: false,
    });
    if (resp.ok()) {
      const body = await resp.json();
      if (body.state === state) return;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`Sandbox ${id} did not reach state '${state}' within ${timeoutMs}ms`);
}
