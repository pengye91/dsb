"""
Web scraping and browser automation API (asynchronous).

This module provides APIs for web scraping and browser automation using
the system Chromium instance via Chrome DevTools Protocol (CDP).

Commands are executed via sandbox.exec() running:
- agent_browser_tools.py: AI-native browser automation using agent-browser CLI
  - Ref-based element selection (@e1, @e2) from accessibility snapshots
  - Web scraping with BM25 filtering for LLM context optimization
  - Free DuckDuckGo search (no API key required)

Legacy tools (kept for backward compatibility):
- web_tools.py: crawl4ai-based web scraping
- browser_tools.py: Playwright Python browser automation

Use with AsyncDSBClient for asynchronous operations.
"""

from __future__ import annotations

import warnings
from typing import Any

from dsb_sdk.constants import (
    DEFAULT_BROWSER_TOOLS_TIMEOUT,
    DEFAULT_HTTP_BUFFER_SECS,
    DEFAULT_WEB_TOOLS_TIMEOUT,
)
from dsb_sdk.exceptions import DSBValidationError
from dsb_sdk.transport.async_transport import AsyncTransport
from dsb_sdk.types.sandbox import Sandbox
from dsb_sdk.types.web import (
    BrowserActionResponse,
    BrowserInfo,
    WebCrawlResponse,
    WebFormat,
    WebHealthResponse,
    WebLinksResponse,
    WebScrapeResult,
    WebScrapeResultWithTab,
    WebScreenshotFormat,
    WebTableResult,
)

# Agent browser tools script path (NEW - AI-native browser automation)
AGENT_BROWSER_TOOLS_PATH = "/opt/tools/agent_browser_tools.py"

# Web tools script path (LEGACY - crawl4ai)
WEB_TOOLS_PATH = "/opt/tools/web_tools.py"

# Browser tools script path (LEGACY - Playwright Python)
BROWSER_TOOLS_PATH = "/opt/tools/browser_tools.py"


class AsyncWebAPI:
    """
    API for web scraping and browser automation (asynchronous).

    Provides methods for:
    - Web scraping (HTML extraction, table extraction, screenshots, etc.)
    - Web search (Google, DuckDuckGo, Bing, Baidu)
    - Browser automation (navigation, clicking, filling forms, etc.)

    Use with AsyncDSBClient for asynchronous operations.

    Example:
        >>> async with AsyncDSBClient() as client:
        ...     result = await client.web.scrape_async(sandbox_id, "https://example.com")
        ...     print(result.content)
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async web API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    def _validate_url(self, url: str) -> None:
        """
        Validate URL format before sending to tool.

        Args:
            url: URL to validate

        Raises:
            ValueError: If URL format is invalid
        """
        if not url or not isinstance(url, str):
            raise ValueError("URL must be a non-empty string")

        url = url.strip()
        # Accept http://, https://, and data: URLs (for browser testing)
        if not url.startswith(('http://', 'https://', 'data:')):
            raise ValueError(f"Invalid URL: must start with http://, https://, or data:, got: {url}")

        if ' ' in url and not url.startswith('data:'):
            raise ValueError(f"Invalid URL: contains spaces, got: {url}")

    # =========================================================================
    # Private Helper Methods (Asynchronous)
    # =========================================================================

    async def _exec_web_tool_async(
        self,
        sandbox_id: str,
        command: str,
        args: dict[str, Any],
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """Execute a web tool command in a sandbox (async).

        Uses agent_browser_tools.py for web scraping with BM25 filtering.
        """
        exec_timeout = timeout or DEFAULT_WEB_TOOLS_TIMEOUT

        # Use agent_browser_tools.py for web scraping
        result = await self.transport.request(
            method="POST",
            path=f"/sandboxes/{sandbox_id}/tools",
            json_data={
                "interpreter": "python",
                "script_path": AGENT_BROWSER_TOOLS_PATH,  # Use new agent_browser_tools.py
                "action": command,
                "args": args,
                "timeout": int(exec_timeout),
            },
            timeout=int(exec_timeout + DEFAULT_HTTP_BUFFER_SECS),
        )

        # Result is already parsed JSON from HTTP response
        if not isinstance(result, dict):
            raise ValueError(f"Invalid response type from transport: {type(result).__name__}")

        # Check for error response
        if "error_message" in result:
            raise ValueError(f"Tool execution failed: {result['error_message']}")

        return result

    async def _exec_browser_tool_async(
        self,
        sandbox_id: str,
        command: str,
        args: dict[str, Any] | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """Execute a browser automation command in a sandbox (async).

        Uses agent_browser_tools.py with ref-based element selection.
        """
        exec_timeout = timeout or DEFAULT_BROWSER_TOOLS_TIMEOUT

        result = await self.transport.request(
            method="POST",
            path=f"/sandboxes/{sandbox_id}/tools",
            json_data={
                "interpreter": "python",
                "script_path": AGENT_BROWSER_TOOLS_PATH,  # Use new agent_browser_tools.py
                "action": command,
                "args": args or {},
                "timeout": int(exec_timeout),
            },
            timeout=int(exec_timeout + DEFAULT_HTTP_BUFFER_SECS),
        )

        # Result is already parsed JSON from HTTP response
        if not isinstance(result, dict):
            raise ValueError(f"Invalid response type from transport: {type(result).__name__}")

        # Check for error response
        if "error_message" in result:
            raise ValueError(f"Browser tool error: {result['error_message']}")

        # agent_browser_tools.py returns data directly (not wrapped in "result")
        # Include status for BrowserActionResponse compatibility
        result_data = result.copy()
        result_data["status"] = result.get("status", "success")
        return result_data

    async def _get_sandbox_async(self, sandbox_id: str) -> Sandbox:
        """Get sandbox details to check image name (async)."""
        response = await self.transport.request(
            method="GET",
            path=f"/sandboxes/{sandbox_id}",
        )
        return Sandbox(**response)

    async def _supports_browser_automation_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> bool:
        """Check if a sandbox supports browser automation (async)."""
        try:
            sandbox = await self._get_sandbox_async(sandbox_id)
            image_name = sandbox.config.image.lower() if sandbox.config.image else ""

            if "slim" in image_name:
                return False
            if "sandbox" in image_name:
                return True
            return True
        except Exception:
            try:
                await self._exec_web_tool_async(sandbox_id, "web_health_check", {}, timeout=timeout)
                return True
            except ValueError:
                return False

    # =========================================================================
    # Helper Methods (Asynchronous)
    # =========================================================================

    async def supports_browser_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> bool:
        """
        Check if the sandbox supports browser automation (async).

        Browser automation requires a full sandbox image (not slim).

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            True if browser automation is supported, False otherwise
        """
        return await self._supports_browser_automation_async(sandbox_id, timeout=timeout)

    async def get_browser_info_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserInfo:
        """
        Get browser capability information for a sandbox (async).

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserInfo with capability details
        """
        try:
            sandbox = await self._get_sandbox_async(sandbox_id)
            image_name = sandbox.config.image
        except Exception:
            image_name = None

        supports = await self._supports_browser_automation_async(sandbox_id, timeout=timeout)

        cdp_port = None
        browser_type = None
        if supports:
            try:
                await self._exec_web_tool_async(sandbox_id, "web_health_check", {}, timeout=timeout)
                cdp_port = 9222
                browser_type = "chromium"
            except ValueError:
                pass

        return BrowserInfo(
            supports_automation=supports,
            browser_type=browser_type,
            cdp_port=cdp_port,
            image_name=image_name,
        )

    # =========================================================================
    # Web Scraping Methods (Asynchronous)
    # =========================================================================

    async def scrape_async(
        self,
        sandbox_id: str,
        url: str,
        format: WebFormat | str = WebFormat.MARKDOWN,
        screenshot: bool = False,
        css_selector: str | None = None,
        word_count_threshold: int = 10,
        # NEW: Advanced filtering parameters
        search_query: str | None = None,
        use_pruning: bool = False,
        pruning_threshold: float = 0.48,
        bm25_threshold: float = 1.0,
        # NEW: Configuration parameters
        wait_until: str = "domcontentloaded",
        cache_mode: str = "bypass",
        page_timeout: int | None = None,
        max_length: int | None = None,
        proxy_config: dict[str, Any] | None = None,
        keep_open: bool = True,  # DEFAULT IS NOW TRUE
        timeout: int | None = None,
    ) -> WebScrapeResult | WebScrapeResultWithTab:
        """
        Scrape a web page with multiple output formats (async).

        By default, keeps the browser tab open for VNC viewing.
        Set keep_open=False to close the tab immediately after scraping.
        """
        # Validate URL before sending to tool
        self._validate_url(url)

        args = {
            "url": url,
            "format": format.value if isinstance(format, WebFormat) else format,
            "screenshot": screenshot,
            "css_selector": css_selector,
            "word_count_threshold": word_count_threshold,
            "search_query": search_query,
            "use_pruning": use_pruning,
            "pruning_threshold": pruning_threshold,
            "bm25_threshold": bm25_threshold,
            "wait_until": wait_until,
            "cache_mode": cache_mode,
            "page_timeout": page_timeout,
            "max_length": max_length,
            "proxy_config": proxy_config,
            "keep_open": keep_open,
        }

        result = await self._exec_web_tool_async(sandbox_id, "web_scrape", args, timeout=timeout)

        if result.get("keep_open"):
            return WebScrapeResultWithTab(**result)
        return WebScrapeResult(**result)

    async def extract_css_async(
        self,
        sandbox_id: str,
        url: str,
        schema: dict[str, str],
        base_selector: str | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """Extract structured data using CSS selectors (async).

        .. deprecated::
            Use :meth:`scrape_async` with the ``css_selector`` parameter instead.
        """
        warnings.warn(
            "extract_css_async() is deprecated. Use scrape_async() with the css_selector parameter instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {"url": url, "schema": schema}
        if base_selector:
            args["base_selector"] = base_selector

        return await self._exec_web_tool_async(sandbox_id, "web_extract_css", args, timeout=timeout)

    async def extract_table_async(
        self,
        sandbox_id: str,
        url: str,
        table_index: int = 0,
        timeout: int | None = None,
    ) -> WebTableResult:
        """Extract a table from a web page as structured data (async).

        .. deprecated::
            Use :meth:`scrape_async` with ``css_selector='table'`` instead.
        """
        warnings.warn(
            "extract_table_async() is deprecated. Use scrape_async() with css_selector='table' instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {"url": url, "table_index": table_index}
        result = await self._exec_web_tool_async(sandbox_id, "web_extract_table", args, timeout=timeout)
        return WebTableResult(**result)

    async def screenshot_async(
        self,
        sandbox_id: str,
        url: str,
        full_page: bool = True,
        format: WebScreenshotFormat | str = WebScreenshotFormat.PNG,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """Capture a screenshot of a web page (async).

        .. deprecated::
            Use :meth:`scrape_async` with ``screenshot=True`` instead.
        """
        warnings.warn(
            "screenshot_async() is deprecated. Use scrape_async() with screenshot=True instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {
            "url": url,
            "full_page": full_page,
            "format": format.value if isinstance(format, WebScreenshotFormat) else format,
        }
        return await self._exec_web_tool_async(sandbox_id, "web_screenshot", args, timeout=timeout)

    async def links_async(
        self,
        sandbox_id: str,
        url: str,
        filter_external: bool = False,
        timeout: int | None = None,
    ) -> WebLinksResponse:
        """Extract all links from a web page (async).

        .. deprecated::
            Use :meth:`scrape_async` with ``format='links'`` instead.
        """
        warnings.warn(
            "links_async() is deprecated. Use scrape_async() with format='links' instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {"url": url, "filter_external": filter_external}
        result = await self._exec_web_tool_async(sandbox_id, "web_links", args, timeout=timeout)
        return WebLinksResponse(**result)

    async def crawl_async(
        self,
        sandbox_id: str,
        urls: list[str],
        format: WebFormat | str = WebFormat.MARKDOWN,
        # NEW: Advanced filtering parameters
        search_query: str | None = None,
        use_pruning: bool = False,
        pruning_threshold: float = 0.48,
        bm25_threshold: float = 1.0,
        # NEW: Configuration parameters
        wait_until: str = "domcontentloaded",
        cache_mode: str = "bypass",
        page_timeout: int | None = None,
        max_length: int | None = None,
        proxy_config: dict[str, Any] | None = None,
        timeout: int | None = None,
    ) -> WebCrawlResponse:
        """Crawl multiple URLs in parallel (async).

        .. deprecated::
            Call :meth:`scrape_async` for each URL individually instead.
        """
        warnings.warn(
            "crawl_async() is deprecated. Call scrape_async() for each URL individually instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {
            "urls": urls,
            "format": format.value if isinstance(format, WebFormat) else format,
            "search_query": search_query,
            "use_pruning": use_pruning,
            "pruning_threshold": pruning_threshold,
            "bm25_threshold": bm25_threshold,
            "wait_until": wait_until,
            "cache_mode": cache_mode,
            "page_timeout": page_timeout,
            "max_length": max_length,
            "proxy_config": proxy_config,
        }
        result = await self._exec_web_tool_async(sandbox_id, "web_crawl", args, timeout=timeout)
        return WebCrawlResponse(**result)

    async def health_check_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> WebHealthResponse:
        """Check the health of web tools and browser connection (async).

        .. deprecated::
            Use ``execute_bash_async()`` with ``curl -sI <url>`` instead.
        """
        warnings.warn(
            "health_check_async() is deprecated. Use execute_bash_async() with curl to check URL reachability.",
            DeprecationWarning,
            stacklevel=2,
        )
        result = await self._exec_web_tool_async(sandbox_id, "web_health_check", {}, timeout=timeout)
        return WebHealthResponse(**result)

    # =========================================================================
    # Browser Automation Methods (Asynchronous)
    # =========================================================================

    async def browser_navigate_async(
        self,
        sandbox_id: str,
        url: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Navigate to a URL (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"url": url}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_navigate", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_go_back_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Navigate back in browser history (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        result = await self._exec_browser_tool_async(sandbox_id, "browser_go_back", timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_go_forward_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Navigate forward in browser history (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        result = await self._exec_browser_tool_async(sandbox_id, "browser_go_forward", timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_get_clickable_elements_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Get list of clickable elements on the current page (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        result = await self._exec_browser_tool_async(sandbox_id, "browser_get_clickable_elements", timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_click_async(
        self,
        sandbox_id: str,
        index: int | None = None,
        selector: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Click an element by index or CSS selector (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        if index is None and selector is None:
            raise ValueError("Must provide either index or selector")

        args: dict[str, Any] = {}
        if index is not None:
            args["index"] = index
        if selector is not None:
            args["selector"] = selector

        result = await self._exec_browser_tool_async(sandbox_id, "browser_click", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_fill_async(
        self,
        sandbox_id: str,
        selector: str,
        value: str,
        clear: bool = True,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Fill a form field (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        args = {"selector": selector, "value": value, "clear": clear}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_form_input_fill", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_scroll_async(
        self,
        sandbox_id: str,
        amount: int | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Scroll the page (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        args = {"amount": amount} if amount is not None else {}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_scroll", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_screenshot_async(
        self,
        sandbox_id: str,
        name: str | None = None,
        full_page: bool = False,
        selector: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Take a screenshot (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        args: dict[str, Any] = {"fullPage": full_page}
        if name:
            args["name"] = name
        if selector:
            args["selector"] = selector

        result = await self._exec_browser_tool_async(sandbox_id, "browser_screenshot", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_new_tab_async(
        self,
        sandbox_id: str,
        url: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Open a new tab (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        args: dict[str, Any] = {}
        if url:
            args["url"] = url
        result = await self._exec_browser_tool_async(sandbox_id, "browser_new_tab", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_tab_list_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """List all open tabs (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        result = await self._exec_browser_tool_async(sandbox_id, "browser_tab_list", timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_switch_tab_async(
        self,
        sandbox_id: str,
        index: int,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Switch to a specific tab (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        args = {"index": index}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_switch_tab", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_evaluate_async(
        self,
        sandbox_id: str,
        script: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Evaluate JavaScript in the browser context (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        args = {"script": script}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_evaluate", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_close_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Close current browser tab and switch to adjacent tab (async).

        If only one tab is open, navigates to about:blank instead.
        With multi-tab mode, each web fetch opens in its own tab.
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        result = await self._exec_browser_tool_async(sandbox_id, "browser_close", timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_health_check_async(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """Check browser health and accessibility (async)."""
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support."
            )

        result = await self._exec_browser_tool_async(sandbox_id, "browser_health_check", timeout=timeout)
        return BrowserActionResponse(**result)

    # =========================================================================
    # NEW: Ref-based Browser Automation Methods (agent-browser)
    # =========================================================================

    async def browser_snapshot_async(
        self,
        sandbox_id: str,
        interactive: bool = True,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Get accessibility snapshot with refs (@e1, @e2) for deterministic element selection (async).

        This is the key improvement from agent-browser - returns refs that can be
        used for click, fill, and other operations instead of relying on
        potentially ambiguous CSS selectors.

        Args:
            sandbox_id: Sandbox UUID
            interactive: Only return interactive elements (default: True)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            Dict with accessibility snapshot containing refs like:
            - @e1, @e2, @e3 for interactive elements
            - Element descriptions (role, name, value)

        Example:
            >>> snapshot = await client.web.browser_snapshot_async(sandbox_id)
            >>> # Look for elements with refs in the snapshot
            >>> # Then use the refs for click/fill operations

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"interactive": interactive}
        return await self._exec_browser_tool_async(sandbox_id, "browser_snapshot", args, timeout=timeout)

    async def browser_click_ref_async(
        self,
        sandbox_id: str,
        ref: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Click an element by ref from accessibility snapshot (async).

        Ref-based selection is more reliable than CSS selectors because
        it uses the same element identification as the accessibility tree.

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot (e.g., "@e1", "@e2")
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Example:
            >>> snapshot = await client.web.browser_snapshot_async(sandbox_id)
            >>> # Find the ref for the button you want to click
            >>> await client.web.browser_click_ref_async(sandbox_id, "@e1")

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"ref": ref}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_click", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_fill_ref_async(
        self,
        sandbox_id: str,
        ref: str,
        value: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Fill a form field by ref from accessibility snapshot (async).

        Ref-based selection is more reliable than CSS selectors because
        it uses the same element identification as the accessibility tree.

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot (e.g., "@e1", "@e2")
            value: Value to fill
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Example:
            >>> snapshot = await client.web.browser_snapshot_async(sandbox_id)
            >>> # Find the ref for the input field
            >>> await client.web.browser_fill_ref_async(sandbox_id, "@e3", "test@example.com")

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"ref": ref, "value": value}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_fill", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_wait_async(
        self,
        sandbox_id: str,
        selector: str | None = None,
        text: str | None = None,
        time_ms: int | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Wait for element, text, or time (async).

        Args:
            sandbox_id: Sandbox UUID
            selector: CSS selector to wait for (optional)
            text: Text to wait for on page (optional)
            time_ms: Time in milliseconds to wait (optional)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Note: Must provide one of selector, text, or time_ms.

        Raises:
            DSBValidationError: If browser automation is not supported
            ValueError: If none of selector, text, or time_ms is provided
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        if selector is None and text is None and time_ms is None:
            raise ValueError("Must provide one of selector, text, or time_ms")

        args: dict[str, Any] = {}
        if selector:
            args["selector"] = selector
        if text:
            args["text"] = text
        if time_ms:
            args["time"] = time_ms

        result = await self._exec_browser_tool_async(sandbox_id, "browser_wait", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_press_key_async(
        self,
        sandbox_id: str,
        key: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Press a key on the keyboard (async).

        Args:
            sandbox_id: Sandbox UUID
            key: Key to press (e.g., "Enter", "Tab", "Escape", "ArrowDown")
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Example:
            >>> await client.web.browser_press_key_async(sandbox_id, "Enter")

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"key": key}
        result = await self._exec_browser_tool_async(sandbox_id, "browser_press_key", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_hover_async(
        self,
        sandbox_id: str,
        ref: str | None = None,
        selector: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Hover over an element (async).

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot (e.g., "@e1") - preferred
            selector: CSS selector - alternative if ref not available
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Note: Must provide either ref or selector.

        Raises:
            DSBValidationError: If browser automation is not supported
            ValueError: If neither ref nor selector is provided
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        if ref is None and selector is None:
            raise ValueError("Must provide either ref or selector")

        args: dict[str, Any] = {}
        if ref:
            args["ref"] = ref
        if selector:
            args["selector"] = selector

        result = await self._exec_browser_tool_async(sandbox_id, "browser_hover", args, timeout=timeout)
        return BrowserActionResponse(**result)

    async def browser_get_text_async(
        self,
        sandbox_id: str,
        ref: str | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Get text content from page or specific element (async).

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot for specific element (optional)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            Dict with "text" key containing the text content

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {}
        if ref:
            args["ref"] = ref

        return await self._exec_browser_tool_async(sandbox_id, "browser_get_text", args, timeout=timeout)

    async def browser_get_html_async(
        self,
        sandbox_id: str,
        ref: str | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Get HTML content from page or specific element (async).

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot for specific element (optional)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            Dict with "html" key containing the HTML content

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not await self._supports_browser_automation_async(sandbox_id, timeout=timeout):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {}
        if ref:
            args["ref"] = ref

        return await self._exec_browser_tool_async(sandbox_id, "browser_get_html", args, timeout=timeout)
