"""
Web scraping and browser automation API (synchronous).

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

Use with DSBClient for synchronous operations.
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
from dsb_sdk.transport.sync import SyncTransport
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


class WebAPI:
    """
    API for web scraping and browser automation (synchronous).

    Provides methods for:
    - Web scraping (HTML extraction, table extraction, screenshots, etc.)
    - Web search (Google, DuckDuckGo, Bing, Baidu)
    - Browser automation (navigation, clicking, filling forms, etc.)

    Use with DSBClient for synchronous operations.

    Example:
        >>> from dsb_sdk import DSBClient
        >>> client = DSBClient()
        >>> result = client.web.scrape(sandbox_id, "https://example.com")
        >>> print(result.content)
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize web API.

        Args:
            transport: SyncTransport instance
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
    # Private Helper Methods
    # =========================================================================

    def _exec_web_tool(
        self,
        sandbox_id: str,
        command: str,
        args: dict[str, Any],
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Execute a web tool command in a sandbox using HTTP tool execution.

        Uses agent_browser_tools.py (NEW) for web scraping with BM25 filtering.

        Args:
            sandbox_id: Sandbox UUID
            command: Web tool command name
            args: Command arguments as dictionary
            timeout: Optional timeout in seconds (default: DEFAULT_WEB_TOOLS_TIMEOUT)

        Returns:
            Parsed response from the tool (direct JSON result)

        Raises:
            ValueError: If command execution fails
        """
        # Use provided timeout or default to DEFAULT_WEB_TOOLS_TIMEOUT seconds
        exec_timeout = timeout or DEFAULT_WEB_TOOLS_TIMEOUT

        # Use agent_browser_tools.py for web scraping
        result = self.transport.request(
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

    def _exec_browser_tool(
        self,
        sandbox_id: str,
        command: str,
        args: dict[str, Any] | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Execute a browser automation command in a sandbox.

        Uses agent_browser_tools.py (NEW) with ref-based element selection.

        Args:
            sandbox_id: Sandbox UUID
            command: Browser tool command name
            args: Command arguments as dictionary
            timeout: Optional timeout in seconds (default: DEFAULT_BROWSER_TOOLS_TIMEOUT)

        Returns:
            Parsed JSON response from the tool

        Raises:
            ValueError: If command execution fails
        """
        # Use provided timeout or default to DEFAULT_BROWSER_TOOLS_TIMEOUT seconds
        exec_timeout = timeout or DEFAULT_BROWSER_TOOLS_TIMEOUT

        result = self.transport.request(
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

    def _get_sandbox(self, sandbox_id: str) -> Sandbox:
        """Get sandbox details to check image name."""
        response = self.transport.request(
            method="GET",
            path=f"/sandboxes/{sandbox_id}",
        )
        return Sandbox(**response)

    def _supports_browser_automation(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> bool:
        """
        Check if a sandbox supports browser automation.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            True if browser automation is supported, False otherwise
        """
        # First, try to get the sandbox info from the raw response
        # This allows us to check for slim images even if the Sandbox model validation fails
        try:
            response = self.transport.request(
                method="GET",
                path=f"/sandboxes/{sandbox_id}",
            )
            # Check config.image directly from raw response
            config = response.get("config", {})
            image_name = config.get("image", "").lower() if config.get("image") else ""

            # Check if image is a slim variant (doesn't support browser automation)
            if "slim" in image_name:
                return False
        except Exception:
            pass  # Will try to get full Sandbox object below

        # If we get here, either the raw check passed (not slim) or failed
        # Try to get a proper Sandbox object for more reliable checks
        try:
            sandbox = self._get_sandbox(sandbox_id)
            image_name = sandbox.config.image.lower() if sandbox.config.image else ""

            # Check if image is a slim variant (doesn't support browser automation)
            if "slim" in image_name:
                return False

            # Full sandbox images should support browser automation
            if "sandbox" in image_name:
                return True

            # Default: assume supports if not clearly slim
            return True
        except Exception:
            # If we can't determine, try to check via health check
            try:
                self._exec_web_tool(sandbox_id, "web_health_check", {}, timeout=timeout)
                return True
            except ValueError:
                return False

    # =========================================================================
    # Helper Methods
    # =========================================================================

    def supports_browser(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> bool:
        """
        Check if the sandbox supports browser automation.

        Browser automation requires a full sandbox image (not slim) with
        VNC and browser tools installed.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            True if browser automation is supported, False otherwise

        Example:
            >>> if client.web.supports_browser(sandbox_id):
            ...     client.web.browser_navigate(sandbox_id, "https://example.com")
        """
        return self._supports_browser_automation(sandbox_id, timeout=timeout)

    def get_browser_info(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserInfo:
        """
        Get browser capability information for a sandbox.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserInfo with capability details

        Example:
            >>> info = client.web.get_browser_info(sandbox_id)
            >>> print(f"Automation: {info.supports_automation}")
        """
        try:
            sandbox = self._get_sandbox(sandbox_id)
            image_name = sandbox.config.image
        except Exception:
            image_name = None

        supports = self._supports_browser_automation(sandbox_id, timeout=timeout)

        # Try to get CDP port info
        cdp_port = None
        browser_type = None
        if supports:
            try:
                self._exec_web_tool(sandbox_id, "web_health_check", {}, timeout=timeout)
                cdp_port = 9222  # CDP is on port 9222
                browser_type = "chromium"
            except ValueError:
                pass

        return BrowserInfo(
            supports_automation=supports,
            browser_type=browser_type,
            cdp_port=cdp_port,
            image_name=image_name,
        )

    # -------------------------------------------------------------------------
    # Web Scraping Methods
    # -------------------------------------------------------------------------

    def scrape(
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
        Scrape a web page with multiple output formats.

        By default, keeps the browser tab open for VNC viewing.
        Set keep_open=False to close the tab immediately after scraping.

        Args:
            sandbox_id: Sandbox UUID
            url: Target URL to scrape
            format: Output format - Union[markdown, html]|Union[text, links] (default: markdown)
            screenshot: Capture screenshot (default: False)
            css_selector: Target specific CSS selector (optional)
            word_count_threshold: Filter content below word count (default: 10)
            search_query: Query for BM25 content filtering (optional)
            use_pruning: Use PruningContentFilter (default: False)
            pruning_threshold: Threshold for pruning (default: 0.48)
            bm25_threshold: Threshold for BM25 (default: 1.0)
            wait_until: Page load condition (default: "domcontentloaded")
            cache_mode: Cache mode (default: "bypass")
            page_timeout: Page timeout in milliseconds (optional)
            max_length: Maximum content length (optional)
            proxy_config: Proxy configuration (optional)
            keep_open: If True (default), keep browser tab open for VNC viewing.
                       If False, close the tab immediately after scraping.
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            WebScrapeResult or WebScrapeResultWithTab (if keep_open=True)

        Raises:
            ValueError: Invalid parameters or scraping failed

        Example:
            >>> result = client.web.scrape(sandbox_id, "https://example.com")
            >>> print(result.content)
            >>> print(result.tab_info)  # Available if keep_open=True (default)
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

        result = self._exec_web_tool(sandbox_id, "web_scrape", args, timeout=timeout)

        if result.get("keep_open"):
            return WebScrapeResultWithTab(**result)
        return WebScrapeResult(**result)

    def extract_css(
        self,
        sandbox_id: str,
        url: str,
        schema: dict[str, str],
        base_selector: str | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Extract structured data using CSS selectors.

        .. deprecated::
            This method is deprecated. Use :meth:`scrape` with the ``css_selector``
            parameter instead, e.g. ``client.web.scrape(sandbox_id, url, css_selector="h1")``.

        Args:
            sandbox_id: Sandbox UUID
            url: Target URL to scrape
            schema: JSON schema mapping field names to CSS selectors
            base_selector: Base selector for multiple items (optional)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            Dictionary with extracted data under 'extracted_data' key

        Example:
            >>> result = client.web.extract_css(
            ...     sandbox_id,
            ...     "https://example.com/products",
            ...     schema={"name": "h1", "price": ".price"}
            ... )
            >>> print(result["extracted_data"])
        """
        warnings.warn(
            "extract_css() is deprecated. Use scrape() with the css_selector parameter instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {
            "url": url,
            "schema": schema,
        }
        if base_selector:
            args["base_selector"] = base_selector

        return self._exec_web_tool(sandbox_id, "web_extract_css", args, timeout=timeout)

    def extract_table(
        self,
        sandbox_id: str,
        url: str,
        table_index: int = 0,
        timeout: int | None = None,
    ) -> WebTableResult:
        """
        Extract a table from a web page as structured data.

        .. deprecated::
            This method is deprecated. Use :meth:`scrape` with
            ``css_selector="table"`` and parse the result instead.

        Args:
            sandbox_id: Sandbox UUID
            url: Target URL
            table_index: Which table to extract (default: 0)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            WebTableResult with headers and rows

        Example:
            >>> result = client.web.extract_table(sandbox_id, "https://example.com/data")
            >>> for row in result.rows:
            ...     print(row)
        """
        warnings.warn(
            "extract_table() is deprecated. Use scrape() with css_selector='table' instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {
            "url": url,
            "table_index": table_index,
        }

        result = self._exec_web_tool(sandbox_id, "web_extract_table", args, timeout=timeout)
        return WebTableResult(**result)

    def screenshot(
        self,
        sandbox_id: str,
        url: str,
        full_page: bool = True,
        format: WebScreenshotFormat | str = WebScreenshotFormat.PNG,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Capture a screenshot of a web page.

        .. deprecated::
            This method is deprecated. Use :meth:`scrape` with ``screenshot=True``
            or use the MCP ``automate_browser`` tool with ``action="screenshot"``.

        Args:
            sandbox_id: Sandbox UUID
            url: Target URL
            full_page: Capture full page scroll (default: True)
            format: Image format - Union[png, jpeg] (default: png)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            Dictionary with screenshot_path, title, and description

        Example:
            >>> result = client.web.screenshot(sandbox_id, "https://example.com")
            >>> print(f"Screenshot saved to: {result['screenshot_path']}")
        """
        warnings.warn(
            "screenshot() is deprecated. Use scrape() with screenshot=True instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {
            "url": url,
            "full_page": full_page,
            "format": format.value if isinstance(format, WebScreenshotFormat) else format,
        }

        return self._exec_web_tool(sandbox_id, "web_screenshot", args, timeout=timeout)

    def links(
        self,
        sandbox_id: str,
        url: str,
        filter_external: bool = False,
        timeout: int | None = None,
    ) -> WebLinksResponse:
        """
        Extract all links from a web page.

        .. deprecated::
            This method is deprecated. Use :meth:`scrape` with ``format="links"`` instead.

        Args:
            sandbox_id: Sandbox UUID
            url: Target URL
            filter_external: Only return external links (default: False)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            WebLinksResponse with extracted links

        Example:
            >>> result = client.web.links(sandbox_id, "https://example.com")
            >>> print(f"Found {result.total_links} links")
        """
        warnings.warn(
            "links() is deprecated. Use scrape() with format='links' instead.",
            DeprecationWarning,
            stacklevel=2,
        )
        args = {
            "url": url,
            "filter_external": filter_external,
        }

        result = self._exec_web_tool(sandbox_id, "web_links", args, timeout=timeout)
        return WebLinksResponse(**result)

    def crawl(
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
        """
        Crawl multiple URLs in parallel.

        .. deprecated::
            This method is deprecated. Call :meth:`scrape` for each URL individually
            instead — agents compose multiple scrape calls more reliably.

        Args:
            sandbox_id: Sandbox UUID
            urls: List of URLs to crawl
            format: Output format - Union[markdown, html]|text (default: markdown)
            search_query: Query for BM25 content filtering (optional)
            use_pruning: Use PruningContentFilter (default: False)
            pruning_threshold: Threshold for pruning (default: 0.48)
            bm25_threshold: Threshold for BM25 (default: 1.0)
            wait_until: Page load condition (default: "domcontentloaded")
            cache_mode: Cache mode (default: "bypass")
            page_timeout: Page timeout in milliseconds (optional)
            max_length: Maximum content length (optional)
            proxy_config: Proxy configuration (optional)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            WebCrawlResponse with crawl results

        Example:
            >>> response = client.web.crawl(
            ...     sandbox_id,
            ...     ["https://example.com/page1", "https://example.com/page2"]
            ... )
            >>> print(f"Success: {response.successful}/{response.total_urls}")
        """
        warnings.warn(
            "crawl() is deprecated. Call scrape() for each URL individually instead.",
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

        result = self._exec_web_tool(sandbox_id, "web_crawl", args, timeout=timeout)
        return WebCrawlResponse(**result)

    def health_check(self, sandbox_id: str, timeout: int | None = None) -> WebHealthResponse:
        """
        Check the health of web tools and browser connection.

        .. deprecated::
            This method is deprecated. Use ``execute_bash`` with
            ``curl -sI <url>`` to check URL reachability instead.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds for the health check command

        Returns:
            WebHealthResponse with browser status

        Example:
            >>> health = client.web.health_check(sandbox_id)
            >>> print(f"Browser ready: {health.browser_ready}")
        """
        warnings.warn(
            "health_check() is deprecated. Use execute_bash() with curl to check URL reachability.",
            DeprecationWarning,
            stacklevel=2,
        )
        result = self._exec_web_tool(sandbox_id, "web_health_check", {}, timeout=timeout)
        return WebHealthResponse(**result)

    # -------------------------------------------------------------------------
    # Browser Automation Methods
    # -------------------------------------------------------------------------

    def browser_navigate(
        self,
        sandbox_id: str,
        url: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Navigate to a URL.

        Args:
            sandbox_id: Sandbox UUID
            url: URL to navigate to
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with navigation result

        Raises:
            DSBValidationError: If browser automation is not supported

        Example:
            >>> result = client.web.browser_navigate(sandbox_id, "https://example.com")
            >>> print(f"Navigated to: {result.url}")
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"url": url}
        result = self._exec_browser_tool(sandbox_id, "browser_navigate", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_go_back(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Navigate back in browser history.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        result = self._exec_browser_tool(sandbox_id, "browser_go_back", timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_go_forward(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Navigate forward in browser history.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        result = self._exec_browser_tool(sandbox_id, "browser_go_forward", timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_get_clickable_elements(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Get list of clickable elements on the current page.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with elements list containing index, tag, text, href

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        result = self._exec_browser_tool(sandbox_id, "browser_get_clickable_elements", timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_click(
        self,
        sandbox_id: str,
        index: int | None = None,
        selector: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Click an element by index or CSS selector.

        Args:
            sandbox_id: Sandbox UUID
            index: Element index from get_clickable_elements (alternative to selector)
            selector: CSS selector (alternative to index)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
            ValueError: If neither index nor selector is provided
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        if index is None and selector is None:
            raise ValueError("Must provide either index or selector")

        args: dict[str, Any] = {}
        if index is not None:
            args["index"] = index
        if selector is not None:
            args["selector"] = selector

        result = self._exec_browser_tool(sandbox_id, "browser_click", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_fill(
        self,
        sandbox_id: str,
        selector: str,
        value: str,
        clear: bool = True,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Fill a form field.

        Args:
            sandbox_id: Sandbox UUID
            selector: CSS selector for the input field
            value: Value to fill
            clear: Clear field before filling (default: True)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {
            "selector": selector,
            "value": value,
            "clear": clear,
        }
        result = self._exec_browser_tool(sandbox_id, "browser_form_input_fill", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_scroll(
        self,
        sandbox_id: str,
        amount: int | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Scroll the page.

        Args:
            sandbox_id: Sandbox UUID
            amount: Pixels to scroll (None for full page scroll to bottom)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"amount": amount} if amount is not None else {}
        result = self._exec_browser_tool(sandbox_id, "browser_scroll", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_screenshot(
        self,
        sandbox_id: str,
        name: str | None = None,
        full_page: bool = False,
        selector: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Take a screenshot.

        Args:
            sandbox_id: Sandbox UUID
            name: Screenshot name (default: "screenshot")
            full_page: Capture full page (default: False)
            selector: CSS selector for element screenshot
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with path field containing screenshot path

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args: dict[str, Any] = {"fullPage": full_page}
        if name:
            args["name"] = name
        if selector:
            args["selector"] = selector

        result = self._exec_browser_tool(sandbox_id, "browser_screenshot", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_new_tab(
        self,
        sandbox_id: str,
        url: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Open a new tab.

        Args:
            sandbox_id: Sandbox UUID
            url: URL to open (optional, opens blank tab if not provided)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args: dict[str, Any] = {}
        if url:
            args["url"] = url
        result = self._exec_browser_tool(sandbox_id, "browser_new_tab", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_tab_list(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        List all open tabs.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with tabs list

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        result = self._exec_browser_tool(sandbox_id, "browser_tab_list", timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_switch_tab(
        self,
        sandbox_id: str,
        index: int,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Switch to a specific tab.

        Args:
            sandbox_id: Sandbox UUID
            index: Tab index to switch to
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"index": index}
        result = self._exec_browser_tool(sandbox_id, "browser_switch_tab", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_evaluate(
        self,
        sandbox_id: str,
        script: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Evaluate JavaScript in the browser context.

        Args:
            sandbox_id: Sandbox UUID
            script: JavaScript code to execute (e.g., "() => document.title")
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result field containing script output

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"script": script}
        result = self._exec_browser_tool(sandbox_id, "browser_evaluate", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_close(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Close current browser tab and switch to adjacent tab.

        If only one tab is open, navigates to about:blank instead
        (cannot close the last tab without killing the browser).

        With multi-tab mode, each web fetch opens in its own tab.
        This closes the active tab and switches to the next one.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        result = self._exec_browser_tool(sandbox_id, "browser_close", timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_health_check(
        self,
        sandbox_id: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Check browser health and accessibility.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with status

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        result = self._exec_browser_tool(sandbox_id, "browser_health_check", timeout=timeout)
        return BrowserActionResponse(**result)

    # =========================================================================
    # NEW: Ref-based Browser Automation Methods (agent-browser)
    # =========================================================================

    def browser_snapshot(
        self,
        sandbox_id: str,
        interactive: bool = True,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Get accessibility snapshot with refs (@e1, @e2) for deterministic element selection.

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
            >>> snapshot = client.web.browser_snapshot(sandbox_id)
            >>> # Look for elements with refs in the snapshot
            >>> # Then use the refs for click/fill operations

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"interactive": interactive}
        return self._exec_browser_tool(sandbox_id, "browser_snapshot", args, timeout=timeout)

    def browser_click_ref(
        self,
        sandbox_id: str,
        ref: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Click an element by ref from accessibility snapshot.

        Ref-based selection is more reliable than CSS selectors because
        it uses the same element identification as the accessibility tree.

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot (e.g., "@e1", "@e2")
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Example:
            >>> snapshot = client.web.browser_snapshot(sandbox_id)
            >>> # Find the ref for the button you want to click
            >>> client.web.browser_click_ref(sandbox_id, "@e1")

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"ref": ref}
        result = self._exec_browser_tool(sandbox_id, "browser_click", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_fill_ref(
        self,
        sandbox_id: str,
        ref: str,
        value: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Fill a form field by ref from accessibility snapshot.

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
            >>> snapshot = client.web.browser_snapshot(sandbox_id)
            >>> # Find the ref for the input field
            >>> client.web.browser_fill_ref(sandbox_id, "@e3", "test@example.com")

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"ref": ref, "value": value}
        result = self._exec_browser_tool(sandbox_id, "browser_fill", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_wait(
        self,
        sandbox_id: str,
        selector: str | None = None,
        text: str | None = None,
        time_ms: int | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Wait for element, text, or time.

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
        if not self._supports_browser_automation(sandbox_id):
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

        result = self._exec_browser_tool(sandbox_id, "browser_wait", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_press_key(
        self,
        sandbox_id: str,
        key: str,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Press a key on the keyboard.

        Args:
            sandbox_id: Sandbox UUID
            key: Key to press (e.g., "Enter", "Tab", "Escape", "ArrowDown")
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            BrowserActionResponse with result

        Example:
            >>> client.web.browser_press_key(sandbox_id, "Enter")

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {"key": key}
        result = self._exec_browser_tool(sandbox_id, "browser_press_key", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_hover(
        self,
        sandbox_id: str,
        ref: str | None = None,
        selector: str | None = None,
        timeout: int | None = None,
    ) -> BrowserActionResponse:
        """
        Hover over an element.

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
        if not self._supports_browser_automation(sandbox_id):
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

        result = self._exec_browser_tool(sandbox_id, "browser_hover", args, timeout=timeout)
        return BrowserActionResponse(**result)

    def browser_get_text(
        self,
        sandbox_id: str,
        ref: str | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Get text content from page or specific element.

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot for specific element (optional)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            Dict with "text" key containing the text content

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {}
        if ref:
            args["ref"] = ref

        return self._exec_browser_tool(sandbox_id, "browser_get_text", args, timeout=timeout)

    def browser_get_html(
        self,
        sandbox_id: str,
        ref: str | None = None,
        timeout: int | None = None,
    ) -> dict[str, Any]:
        """
        Get HTML content from page or specific element.

        Args:
            sandbox_id: Sandbox UUID
            ref: Element ref from snapshot for specific element (optional)
            timeout: Optional timeout in seconds. Uses server default if not specified.

        Returns:
            Dict with "html" key containing the HTML content

        Raises:
            DSBValidationError: If browser automation is not supported
        """
        if not self._supports_browser_automation(sandbox_id):
            raise DSBValidationError(
                "Browser automation requires a full sandbox image with VNC support. "
                "Use 'dsb/sandbox' image, not 'dsb/sandbox-slim'."
            )

        args = {}
        if ref:
            args["ref"] = ref

        return self._exec_browser_tool(sandbox_id, "browser_get_html", args, timeout=timeout)
