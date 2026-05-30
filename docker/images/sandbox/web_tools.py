#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
"""
web_tools.py - Advanced web scraping tools using crawl4ai

This script provides advanced web scraping capabilities by connecting to the
system Chromium instance via Chrome DevTools Protocol (CDP) on port 9222.

Usage: python web_tools.py <command> <json_args>

Available commands:
  - web_scrape: Basic web scraping with multiple output formats
  - web_extract_css: Structured data extraction using CSS selectors
  - web_extract_table: Extract tables from web pages as JSON
  - web_screenshot: Capture screenshots with optional content extraction
  - web_links: Extract all links from a page
  - web_crawl: Multi-page crawling
  - web_health_check: Verify CDP connection to Chromium

Features:
  - Connects to existing Chromium via CDP (no new browser launches)
  - LLM-friendly output formats (markdown, cleaned HTML, structured data)
  - Advanced content selection and filtering
  - CSS-based structured extraction
  - Table extraction as structured JSON
  - Screenshot capture
  - Multi-page parallel crawling
"""

import asyncio
import json

# Disable crawl4ai rich output
import os
import sys
from typing import Any, Dict, List, Optional
from urllib.parse import urlparse

from crawl4ai import AsyncWebCrawler, BrowserConfig, CrawlerRunConfig
from crawl4ai.extraction_strategy import JsonCssExtractionStrategy

# Import structured error handling
try:
    from error_handler import SandboxError
except ImportError:
    # Fallback for backward compatibility
    class SandboxError(Exception):
        def __init__(self, message: str, status_code: int = 400):
            self.message = message
            self.status_code = status_code
            super().__init__(message)

# Try to import advanced content filtering features
try:
    from crawl4ai.content_filter_strategy import BM25ContentFilter, PruningContentFilter
    from crawl4ai.markdown_generation_strategy import DefaultMarkdownGenerator
    ADVANCED_FILTERS_AVAILABLE = True
except ImportError:
    ADVANCED_FILTERS_AVAILABLE = False

os.environ["RICH_DISABLE"] = "1"
os.environ["CRAWL4AI_LOGGING_LEVEL"] = "0"

# Redirect crawl4ai logging
import logging

logging.getLogger("crawl4ai").setLevel(logging.CRITICAL)

# CDP connection URL - connects to system Chromium managed by supervisord
CDP_URL = "http://localhost:9222"

# Command timeout in seconds
COMMAND_TIMEOUT = 60

# Crawl4AI base directory - must be writable by dsb user
CRAWL4AI_BASE_DIR = "/home/dsb/.crawl4ai"

# Global registry for persistent crawler instances (for keep_open functionality)
_persistent_crawlers: Dict[str, Any] = {}
_crawlers_lock = asyncio.Lock()
_browser_ops_lock = asyncio.Lock()

def _clean_css_from_markdown(markdown: str) -> str:
    """
    Remove CSS content from markdown text.

    This handles:
    1. CSS in code blocks (```css ...```)
    2. Inline styles in HTML (style="...")
    3. CSS rule patterns ({ ... })
    4. HTML style tags (<style>...</style>)
    5. CSS class attributes and related styling

    Args:
        markdown: The markdown text to clean

    Returns:
        Cleaned markdown text
    """
    if not markdown:
        return markdown

    import re

    # Remove CSS code blocks - both ```css and ``` with css content
    markdown = re.sub(
        r'```css\s*\n.*?```',
        '',
        markdown,
        flags=re.DOTALL | re.MULTILINE | re.IGNORECASE
    )

    # Remove <style>...</style> blocks (single and multi-line)
    markdown = re.sub(
        r'<style[^>]*>.*?</style>',
        '',
        markdown,
        flags=re.DOTALL | re.MULTILINE | re.IGNORECASE
    )

    # Remove inline style attributes: style="..." or style='...'
    markdown = re.sub(
        r'\sstyle\s*=\s*["\'][^"\']*["\']',
        '',
        markdown,
        flags=re.IGNORECASE
    )

    # Remove class attributes that contain css- or look like CSS classes
    markdown = re.sub(
        r'\sclass\s*=\s*["\'][^"\']*["\']',
        '',
        markdown,
        flags=re.IGNORECASE
    )

    # Remove common CSS-related HTML attributes
    css_attrs_pattern = r'\s(data-css|data-theme|data-style|data-font)[^"\' >]*["\'][^"\']*["\']'
    markdown = re.sub(css_attrs_pattern, '', markdown, flags=re.IGNORECASE)

    # Remove CSS rule blocks that might appear as text
    # Pattern: selector { property: value; ... }
    def remove_css_rules(text):
        """Remove CSS rule blocks while preserving other curly braces."""
        lines = []
        for line in text.split('\n'):
            stripped = line.strip()
            # Check if this looks like a CSS rule
            # (contains properties like color:, background:, etc.)
            css_indicators = [
                'color:', 'background:', 'margin:', 'padding:', 'font:',
                'border:', 'display:', 'position:', 'width:', 'height:',
                'top:', 'left:', 'right:', 'bottom:', 'float:',
                'text-align:', 'text-decoration:', 'font-weight:', 'font-size:',
                'transform:', 'transition:', 'animation:', 'opacity:',
                'font-family:', 'line-height:', 'z-index:', 'overflow:',
                'white-space:', 'word-wrap:', 'box-sizing:', 'flex:',
                'grid:', 'align-items:', 'justify-content:'
            ]
            # Only remove if line has multiple CSS-like patterns
            css_count = sum(1 for ind in css_indicators if ind in stripped.lower())
            if css_count >= 2:
                continue
            lines.append(line)
        return '\n'.join(lines)

    markdown = remove_css_rules(markdown)

    # Clean up multiple consecutive blank lines
    markdown = re.sub(r'\n\s*\n\s*\n', '\n\n', markdown)

    return markdown.strip()


async def get_persistent_crawler(cdp_url: str = CDP_URL) -> Dict[str, Any]:
    """
    Get or create a persistent AsyncWebCrawler instance.

    The persistent crawler keeps browser tabs open after scraping for VNC viewing.

    Args:
        cdp_url: CDP connection URL

    Returns:
        Dictionary with crawler, pages dict, and cdp_url
    """
    global _persistent_crawlers

    async with _crawlers_lock:
        if "default" not in _persistent_crawlers:
            browser_config = BrowserConfig(
                cdp_url=cdp_url,
                headless=False,
                verbose=False,
            )

            crawler = AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR)
            async with _browser_ops_lock:
                await crawler.start()

            _persistent_crawlers["default"] = {
                "crawler": crawler,
                "pages": {},  # Map page_id -> (page, url, created_at)
                "cdp_url": cdp_url,
            }

        return _persistent_crawlers["default"]


def _build_config(
    search_query: Optional[str] = None,
    use_pruning: bool = False,
    pruning_threshold: float = 0.48,
    bm25_threshold: float = 1.0,
    word_count_threshold: int = 10,
    excluded_tags: Optional[List[str]] = None,
    wait_until: str = "domcontentloaded",
    cache_mode: str = "bypass",
    page_timeout: Optional[int] = None,
) -> Dict[str, Any]:
    """Build CrawlerRunConfig parameters with advanced content filtering support."""
    config_params = {
        "word_count_threshold": word_count_threshold,
        # CRITICAL FIX: Add script/style to default exclusions for cleaner content
        "excluded_tags": excluded_tags or [
            "nav", "footer", "header", "aside", "script", "style", "noscript",
            "form", "iframe", "link", "meta"
        ],
        "wait_until": wait_until,
        "cache_mode": cache_mode,
    }

    if page_timeout:
        config_params["page_timeout"] = page_timeout

    # Handle content filtering
    if (use_pruning or search_query) and ADVANCED_FILTERS_AVAILABLE:
        try:
            if search_query:
                content_filter = BM25ContentFilter(
                    user_query=search_query,
                    bm25_threshold=bm25_threshold
                )
            else:
                content_filter = PruningContentFilter(
                    threshold=pruning_threshold,
                    threshold_type="fixed",
                    min_word_threshold=2
                )

            config_params["markdown_generator"] = DefaultMarkdownGenerator(
                content_filter=content_filter
            )
        except Exception:
            pass  # Gracefully degrade if filters fail

    return config_params


async def health_check(args: Dict[str, Any]) -> Dict[str, Any]:
    """Verify CDP connection to Chromium"""
    try:
        browser_config = BrowserConfig(cdp_url=CDP_URL, headless=False, verbose=False)

        async with _browser_ops_lock:
            async with AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR) as crawler:
                # Try to fetch a simple page
                result = await crawler.arun(
                    url="https://example.com",
                    config=CrawlerRunConfig(
                        word_count_threshold=0,
                    ),
                )

            if result.success:
                return {
                    "message": "Successfully connected to Chromium via CDP",
                    "cdp_url": CDP_URL,
                    "browser_ready": True,
                }
            else:
                raise SandboxError(
                    message=f"Failed to load page: {result.error_message}"
                )
    except SandboxError:
        raise
    except Exception as e:
        raise SandboxError(
            message=f"CDP connection failed: {str(e)}"
        )


async def web_scrape(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Basic web scraping with multiple output formats.
    Each call creates a NEW browser tab (no reuse).
    NOTE: Tab creation behavior changed to prevent reuse - each call now gets a fresh browser instance.

    Args:
        url: Target URL
        format: Output format - markdown|html|text|links (default: markdown)
        screenshot: Capture screenshot (default: false)
        css_selector: Target specific CSS selector (optional)
        word_count_threshold: Filter content below word count (default: 10)
        search_query: Query for BM25 content filtering (optional)
        use_pruning: Use PruningContentFilter (default: false)
        pruning_threshold: Threshold for pruning (default: 0.48)
        bm25_threshold: Threshold for BM25 (default: 1.0)
        wait_until: Page load condition (default: "domcontentloaded")
        cache_mode: Cache mode (default: "bypass")
        page_timeout: Page timeout in milliseconds (optional)
        max_length: Maximum content length (optional)
        proxy_config: Proxy configuration (optional)
        keep_open: If False, close browser tab after scraping (default: True)

    Returns:
        Dictionary with scraped data and optionally tab_info if keep_open=True
    """
    url = args.get("url")
    if not url:
        raise SandboxError(
            message="Missing required parameter: url"
        )

    output_format = args.get("format", "markdown")
    screenshot = args.get("screenshot", False)
    css_selector = args.get("css_selector")
    word_count_threshold = args.get("word_count_threshold", 10)

    # New parameters
    search_query = args.get("search_query")
    use_pruning = args.get("use_pruning", False)
    pruning_threshold = args.get("pruning_threshold", 0.48)
    bm25_threshold = args.get("bm25_threshold", 1.0)
    wait_until = args.get("wait_until", "domcontentloaded")
    cache_mode = args.get("cache_mode", "bypass")
    page_timeout = args.get("page_timeout")
    max_length = args.get("max_length")
    proxy_config = args.get("proxy_config")

    # Validate output format
    valid_formats = ["markdown", "html", "text", "links"]
    if output_format not in valid_formats:
        raise SandboxError(
            message=f"Invalid format: {output_format}. Must be one of {valid_formats}"
        )

    return await _web_scrape_standard(
        url=url,
        output_format=output_format,
        screenshot=screenshot,
        css_selector=css_selector,
        word_count_threshold=word_count_threshold,
        search_query=search_query,
        use_pruning=use_pruning,
        pruning_threshold=pruning_threshold,
        bm25_threshold=bm25_threshold,
        wait_until=wait_until,
        cache_mode=cache_mode,
        page_timeout=page_timeout,
        max_length=max_length,
        proxy_config=proxy_config,
    )


async def _web_scrape_standard(
    url: str,
    output_format: str,
    screenshot: bool = False,
    css_selector: Optional[str] = None,
    word_count_threshold: int = 10,
    search_query: Optional[str] = None,
    use_pruning: bool = False,
    pruning_threshold: float = 0.48,
    bm25_threshold: float = 1.0,
    wait_until: str = "domcontentloaded",
    cache_mode: str = "bypass",
    page_timeout: Optional[int] = None,
    max_length: Optional[int] = None,
    proxy_config: Optional[Dict[str, Any]] = None,
) -> Dict[str, Any]:
    """
    Standard web scraping that closes the page after completion.

    Only used when keep_open=False is explicitly specified.
    """
    try:
        # Build browser config with proxy support
        browser_params = {
            "cdp_url": CDP_URL,
            "headless": False,
            "verbose": False
        }
        if proxy_config:
            browser_params.update(proxy_config)
        browser_config = BrowserConfig(**browser_params)

        # Build crawl config using helper
        config_params = _build_config(
            search_query=search_query,
            use_pruning=use_pruning,
            pruning_threshold=pruning_threshold,
            bm25_threshold=bm25_threshold,
            word_count_threshold=word_count_threshold,
            wait_until=wait_until,
            cache_mode=cache_mode,
            page_timeout=page_timeout,
        )
        if screenshot:
            config_params["screenshot"] = True
        if css_selector:
            config_params["css_selector"] = css_selector

        config = CrawlerRunConfig(**config_params)

        async with _browser_ops_lock:
            async with AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR) as crawler:
                result = await crawler.arun(url=url, config=config)

            if not result.success:
                raise SandboxError(
                    message=f"Failed to scrape {url}: {result.error_message}"
                )

            # Return data based on requested format
            response_data = {
                "url": url,
                "title": result.metadata.get("title", "") if result.metadata else "",
                "keep_open": False,
            }

            if output_format == "markdown":
                content = ""
                # Try multiple fields for better fallbacks
                if hasattr(result, "fit_markdown") and result.fit_markdown:
                    content = result.fit_markdown
                elif result.markdown:
                    if hasattr(result.markdown, "raw_markdown"):
                        content = result.markdown.raw_markdown
                    else:
                        content = str(result.markdown)
                else:
                    if hasattr(result, "extracted_content") and result.extracted_content:
                        content = result.extracted_content
                    elif hasattr(result, "text"):
                        content = result.text

                # Clean CSS from markdown output
                content = _clean_css_from_markdown(content)

                if max_length and len(content) > max_length:
                    content = content[:max_length] + "..."

                response_data["content"] = content or ""
            elif output_format == "html":
                response_data["content"] = result.cleaned_html
            elif output_format == "text":
                # extracted_content might be None, use fallback
                content = result.extracted_content
                if content is None:
                    # Fallback to markdown if available
                    if result.markdown and hasattr(result.markdown, "raw_markdown"):
                        content = result.markdown.raw_markdown
                    elif result.markdown:
                        content = result.markdown
                    else:
                        content = ""
                response_data["content"] = content
            elif output_format == "links":
                response_data["links"] = {
                    "internal": result.links.get("internal", []),
                    "external": result.links.get("external", []),
                }

            if screenshot and result.screenshot:
                # screenshot is a Base64-encoded string
                response_data["screenshot"] = result.screenshot
                response_data["screenshot_encoding"] = "base64"

            return response_data

    except SandboxError:
        raise
    except TimeoutError as e:
        raise SandboxError(
            message=f"Scraping timed out for {url}"
        )
    except ConnectionError as e:
        raise SandboxError(
            message=f"Connection failed for {url}"
        )
    except Exception as e:
        raise SandboxError(
            message=f"Scraping failed: {str(e)}"
        )


async def web_extract_css(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Structured data extraction using CSS selectors (JsonCssExtractionStrategy)

    Args:
        url: Target URL
        schema: JSON schema mapping field names to CSS selectors
        base_selector: Base selector for multiple items (optional)

    Returns:
        Dictionary with extracted data
    """
    url = args.get("url")
    schema = args.get("schema")
    args.get("base_selector")

    if not url:
        raise SandboxError(
            message="Missing required parameter: url"
        )
    if not schema:
        raise SandboxError(
            message="Missing required parameter: schema"
        )

    try:
        browser_config = BrowserConfig(cdp_url=CDP_URL, headless=False, verbose=False)

        # Create extraction strategy
        strategy = JsonCssExtractionStrategy(schema)

        config = CrawlerRunConfig(
            extraction_strategy=strategy,
        )

        async with _browser_ops_lock:
            async with AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR) as crawler:
                result = await crawler.arun(url=url, config=config)

            if not result.success:
                raise SandboxError(
                    message=f"Failed to extract: {result.error_message}"
                )

            # Parse extracted JSON
            extracted_data = (
                json.loads(result.extracted_content) if result.extracted_content else {}
            )

            return {"url": url, "extracted_data": extracted_data}

    except json.JSONDecodeError as e:
        raise SandboxError(
            message="Failed to parse extracted content as JSON"
        )
    except SandboxError:
        raise
    except Exception as e:
        raise SandboxError(
            message=f"CSS extraction failed: {str(e)}"
        )


async def web_extract_table(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Extract tables from web pages as JSON

    Args:
        url: Target URL
        table_index: Which table to extract (default: 0)

    Returns:
        Dictionary with extracted table data
    """
    url = args.get("url")
    if not url:
        raise SandboxError(
            message="Missing required parameter: url"
        )

    table_index = args.get("table_index", 0)

    try:
        browser_config = BrowserConfig(cdp_url=CDP_URL, headless=False, verbose=False)

        config = CrawlerRunConfig()

        async with _browser_ops_lock:
            async with AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR) as crawler:
                result = await crawler.arun(url=url, config=config)

            if not result.success:
                raise SandboxError(
                    message=f"Failed to extract tables: {result.error_message}"
                )

            # Extract tables using crawl4ai's table extraction
            # Note: crawl4ai doesn't have built-in table extraction in basic config
            # We'll use extracted_content and parse tables
            from bs4 import BeautifulSoup

            soup = BeautifulSoup(result.cleaned_html, "lxml")
            tables = soup.find_all("table")

            if table_index >= len(tables):
                raise SandboxError(
                    message=f"Table index {table_index} out of range (found {len(tables)} tables)"
                )

            table = tables[table_index]

            # Extract table data
            rows = []
            headers = []

            # Extract headers
            thead = table.find("thead")
            if thead:
                header_row = thead.find("tr")
                if header_row:
                    headers = [
                        th.get_text(strip=True)
                        for th in header_row.find_all(["th", "td"])
                    ]

            # Extract body rows
            tbody = table.find("tbody") or table
            for tr in tbody.find_all("tr"):
                cells = [td.get_text(strip=True) for td in tr.find_all(["td", "th"])]
                if cells:
                    rows.append(cells)

            return {
                "url": url,
                "table_index": table_index,
                "total_tables": len(tables),
                "headers": headers,
                "rows": rows,
            }

    except SandboxError:
        raise
    except Exception as e:
        raise SandboxError(
            message=f"Table extraction failed: {str(e)}"
        )


async def web_screenshot(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Capture screenshot with optional content extraction

    Args:
        url: Target URL
        full_page: Capture full page scroll (default: true)
        format: Image format - png|jpeg (default: png)

    Returns:
        Dictionary with screenshot data
    """
    url = args.get("url")
    if not url:
        raise SandboxError(
            message="Missing required parameter: url"
        )

    args.get("full_page", True)

    try:
        browser_config = BrowserConfig(cdp_url=CDP_URL, headless=False, verbose=False)

        config = CrawlerRunConfig(
            screenshot=True,
            screenshot_wait_for=2.0,  # Wait for dynamic content
        )

        async with _browser_ops_lock:
            async with AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR) as crawler:
                result = await crawler.arun(url=url, config=config)

            if not result.success:
                raise SandboxError(
                    message=f"Failed to capture screenshot: {result.error_message}"
                )

            return {
                "url": url,
                "screenshot": result.screenshot,
                "screenshot_encoding": "base64",
                "title": result.metadata.get("title", "")
                if result.metadata
                else "",
                "description": result.metadata.get("description", "")
                if result.metadata
                else "",
            }

    except SandboxError:
        raise
    except Exception as e:
        raise SandboxError(
            message=f"Screenshot failed: {str(e)}"
        )


async def web_links(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Extract all links from a page

    Args:
        url: Target URL
        filter_external: Only return external links (default: false)

    Returns:
        Dictionary with extracted links
    """
    url = args.get("url")
    if not url:
        raise SandboxError(
            message="Missing required parameter: url"
        )

    filter_external = args.get("filter_external", False)

    try:
        browser_config = BrowserConfig(cdp_url=CDP_URL, headless=False, verbose=False)

        config = CrawlerRunConfig(
            word_count_threshold=0,
        )

        async with _browser_ops_lock:
            async with AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR) as crawler:
                result = await crawler.arun(url=url, config=config)

            if not result.success:
                raise SandboxError(
                    message=f"Failed to extract links: {result.error_message}"
                )

            base_domain = urlparse(url).netloc
            # Extract href from link objects (crawl4ai returns dicts)
            internal_links = [
                link.get("href") if isinstance(link, dict) else link
                for link in result.links.get("internal", [])
            ]
            external_links = [
                link.get("href") if isinstance(link, dict) else link
                for link in result.links.get("external", [])
            ]
            all_links = internal_links + external_links

            if filter_external:
                # Filter to only external links
                all_links = [
                    link for link in all_links if urlparse(link).netloc != base_domain
                ]

            return {"url": url, "total_links": len(all_links), "links": all_links}

    except SandboxError:
        raise
    except Exception as e:
        raise SandboxError(
            message=f"Link extraction failed: {str(e)}"
        )


async def web_crawl(args: Dict[str, Any]) -> Dict[str, Any]:
    """
    Multi-page crawling

    Args:
        urls: List of URLs to crawl
        format: Output format - markdown|html|text (default: markdown)
        search_query: Query for BM25 content filtering (optional)
        use_pruning: Use PruningContentFilter (default: false)
        pruning_threshold: Threshold for pruning (default: 0.48)
        bm25_threshold: Threshold for BM25 (default: 1.0)
        wait_until: Page load condition (default: "domcontentloaded")
        cache_mode: Cache mode (default: "bypass")
        page_timeout: Page timeout in milliseconds (optional)
        max_length: Maximum content length (optional)
        proxy_config: Proxy configuration (optional)

    Returns:
        Dictionary with crawl results
    """
    urls = args.get("urls", [])
    if not urls or not isinstance(urls, list):
        raise SandboxError(
            message="Missing or invalid parameter: urls (must be a list)"
        )

    output_format = args.get("format", "markdown")

    # Validate output format
    valid_formats = ["markdown", "html", "text"]
    if output_format not in valid_formats:
        raise SandboxError(
            message=f"Invalid format: {output_format}. Must be one of {valid_formats}"
        )

    # New parameters
    search_query = args.get("search_query")
    use_pruning = args.get("use_pruning", False)
    pruning_threshold = args.get("pruning_threshold", 0.48)
    bm25_threshold = args.get("bm25_threshold", 1.0)
    wait_until = args.get("wait_until", "domcontentloaded")
    cache_mode = args.get("cache_mode", "bypass")
    page_timeout = args.get("page_timeout")
    max_length = args.get("max_length")
    proxy_config = args.get("proxy_config")

    try:
        # Build browser config with proxy support
        browser_params = {
            "cdp_url": CDP_URL,
            "headless": False,
            "verbose": False
        }
        if proxy_config:
            browser_params.update(proxy_config)
        browser_config = BrowserConfig(**browser_params)

        # Build crawl config using helper
        config_params = _build_config(
            search_query=search_query,
            use_pruning=use_pruning,
            pruning_threshold=pruning_threshold,
            bm25_threshold=bm25_threshold,
            word_count_threshold=10,
            wait_until=wait_until,
            cache_mode=cache_mode,
            page_timeout=page_timeout,
        )

        config = CrawlerRunConfig(**config_params)

        async with _browser_ops_lock:
            async with AsyncWebCrawler(config=browser_config, base_directory=CRAWL4AI_BASE_DIR) as crawler:
                # Use arun_many for parallel crawling
                results = await crawler.arun_many(urls=urls, config=config)

            response_data = []
            for result in results:
                data = {
                    "url": result.url,
                    "success": result.success,
                }

                if result.success:
                    data["title"] = (
                        result.metadata.get("title", "") if result.metadata else ""
                    )

                    # Handle markdown content properly with better fallbacks
                    if output_format == "markdown":
                        content = ""
                        if hasattr(result, "fit_markdown") and result.fit_markdown:
                            content = result.fit_markdown
                        elif result.markdown and hasattr(result.markdown, "raw_markdown"):
                            content = result.markdown.raw_markdown
                        elif result.markdown:
                            content = result.markdown
                        else:
                            if hasattr(result, "extracted_content") and result.extracted_content:
                                content = result.extracted_content
                            elif hasattr(result, "text"):
                                content = result.text

                        if max_length and len(content) > max_length:
                            content = content[:max_length] + "..."

                        data["content"] = content or ""
                    elif output_format == "html":
                        content = result.cleaned_html
                        if max_length and len(content) > max_length:
                            content = content[:max_length] + "..."
                        data["content"] = content
                    elif output_format == "text":
                        content = result.extracted_content
                        if not content:
                            if result.markdown and hasattr(result.markdown, "raw_markdown"):
                                content = result.markdown.raw_markdown
                            elif result.markdown:
                                content = result.markdown

                        if max_length and len(content) > max_length:
                            content = content[:max_length] + "..."
                        data["content"] = content or ""

                    if result.links:
                        data["link_count"] = len(
                            result.links.get("internal", [])
                        ) + len(result.links.get("external", []))
                else:
                    data["error"] = result.error_message

                response_data.append(data)

            return {
                "total_urls": len(urls),
                "successful": sum(1 for r in results if r.success),
                "failed": sum(1 for r in results if not r.success),
                "results": response_data,
            }

    except SandboxError:
        raise
    except TimeoutError as e:
        raise SandboxError(
            message=f"Crawling timed out for {len(urls)} URLs"
        )
    except Exception as e:
        raise SandboxError(
            message=f"Crawling failed: {str(e)}"
        )


# Command mapping for tool_proxy
# Maps action names (with web_ prefix) to actual function names
COMMANDS = {
    "web_health_check": health_check,
    "web_scrape": web_scrape,
    "web_extract_css": web_extract_css,
    "web_extract_table": web_extract_table,
    "web_screenshot": web_screenshot,
    "web_links": web_links,
    "web_crawl": web_crawl,
}


async def main():
    """Main entry point"""
    try:
        if len(sys.argv) < 2:
            raise SandboxError(
                message=f"Usage: {sys.argv[0]} <command>"
            )

        command = sys.argv[1]

        # Read JSON arguments from stdin to avoid shell interpretation issues
        try:
            args_json = sys.stdin.read().strip()
            if not args_json:
                args_json = "{}"
            args = json.loads(args_json)
        except json.JSONDecodeError as e:
            raise SandboxError(
                message=f"Invalid JSON arguments from stdin: {str(e)}"
            )
        except Exception as e:
            raise SandboxError(
                message=f"Error reading arguments: {str(e)}"
            )

        # Command handlers
        commands = {
            "web_health_check": health_check,
            "web_scrape": web_scrape,
            "web_extract_css": web_extract_css,
            "web_extract_table": web_extract_table,
            "web_screenshot": web_screenshot,
            "web_links": web_links,
            "web_crawl": web_crawl,
        }

        handler = commands.get(command)
        if not handler:
            raise SandboxError(
                message=f"Unknown command: {command}"
            )

        # Execute command with timeout
        try:
            result = await asyncio.wait_for(handler(args), timeout=COMMAND_TIMEOUT)
            # Print result as JSON for direct execution
            sys.stdout.write(json.dumps(result, ensure_ascii=False, default=str))
            sys.stdout.flush()
        except asyncio.TimeoutError:
            raise SandboxError(
                message=f"Command timed out after {COMMAND_TIMEOUT} seconds"
            )
    except SandboxError as e:
        # Print error as JSON
        error_json = {"error_message": e.message, "status_code": e.status_code}
        sys.stdout.write(json.dumps(error_json, ensure_ascii=False))
        sys.stdout.flush()
        sys.exit(1)
    except Exception as e:
        # Print error as JSON
        error_json = {"error_message": str(e), "status_code": 500}
        sys.stdout.write(json.dumps(error_json, ensure_ascii=False))
        sys.stdout.flush()
        sys.exit(1)


if __name__ == "__main__":
    asyncio.run(main())
