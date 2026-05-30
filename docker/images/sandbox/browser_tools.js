/**
 * browser_tools.js
 * Implements browser automation tools via Playwright.
 *
 * This script connects to a browser via Chrome DevTools Protocol (CDP) on port 9222.
 * The browser renders to a virtual display (from $ env var, default :99)
 * and is viewable via VNC on port 5900.
 *
 * Architecture:
 * - Primary: Connects to existing browser on http://127.0.0.1:9222
 * - Fallback: Launches new Chromium as detached daemon process if no browser exists
 * - Persistence: Once launched, the browser persists for the sandbox lifetime
 * - Cleanup: None needed - sandbox destruction handles cleanup
 *
 * VNC Access:
 * - View browser at localhost:5900 (VNC server attaches to configured display)
 * - All browser windows/tabs visible in real-time
 *
 * Features:
 * - Automatic retry logic: Waits for browser to be accessible with up to 30 retries
 * - Health check: Use 'browser_health_check' command to verify browser status
 * - New tab per navigation: Each browser_navigate opens URL in new tab
 * - Command timeout: 30 seconds per command execution
 *
 * Usage: node browser_tools.js <command> <json_args>
 * Example: node browser_tools.js browser_navigate '{"url": "https://example.com"}'
 *
 * Available commands:
 * - browser_navigate, browser_go_back, browser_go_forward
 * - browser_get_markdown, browser_get_text, browser_read_links
 * - browser_screenshot, browser_get_clickable_elements
 * - browser_click, browser_form_input_fill, browser_evaluate
 * - browser_scroll, browser_new_tab, browser_tab_list
 * - browser_switch_tab, browser_close, browser_health_check
 */

const { chromium } = require('playwright');
const { spawn } = require('child_process');
const TurndownService = require('turndown');
const { stderr, stdout } = require('process');

const CDP_URL = 'http://127.0.0.1:9222';

// Global display variable from environment (read once at script load)
const DISPLAY = process.env.DISPLAY || ':99';

// --- Error Handling Classes ---

class SandboxError extends Error {
    constructor(message, options = {}) {
        super(message);
        this.name = 'SandboxError';
        this.errorType = options.errorType || 'UnknownError';
        this.operation = options.operation || 'unknown';
        this.parameters = options.parameters || {};
        this.suggestion = options.suggestion || null;
        this.retryable = options.retryable || false;
    }
}

function errorResponse(error, operation, parameters = {}) {
    const errorData = {
        status: 'error',
        error_type: error.errorType || 'UnknownError',
        error: error.message || String(error),
        operation: operation,
        parameters: sanitizeParameters(parameters),
        timestamp: new Date().toISOString(),
    };

    if (error.suggestion) {
        errorData.suggestion = error.suggestion;
    }

    if (error.retryable) {
        errorData.retryable = true;
    }

    const jsonStr = JSON.stringify(errorData, null, 2);

    // Print to stdout (for SDK parsing)
    console.log(jsonStr);

    // Print to stderr (for Unix convention)
    console.error(jsonStr);

    // Exit with error code
    process.exit(1);
}

function sanitizeParameters(params) {
    const sensitiveKeys = ['password', 'api_key', 'token', 'secret', 'credential'];
    const sanitized = {};

    for (const [key, value] of Object.entries(params)) {
        const isSensitive = sensitiveKeys.some(sensitive =>
            key.toLowerCase().includes(sensitive)
        );

        if (isSensitive) {
            sanitized[key] = '***REDACTED***';
        } else if (typeof value === 'string' && value.length > 200) {
            sanitized[key] = value.substring(0, 200) + '... (truncated)';
        } else {
            sanitized[key] = value;
        }
    }

    return sanitized;
}

function successResponse(result, operation = null) {
    const responseData = {
        status: 'success',
        result: result,
    };

    if (operation) {
        responseData.operation = operation;
    }

    console.log(JSON.stringify(responseData, null, 2));
}

// --- Helper Functions ---

/**
 * Launch Chromium as a detached daemon process
 * This ensures the browser persists even after the script exits
 * @returns {Promise<boolean>} - True if launch succeeded, false otherwise
 */
async function launchChromiumDaemon() {
  try {
    // Chromium command with all required arguments
    const chromiumArgs = [
      '--no-sandbox',
      '--disable-setuid-sandbox',
      '--disable-gpu',
      '--disable-dev-shm-usage',
      '--remote-debugging-port=9222',
      '--remote-debugging-address=0.0.0.0',
      '--user-data-dir=/tmp/.chromium-cdp',
      '--start-maximized',
      `--display=${DISPLAY}`
    ];

    // Launch Chromium as a detached process
    // Use system chromium as single source of truth
    const child = spawn('/usr/bin/chromium', chromiumArgs, {
      detached: true,  // Make it a daemon process
      stdio: 'ignore', // Don't inherit parent's stdio
      env: {
        ...process.env,
        DISPLAY: DISPLAY
      }
    });

    // Unref the child process so the parent can exit independently
    child.unref();

    // Give the browser time to start
    await new Promise(resolve => setTimeout(resolve, 5000));

    console.error(JSON.stringify({
      message: 'Launched Chromium as detached daemon process',
      pid: child.pid,
      stderr: child.stderr,
      stdout: child.stdout,
      display: DISPLAY,
      command: '/usr/bin/chromium ' + chromiumArgs.join(' ')
    }));

    return true;
  } catch (error) {
    console.error(JSON.stringify({
      error: `Failed to launch Chromium daemon: ${error.message}`
    }));
    return false;
  }
}

/**
 * Wait for browser to be accessible with retry logic
 * Falls back to launching a detached Chromium daemon if connection fails
 * @param {number} maxRetries - Maximum number of retry attempts
 * @param {number} retryDelay - Delay between retries in milliseconds
 * @returns {Promise<Object>} - Connected browser instance or null if failed
 */
async function waitForBrowser(maxRetries = 30, retryDelay = 2000) {
  const totalWaitTime = (maxRetries - 1) * retryDelay / 1000; // Convert to seconds

  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      // Try to connect to the browser and KEEP the connection
      const browser = await chromium.connectOverCDP(CDP_URL);

      if (attempt > 1) {
        console.error(JSON.stringify({
          message: `Connected to existing browser after ${attempt} attempts (${((attempt - 1) * retryDelay / 1000).toFixed(1)}s)`
        }));
      }

      return browser;
    } catch (error) {
      // If this is the last attempt, try launching a new browser
      if (attempt === maxRetries) {
        console.error(JSON.stringify({
          message: `Could not connect to browser on ${CDP_URL} after ${maxRetries} attempts (${totalWaitTime}s)`,
          error: error.message,
          fallback: 'Launching new chromium as detached daemon process...'
        }));

        try {
          // Launch Chromium as a detached daemon process
          console.error(JSON.stringify({
            message: 'Starting Chromium daemon launch...',
            display: DISPLAY,
            cdpUrl: CDP_URL
          }));

          const launched = await launchChromiumDaemon();

          if (!launched) {
            throw new Error('Failed to launch Chromium daemon');
          }

          // Now connect to the newly launched browser via CDP
          console.error(JSON.stringify({
            message: 'Attempting to connect to Chromium via CDP...',
            url: CDP_URL
          }));
          const browser = await chromium.connectOverCDP(CDP_URL);

          console.error(JSON.stringify({
            message: 'Successfully launched persistent chromium daemon on port 9222',
            vncPort: '5900',
            cdpPort: '9222'
          }));

          return browser;
        } catch (launchError) {
          console.error(JSON.stringify({
            error: `Failed to launch browser: ${launchError.message}`,
            stack: launchError.stack
          }));
          return null;
        }
      }

      // Log retry message
      const waitSeconds = retryDelay / 1000;
      console.error(JSON.stringify({
        message: `Browser not accessible (attempt ${attempt}/${maxRetries}), retrying in ${waitSeconds}s...`
      }));

      // Wait before next retry
      await new Promise(resolve => setTimeout(resolve, retryDelay));
    }
  }

  return null;
}

async function getPage(browser) {
  // Try to find the active page in the first context
  const contexts = browser.contexts();
  const context = contexts.length > 0 ? contexts[0] : await browser.newContext();
  const pages = context.pages();
  if (pages.length > 0) return pages[0];
  return await context.newPage();
}

async function injectDomManager(page) {
  await page.evaluate(() => {
    if (window._domManager) return;

    window._domManager = {
      elements: new Map(),
      nextId: 0,

      highlight(index) {
        const el = this.elements.get(index);
        if (el) {
          const old = el.style.outline;
          el.style.outline = '2px solid red';
          setTimeout(() => el.style.outline = old, 2000);
        }
      },

      scan() {
        this.elements.clear();
        this.nextId = 0;
        const selector = 'a, button, input, select, textarea, [role="button"], [onclick]';
        const els = document.querySelectorAll(selector);
        let results = [];
        els.forEach((el) => {
          // Filter invisible
          const rect = el.getBoundingClientRect();
          if (rect.width === 0 || rect.height === 0) return;

          const id = this.nextId++;
          this.elements.set(id, el);
          results.push({
            index: id,
            tagName: el.tagName.toLowerCase(),
            text: (el.innerText || el.textContent || '').slice(0, 50).replace(/\n/g, ' '),
            href: el.href || null,
            selector: this.getSelector(el)
          });
        });
        return results;
      },

      click(index) {
        const el = this.elements.get(index);
        if (!el) throw new Error(`Element ${index} not found`);
        // Trigger proper click event that works with React/Vue
        const event = new MouseEvent('click', {
          bubbles: true,
          cancelable: true,
          view: window
        });
        el.dispatchEvent(event);
      },

      fill(index, value) {
        const el = this.elements.get(index);
        if (!el) throw new Error(`Element ${index} not found`);
        el.value = value;
        // Trigger change and input events for React/Vue/Angular
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
      },

      getSelector(el) {
        // Proper CSS escaping
        if (el.id) {
          // Escape CSS selector
          const escapedId = CSS.escape(el.id);
          return `#${escapedId}`;
        }
        if (el.className && typeof el.className === 'string') {
          const classes = el.className.trim().split(/\s+/).filter(Boolean);
          if (classes.length > 0) {
            return '.' + classes.map(c => CSS.escape(c)).join('.');
          }
        }
        return el.tagName.toLowerCase();
      }
    };
  });
}

// --- Tool Implementations ---

const tools = {
  async browser_navigate({ url }, page, browser) {
    if (!url || !url.match(/^https?:\/\//)) {
      throw new SandboxError('Invalid URL: must start with http:// or https://', {
        errorType: 'ValidationError',
        operation: 'browser_navigate',
        parameters: { url },
        suggestion: 'Ensure the URL starts with http:// or https://'
      });
    }

    try {
      // Create a new page (tab) for navigation instead of reusing current one
      const context = browser.contexts()[0] || await browser.newContext();

      // Navigate to the URL
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });

      // Get all pages to show tab history
      const allPages = context.pages();
      const tabList = await Promise.all(allPages.map(async (p, i) => ({
        index: i,
        title: await p.title().catch(() => ''),
        url: p.url()
      })));

      return {
        url: page.url(),
        tabId: allPages.indexOf(page),
        message: `navigated to ${url}`,
        totalTabs: tabList.length,
        tabs: tabList
      };
    } catch (error) {
      if (error.name === 'TimeoutError') {
        throw new SandboxError(`Navigation timeout for ${url}`, {
          errorType: 'TimeoutError',
          operation: 'browser_navigate',
          parameters: { url },
          suggestion: 'The page may be loading slowly. Try increasing the timeout or check if the URL is accessible.',
          retryable: true
        });
      }
      throw new SandboxError(`Failed to navigate to ${url}: ${error.message}`, {
        errorType: 'ConnectionError',
        operation: 'browser_navigate',
        parameters: { url },
        suggestion: 'Check if the URL is accessible and the network connection is stable.',
        retryable: true
      });
    }
  },

  async browser_go_back({}, page) {
    try {
      await page.goBack();
      return { url: page.url() };
    } catch (error) {
      throw new SandboxError(`Failed to go back: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_go_back',
        suggestion: 'There may be no previous page in history'
      });
    }
  },

  async browser_go_forward({}, page) {
    try {
      await page.goForward();
      return { url: page.url() };
    } catch (error) {
      throw new SandboxError(`Failed to go forward: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_go_forward',
        suggestion: 'There may be no next page in history'
      });
    }
  },

  async browser_get_markdown({}, page) {
    console.error(JSON.stringify({
      deprecation: 'browser_get_markdown is deprecated. Use: python /opt/browser_tools/web_tools.py web_scrape \'{"url": "<your_url>", "format": "markdown"}\'',
      alternative: 'web_tools.py web_scrape with format="markdown"'
    }));
    const html = await page.content();
    const turndown = new TurndownService();
    turndown.remove(['script', 'style', 'noscript', 'iframe']);
    return { markdown: turndown.turndown(html) };
  },

  async browser_get_text({}, page) {
    console.error(JSON.stringify({
      deprecation: 'browser_get_text is deprecated. Use: python /opt/browser_tools/web_tools.py web_scrape \'{"url": "<your_url>", "format": "text"}\'',
      alternative: 'web_tools.py web_scrape with format="text"'
    }));
    const text = await page.evaluate(() => document.body.innerText);
    return { text };
  },

  async browser_read_links({}, page) {
    console.error(JSON.stringify({
      deprecation: 'browser_read_links is deprecated. Use: python /opt/browser_tools/web_tools.py web_links \'{"url": "<your_url>"}\'',
      alternative: 'web_tools.py web_links'
    }));
    const links = await page.evaluate(() =>
      Array.from(document.querySelectorAll('a[href]')).map(a => ({ text: a.innerText, href: a.href }))
    );
    return { links };
  },

  async browser_screenshot({ name, fullPage, selector }, page) {
    // Sanitize filename to prevent path traversal
    const safeName = (name || 'screenshot').replace(/[^a-zA-Z0-9_-]/g, '_');
    const path = `/tmp/${safeName}.jpg`;

    try {
      if (selector) {
        // Check if element exists
        const element = page.locator(selector).first();
        const count = await element.count();
        if (count === 0) {
          throw new SandboxError(`Element with selector "${selector}" not found`, {
            errorType: 'ValidationError',
            operation: 'browser_screenshot',
            parameters: { name, selector },
            suggestion: 'Verify the CSS selector is correct and the element exists on the page'
          });
        }
        await element.screenshot({ path, type: 'jpeg' });
      } else {
        await page.screenshot({ path, fullPage: !!fullPage, type: 'jpeg' });
      }
      return { path };
    } catch (error) {
      if (error instanceof SandboxError) {
        throw error;
      }
      throw new SandboxError(`Screenshot failed: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_screenshot',
        parameters: { name, selector },
        suggestion: 'Ensure the page has fully loaded and the element is visible'
      });
    }
  },

  async browser_get_clickable_elements({}, page) {
    try {
      await injectDomManager(page);
      const elements = await page.evaluate(() => window._domManager.scan());
      return { elements };
    } catch (error) {
      throw new SandboxError(`Failed to get clickable elements: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_get_clickable_elements',
        suggestion: 'Ensure the page has fully loaded'
      });
    }
  },

  async browser_click({ index, selector }, page) {
    try {
      if (index !== undefined) {
        await injectDomManager(page);
        await page.evaluate((i) => window._domManager.click(i), index);
      } else if (selector) {
        await page.click(selector);
      } else {
        throw new SandboxError("Must provide index or selector", {
          errorType: 'ValidationError',
          operation: 'browser_click',
          parameters: { index, selector },
          suggestion: 'Provide either element index from get_clickable_elements or a CSS selector'
        });
      }
      // Wait briefly for any navigation or state change
      await page.waitForTimeout(500);
      return {};
    } catch (error) {
      if (error instanceof SandboxError) {
        throw error;
      }
      throw new SandboxError(`Failed to click element: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_click',
        parameters: { index, selector },
        suggestion: 'Ensure the element is visible and clickable'
      });
    }
  },

  async browser_form_input_fill({ selector, index, value, clear }, page) {
    try {
      if (index !== undefined) {
        await injectDomManager(page);
        await page.evaluate(({i, v}) => window._domManager.fill(i, v), {i: index, v: value});
      } else if (selector) {
        if (clear) await page.fill(selector, '');
        await page.type(selector, value);
      } else {
        throw new SandboxError("Must provide index or selector", {
          errorType: 'ValidationError',
          operation: 'browser_form_input_fill',
          parameters: { index, selector },
          suggestion: 'Provide either element index from get_clickable_elements or a CSS selector'
        });
      }
      // Wait for UI to update after filling
      await page.waitForTimeout(100);
      return {};
    } catch (error) {
      if (error instanceof SandboxError) {
        throw error;
      }
      throw new SandboxError(`Failed to fill form input: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_form_input_fill',
        parameters: { index, selector },
        suggestion: 'Ensure the input element is visible and editable'
      });
    }
  },

  async browser_evaluate({ script }, page) {
    try {
      // script is expected to be "() => { ... }" or "function() { ... }"
      // We need to eval it first to convert string to function
      const fn = eval(`(${script})`);
      const result = await page.evaluate(fn);
      return { result };
    } catch (error) {
      throw new SandboxError(`Script evaluation failed: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_evaluate',
        parameters: { script: script ? script.substring(0, 100) + '...' : '' },
        suggestion: 'Check the JavaScript syntax and ensure the script is valid'
      });
    }
  },

  async browser_scroll({ amount }, page) {
     try {
       if (amount) {
         await page.evaluate((y) => window.scrollBy(0, y), amount);
       } else {
         await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
       }
       return {};
     } catch (error) {
       throw new SandboxError(`Failed to scroll: ${error.message}`, {
         errorType: 'CommandExecutionError',
         operation: 'browser_scroll',
         suggestion: 'The page may not be scrollable'
       });
     }
  },

  async browser_new_tab({ url }, page, browser) {
    try {
      // Get or create a context
      let context = browser.contexts().length > 0 ? browser.contexts()[0] : await browser.newContext();
      const newPage = await context.newPage();
      if (url) {
        await newPage.goto(url, { waitUntil: 'domcontentloaded' });
      }
      return { tabId: context.pages().indexOf(newPage) };
    } catch (error) {
      throw new SandboxError(`Failed to create new tab: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_new_tab',
        parameters: { url },
        suggestion: 'Browser may be at maximum capacity or unavailable'
      });
    }
  },

  async browser_tab_list({}, page, browser) {
    try {
      if (browser.contexts().length === 0) {
        return { tabs: [] };
      }
      const pages = browser.contexts()[0].pages();
      const list = await Promise.all(pages.map(async (p, i) => ({
        index: i,
        title: await p.title().catch(() => ''), // Handle errors for pages that can't provide title
        url: p.url(),
        active: p === page
      })));
      return { tabs: list };
    } catch (error) {
      throw new SandboxError(`Failed to list tabs: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_tab_list',
        suggestion: 'Browser contexts may be unavailable'
      });
    }
  },

  async browser_switch_tab({ index }, _page, browser) {
    try {
      if (browser.contexts().length === 0) {
         throw new SandboxError('No browser contexts available', {
           errorType: 'ValidationError',
           operation: 'browser_switch_tab',
           parameters: { index },
           suggestion: 'Ensure the browser is running and has at least one tab'
         });
      }
      const pages = browser.contexts()[0].pages();
      if (index >= 0 && index < pages.length) {
        await pages[index].bringToFront();
        return { index };
      }
      throw new SandboxError(`Invalid tab index ${index}. Available indices: 0-${pages.length - 1}`, {
        errorType: 'ValidationError',
        operation: 'browser_switch_tab',
        parameters: { index, availableIndices: `0-${pages.length - 1}` },
        suggestion: 'Use browser_tab_list to see available tabs'
      });
    } catch (error) {
      if (error instanceof SandboxError) {
        throw error;
      }
      throw new SandboxError(`Failed to switch tab: ${error.message}`, {
        errorType: 'CommandExecutionError',
        operation: 'browser_switch_tab',
        parameters: { index }
      });
    }
  },

  async browser_close({}, _page, _browser) {
    // Browser is a detached daemon process and persists for sandbox lifetime
    // Sandbox destruction will handle cleanup - no need to close here
    return { message: 'Browser closed (persists as daemon for sandbox lifetime)' };
  },

  async browser_health_check({}, _page, browser) {
    try {
      // Simple health check - just verify we can get browser info
      const version = await browser.version();
      const contexts = browser.contexts();

      return {
        status: 'healthy',
        browserVersion: version,
        contextCount: contexts.length,
        display: DISPLAY,
        message: 'Browser is accessible and responding'
      };
    } catch (error) {
      throw new SandboxError(`Browser health check failed: ${error.message}`, {
        errorType: 'ConnectionError',
        operation: 'browser_health_check',
        suggestion: 'Browser may not be running or CDP connection failed',
        retryable: true
      });
    }
  }
};

// --- Main Execution ---

(async () => {
  const command = process.argv[2];

  if (!command) {
    errorResponse(
      new SandboxError("Usage: node browser_tools.js <command>", {
        errorType: 'ValidationError',
        operation: 'main'
      }),
      'main'
    );
  }

  // Read JSON arguments from stdin to avoid shell interpretation issues
  let args = {};
  try {
    let inputData = '';

    // Read all data from stdin
    for await (const chunk of process.stdin) {
      inputData += chunk;
    }

    // Parse JSON from stdin (empty input = empty object)
    args = inputData.trim() ? JSON.parse(inputData) : {};
  } catch (e) {
    errorResponse(
      new SandboxError("Invalid JSON arguments from stdin: " + e.message, {
        errorType: 'ValidationError',
        operation: 'main',
        suggestion: 'Ensure stdin contains valid JSON'
      }),
      'main'
    );
  }

  let browser;

  try {
    // Log the start of command execution
    console.error(JSON.stringify({
      message: `Starting command: ${command}`,
      args: args
    }));

    // Wait for browser to be accessible with retry logic
    browser = await waitForBrowser(1, 2000);

    if (!browser) {
      throw new SandboxError('Browser is not accessible after maximum retry attempts', {
        errorType: 'ConnectionError',
        operation: 'waitForBrowser',
        parameters: { attemptedUrl: CDP_URL },
        suggestion: 'Ensure the browser daemon is running on port 9222',
        retryable: true
      });
    }

    const page = await getPage(browser);

    if (!tools[command]) {
      throw new SandboxError(`Unknown command: ${command}`, {
        errorType: 'ValidationError',
        operation: 'main',
        parameters: { command },
        suggestion: 'Use a valid browser tool command'
      });
    }

    // Add overall command timeout
    const commandTimeout = setTimeout(() => {
      throw new SandboxError(`Command "${command}" timed out after 120 seconds`, {
        errorType: 'TimeoutError',
        operation: command,
        parameters: args,
        suggestion: 'The operation took too long. Try optimizing the command or increasing timeout',
        retryable: false
      });
    }, 120000);

    const result = await tools[command](args, page, browser);
    clearTimeout(commandTimeout);

    successResponse(result, command);
  } catch (e) {
    if (e instanceof SandboxError) {
      errorResponse(e, e.operation || command, e.parameters);
    } else {
      errorResponse(
        new SandboxError(e.message, {
          errorType: 'UnknownError',
          operation: command,
          parameters: args
        }),
        command,
        args
      );
    }
  } finally {
    // No cleanup needed - browser persists for sandbox lifetime
    // Sandbox destruction will handle cleanup
    process.exit(0);
  }
})();
