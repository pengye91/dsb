import { test, expect } from '@playwright/test';

const API_URL = process.env.API_URL || 'http://localhost:8080';
const ADMIN_KEY = 'YOUR_API_KEY_HERE';

test('Dashboard login and interaction', async ({ page }) => {
  // Navigate to dashboard
  await page.goto(`${API_URL}/admin/dashboard`);
  
  // Wait a bit
  await page.waitForTimeout(2000);
  
  // Fill login
  await page.fill('input', ADMIN_KEY);
  await page.click('button[type="submit"]');
  
  // Wait for login
  await page.waitForTimeout(2000);
  
  const content = await page.content();
  console.log(content.substring(0, 200));
});
