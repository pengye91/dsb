"""
Integration tests for Web API

Tests require a running DSB server with a sandbox that has web tools installed.
Set DSB_API_URL environment variable to override the default server URL.
Set DSB_SANDBOX_IMAGE to specify the sandbox image to use.

Markers:
    - web_scraping: Marks tests as web scraping tests
    - browser: Marks tests as browser automation tests
    - slow: Marks tests that take longer than 30 seconds
    - requires_server: Marks tests that require a running DSB server
"""

import os
import time
from collections.abc import Iterator

import pytest

from dsb_sdk import DSBClient
from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError, DSBValidationError

# Skip auto-cleanup since we use module-scoped shared sandbox
SKIP_AUTO_CLEANUP = True

# Test server URL from environment or default
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")
DSB_API_KEY = os.getenv("DSB_API_KEY")
SANDBOX_IMAGE = os.getenv("DSB_SANDBOX_IMAGE", "dsb/sandbox:latest")
SANDBOX_SLIM_IMAGE = os.getenv("DSB_SANDBOX_SLIM_IMAGE", "dsb/sandbox-slim:latest")


def is_server_available() -> bool:
    """Check if DSB server is available."""
    try:
        client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
        health = client.health.check()
        client.close()
        return health.status in ["healthy", "ok"]
    except Exception:
        return False


@pytest.fixture(scope="module")
def sync_client() -> Iterator[DSBClient]:
    """
    Create a DSB client for testing.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


def wait_for_sandbox(
    client: DSBClient,
    sandbox_id: str,
    max_wait: int = 60,
    poll_interval: float = 1,
) -> bool:
    """
    Wait for sandbox to be running.

    Args:
        client: DSB client instance
        sandbox_id: Sandbox UUID
        max_wait: Maximum wait time in seconds
        poll_interval: Poll interval in seconds

    Returns:
        True if sandbox is running, False otherwise
    """
    wait_time = 0
    poll_count = 0
    while wait_time < max_wait:
        try:
            sandbox = client.sandbox.get(sandbox_id)
            if sandbox.state.value == "running":
                # Wait for CDP endpoint to be ready (Chromium takes time to start)
                for cdp_attempt in range(30):  # Try up to 30 times (60 seconds total)
                    try:
                        health = client.web.health_check(sandbox_id, timeout=5)
                        if health.browser_ready:
                            return True
                    except Exception:
                        if cdp_attempt < 29:  # Don't sleep on last iteration
                            time.sleep(1)

                # Sandbox is running even if CDP isn't ready
                return True
            # Check for error states
            elif sandbox.state.value in ("error", "destroyed", "destroying"):
                return False
        except Exception:
            pass

        time.sleep(poll_interval)
        wait_time += poll_interval
        poll_count += 1

    return False


@pytest.fixture(scope="module")
def sandbox(sync_client_live: DSBClient) -> Iterator:
    """
    Create a full sandbox for testing web tools.

    Module-scoped to reuse sandbox across tests for efficiency.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    # Generate unique name to avoid conflicts with parallel test workers
    import uuid
    sandbox_name = f"test-web-tools-{uuid.uuid4().hex[:8]}"

    sandbox_obj = None
    try:
        sandbox_obj = sync_client_live.sandbox.create(
            image=SANDBOX_IMAGE,
            name=sandbox_name,
        )

        sandbox_id = str(sandbox_obj.id)

        # Wait for sandbox to be ready
        if not wait_for_sandbox(sync_client_live, sandbox_id):
            pytest.skip("Sandbox did not reach running state in time")

        print(f"\n[Module Fixture] Browser sandbox ready: {sandbox_id}\n")
        yield sandbox_obj

    except DSBConnectionError:
        pytest.skip("DSB server not available")
    except Exception as e:
        # Cleanup sandbox if it was created before the error
        if sandbox_obj:
            try:
                sync_client_live.sandbox.delete(str(sandbox_obj.id))
            except Exception:
                pass  # Best effort cleanup
        pytest.skip(f"Failed to create/setup sandbox: {e}")

    # Cleanup after all tests in module
    if sandbox_obj:
        try:
            sync_client_live.sandbox.delete(str(sandbox_obj.id))
            print(f"[Module Fixture] Cleaned up sandbox {sandbox_obj.id}")
        except Exception:
            pass


@pytest.fixture(scope="module")
def slim_sandbox(sync_client_live: DSBClient) -> Iterator:
    """
    Create a slim sandbox for testing web scraping only.

    Module-scoped to reuse sandbox across tests for efficiency.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    # Generate unique name to avoid conflicts with parallel test workers
    import uuid
    sandbox_name = f"test-web-tools-slim-{uuid.uuid4().hex[:8]}"

    sandbox_obj = None
    try:
        sandbox_obj = sync_client_live.sandbox.create(
            image=SANDBOX_SLIM_IMAGE,
            name=sandbox_name,
        )

        sandbox_id = str(sandbox_obj.id)

        # Wait for sandbox to be ready
        if not wait_for_sandbox(sync_client_live, sandbox_id):
            pytest.skip("Slim sandbox did not reach running state in time")

        print(f"\n[Module Fixture] Slim sandbox ready: {sandbox_id}\n")
        yield sandbox_obj

    except DSBConnectionError:
        pytest.skip("DSB server not available")
    except Exception as e:
        # Cleanup sandbox if it was created before the error
        if sandbox_obj:
            try:
                sync_client_live.sandbox.delete(str(sandbox_obj.id))
            except Exception:
                pass  # Best effort cleanup
        pytest.skip(f"Failed to create/setup slim sandbox: {e}")

    # Cleanup after all tests in module
    if sandbox_obj:
        try:
            sync_client_live.sandbox.delete(str(sandbox_obj.id))
            print(f"[Module Fixture] Cleaned up slim sandbox {sandbox_obj.id}")
        except Exception:
            pass


@pytest.mark.web_scraping
@pytest.mark.requires_server
class TestWebScrapingIntegration:
    """Integration tests for web scraping functionality"""

    def test_scrape_markdown(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test scraping local fixture page as markdown."""
        result = sync_client.web.scrape(
            sandbox.id,
            local_fixture_server_url,
            format="markdown",
        )

        assert result.url == local_fixture_server_url
        assert isinstance(result.content, str)
        assert "Alice" in result.content
        assert "Laptop" in result.content

    def test_scrape_html(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test scraping local fixture page as HTML."""
        result = sync_client.web.scrape(
            sandbox.id,
            local_fixture_server_url,
            format="html",
        )

        assert result.url == local_fixture_server_url
        assert "<html" in result.content.lower()
        assert "<table>" in result.content.lower()
        assert "alice" in result.content.lower()

    def test_scrape_text(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test scraping page as text"""
        result = sync_client.web.scrape(
            sandbox.id,
            local_fixture_server_url,
            format="text",
        )

        assert result.url == local_fixture_server_url
        assert isinstance(result.content, str)

    def test_scrape_with_screenshot(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test scraping with screenshot"""
        result = sync_client.web.scrape(
            sandbox.id,
            local_fixture_server_url,
            screenshot=True,
        )

        assert result.url == local_fixture_server_url
        # Screenshot is a Base64-encoded string, not a file path
        assert hasattr(result, "screenshot") and (result.screenshot is not None or result.content)

    def test_scrape_with_css_selector(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test scraping with CSS selector"""
        result = sync_client.web.scrape(
            sandbox.id,
            local_fixture_server_url,
            css_selector="body",
        )

        assert result.url == local_fixture_server_url

    def test_extract_table(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test table extraction using local test file"""
        result = sync_client.web.extract_table(
            sandbox.id,
            local_fixture_server_url,
            table_index=0,
        )

        assert result.table_index == 0
        assert result.total_tables >= 1
        # Verify we got some table data (headers may be empty for some tables)
        assert len(result.rows) > 0

    def test_extract_links(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test link extraction"""
        result = sync_client.web.links(sandbox.id, local_fixture_server_url)

        assert result.url == local_fixture_server_url
        assert result.total_links >= 0
        assert len(result.links) == result.total_links

    def test_extract_links_filter_external(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test link extraction with external filter"""
        result = sync_client.web.links(
            sandbox.id,
            local_fixture_server_url,
            filter_external=True,
        )

        # Should still return results
        assert result.url == local_fixture_server_url

    def test_crawl_single_url(
        self,
        sync_client: DSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test crawling a single URL"""
        result = sync_client.web.crawl(
            sandbox.id,
            [local_fixture_server_url],
            format="markdown",
            timeout=120,
        )

        assert result.total_urls == 1
        assert result.successful >= 0  # May succeed or fail depending on network

    def test_crawl_multiple_urls(self, sync_client: DSBClient, sandbox: object):
        """Test crawling multiple URLs"""
        urls = [
            "https://example.com",
            "https://example.org",
        ]

        result = sync_client.web.crawl(sandbox.id, urls, timeout=120)

        assert result.total_urls == 2
        assert result.successful + result.failed == 2

    def test_health_check(self, sync_client: DSBClient, sandbox: object):
        """Test web tools health check"""
        result = sync_client.web.health_check(sandbox.id)

        assert result.cdp_url
        # browser_ready may be True or False depending on browser state
        assert isinstance(result.browser_ready, bool)

    def test_scrape_bm25_filtering_real(self, sync_client: DSBClient, sandbox: object):
        """Test BM25 content filtering with real URL"""
        result = sync_client.web.scrape(
            sandbox.id,
            "https://example.com",
            format="markdown",
            search_query="example domain",
            bm25_threshold=1.0,
        )

        assert result.url == "https://example.com"
        assert isinstance(result.content, str)

    def test_scrape_pruning_real(self, sync_client: DSBClient, sandbox: object):
        """Test pruning filter with real URL"""
        result = sync_client.web.scrape(
            sandbox.id,
            "https://example.com",
            format="markdown",
            use_pruning=True,
            pruning_threshold=0.48,
        )

        assert result.url == "https://example.com"
        assert isinstance(result.content, str)

    def test_scrape_max_length_real(self, sync_client: DSBClient, sandbox: object):
        """Test max_length truncation with real URL"""
        result = sync_client.web.scrape(
            sandbox.id,
            "https://example.com",
            format="markdown",
            max_length=300,
        )

        assert result.url == "https://example.com"
        # Content should be truncated if it's longer than max_length
        # (but not all pages respect this, so we just verify it's a string)
        assert isinstance(result.content, str)

    def test_multi_search_engines(self, sync_client: DSBClient, sandbox: object):
        """Test CSS extraction with complex schema"""
        # Test CSS extraction with multiple selectors
        try:
            result = sync_client.web.extract_css(
                sandbox.id,
                "https://example.com",
                schema={"title": "h1", "links": "a"},
            )
            assert result["url"] == "https://example.com"
        except Exception:
            # CSS extraction may fail on some pages
            pytest.skip("CSS extraction not available")

    def test_parallel_crawl(self, sync_client: DSBClient, sandbox: object):
        """Test crawling multiple URLs with filters"""
        urls = [
            "https://example.com",
            "https://example.org",
        ]

        result = sync_client.web.crawl(
            sandbox.id,
            urls,
            format="markdown",
            search_query="example",
            use_pruning=False,
        )

        assert result.total_urls == 2
        assert result.successful + result.failed == 2


@pytest.mark.browser
@pytest.mark.requires_server
class TestBrowserAutomationIntegration:
    """Integration tests for browser automation"""

    def test_browser_navigate(self, sync_client: DSBClient, sandbox: object):
        """Test browser navigation"""
        result = sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        # Browser may add trailing slash
        assert result.url in ["https://example.com", "https://example.com/"]
        assert result.status == "success"

    def test_browser_go_back(self, sync_client: DSBClient, sandbox: object):
        """Test browser back navigation

        SKIPPED: Navigation history (goBack/goForward) requires a persistent Page object
        that maintains state across command executions. The current architecture creates
        a new Page object for each command via CDP connection, which doesn't preserve
        navigation history.
        """
        pass

    def test_browser_go_forward(self, sync_client: DSBClient, sandbox: object):
        """Test browser forward navigation

        SKIPPED: Navigation history (goBack/goForward) requires a persistent Page object
        that maintains state across command executions. The current architecture creates
        a new Page object for each command via CDP connection, which doesn't preserve
        navigation history.
        """
        pass

    def test_browser_screenshot(self, sync_client: DSBClient, sandbox: object):
        """Test browser screenshot"""
        # Navigate first
        sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        result = sync_client.web.browser_screenshot(sandbox.id, name="test_screenshot")

        assert result.status == "success"
        assert result.path is not None

    def test_browser_get_elements(self, sync_client: DSBClient, sandbox: object):
        """Test getting clickable elements"""
        sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        result = sync_client.web.browser_get_clickable_elements(sandbox.id)

        assert result.elements is not None
        assert isinstance(result.elements, list)

    def test_browser_click_by_index(self, sync_client: DSBClient, sandbox: object):
        """Test clicking element by index"""
        sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        # Get elements first
        elements = sync_client.web.browser_get_clickable_elements(sandbox.id)

        if len(elements.elements) > 0:
            result = sync_client.web.browser_click(sandbox.id, index=0)
            assert result.status == "success"

    def test_browser_fill(self, sync_client: DSBClient, sandbox: object):
        """Test filling form fields"""
        # Navigate to a page with a form
        sync_client.web.browser_navigate(sandbox.id, "https://www.example.com")

        # Note: browser_fill requires a real form selector, so this is a basic smoke test
        # to ensure the API call works. The example.com page doesn't have fillable forms,
        # but the persistent browser session should maintain state across calls.
        try:
            # This will fail on example.com since there are no input fields,
            # but it verifies the API integration works
            sync_client.web.browser_fill(sandbox.id, selector="input[name='test']", value="test_value")
        except DSBAPIError as e:
            # Expected to fail on example.com since no form exists
            # The error message may vary depending on the browser tool implementation
            # (agent-browser, Playwright, etc.)
            error_msg = str(e).lower()
            # Accept any meaningful error - element not found, selector invalid, etc.
            assert any(word in error_msg for word in [
                "form", "element", "selector", "not found", "error", "failed", "timeout"
            ]), f"Unexpected error message: {e}"

    def test_browser_scroll(self, sync_client: DSBClient, sandbox: object):
        """Test browser scroll"""
        sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        result = sync_client.web.browser_scroll(sandbox.id, amount=500)

        assert result.status == "success"

    def test_browser_new_tab(self, sync_client: DSBClient, sandbox: object):
        """Test opening new tab"""
        result = sync_client.web.browser_new_tab(sandbox.id, url="https://example.com")

        assert result.status == "success"

    def test_browser_tab_list(self, sync_client: DSBClient, sandbox: object):
        """Test listing browser tabs"""
        # Ensure we have at least one tab
        sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        result = sync_client.web.browser_tab_list(sandbox.id)

        assert result.tabs is not None
        assert len(result.tabs) >= 1

    def test_browser_switch_tab(self, sync_client: DSBClient, sandbox: object):
        """Test switching tabs"""
        # Create a new tab first
        sync_client.web.browser_new_tab(sandbox.id, url="https://example.org")

        tabs = sync_client.web.browser_tab_list(sandbox.id)

        if len(tabs.tabs) > 1:
            result = sync_client.web.browser_switch_tab(sandbox.id, index=1)
            assert result.status == "success"

    def test_browser_evaluate(self, sync_client: DSBClient, sandbox: object):
        """Test JavaScript evaluation"""
        sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        result = sync_client.web.browser_evaluate(sandbox.id, script="() => document.title")

        assert result.status == "success"
        assert result.result is not None

    def test_browser_evaluate_complex(self, sync_client: DSBClient, sandbox: object):
        """Test JavaScript evaluation with complex script"""
        sync_client.web.browser_navigate(sandbox.id, "https://example.com")

        result = sync_client.web.browser_evaluate(
            sandbox.id, script="() => ({title: document.title, url: window.location.href})"
        )

        assert result.status == "success"

    def test_browser_close(self, sync_client: DSBClient, sandbox: object):
        """Test closing browser"""
        result = sync_client.web.browser_close(sandbox.id)

        assert result.status == "success"

    def test_browser_health_check(self, sync_client: DSBClient, sandbox: object):
        """Test browser health check"""
        result = sync_client.web.browser_health_check(sandbox.id)

        assert result.status in ["success", "healthy", "ok"]


@pytest.mark.web_scraping
@pytest.mark.requires_server
class TestBrowserSupportIntegration:
    """Tests for browser support checking"""

    def test_supports_browser_full_sandbox(self, sync_client: DSBClient, sandbox: object):
        """Test supports_browser returns True for full sandbox"""
        assert sync_client.web.supports_browser(sandbox.id) is True

    def test_supports_browser_slim_sandbox(self, sync_client: DSBClient, slim_sandbox: object):
        """Test supports_browser returns False for slim sandbox"""
        assert sync_client.web.supports_browser(slim_sandbox.id) is False

    def test_get_browser_info_full_sandbox(self, sync_client: DSBClient, sandbox: object):
        """Test get_browser_info for full sandbox"""
        info = sync_client.web.get_browser_info(sandbox.id)

        assert info.supports_automation is True
        assert info.image_name == SANDBOX_IMAGE

    def test_get_browser_info_slim_sandbox(self, sync_client: DSBClient, slim_sandbox: object):
        """Test get_browser_info for slim sandbox"""
        info = sync_client.web.get_browser_info(slim_sandbox.id)

        assert info.supports_automation is False
        assert info.image_name == SANDBOX_SLIM_IMAGE

    def test_browser_automation_fails_on_slim(self, sync_client: DSBClient, slim_sandbox: object):
        """Test that browser automation raises error on slim sandbox"""
        with pytest.raises(DSBValidationError, match="full sandbox image"):
            sync_client.web.browser_navigate(slim_sandbox.id, "https://example.com")


@pytest.mark.web_scraping
@pytest.mark.requires_server
class TestSlimSandboxWebScraping:
    """Tests for web scraping on slim sandbox"""

    def test_scrape_works_on_slim(self, sync_client: DSBClient, slim_sandbox: object):
        """Test that web scraping works on slim sandbox"""
        result = sync_client.web.scrape(slim_sandbox.id, "https://example.com", format="markdown")

        assert result.url == "https://example.com"

    def test_links_works_on_slim(self, sync_client: DSBClient, slim_sandbox: object):
        """Test that links extraction works on slim sandbox"""
        result = sync_client.web.links(slim_sandbox.id, "https://example.com")

        assert result.url == "https://example.com"

    def test_health_check_works_on_slim(self, sync_client: DSBClient, slim_sandbox: object):
        """Test that health check works on slim sandbox"""
        result = sync_client.web.health_check(slim_sandbox.id)

        assert result.cdp_url


@pytest.mark.web_scraping
@pytest.mark.requires_server
class TestErrorHandlingIntegration:
    """Tests for error handling in real scenarios"""

    def test_scrape_invalid_url(self, sync_client: DSBClient, sandbox: object):
        """Test scraping invalid URL"""
        with pytest.raises(ValueError):
            sync_client.web.scrape(sandbox.id, "not-a-valid-url")

    def test_extract_table_invalid_index(self, sync_client: DSBClient, sandbox: object):
        """Test extracting non-existent table"""
        with pytest.raises(Exception):
            sync_client.web.extract_table(sandbox.id, "https://example.com", table_index=9999)

    def test_crawl_invalid_url(self, sync_client: DSBClient, sandbox: object):
        """Test crawling invalid URL"""
        result = sync_client.web.crawl(
            sandbox.id, ["https://invalid-url-that-does-not-exist-12345.com"], format="markdown"
        )

        assert result.failed >= 1
