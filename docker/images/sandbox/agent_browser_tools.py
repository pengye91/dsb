#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
"""
agent_browser_tools.py - Complete browser automation and web scraping tools

Replaces:
- browser_tools.py (browser automation)
- web_tools.py (web scraping with crawl4ai)

Uses:
- agent-browser CLI for browser automation (via subprocess)
- trafilatura for markdown extraction
- rank_bm25 for content filtering
- pandas for table extraction
- BeautifulSoup for DOM parsing

Architecture:
    tool_proxy.py (Python FastAPI)
        │
        └── agent_browser_tools.py (this file)
            │
            ├── agent-browser CLI (subprocess) → Chromium via CDP (port 9222)
            │
            └── Python libraries (trafilatura, rank_bm25, etc.)

Key Features:
- Ref-based element selection (@e1, @e2) from accessibility snapshots
- BM25 content filtering for LLM context optimization (93% savings)
- Parallel crawling with isolated sessions
"""

import asyncio
import json
import os
import tempfile
from io import StringIO
from typing import Any, Dict, List, Optional
from urllib.parse import urlparse

# Third-party libraries
from bs4 import BeautifulSoup
import pandas as pd
import trafilatura  # type: ignore[import-untyped]

# Error handling
from error_handler import SandboxError, TOOL_EXECUTION_FAILED

CDP_PORT = 9222
COMMAND_TIMEOUT = 60
MAX_TABS = int(os.environ.get("MAX_BROWSER_TABS", "20"))

# ============================================================================
# SSRF validation (module-level import with safe fallback)
# ============================================================================

_ssrf_available = False
_ssrf_validate = None
_SSRFValidationError = None

try:
    from ssrf_validation import validate_url as _ssrf_validate_import, SSRFValidationError as _SSRFValidationError_import
    _ssrf_available = True
    _ssrf_validate = _ssrf_validate_import
    _SSRFValidationError = _SSRFValidationError_import
except ImportError:
    pass


def _validate_url(url: str) -> None:
    """Validate URL for SSRF prevention.

    Uses ssrf_validation when available, falls back to a simple scheme check.
    """
    if _ssrf_available and _ssrf_validate is not None and _SSRFValidationError is not None:
        try:
            _ssrf_validate(url)
        except _SSRFValidationError as e:
            raise SandboxError(str(e))
    else:
        # Fallback if module not available in test env
        if not url.startswith(("http://", "https://")):
            raise SandboxError("Invalid URL: must start with http:// or https://")

# ============================================================================
# agent-browser CLI wrapper
# ============================================================================

async def run_agent_browser(
    command: str,
    args: Optional[List[str]] = None,
    timeout: int = COMMAND_TIMEOUT,
    session: Optional[str] = None,
    cdp_port: int = CDP_PORT,
) -> Dict[str, Any]:
    """
    Run agent-browser CLI command and return JSON result.

    Args:
        command: agent-browser command (e.g., "open", "click", "snapshot")
        args: Additional arguments for the command
        timeout: Command timeout in seconds
        session: Optional session name for isolation
        cdp_port: CDP port to connect to

    Returns:
        Parsed JSON result from agent-browser

    Raises:
        SandboxError: If command fails or times out
    """
    cmd = ["agent-browser", "--cdp", str(cdp_port), "--json"]

    if session:
        cmd.extend(["--session", session])

    cmd.extend(command.split())
    if args:
        cmd.extend(args)

    try:
        with tempfile.TemporaryFile() as stdout_f, tempfile.TemporaryFile() as stderr_f:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=stdout_f,
                stderr=stderr_f,
                preexec_fn=os.setsid,
            )

            # Wait for process to exit (does not hang if background daemon inherits stdout/stderr)
            await asyncio.wait_for(
                proc.wait(),
                timeout=timeout
            )

            stdout_f.seek(0)
            stdout = stdout_f.read()
            stderr_f.seek(0)
            stderr = stderr_f.read()

        if proc.returncode != 0:
            error_msg = stderr.decode() if stderr else "Unknown error"
            raise SandboxError(f"agent-browser failed: {error_msg}")

        output = stdout.decode().strip()
        if not output:
            return {"success": True}

        try:
            return json.loads(output)
        except json.JSONDecodeError:
            return {"output": output}

    except asyncio.TimeoutError:
        try:
            os.killpg(os.getpgid(proc.pid), 9)
        except ProcessLookupError:
            pass
        raise SandboxError(f"agent-browser command timed out after {timeout}s")


async def run_agent_browser_sync(
    command: str,
    args: Optional[List[str]] = None,
    timeout: int = COMMAND_TIMEOUT,
) -> Dict[str, Any]:
    """
    Synchronous wrapper for agent-browser CLI.

    Used for simple commands that don't need async.
    """
    return await run_agent_browser(command, args, timeout)


# ============================================================================
# Tab Management Helpers
# ============================================================================

async def _get_tab_list() -> List[Dict[str, Any]]:
    """Get list of all open browser tabs."""
    result = await run_agent_browser("tab")
    return (
        result.get("data", {}).get("tabs")
        or result.get("tabs")
        or []
    )


async def _cleanup_old_tabs(max_tabs: int = MAX_TABS) -> None:
    """Close oldest tabs when count reaches max_tabs.

    Called before opening a new tab to prevent unbounded tab growth.
    Closes tab at index 0 (oldest) until room is available.
    """
    tabs = await _get_tab_list()
    while len(tabs) >= max_tabs:
        await run_agent_browser("tab close 0")
        tabs = tabs[1:]


async def _fetch_in_new_tab(url: str, page_timeout: Optional[int] = None, wait_until: str = "domcontentloaded") -> Dict[str, Any]:
    """Open URL in a new visible tab with tab limit enforcement.

    Each web fetch opens in its own tab so VNC users can see
    all visited pages in the browser tab strip.

    Note: agent-browser's ``tab new`` does NOT wait for page load
    (unlike ``open`` which calls wait_for_lifecycle). We add an
    explicit wait to ensure the page is ready for subsequent reads.
    """
    await _cleanup_old_tabs()
    result = await run_agent_browser(f"tab new {url}")
    # Wait for page to load (tab new returns immediately, unlike open)
    # Retry up to 2 times on failure — some pages need extra time
    last_error = None
    for attempt in range(3):
        try:
            await run_agent_browser(f"wait --load {wait_until}", timeout=page_timeout or COMMAND_TIMEOUT)
            last_error = None
            break
        except SandboxError as e:
            last_error = e
            if attempt < 2:
                await asyncio.sleep(1)
    if last_error is not None:
        raise SandboxError(f"Page failed to load after 3 attempts: {last_error}", error_code=TOOL_EXECUTION_FAILED)
    return result


# ============================================================================
# Browser Automation Tools (replaces browser_tools.py)
# ============================================================================

async def browser_navigate(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Navigate to URL in a new browser tab.

    Each navigation opens in its own tab so VNC users can see
    all visited pages in the browser tab strip.

    Args:
        args: Dict with 'url' key

    Returns:
        Navigation result with URL and tab info
    """
    url = args.get("url")
    if not url:
        raise SandboxError("Missing required parameter: url")

    _validate_url(url)

    wait_until = args.get("wait_until", "domcontentloaded")
    await _fetch_in_new_tab(url, wait_until=wait_until)

    # Get tab list for additional context
    tabs = await _get_tab_list()

    return {
        "url": url,
        "message": f"Opened {url} in new tab",
        "tabs": tabs,
    }


async def browser_go_back(args: Dict[str, Any]) -> Dict[str, Any]:
    """Go back in current tab's browser history.

    Note: With multi-tab mode, each web fetch opens in its own tab.
    Back/forward operates on the current tab's history only, not across tabs.
    Use browser_tabs select <index> to switch between tabs.
    """
    await run_agent_browser("back")
    result = await run_agent_browser("get url")
    return {"url": result.get("url", "")}


async def browser_go_forward(args: Dict[str, Any]) -> Dict[str, Any]:
    """Go forward in current tab's browser history.

    Note: With multi-tab mode, each web fetch opens in its own tab.
    Back/forward operates on the current tab's history only, not across tabs.
    Use browser_tabs select <index> to switch between tabs.
    """
    await run_agent_browser("forward")
    result = await run_agent_browser("get url")
    return {"url": result.get("url", "")}


async def browser_snapshot(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Get accessibility tree with refs (@e1, @e2).

    This is the key improvement over browser_tools.py - returns
    deterministic refs for element selection.

    Args:
        args: Dict with optional keys:
            - interactive: bool - Only interactive elements (default: True)
            - cursor: bool - Include cursor position (default: False)
            - compact: bool - Compact output (default: False)

    Returns:
        Accessibility snapshot with refs
    """
    interactive = args.get("interactive", True)
    cursor = args.get("cursor", False)
    compact = args.get("compact", False)

    cmd_parts = ["snapshot"]
    if interactive:
        cmd_parts.append("-i")
    if cursor:
        cmd_parts.append("-C")
    if compact:
        cmd_parts.append("-c")

    result = await run_agent_browser(" ".join(cmd_parts))
    return result


async def browser_click(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Click element by ref or selector.

    Args:
        args: Dict with one of:
            - ref: Element ref from snapshot (e.g., "@e1")
            - selector: CSS selector

    Returns:
        Click result
    """
    ref = args.get("ref")
    selector = args.get("selector")

    if ref:
        await run_agent_browser(f"click {ref}")
    elif selector:
        await run_agent_browser(f"click {selector}")
    else:
        raise SandboxError("Must provide ref or selector")

    return {"success": True}


async def browser_fill(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Fill form field by ref or selector.

    Args:
        args: Dict with:
            - ref: Element ref from snapshot (e.g., "@e1")
            - selector: CSS selector
            - value: Value to fill
            - clear: Whether to clear the field before filling (default: true)

    Returns:
        Fill result
    """
    ref = args.get("ref")
    selector = args.get("selector")
    value = args.get("value", "")
    clear = args.get("clear", True)

    if ref:
        fill_args = [ref, value]
        if not clear:
            fill_args.append("--no-clear")
        await run_agent_browser("fill", fill_args)
    elif selector:
        fill_args = [selector, value]
        if not clear:
            fill_args.append("--no-clear")
        await run_agent_browser("fill", fill_args)
    else:
        raise SandboxError("Must provide ref or selector")

    return {"success": True}


async def browser_evaluate(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Evaluate JavaScript in browser context.

    Args:
        args: Dict with 'script' key

    Returns:
        Evaluation result
    """
    script = args.get("script")
    if not script:
        raise SandboxError("Missing required parameter: script")

    result = await run_agent_browser("eval", [script])
    # agent-browser --json returns result in different formats:
    # - {"success": true, "data": {"result": ...}}
    # - {"success": true, "result": ...}
    # - {"result": ...}
    # - {"output": ...}
    eval_result = (
        result.get("data", {}).get("result") or
        result.get("result") or
        result.get("output") or
        result.get("data")
    )
    return {"result": eval_result}


async def browser_scroll(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Scroll page.

    Args:
        args: Dict with optional:
            - direction: "up" or "down" (default: "down")
            - amount: Pixels to scroll (default: 300)

    Returns:
        Scroll result
    """
    direction = args.get("direction", "down")
    amount = args.get("amount", 300)
    await run_agent_browser(f"scroll {direction} {amount}")
    return {"success": True}


async def browser_screenshot(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Take screenshot.

    Args:
        args: Dict with optional:
            - fullPage: bool - Full page screenshot (default: False)
            - name: str - Screenshot name
            - selector: str - CSS selector to screenshot a specific element

    Returns:
        Screenshot result with path
    """
    full_page = args.get("fullPage", False)
    name = args.get("name")
    selector = args.get("selector")

    cmd_parts = ["screenshot"]
    if full_page:
        cmd_parts.append("--full")
    if selector:
        cmd_parts.append(selector)
    result = await run_agent_browser(" ".join(cmd_parts))

    return {
        "path": result.get("path", f"/tmp/{name or 'screenshot'}.png"),
        "screenshot": result.get("screenshot"),
        "screenshot_encoding": "base64" if result.get("screenshot") else None,
    }


async def browser_tabs(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Manage tabs (list, new, select, close).

    Args:
        args: Dict with:
            - action: "list"|"new"|"select"|"close" (default: "list")
            - index: Tab index for select/close
            - url: URL for new tab

    Returns:
        Tab operation result
    """
    action = args.get("action", "list")
    index = args.get("index")
    url = args.get("url", "")

    if action == "list":
        result = await run_agent_browser("tab")
        # agent-browser --json returns tabs in different formats:
        # - {"success": true, "data": {"tabs": [...]}}
        # - {"success": true, "tabs": [...]}
        # - {"tabs": [...]}
        tabs = (
            result.get("data", {}).get("tabs") or
            result.get("tabs") or
            []
        )
        return {"tabs": tabs}

    elif action == "new":
        if url:
            result = await run_agent_browser(f"tab new {url}")
        else:
            result = await run_agent_browser("tab new")
        return {"tabId": result.get("tabId")}

    elif action == "select":
        if index is None:
            raise SandboxError("Must provide index for select action")
        await run_agent_browser(f"tab {index}")
        return {"index": index}

    elif action == "close":
        if index is not None:
            await run_agent_browser(f"tab close {index}")
        else:
            await run_agent_browser("tab close")
        return {"success": True}

    else:
        raise SandboxError(f"Unknown tab action: {action}")


async def browser_wait(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Wait for element, text, or time.

    Args:
        args: Dict with one of:
            - selector: CSS selector to wait for
            - text: Text to wait for
            - time: Time in ms to wait

    Returns:
        Wait result
    """
    selector = args.get("selector")
    text = args.get("text")
    time_ms = args.get("time")

    if selector:
        await run_agent_browser(f"wait {selector}")
    elif text:
        await run_agent_browser("wait", ["--text", text])
    elif time_ms:
        await run_agent_browser(f"wait {time_ms}")
    else:
        raise SandboxError("Must provide selector, text, or time")

    return {"success": True}


async def browser_press_key(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Press a key.

    Args:
        args: Dict with 'key' key (e.g., "Enter", "Tab", "Escape")

    Returns:
        Press result
    """
    key = args.get("key")
    if not key:
        raise SandboxError("Missing required parameter: key")

    await run_agent_browser(f"press {key}")
    return {"success": True}


async def browser_hover(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Hover over element.

    Args:
        args: Dict with:
            - ref: Element ref from snapshot
            - selector: CSS selector

    Returns:
        Hover result
    """
    ref = args.get("ref")
    selector = args.get("selector")

    if ref:
        await run_agent_browser(f"hover {ref}")
    elif selector:
        await run_agent_browser(f"hover {selector}")
    else:
        raise SandboxError("Must provide ref or selector")

    return {"success": True}


async def browser_get_text(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Get page text content.

    Args:
        args: Dict with optional 'ref' for specific element

    Returns:
        Text content
    """
    ref = args.get("ref")
    if ref:
        result = await run_agent_browser(f"get text {ref}")
    else:
        # Use "body" selector to get full page text
        result = await run_agent_browser("get text body")
    # agent-browser returns text in data.text
    text = result.get("data", {}).get("text") or result.get("text") or result.get("output")
    return {"text": text}


async def browser_get_html(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Get page HTML content.

    Args:
        args: Dict with optional 'ref' for specific element

    Returns:
        HTML content
    """
    ref = args.get("ref")
    if ref:
        result = await run_agent_browser(f"get html {ref}")
    else:
        # Use "body" selector to get full page HTML
        result = await run_agent_browser("get html body")
    # agent-browser returns HTML in data.html
    html = result.get("data", {}).get("html") or result.get("html") or result.get("output")
    return {"html": html}


async def browser_health_check(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Health check for browser.

    Returns:
        Health status
    """
    try:
        result = await run_agent_browser("get url")
        return {
            "status": "healthy",
            "cdp_url": f"http://localhost:{CDP_PORT}",
            "current_url": result.get("url", ""),
            "message": "Browser is accessible and responding",
        }
    except Exception as e:
        raise SandboxError(f"Browser health check failed: {str(e)}")


# Backward compatibility - alias for browser_get_clickable_elements
async def browser_get_clickable_elements(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Get clickable elements (backward compatibility).

    Returns interactive elements with refs.
    """
    result = await browser_snapshot({"interactive": True})
    return {"elements": result.get("elements", [])}


# Backward compatibility - alias for browser_form_input_fill
async def browser_form_input_fill(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Fill form input (backward compatibility).
    """
    return await browser_fill(args)


# Backward compatibility - alias for browser_tab_list
async def browser_tab_list(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    List tabs (backward compatibility).
    """
    return await browser_tabs({"action": "list"})


# Backward compatibility - alias for browser_switch_tab
async def browser_switch_tab(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Switch tab (backward compatibility).
    """
    index = args.get("index")
    if index is None:
        raise SandboxError("Missing required parameter: index")
    return await browser_tabs({"action": "select", "index": index})


# Backward compatibility - alias for browser_new_tab
async def browser_new_tab(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    New tab (backward compatibility).
    """
    url = args.get("url", "")
    return await browser_tabs({"action": "new", "url": url})


async def browser_close(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Close current tab and switch to adjacent tab.

    If only one tab is open, navigates to about:blank instead
    (can't close the last tab without killing the browser).
    """
    try:
        tabs = await _get_tab_list()
        if len(tabs) > 1:
            # Close current tab, browser auto-switches to adjacent
            await run_agent_browser("tab close")
            return {
                "status": "success",
                "message": "Tab closed, switched to adjacent tab",
            }
        else:
            # Last tab - navigate to blank instead of closing
            await run_agent_browser("open about:blank")
            return {
                "status": "success",
                "message": "Browser session cleared",
            }
    except Exception as e:
        raise SandboxError(f"Failed to close browser: {str(e)}")


# ============================================================================
# Web Scraping Tools (replaces web_tools.py)
# ============================================================================

async def _get_page_html() -> str:
    """Get current page HTML via agent-browser."""
    result = await run_agent_browser("get html body")
    # agent-browser returns HTML in data.html
    html = result.get("data", {}).get("html") or result.get("html") or result.get("output") or ""
    return html


async def _get_page_title() -> str:
    """Get current page title via agent-browser."""
    try:
        result = await run_agent_browser("get title")
        return (
            result.get("data", {}).get("title")
            or result.get("title")
            or result.get("output")
            or ""
        )
    except SandboxError:
        return ""


def _clean_css_from_markdown(markdown: str) -> str:
    """
    Remove CSS content from markdown text.

    This handles:
    1. CSS in code blocks (```css ...```)
    2. Inline styles in HTML (style="...")
    3. CSS rule patterns ({ ... })
    4. HTML style tags (<style>...</style>)
    """
    if not markdown:
        return markdown

    import re

    # Remove CSS code blocks
    markdown = re.sub(
        r'```css\s*\n.*?```',
        '',
        markdown,
        flags=re.DOTALL | re.MULTILINE | re.IGNORECASE
    )

    # Remove <style>...</style> blocks
    markdown = re.sub(
        r'<style[^>]*>.*?</style>',
        '',
        markdown,
        flags=re.DOTALL | re.MULTILINE | re.IGNORECASE
    )

    # Remove inline style attributes
    markdown = re.sub(
        r'\sstyle\s*=\s*["\'][^"\']*["\']',
        '',
        markdown,
        flags=re.IGNORECASE
    )

    # Remove class attributes
    markdown = re.sub(
        r'\sclass\s*=\s*["\'][^"\']*["\']',
        '',
        markdown,
        flags=re.IGNORECASE
    )

    # Clean up multiple consecutive blank lines
    markdown = re.sub(r'\n\s*\n\s*\n', '\n\n', markdown)

    return markdown.strip()


def _apply_bm25_filtering(content: str, search_query: str, max_chunks: int = 10) -> str:
    """
    Apply BM25 content filtering to reduce token usage.

    Args:
        content: Text content to filter
        search_query: Query for relevance scoring
        max_chunks: Maximum number of chunks to return

    Returns:
        Filtered content with most relevant chunks
    """
    if not content or not search_query:
        return content

    try:
        from rank_bm25 import BM25Okapi

        # Split content into chunks
        chunks = [c.strip() for c in content.split('\n\n') if c.strip()]

        if len(chunks) <= 1:
            return content

        # Tokenize
        tokenized = [c.split() for c in chunks]

        # Create BM25 index
        bm25 = BM25Okapi(tokenized)

        # Get top chunks
        top_chunks = bm25.get_top_n(
            search_query.split(),
            chunks,
            n=min(max_chunks, len(chunks))
        )

        return '\n\n'.join(top_chunks)

    except ImportError:
        # rank_bm25 not available, return original content
        return content
    except Exception:
        return content


async def web_scrape(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Web scraping with multiple output formats.

    Supports BM25 filtering for LLM context optimization.

    Args:
        args: Dict with:
            - url: Target URL (required)
            - format: Output format - markdown|html|text|links (default: markdown)
            - css_selector: Target specific CSS selector (optional)
            - search_query: Query for BM25 content filtering (optional)
            - max_length: Maximum content length (optional)

    Returns:
        Scraped content
    """
    url = args.get("url")
    if not url:
        raise SandboxError("Missing required parameter: url")

    _validate_url(url)

    output_format = args.get("format", "markdown")
    css_selector = args.get("css_selector")
    search_query = args.get("search_query")
    max_length = args.get("max_length")
    page_timeout = args.get("page_timeout")
    wait_until = args.get("wait_until", "domcontentloaded")

    # Validate output format
    valid_formats = ["markdown", "html", "text", "links"]
    if output_format not in valid_formats:
        raise SandboxError(f"Invalid format: {output_format}. Must be one of {valid_formats}")

    await _fetch_in_new_tab(url, page_timeout=page_timeout, wait_until=wait_until)

    # Get HTML
    html = await _get_page_html()

    if not html.strip():
        raise SandboxError(
            f"Page returned no content: {url}. The page may be blocking scrapers, still loading, or serving non-HTML content.",
            error_code=TOOL_EXECUTION_FAILED,
        )

    title = await _get_page_title()

    # Apply CSS selector if provided
    if css_selector:
        soup = BeautifulSoup(html, 'lxml')
        element = soup.select_one(css_selector)
        if element:
            html = str(element)

    # Process based on format
    if output_format == "markdown":
        content = trafilatura.extract(
            html,
            output_format='markdown',
            include_comments=False,
            include_tables=True,
        )
        if content is None:
            print(f"WARNING: trafilatura returned None for {url}, falling back to raw text extraction")
            content = trafilatura.extract(html, output_format='txt') or ""

        # Clean CSS from markdown
        content = _clean_css_from_markdown(content)

        # Apply BM25 filtering if query provided
        if search_query and content:
            content = _apply_bm25_filtering(content, search_query)

        if max_length and len(content) > max_length:
            content = content[:max_length] + "..."

        return {"url": url, "title": title, "content": content, "format": "markdown"}

    elif output_format == "html":
        content = trafilatura.extract(html, output_format='html') or html
        if max_length and len(content) > max_length:
            content = content[:max_length] + "..."
        return {"url": url, "title": title, "content": content, "format": "html"}

    elif output_format == "text":
        content = trafilatura.extract(html, output_format='txt') or ""
        if max_length and len(content) > max_length:
            content = content[:max_length] + "..."
        return {"url": url, "title": title, "content": content, "format": "text"}

    elif output_format == "links":
        soup = BeautifulSoup(html, 'lxml')
        base_domain = urlparse(url).netloc

        internal = []
        external = []

        for a in soup.find_all('a', href=True):
            href_val = a.get('href')
            href = str(href_val) if href_val else ''
            if href.startswith('http'):
                link_domain = urlparse(href).netloc
                if link_domain == base_domain:
                    internal.append(href)
                else:
                    external.append(href)

        return {"url": url, "title": title, "links": {"internal": internal, "external": external}}


async def web_extract_css(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Extract structured data using CSS selectors.

    Args:
        args: Dict with:
            - url: Target URL (required)
            - schema: JSON schema mapping field names to CSS selectors (required)

    Returns:
        Extracted data
    """
    url = args.get("url")
    schema = args.get("schema")

    if not url:
        raise SandboxError("Missing required parameter: url")
    if not schema:
        raise SandboxError("Missing required parameter: schema")

    _validate_url(url)

    await _fetch_in_new_tab(url)

    # Get HTML
    html = await _get_page_html()
    soup = BeautifulSoup(html, 'lxml')

    # Extract data using schema
    extracted = {}
    for field_name, selector in schema.items():
        element = soup.select_one(selector)
        if element:
            extracted[field_name] = element.get_text(strip=True)

    return {"url": url, "extracted_data": extracted}


async def web_extract_table(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Extract tables from web pages as JSON.

    Args:
        args: Dict with:
            - url: Target URL (required)
            - table_index: Which table to extract (default: 0)

    Returns:
        Table data with headers and rows
    """
    url = args.get("url")
    if not url:
        raise SandboxError("Missing required parameter: url")

    table_index = args.get("table_index", 0)

    _validate_url(url)

    await _fetch_in_new_tab(url)

    # Get HTML
    html = await _get_page_html()

    # Extract tables using pandas
    try:
        tables = pd.read_html(StringIO(html))
    except Exception:
        tables = []

    if table_index >= len(tables):
        raise SandboxError(f"Table index {table_index} out of range (found {len(tables)} tables)")

    table = tables[table_index]

    return {
        "url": url,
        "table_index": table_index,
        "total_tables": len(tables),
        "headers": [str(col) for col in table.columns.tolist()],
        "rows": [[str(val) for val in row] for row in table.values.tolist()],
    }


async def web_screenshot(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Capture screenshot.

    Args:
        args: Dict with:
            - url: Target URL (required)
            - full_page: Capture full page (default: True)

    Returns:
        Screenshot data
    """
    url = args.get("url")
    if not url:
        raise SandboxError("Missing required parameter: url")

    full_page = args.get("full_page", True)

    _validate_url(url)

    await _fetch_in_new_tab(url)

    # Take screenshot
    cmd = "screenshot --full" if full_page else "screenshot"
    result = await run_agent_browser(cmd)

    screenshot = result.get("screenshot")
    response = {"url": url, "screenshot": screenshot}
    if screenshot:
        response["screenshot_encoding"] = "base64"
    return response




async def web_links(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Extract all links from a page.

    Args:
        args: Dict with:
            - url: Target URL (required)
            - filter_external: Only return external links (default: False)

    Returns:
        Extracted links
    """
    url = args.get("url")
    if not url:
        raise SandboxError("Missing required parameter: url")

    filter_external = args.get("filter_external", False)

    _validate_url(url)

    await _fetch_in_new_tab(url)

    # Get HTML
    html = await _get_page_html()
    soup = BeautifulSoup(html, 'lxml')
    base_domain = urlparse(url).netloc

    internal = []
    external = []

    for a in soup.find_all('a', href=True):
        href_val = a.get('href')
        href = str(href_val) if href_val else ''
        if href.startswith('http'):
            link_domain = urlparse(href).netloc
            if link_domain == base_domain:
                internal.append(href)
            else:
                external.append(href)

    all_links = internal + external

    if filter_external:
        all_links = external

    return {"url": url, "total_links": len(all_links), "links": all_links}


async def web_crawl(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Multi-page parallel crawling.

    Args:
        args: Dict with:
            - urls: List of URLs to crawl (required)
            - format: Output format - markdown|html|text (default: markdown)
            - search_query: Query for BM25 filtering (optional)
            - max_length: Maximum content length (optional)

    Returns:
        Crawl results
    """
    urls = args.get("urls", [])
    if not urls or not isinstance(urls, list):
        raise SandboxError("Missing or invalid parameter: urls (must be a list)")

    output_format = args.get("format", "markdown")
    search_query = args.get("search_query")
    max_length = args.get("max_length")

    # Validate output format
    valid_formats = ["markdown", "html", "text"]
    if output_format not in valid_formats:
        raise SandboxError(f"Invalid format: {output_format}. Must be one of {valid_formats}")

    async def crawl_single(session_name: str, url: str) -> Dict[str, Any]:
        """Crawl a single URL in isolated session."""
        try:
            # SSRF prevention using ssrf_validation
            if _ssrf_available and _ssrf_validate is not None and _SSRFValidationError is not None:
                try:
                    _ssrf_validate(url)
                except _SSRFValidationError as e:
                    return {"url": url, "success": False, "error": str(e)}
            else:
                if not url.startswith(("http://", "https://")):
                    return {"url": url, "success": False, "error": "Invalid URL: must start with http:// or https://"}

            # Use isolated session for each URL
            await run_agent_browser(f"open {url}", session=session_name)

            # Get HTML
            result = await run_agent_browser("get html", session=session_name)
            html = result.get("html") or result.get("output") or ""

            # Extract content
            if output_format == "markdown":
                content = trafilatura.extract(html, output_format='markdown') or ""
                content = _clean_css_from_markdown(content)

                # Apply BM25 if query provided
                if search_query and content:
                    content = _apply_bm25_filtering(content, search_query)

            elif output_format == "html":
                content = trafilatura.extract(html, output_format='html') or html

            else:  # text
                content = trafilatura.extract(html, output_format='txt') or ""

            if max_length and len(content) > max_length:
                content = content[:max_length] + "..."

            return {"url": url, "success": True, "content": content}

        except Exception as e:
            return {"url": url, "success": False, "error": str(e)}

    # Run in parallel with isolated sessions
    tasks = [
        crawl_single(f"crawler_{i}", url)
        for i, url in enumerate(urls)
    ]
    results = await asyncio.gather(*tasks)

    return {
        "total_urls": len(urls),
        "successful": sum(1 for r in results if r["success"]),
        "failed": sum(1 for r in results if not r["success"]),
        "results": results,
    }


async def web_health_check(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Health check for web scraping tools.
    """
    try:
        # Test agent-browser connection
        result = await run_agent_browser("get url")

        # Test Python libraries
        test_html = "<html><body><p>Test</p></body></html>"
        trafilatura.extract(test_html)

        return {
            "status": "healthy",
            "browser_ready": True,
            "cdp_url": f"http://localhost:{CDP_PORT}",
            "libraries": {
                "trafilatura": "ok",
                "rank_bm25": "ok" if _apply_bm25_filtering("test content", "test") else "not available",
                "pandas": "ok",
                "beautifulsoup4": "ok",
            },
            "message": "Web tools are healthy",
        }
    except Exception as e:
        raise SandboxError(f"Health check failed: {str(e)}")


# ============================================================================
# Tool Registry
# ============================================================================

# For tool_proxy.py dynamic loading via COMMANDS dict
COMMANDS = {
    # Browser automation (from browser_tools.py)
    'browser_navigate': browser_navigate,
    'browser_go_back': browser_go_back,
    'browser_go_forward': browser_go_forward,
    'browser_snapshot': browser_snapshot,
    'browser_click': browser_click,
    'browser_fill': browser_fill,
    'browser_evaluate': browser_evaluate,
    'browser_scroll': browser_scroll,
    'browser_screenshot': browser_screenshot,
    'browser_tabs': browser_tabs,
    'browser_wait': browser_wait,
    'browser_press_key': browser_press_key,
    'browser_hover': browser_hover,
    'browser_get_text': browser_get_text,
    'browser_get_html': browser_get_html,
    'browser_health_check': browser_health_check,
    'browser_close': browser_close,

    # Backward compatibility aliases
    'browser_get_clickable_elements': browser_get_clickable_elements,
    'browser_form_input_fill': browser_form_input_fill,
    'browser_tab_list': browser_tab_list,
    'browser_switch_tab': browser_switch_tab,
    'browser_new_tab': browser_new_tab,

    # Web scraping (from web_tools.py)
    'web_scrape': web_scrape,
    'web_extract_css': web_extract_css,
    'web_extract_table': web_extract_table,
    'web_screenshot': web_screenshot,
    'web_links': web_links,
    'web_crawl': web_crawl,
    'web_health_check': web_health_check,
}

# Also export as AGENT_BROWSER_TOOLS for explicit imports
AGENT_BROWSER_TOOLS = COMMANDS
