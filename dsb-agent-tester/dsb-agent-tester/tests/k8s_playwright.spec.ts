import { test, expect } from '@playwright/test';

const API_URL = process.env.API_URL || 'http://localhost:8080';
const ADMIN_KEY = 'YOUR_API_KEY_HERE';

test('Dashboard login and interaction', async ({ page }) => {
  // Navigate to dashboard
  await page.goto(`${API_URL}/admin/dashboard`);
  
  // Fill login
  await page.fill('input[name="apiKey"]', ADMIN_KEY);
  await page.click('button[type="submit"]');
  
  // Wait for login
  await page.waitForURL('**/admin/dashboard*');
  
  // Expect to be on dashboard
  await expect(page).toHaveURL(/.*admin\/dashboard.*/);
});
