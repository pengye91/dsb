const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  
  console.log("Navigating to dashboard...");
  await page.goto("http://localhost:8080/dashboard");
  
  await page.waitForTimeout(2000);
  console.log(await page.content());
  
  await browser.close();
})();
