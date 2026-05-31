import { test, expect } from '@playwright/test';

const BASE_PATH = process.env.E2E_BASE_PATH || '/dsb';

test.describe(`Base path: ${BASE_PATH}`, () => {
  test.describe('SPA loading', () => {
    test('index.html loads with correct asset paths', async ({ request }) => {
      const resp = await request.get(`${BASE_PATH}/`);
      expect(resp.status()).toBe(200);

      const html = await resp.text();
      expect(html).toContain('<div id="root"></div>');

      const jsMatch = html.match(/src="([^"]*\/assets\/[^"]+\.js)"/);
      expect(jsMatch).not.toBeNull();
      expect(jsMatch![1]).toContain(BASE_PATH);
    });

    test('JS bundle loads successfully through proxy', async ({ request }) => {
      const indexResp = await request.get(`${BASE_PATH}/`);
      const html = await indexResp.text();
      const jsMatch = html.match(/src="([^"]*\/assets\/[^"]+\.js)"/);
      expect(jsMatch).not.toBeNull();

      const jsResp = await request.get(jsMatch![1]);
      expect(jsResp.status()).toBe(200);
      expect(jsResp.headers()['content-type']).toContain('javascript');
    });

    test('CSS bundle loads successfully through proxy', async ({ request }) => {
      const indexResp = await request.get(`${BASE_PATH}/`);
      const html = await indexResp.text();
      const cssMatch = html.match(/href="([^"]*\/assets\/[^"]+\.css)"/);
      if (cssMatch) {
        const cssResp = await request.get(cssMatch![1]);
        expect(cssResp.status()).toBe(200);
      }
    });
  });

  test.describe('API proxy through base path', () => {
    test('GET /dsb/api/health returns 200', async ({ request }) => {
      const resp = await request.get(`${BASE_PATH}/api/health`);
      expect(resp.status()).toBe(200);

      const body = await resp.json();
      expect(body.status).toBe('ok');
    });
  });

  test.describe('SPA client-side routing', () => {
    test('/dsb/ renders the settings page (no API key)', async ({ page }) => {
      await page.goto(`${BASE_PATH}/`);
      await page.waitForSelector('#root');

      const content = await page.textContent('body');
      expect(content).toContain('Settings');
    });

    test('/dsb/settings loads SPA (not 404)', async ({ page }) => {
      const resp = await page.goto(`${BASE_PATH}/settings`);
      expect(resp?.status()).toBe(200);

      await page.waitForSelector('#root');
      const content = await page.textContent('body');
      expect(content).toContain('Settings');
    });

    test('/dsb/sandboxes loads SPA (not 404)', async ({ page }) => {
      const resp = await page.goto(`${BASE_PATH}/sandboxes`);
      expect(resp?.status()).toBe(200);

      await page.waitForSelector('#root');
      const url = page.url();
      expect(url).toContain(BASE_PATH);
    });

    test('/dsb/images loads SPA (not 404)', async ({ page }) => {
      const resp = await page.goto(`${BASE_PATH}/images`);
      expect(resp?.status()).toBe(200);

      await page.waitForSelector('#root');
      const url = page.url();
      expect(url).toContain(BASE_PATH);
    });
  });

  test.describe('Asset isolation (without base path)', () => {
    test('GET /assets/ without prefix returns 404', async ({ request }) => {
      const resp = await request.get('/assets/', { failOnStatusCode: false });
      expect(resp.status()).toBe(404);
    });

    test('GET / without prefix returns 404 or redirect', async ({ request }) => {
      const resp = await request.get('/', { failOnStatusCode: false });
      expect(resp.status()).not.toBe(200);
    });
  });
});
