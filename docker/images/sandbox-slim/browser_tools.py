#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
"""
browser_tools.py - Browser automation via Playwright Python

This module provides browser automation tools using Playwright Python.
It connects to a browser via Chrome DevTools Protocol (CDP) on port 9222.

Architecture:
- Primary: Connects to existing browser on http://127.0.0.1:9222
- Fallback: Launches new Chromium if no browser exists
- Persistence: Browser state managed by tool_proxy.py
- Cleanup: Handled by tool_proxy.py lifespan

Features:
- Connect to CDP browser (existing Chromium)
- Navigate, screenshot, click, form fill
- Tab management (new, switch, list)
- JavaScript evaluation
- Element scanning and interaction

Based on official Playwright Python documentation:
https://playwright.dev/python/docs/api/class-page
https://playwright.dev/python/docs/api/class-playwright
"""

import asyncio
import os
from typing import Any, Dict, List, Optional
from playwright.async_api import async_playwright, Browser, Page, BrowserContext

# Environment configuration
CDP_URL = os.getenv("CDP_URL", "http://localhost:9222")
DISPLAY = os.getenv("DISPLAY", ":1")


# ============================================================================
# Error Handling
# ============================================================================

class SandboxError(Exception):
    """Custom error for sandbox operations."""
    def __init__(self, message: str, status_code: int = 500):
        self.message = message
        self.status_code = status_code
        super().__init__(message)


# ============================================================================
# Browser Connection Management
# ============================================================================

# Note: Browser lifecycle is managed by tool_proxy.py
# This module only provides tool functions that operate on the browser/page
# passed to them by execute_browser_tool()

async def get_page(browser: Browser) -> Page:
    """
    Get or create the active page.

    Args:
        browser: Playwright browser instance

    Returns:
        Active or new page
    """
    if not browser:
        raise SandboxError("Browser object is null or undefined")

    # Get existing context (CDP browsers have at least one context)
    contexts = browser.contexts
    if contexts:
        context = contexts[0]
    else:
        raise SandboxError("No browser context available")

    # Get existing page or create new one
    pages = context.pages
    if pages:
        return pages[0]
    else:
        return await context.new_page()


async def inject_dom_manager(page: Page) -> None:
    """
    Inject DOM manager for element scanning and interaction.

    Args:
        page: Playwright page instance
    """
    await page.evaluate("""() => {
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
                    // Filter invisible elements
                    const rect = el.getBoundingClientRect();
                    if (rect.width === 0 || rect.height === 0) return;

                    const id = this.nextId++;
                    this.elements.set(id, el);
                    results.push({
                        index: id,
                        tagName: el.tagName.toLowerCase(),
                        text: (el.innerText || el.textContent || '').slice(0, 50).replace(/\\n/g, ' '),
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
                    // Escape CSS selector (CSS.escape not available in all browsers)
                    const escapedId = el.id.replace(/[^a-zA-Z0-9_-]/g, '\\\\$&');
                    return '#' + escapedId;
                }
                if (el.className && typeof el.className === 'string') {
                    const classes = el.className.trim().split(/\\s+/).filter(Boolean);
                    if (classes.length > 0) {
                        return '.' + classes.map(c => c.replace(/[^a-zA-Z0-9_-]/g, '\\\\$&')).join('.');
                    }
                }
                return el.tagName.toLowerCase();
            }
        };
    }""")


# ============================================================================
# Browser Automation Tools
# ============================================================================

async def browser_navigate(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """
    Navigate to URL in browser.

    Args:
        args: Dict with 'url' key
        page: Playwright page instance
        browser: Playwright browser instance

    Returns:
        Navigation result with URL and tab info
    """
    url = args.get('url')
    if not url or not url.startswith(('http://', 'https://', 'data:')):
        raise SandboxError('Invalid URL: must start with http://, https://, or data:')

    try:
        await page.goto(url)

        # Get all pages to show tab history
        context = page.context
        all_pages = context.pages
        tab_list = []

        for i, p in enumerate(all_pages):
            try:
                title = await p.title()
            except:
                title = ''
            tab_list.append({
                'index': i,
                'title': title,
                'url': p.url,
                'active': p == page
            })

        return {
            'url': page.url,
            'tabId': all_pages.index(page),
            'message': f'navigated to {url}',
            'totalTabs': len(tab_list),
            'tabs': tab_list
        }

    except Exception as e:
        raise SandboxError(f'Failed to navigate to {url}: {str(e)}')


async def browser_go_back(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Go back in browser history."""
    try:
        await page.go_back()
        return {'url': page.url}
    except Exception as e:
        raise SandboxError(f'Failed to go back: {str(e)}')


async def browser_go_forward(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Go forward in browser history."""
    try:
        await page.go_forward()
        return {'url': page.url}
    except Exception as e:
        raise SandboxError(f'Failed to go forward: {str(e)}')


async def browser_get_clickable_elements(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Get all clickable elements on page."""
    try:
        await inject_dom_manager(page)
        elements = await page.evaluate('() => window._domManager.scan()')
        return {'elements': elements}
    except Exception as e:
        raise SandboxError(f'Failed to get clickable elements: {str(e)}')


async def browser_click(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Click element by index or selector."""
    index = args.get('index')
    selector = args.get('selector')

    try:
        if index is not None:
            await inject_dom_manager(page)
            await page.evaluate(f'() => window._domManager.click({index})')
        elif selector:
            await page.click(selector)
        else:
            raise SandboxError("Must provide index or selector")

        # Wait for navigation or state change
        await asyncio.sleep(0.5)
        return {}

    except Exception as e:
        raise SandboxError(f'Failed to click element: {str(e)}')


async def browser_form_input_fill(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Fill form input by index or selector."""
    selector = args.get('selector')
    index = args.get('index')
    value = args.get('value', '')
    clear = args.get('clear', False)

    try:
        if index is not None:
            await inject_dom_manager(page)
            await page.evaluate(f'() => window._domManager.fill({index}, {value!r})')
        elif selector:
            # Wait for selector
            await page.wait_for_selector(selector, timeout=5000)

            if clear:
                await page.fill(selector, '')
            await page.fill(selector, value)
        else:
            raise SandboxError("Must provide index or selector")

        # Wait for UI to update
        await asyncio.sleep(0.1)
        return {}

    except Exception as e:
        raise SandboxError(f'Failed to fill form input: {str(e)}')


async def browser_evaluate(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Evaluate JavaScript in page context."""
    script = args.get('script')
    if not script:
        raise SandboxError("Must provide script")

    try:
        result = await page.evaluate(script)
        return {'result': result}
    except Exception as e:
        raise SandboxError(f'Script evaluation failed: {str(e)}')


async def browser_scroll(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Scroll page."""
    amount = args.get('amount')

    try:
        if amount:
            await page.evaluate(f'() => window.scrollBy(0, {amount})')
        else:
            await page.evaluate('() => window.scrollTo(0, document.body.scrollHeight)')
        return {}
    except Exception as e:
        raise SandboxError(f'Failed to scroll: {str(e)}')


async def browser_screenshot(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Take screenshot of page."""
    name = args.get('name')
    full_page = args.get('fullPage', False)

    try:
        import time
        timestamp = int(time.time() * 1000)
        screenshot_name = name or f'screenshot_{timestamp}'
        screenshot_path = f'/tmp/{screenshot_name}.png'

        await page.screenshot(path=screenshot_path, full_page=full_page)

        return {'path': screenshot_path}

    except Exception as e:
        raise SandboxError(f'Failed to take screenshot: {str(e)}')


async def browser_new_tab(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Open new tab."""
    url = args.get('url')

    try:
        if not browser:
            raise SandboxError("Browser object is null or undefined")

        context = page.context
        new_page = await context.new_page()

        if url:
            await new_page.goto(url, wait_until='domcontentloaded')

        all_pages = context.pages
        return {'tabId': all_pages.index(new_page)}

    except Exception as e:
        raise SandboxError(f'Failed to create new tab: {str(e)}')


async def browser_tab_list(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """List all tabs."""
    try:
        if not browser:
            raise SandboxError("Browser object is null or undefined")

        context = page.context
        pages = context.pages
        tab_list = []

        for i, p in enumerate(pages):
            try:
                title = await p.title()
            except:
                title = ''
            tab_list.append({
                'index': i,
                'title': title,
                'url': p.url,
                'active': p == page
            })

        return {'tabs': tab_list}

    except Exception as e:
        raise SandboxError(f'Failed to list tabs: {str(e)}')


async def browser_switch_tab(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Switch to tab by index."""
    index = args.get('index')

    try:
        if not browser:
            raise SandboxError("Browser object is null or undefined")

        context = page.context
        pages = context.pages

        if len(pages) == 0:
            raise SandboxError('No browser pages available')

        if index >= 0 and index < len(pages):
            # Playwright doesn't have bringToFront, use page itself
            await pages[index].wait_for_load_state('domcontentloaded')
            return {'index': index}

        raise SandboxError(f'Invalid tab index {index}. Available indices: 0-{len(pages) - 1}')

    except Exception as e:
        raise SandboxError(f'Failed to switch tab: {str(e)}')


async def browser_close(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Close browser."""
    # Note: Browser closing is handled by tool_proxy.py lifecycle
    # This is a no-op in the new architecture
    return {'message': 'Browser close handled by tool_proxy.py'}


async def browser_health_check(args: Dict[str, Any], page: Page, browser: Browser) -> Dict[str, Any]:
    """Health check for browser."""
    try:
        if not browser:
            raise SandboxError("Browser object is null or undefined")

        context = page.context
        pages = context.pages

        # Test if browser is actually ready by evaluating JavaScript
        # This ensures the CDP connection is fully functional
        try:
            await page.evaluate("() => true")
        except Exception as js_error:
            raise SandboxError(f'Browser not ready: {str(js_error)}')

        return {
            'status': 'healthy',
            'browserVersion': browser.version,
            'pageCount': len(pages),
            'display': DISPLAY,
            'message': 'Browser is accessible and responding'
        }

    except Exception as e:
        raise SandboxError(f'Browser health check failed: {str(e)}')


# ============================================================================
# Tool Registry
# ============================================================================

BROWSER_TOOLS = {
    'browser_navigate': browser_navigate,
    'browser_go_back': browser_go_back,
    'browser_go_forward': browser_go_forward,
    'browser_get_clickable_elements': browser_get_clickable_elements,
    'browser_click': browser_click,
    'browser_form_input_fill': browser_form_input_fill,
    'browser_evaluate': browser_evaluate,
    'browser_scroll': browser_scroll,
    'browser_screenshot': browser_screenshot,
    'browser_new_tab': browser_new_tab,
    'browser_tab_list': browser_tab_list,
    'browser_switch_tab': browser_switch_tab,
    'browser_close': browser_close,
    'browser_health_check': browser_health_check,
}


async def execute_browser_tool(action: str, args: Dict[str, Any], browser: Browser, page: Page) -> Any:
    """
    Execute a browser automation tool.

    Args:
        action: Tool name to execute
        args: Arguments for the tool
        browser: Playwright browser instance
        page: Playwright page instance

    Returns:
        Tool execution result

    Raises:
        SandboxError: If tool not found or execution fails
    """
    if action not in BROWSER_TOOLS:
        raise SandboxError(f'Unknown browser tool: {action}')

    tool_func = BROWSER_TOOLS[action]
    return await tool_func(args, page, browser)
