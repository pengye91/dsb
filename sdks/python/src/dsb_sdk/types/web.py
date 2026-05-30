"""
Web scraping and browser automation type definitions.

Provides Pydantic models for web scraping and browser automation responses
from the DSB sandbox web tools.
"""

from __future__ import annotations

from enum import Enum
from typing import Any

from pydantic import BaseModel, Field


class WebFormat(str, Enum):
    """Output format for web scraping."""

    MARKDOWN = "markdown"
    HTML = "html"
    TEXT = "text"
    LINKS = "links"


class WebScreenshotFormat(str, Enum):
    """Screenshot image format."""

    PNG = "png"
    JPEG = "jpeg"


class WebScrapeResult(BaseModel):
    """Result from web scraping.

    Attributes:
        url: Original URL that was scraped.
        title: Page title extracted from the page.
        content: Scraped content in the requested format.
        screenshot: Base64-encoded screenshot if screenshot was requested.
        screenshot_encoding: Encoding format (always "base64").
        screenshot_path: File path where screenshot was saved.
        keep_open: Whether the browser tab was kept open after scraping.
    """

    url: str = Field(..., description="Original URL that was scraped")
    title: str = Field(default="", description="Page title extracted from the page")
    content: str = Field(default="", description="Scraped content in the requested format")
    screenshot: str | None = Field(
        default=None, description="Base64-encoded screenshot if screenshot was requested"
    )
    screenshot_encoding: str | None = Field(
        default=None, description="Encoding format (e.g., 'base64')"
    )
    screenshot_path: str | None = Field(
        default=None, description="File path where screenshot was saved"
    )
    keep_open: bool = Field(default=False, description="Whether the browser tab was kept open")


class WebScrapeTabInfo(BaseModel):
    """Information about a kept-open browser tab.

    Attributes:
        page_id: Unique page identifier.
        url: URL of the page.
        title: Page title.
    """

    page_id: str = Field(..., description="Unique page identifier")
    url: str = Field(..., description="URL of the page")
    title: str = Field(default="", description="Page title")


class WebScrapeResultWithTab(WebScrapeResult):
    """
    Result from web_scrape with keep_open=True (default).

    Extends WebScrapeResult with tab information for VNC viewing.

    Attributes:
        tab_info: Information about the kept-open browser tab.
    """

    tab_info: WebScrapeTabInfo | None = Field(
        default=None, description="Tab information for VNC viewing"
    )


class WebLinksResponse(BaseModel):
    """Response from link extraction.

    Attributes:
        url: Original URL that was analyzed.
        total_links: Total number of links found.
        links: List of extracted URLs.
    """

    url: str = Field(..., description="Original URL that was analyzed")
    total_links: int = Field(..., description="Total number of links found")
    links: list[str] = Field(default_factory=list, description="List of extracted URLs")


class WebTableResult(BaseModel):
    """Result from table extraction.

    Attributes:
        url: Original URL that was analyzed.
        table_index: Index of the extracted table (0-based).
        total_tables: Total number of tables found on the page.
        headers: Column headers of the table.
        rows: Data rows of the table.
    """

    url: str = Field(..., description="Original URL that was analyzed")
    table_index: int = Field(..., description="Index of the extracted table (0-based)")
    total_tables: int = Field(..., description="Total number of tables found on the page")
    headers: list[str] = Field(default_factory=list, description="Column headers of the table")
    rows: list[list[str]] = Field(default_factory=list, description="Data rows of the table")


class WebCrawlResult(BaseModel):
    """Result from crawling a single URL.

    Attributes:
        url: URL that was crawled.
        success: Whether crawling succeeded.
        title: Page title if successful.
        content: Crawled content if successful.
        link_count: Number of links found on the page.
        error: Error message if crawling failed.
    """

    url: str = Field(..., description="URL that was crawled")
    success: bool = Field(..., description="Whether crawling succeeded")
    title: str = Field(default="", description="Page title if successful")
    content: str = Field(default="", description="Crawled content if successful")
    link_count: int = Field(default=0, description="Number of links found on the page")
    error: str | None = Field(default=None, description="Error message if crawling failed")


class WebCrawlResponse(BaseModel):
    """Response from multi-page crawling.

    Attributes:
        total_urls: Total number of URLs to crawl.
        successful: Number of successfully crawled URLs.
        failed: Number of failed URLs.
        results: List of individual crawl results.
    """

    total_urls: int = Field(..., description="Total number of URLs to crawl")
    successful: int = Field(..., description="Number of successfully crawled URLs")
    failed: int = Field(..., description="Number of failed URLs")
    results: list[WebCrawlResult] = Field(
        default_factory=list, description="List of individual crawl results"
    )


class WebHealthResponse(BaseModel):
    """Response from web tools health check.

    Attributes:
        message: Health status message.
        cdp_url: Chrome DevTools Protocol connection URL.
        browser_ready: Whether the browser is ready for use.
    """

    message: str = Field(..., description="Health status message")
    cdp_url: str = Field(..., description="Chrome DevTools Protocol connection URL")
    browser_ready: bool = Field(..., description="Whether the browser is ready for use")


class BrowserAction(str, Enum):
    """Browser automation actions."""

    NAVIGATE = "navigate"
    GO_BACK = "go_back"
    GO_FORWARD = "go_forward"
    GET_MARKDOWN = "get_markdown"
    GET_TEXT = "get_text"
    SCREENSHOT = "screenshot"
    CLICK = "click"
    FILL = "fill"
    SCROLL = "scroll"
    NEW_TAB = "new_tab"
    SWITCH_TAB = "switch_tab"
    TAB_LIST = "tab_list"
    EVALUATE = "evaluate"
    CLOSE = "close"
    HEALTH_CHECK = "health_check"


class BrowserClickRequest(BaseModel):
    """Request to click an element.

    Attributes:
        index: Element index from get_clickable_elements (alternative to selector).
        selector: CSS selector for the element (alternative to index).
    """

    index: int | None = Field(default=None, description="Element index from get_clickable_elements")
    selector: str | None = Field(default=None, description="CSS selector for the element")


class BrowserFillRequest(BaseModel):
    """Request to fill a form field.

    Attributes:
        selector: CSS selector for the input field.
        value: Value to fill into the field.
        clear: Whether to clear the field before filling.
    """

    selector: str = Field(..., description="CSS selector for the input field")
    value: str = Field(..., description="Value to fill into the field")
    clear: bool = Field(default=True, description="Whether to clear the field before filling")


class BrowserScrollRequest(BaseModel):
    """Request to scroll the page.

    Attributes:
        amount: Scroll amount in pixels. If None, scrolls to bottom of page.
    """

    amount: int | None = Field(
        default=None, description="Scroll amount in pixels. If None, scrolls to bottom."
    )


class BrowserEvaluateRequest(BaseModel):
    """Request to evaluate JavaScript.

    Attributes:
        script: JavaScript code to execute.
    """

    script: str = Field(..., description="JavaScript code to execute")


class BrowserScreenshotRequest(BaseModel):
    """Request to take a screenshot.

    Attributes:
        name: Screenshot name/file prefix.
        full_page: Whether to capture the full page.
        selector: CSS selector for element-specific screenshot.
    """

    name: str | None = Field(default=None, description="Screenshot name/file prefix")
    full_page: bool = Field(default=False, description="Whether to capture the full page")
    selector: str | None = Field(default=None, description="CSS selector for element screenshot")


class BrowserTabInfo(BaseModel):
    """Information about a browser tab.

    Attributes:
        index: Tab index (0-based).
        title: Page title in the tab.
        url: Current URL in the tab.
        active: Whether this is the active tab.
    """

    index: int = Field(..., description="Tab index (0-based)")
    title: str = Field(default="", description="Page title in the tab")
    url: str = Field(default="", description="Current URL in the tab")
    active: bool = Field(default=False, description="Whether this is the active tab")


class BrowserNavigateRequest(BaseModel):
    """Request to navigate to a URL.

    Attributes:
        url: URL to navigate to.
    """

    url: str = Field(..., description="URL to navigate to")


class BrowserActionResponse(BaseModel):
    """Generic browser action response.

    Attributes:
        status: Action status (e.g., "success", "error").
        url: Current page URL after navigation.
        tabs: List of open tabs.
        elements: List of clickable elements.
        result: Result from evaluate action.
        path: Screenshot file path.
        error_message: Error message if status is "error".
    """

    status: str = Field(..., description="Action status (e.g., success, error)")
    url: str | None = Field(default=None, description="Current page URL after navigation")
    tabs: list[BrowserTabInfo] | None = Field(
        default=None, description="List of open tabs if applicable"
    )
    elements: list[dict[str, Any]] | None = Field(
        default=None, description="List of clickable elements if applicable"
    )
    result: Any = Field(default=None, description="Result from evaluate action")
    path: str | None = Field(default=None, description="Screenshot file path if applicable")
    error_message: str | None = Field(default=None, description="Error message if status is error")


class BrowserInfo(BaseModel):
    """Browser capability information for a sandbox.

    Attributes:
        supports_automation: Whether browser automation is supported.
        browser_type: Type of browser (e.g., "chromium").
        cdp_port: Chrome DevTools Protocol port if available.
        image_name: Name of the sandbox image.
    """

    supports_automation: bool = Field(..., description="Whether browser automation is supported")
    browser_type: str | None = Field(default=None, description="Type of browser")
    cdp_port: int | None = Field(default=None, description="CDP port if available")
    image_name: str | None = Field(default=None, description="Name of the sandbox image")
