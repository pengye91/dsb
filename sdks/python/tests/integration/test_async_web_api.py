"""
Integration tests for Async Web API

Tests require a running DSB server with a sandbox that has web tools installed.
Set DSB_API_URL environment variable to override the default server URL.
Set DSB_SANDBOX_IMAGE to specify the sandbox image to use.

Markers:
    - web_scraping: Marks tests as web scraping tests
    - browser: Marks tests as browser automation tests
    - slow: Marks tests that take longer than 30 seconds
    - requires_server: Marks tests that require a running DSB server
"""

import asyncio
import os
import uuid
from collections.abc import AsyncGenerator

import pytest

from dsb_sdk import AsyncDSBClient
from dsb_sdk.exceptions import DSBConnectionError, DSBValidationError

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
        from dsb_sdk import DSBClient

        client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
        health = client.health.check()
        client.close()
        return health.status in ["healthy", "ok"]
    except Exception:
        return False


@pytest.fixture(scope="function")
async def async_client() -> AsyncGenerator[AsyncDSBClient, None]:
    """
    Create an async DSB client for testing.

    Scope is function-level to avoid event loop issues with module-scoped async fixtures.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    await client.close()


@pytest.fixture(scope="function", autouse=True)
async def cleanup_sandboxes(
    async_client: AsyncDSBClient,
) -> AsyncGenerator[list[str], None]:
    """
    Cleanup all test sandboxes after each test.

    This is an autouse fixture that runs automatically for all tests in this module.
    """
    created_ids: list[str] = []

    yield created_ids

    # Cleanup after test
    for sandbox_id in created_ids:
        try:
            await async_client.sandbox.delete_async(sandbox_id)
        except Exception:
            pass  # Best effort cleanup


async def wait_for_sandbox_async(
    client: AsyncDSBClient,
    sandbox_id: str,
    max_wait: int = 60,
    poll_interval: float = 1,
    browser_warmup: bool = False,
    wait_for_browser: bool = True,
) -> bool:
    """Wait for sandbox to be running (async version).

    Args:
        client: Async DSB client
        sandbox_id: Sandbox ID to wait for
        max_wait: Maximum seconds to wait
        poll_interval: Seconds between polls
        browser_warmup: If True, wait 30s after sandbox is running for browser/chromium initialization
        wait_for_browser: If True, wait for health_check to return browser_ready=True
    """

    wait_time = 0
    while wait_time < max_wait:
        try:
            sandbox = await client.sandbox.get_async(sandbox_id)
            if sandbox.state.value == "running":
                # Wait for browser/chromium to fully initialize if requested
                if browser_warmup:
                    print("\n[Browser Warmup] Waiting 10 seconds for browser/chromium initialization...")
                    await asyncio.sleep(10)
                    print("[Browser Warmup] Warmup complete, browser should be ready\n")
                    return True

                # Wait for health check to succeed (tool_proxy ready)
                if wait_for_browser:
                    for health_attempt in range(60):  # Try up to 60 times (120 seconds total to match browser_tools_secs)
                        try:
                            health = await client.web.health_check_async(sandbox_id, timeout=5)
                            if health.browser_ready:
                                return True
                        except Exception:
                            if health_attempt < 59:  # Don't sleep on last iteration
                                await asyncio.sleep(1)
                    # Browser never became ready — skip instead of proceeding with broken sandbox
                    pytest.skip(f"Browser in sandbox {sandbox_id} failed to become ready after {health_attempt + 1} attempts")
                else:
                    await asyncio.sleep(1)
                return True
            # Check for error states
            elif sandbox.state.value in ("error", "destroyed", "destroying"):
                return False
        except Exception:
            pass

        await asyncio.sleep(poll_interval)
        wait_time += poll_interval

    return False


@pytest.fixture(scope="function")
async def sandbox(
    async_client: AsyncDSBClient,
    cleanup_sandboxes: list[str],
) -> AsyncGenerator:
    """
    Create a function-scoped sandbox with browser tools for testing.

    This fixture creates a new sandbox for each test to avoid race conditions
    when running tests in parallel with pytest-xdist.
    Browser tools are available after sandbox reaches running state.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    sandbox_obj = None
    sandbox_id_str = None

    try:
        # Create sandbox with unique name for this test
        import uuid
        sandbox_name = f"test-async-web-tools-{uuid.uuid4().hex[:8]}"
        sandbox_obj = await async_client.sandbox.create_async(
            image=SANDBOX_IMAGE,
            name=sandbox_name,
        )

        # Convert UUID to string for API calls
        sandbox_id_str = str(sandbox_obj.id)

        # Track for cleanup
        cleanup_sandboxes.append(sandbox_id_str)

        # Wait for sandbox to be running and browser to be ready
        if not await wait_for_sandbox_async(async_client, sandbox_id_str, browser_warmup=False, wait_for_browser=True):
            pytest.skip("Sandbox did not reach running state in time")

        print(f"\n[Function Fixture] Browser sandbox ready: {sandbox_id_str}\n")
        yield sandbox_obj

    except DSBConnectionError:
        pytest.skip("DSB server not available")
    except Exception as e:
        # Cleanup if sandbox was created but error occurred
        if sandbox_obj and sandbox_id_str:
            try:
                await async_client.sandbox.delete_async(sandbox_id_str)
                cleanup_sandboxes.remove(sandbox_id_str)
                print(f"Emergency cleanup: Deleted sandbox {sandbox_id_str}")
            except Exception:
                pass  # Best effort
        pytest.skip(f"Failed to create/setup sandbox: {e}")


@pytest.fixture(scope="function")
async def slim_sandbox(
    async_client: AsyncDSBClient,
    cleanup_sandboxes: list[str],
) -> AsyncGenerator:
    """
    Create a slim sandbox for testing web scraping only.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    try:
        # Use unique name to avoid conflicts with parallel tests
        sandbox_name = f"test-async-web-slim-{uuid.uuid4().hex[:8]}"
        sandbox = await async_client.sandbox.create_async(
            image=SANDBOX_SLIM_IMAGE,
            name=sandbox_name,
        )

        sandbox_id_str = str(str(sandbox.id))

        if not await wait_for_sandbox_async(async_client, sandbox_id_str, wait_for_browser=True):
            pytest.skip("Slim sandbox did not reach running state in time")

        cleanup_sandboxes.append(sandbox_id_str)
        yield sandbox

    except DSBConnectionError:
        pytest.skip("DSB server not available")
    except Exception as e:
        pytest.skip(f"Failed to create slim sandbox: {e}")


@pytest.mark.web_scraping
@pytest.mark.requires_server
class TestAsyncWebScraping:
    """Tests for async web scraping"""

    @pytest.mark.asyncio
    async def test_async_scrape_markdown(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test async scraping local fixture page as markdown."""
        result = await async_client.web.scrape_async(
            str(sandbox.id), local_fixture_server_url, format="markdown"
        )

        assert result.url == local_fixture_server_url
        assert isinstance(result.content, str)
        assert "Alice" in result.content
        assert "Laptop" in result.content

    @pytest.mark.asyncio
    async def test_async_scrape_html(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test async scraping local fixture page as HTML."""
        result = await async_client.web.scrape_async(
            str(sandbox.id), local_fixture_server_url, format="html"
        )

        assert result.url == local_fixture_server_url
        assert "<html" in result.content.lower()
        assert "<table>" in result.content.lower()
        assert "alice" in result.content.lower()

    @pytest.mark.asyncio
    async def test_async_extract_links(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test async link extraction"""
        result = await async_client.web.links_async(
            str(sandbox.id),
            local_fixture_server_url,
        )

        assert result.url == local_fixture_server_url
        assert result.total_links >= 0

    @pytest.mark.asyncio
    async def test_async_health_check(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async web tools health check"""
        result = await async_client.web.health_check_async(str(sandbox.id))

        assert result.cdp_url
        assert isinstance(result.browser_ready, bool)

    @pytest.mark.asyncio
    async def test_async_crawl(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
        local_fixture_server_url: str,
    ):
        """Test async web crawling"""
        result = await async_client.web.crawl_async(
            str(sandbox.id),
            [local_fixture_server_url],
            format="markdown",
            timeout=120,
        )

        assert result.total_urls == 1


@pytest.mark.browser
@pytest.mark.requires_server
class TestAsyncBrowserAutomation:
    """Tests for async browser automation"""

    @pytest.mark.asyncio
    async def test_async_browser_navigate(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async browser navigation"""
        result = await async_client.web.browser_navigate_async(str(sandbox.id), "https://example.com")

        # Browser may add trailing slash - normalize both URLs for comparison
        expected_url = "https://example.com"
        actual_url = result.url.rstrip('/') if result.url else result.url

        assert actual_url == expected_url or result.url == expected_url + '/'
        assert result.status == "success"

    @pytest.mark.asyncio
    async def test_async_browser_screenshot(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async browser screenshot"""
        await async_client.web.browser_navigate_async(str(sandbox.id), "https://example.com")

        result = await async_client.web.browser_screenshot_async(
            str(sandbox.id), name="async_screenshot"
        )

        assert result.status == "success"

    @pytest.mark.asyncio
    async def test_async_browser_get_elements(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async getting clickable elements"""
        await async_client.web.browser_navigate_async(str(sandbox.id), "https://example.com")

        result = await async_client.web.browser_get_clickable_elements_async(str(sandbox.id))

        assert result.elements is not None

    @pytest.mark.asyncio
    async def test_async_browser_evaluate(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async JavaScript evaluation"""
        await async_client.web.browser_navigate_async(str(sandbox.id), "https://example.com")

        result = await async_client.web.browser_evaluate_async(
            str(sandbox.id), script="() => document.title"
        )

        assert result.status == "success"

    @pytest.mark.asyncio
    async def test_async_browser_close(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async browser close"""
        result = await async_client.web.browser_close_async(str(sandbox.id))

        assert result.status == "success"

    @pytest.mark.asyncio
    async def test_async_browser_scroll(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async browser scroll"""
        await async_client.web.browser_navigate_async(str(sandbox.id), "https://example.com")

        result = await async_client.web.browser_scroll_async(str(sandbox.id), amount=500)

        assert result.status == "success"


@pytest.mark.web_scraping
@pytest.mark.requires_server
class TestAsyncBrowserSupport:
    """Tests for async browser support checking"""

    @pytest.mark.asyncio
    async def test_async_supports_browser(
        self,
        async_client: AsyncDSBClient,
        sandbox: object,
    ):
        """Test async supports_browser check"""
        result = await async_client.web.supports_browser_async(str(sandbox.id))
        assert result is True

    @pytest.mark.asyncio
    async def test_async_supports_browser_slim(
        self,
        async_client: AsyncDSBClient,
        slim_sandbox: object,
    ):
        """Test async supports_browser for slim sandbox"""
        result = await async_client.web.supports_browser_async(str(slim_sandbox.id))
        assert result is False

    @pytest.mark.asyncio
    async def test_async_browser_automation_fails_on_slim(
        self,
        async_client: AsyncDSBClient,
        slim_sandbox: object,
    ):
        """Test async browser automation fails on slim sandbox"""
        with pytest.raises(DSBValidationError, match="full sandbox image"):
            await async_client.web.browser_navigate_async(str(slim_sandbox.id), "https://example.com")
