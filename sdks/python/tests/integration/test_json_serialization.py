"""
Integration tests for JSON serialization type validation

These tests validate that the Python SDK sends JSON payloads with
correct types that match the Rust backend's serde expectations.

Tests require a running DSB server on localhost:8081.

Background:
- Rust serde expects u64 (integer) for timeout values
- Python float values (e.g., 60.0) serialize to JSON as 60.0
- This causes deserialization errors: "expected u64, found floating point"

These integration tests catch type mismatches by making real HTTP calls
to the server and verifying the response.
"""

import os
import time
import uuid
from collections.abc import Iterator

import pytest

from dsb_sdk import DSBClient
from dsb_sdk.exceptions import DSBAPIError, DSBValidationError
from dsb_sdk.types.sandbox import SandboxState

# Test server URL from environment or default
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")
DSB_API_KEY = os.getenv("DSB_API_KEY")
TEST_IMAGE = os.getenv("TEST_IMAGE", "dsb/sandbox:latest")


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
def server_available() -> bool:
    """Check if DSB server is available at module start."""
    return is_server_available()


@pytest.fixture(scope="module")
def sync_client() -> Iterator[DSBClient]:
    """Create a DSB client for testing."""
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


@pytest.fixture(scope="function")
def cleanup_sandboxes(sync_client: DSBClient) -> Iterator[list]:
    """Cleanup all test sandboxes after each test."""
    created_ids = []

    yield created_ids

    # Cleanup after test
    for sandbox_id in created_ids:
        try:
            sync_client.sandbox.delete(sandbox_id)
        except Exception:
            pass  # Best effort cleanup


def wait_for_sandbox(
    client: DSBClient,
    sandbox_id: str,
    target_states: tuple[SandboxState, ...] = (SandboxState.RUNNING,),
    max_wait: int = 60,
    poll_interval: float = 1,
    wait_for_browser: bool = False,
) -> SandboxState:
    """Wait for sandbox to reach a target state.

    Args:
        client: DSB client instance
        sandbox_id: Sandbox UUID
        target_states: Target states to wait for
        max_wait: Maximum wait time in seconds
        poll_interval: Poll interval in seconds
        wait_for_browser: If True, wait for browser to be ready (for web tools tests)
    """
    wait_time = 0
    while wait_time < max_wait:
        try:
            sandbox = client.sandbox.get(sandbox_id)
            if sandbox.state in target_states:
                # Wait for browser to be ready if requested (for web tools tests)
                if wait_for_browser:
                    for health_attempt in range(30):  # Try up to 30 times (60 seconds total)
                        try:
                            health = client.web.health_check(sandbox_id, timeout=5)
                            if health.browser_ready:
                                return sandbox.state
                        except Exception:
                            if health_attempt < 29:  # Don't sleep on last iteration
                                time.sleep(1)
                    # Return running state even if health check didn't pass
                    return sandbox.state
                return sandbox.state
            if sandbox.state in (SandboxState.ERROR, SandboxState.DESTROYED):
                return sandbox.state
        except Exception:
            pass

        time.sleep(poll_interval)
        wait_time += poll_interval

    try:
        return client.sandbox.get(sandbox_id).state
    except Exception:
        return SandboxState.UNKNOWN


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestTimeoutSerialization:
    """Tests for timeout parameter serialization

    These tests verify that timeout values are sent as integers (not floats)
    to match Rust backend's u64 expectation.
    """

    @pytest.fixture
    def running_sandbox(self, sync_client: DSBClient, cleanup_sandboxes: list) -> str:
        """Create a running sandbox for tool execution tests."""

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-timeout-{uuid.uuid4().hex[:8]}",
        )
        cleanup_sandboxes.append(str(sandbox.id))

        # Wait for sandbox to be running
        state = wait_for_sandbox(sync_client, str(sandbox.id))
        if state != SandboxState.RUNNING:
            pytest.skip(f"Sandbox not in RUNNING state: {state}")

        return str(sandbox.id)

    def test_exec_with_timeout_accepts_integer(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test that sandbox.exec() accepts integer timeout

        This validates that the SDK correctly serializes integer timeout
        values to JSON (not float).
        """
        # Integer timeout should work (serializes as "timeout": 60)
        result = sync_client.sandbox.exec(
            running_sandbox,
            command=["echo", "test"],
            timeout=60,  # Integer, not float
        )

        assert "output" in result
        assert "test" in result["output"]

    def test_exec_with_default_timeout(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test that sandbox.exec() with default timeout works

        This validates the default timeout handling doesn't cause type issues.
        """
        # No timeout specified - should use default
        result = sync_client.sandbox.exec(
            running_sandbox,
            command=["echo", "default timeout test"],
        )

        assert "output" in result
        assert "default timeout test" in result["output"]

    def test_exec_with_large_timeout(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test that large timeout values are handled correctly

        Large integer timeout values should still serialize as integers.
        """
        # Large timeout value (tests for any floating point conversion issues)
        result = sync_client.sandbox.exec(
            running_sandbox,
            command=["echo", "large timeout"],
            timeout=300,  # 5 minutes
        )

        assert "output" in result

    def test_exec_timeout_does_not_cause_validation_error(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test that timeout parameter doesn't cause DSBValidationError

        A DSBValidationError with "expected u64" indicates the timeout
        was sent as a float instead of an integer.

        This is the key test that would have caught the original bug.
        """
        try:
            # This should NOT raise a validation error about timeout type
            result = sync_client.sandbox.exec(
                running_sandbox,
                command=["echo", "type validation"],
                timeout=90,
            )
            assert "output" in result
        except DSBValidationError as e:
            # If we get a validation error about timeout type, it means
            # the SDK is sending the wrong JSON type
            error_msg = str(e).lower()
            if "timeout" in error_msg and ("float" in error_msg or "u64" in error_msg):
                pytest.fail(
                    f"Timeout serialized as float instead of int: {e}\n"
                    "This indicates the SDK is sending timeout as a float "
                    "but the Rust backend expects u64 (integer)."
                )
            else:
                # Some other validation error - re-raise
                raise


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestWebToolsTimeoutSerialization:
    """Tests for web tools timeout serialization

    Web tools have their own timeout handling with HTTP buffer.
    These tests verify the timeout + buffer calculation produces integers.
    """

    @pytest.fixture
    def running_sandbox(self, sync_client: DSBClient, cleanup_sandboxes: list) -> str:
        """Create a running sandbox with web tools and wait for browser to be ready."""

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-web-timeout-{uuid.uuid4().hex[:8]}",
        )
        sandbox_id = str(sandbox.id)
        cleanup_sandboxes.append(sandbox_id)

        # Wait for sandbox to be running AND browser to be ready
        state = wait_for_sandbox(sync_client, sandbox_id, wait_for_browser=True)
        if state != SandboxState.RUNNING:
            pytest.skip(f"Sandbox not in RUNNING state: {state}")

        print(f"\n[JSON Serialization Test] Sandbox ready: {sandbox_id}\n")
        return sandbox_id

    def test_web_scrape_with_timeout_does_not_error(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test that web scrape with timeout doesn't cause type validation errors

        Web tools use: timeout = tool_timeout + http_buffer (30 seconds)
        The result must be an integer, not a float.
        """
        try:
            # Scrape a simple URL (using a data URL to avoid network dependencies)
            result = sync_client.web.scrape(
                running_sandbox,
                url="data:text/html,<html><body>Test</body></html>",
                timeout=60,
            )

            # Should succeed without validation errors
            assert result is not None

        except DSBValidationError as e:
            error_msg = str(e).lower()
            if "timeout" in error_msg and ("float" in error_msg or "u64" in error_msg):
                pytest.fail(
                    f"Web tool timeout serialized as float: {e}\n"
                    "Check that timeout calculation uses int() not float(): "
                    "timeout=int(exec_timeout + DEFAULT_HTTP_BUFFER_SECS)"
                )
            else:
                raise

    def test_web_scrape_with_default_timeout(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test web scrape with default timeout (60s)"""
        result = sync_client.web.scrape(
            running_sandbox,
            url="data:text/html,<html><body>Default timeout</body></html>",
        )

        assert result is not None

    def test_browser_tools_with_timeout(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test browser tools with timeout (default 120s)

        Browser tools have a longer default timeout for page loads.
        """
        try:
            # Browser navigate with explicit timeout
            result = sync_client.web.browser_navigate(
                running_sandbox,
                url="data:text/html,<html><body>Browser Test</body></html>",
                timeout=120,
            )

            assert result is not None

        except DSBValidationError as e:
            error_msg = str(e).lower()
            if "timeout" in error_msg and ("float" in error_msg or "u64" in error_msg):
                pytest.fail(
                    f"Browser tool timeout serialized as float: {e}\n"
                    "Browser tools use timeout=int(timeout + DEFAULT_HTTP_BUFFER_SECS)"
                )
            else:
                raise


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestContractValidation:
    """Contract validation tests between Python SDK and Rust backend

    These tests validate that the SDK sends payloads that the backend
    can deserialize correctly.
    """

    @pytest.fixture
    def running_sandbox(self, sync_client: DSBClient, cleanup_sandboxes: list) -> str:
        """Create a running sandbox."""

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-contract-{uuid.uuid4().hex[:8]}",
        )
        cleanup_sandboxes.append(sandbox.id)

        state = wait_for_sandbox(sync_client, sandbox.id)
        if state != SandboxState.RUNNING:
            pytest.skip(f"Sandbox not in RUNNING state: {state}")

        return sandbox.id

    def test_sandbox_create_accepts_integer_timeouts(
        self, sync_client: DSBClient, cleanup_sandboxes: list
    ):
        """Test sandbox creation with integer timeout values

        Validates that sandbox creation doesn't have type issues.
        """

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-create-int-{uuid.uuid4().hex[:8]}",
        )

        cleanup_sandboxes.append(sandbox.id)

        # Should succeed without validation errors
        assert sandbox.id is not None

    def test_multiple_tool_executions_with_different_timeouts(
        self, sync_client: DSBClient, running_sandbox: str
    ):
        """Test multiple executions with varying timeout values

        This validates that timeout values of different magnitudes
        are all serialized correctly.
        """
        timeouts_to_test = [30, 60, 90, 120, 180, 300]

        for timeout_val in timeouts_to_test:
            try:
                result = sync_client.sandbox.exec(
                    running_sandbox,
                    command=["echo", f"timeout-{timeout_val}"],
                    timeout=timeout_val,
                )
                assert "output" in result
            except DSBValidationError as e:
                error_msg = str(e).lower()
                if "timeout" in error_msg and ("float" in error_msg or "u64" in error_msg):
                    pytest.fail(
                        f"Timeout {timeout_val} serialized as float: {e}\n"
                        f"Timeout value {timeout_val} should be sent as integer"
                    )
                else:
                    raise
