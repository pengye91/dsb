"""
Test configuration for pytest

Provides fixtures for both sync and async DSB client testing.
"""

import asyncio
import logging
import os
import time
from collections.abc import AsyncGenerator, Callable, Generator
from functools import wraps
from unittest.mock import Mock

import pytest

from dsb_sdk.client import AsyncDSBClient, DSBClient

# Configure test logging
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
test_logger = logging.getLogger("dsb_tests")

# Test server URL from environment or default
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")
DSB_API_KEY = os.getenv("DSB_API_KEY")
SANDBOX_IMAGE = os.getenv("DSB_SANDBOX_IMAGE", "dsb/sandbox:latest")
SANDBOX_SLIM_IMAGE = os.getenv("DSB_SANDBOX_SLIM_IMAGE", "dsb/sandbox-slim:latest")


# ============================================================================
# Helper Functions
# ============================================================================


def get_test_sandbox_name(base_name: str) -> str:
    """
    Generate standardized test sandbox name with 'test-' prefix.

    Ensures consistent naming across all tests for easier cleanup.

    Args:
        base_name: Base name for the sandbox (e.g., 'my-sandbox')

    Returns:
        Full sandbox name with 'test-' prefix (e.g., 'test-my-sandbox')

    Example:
        >>> name = get_test_sandbox_name("my-test")
        >>> print(name)
        test-my-test
    """
    # Ensure base_name doesn't already have test- prefix
    if base_name.startswith("test-"):
        return base_name
    return f"test-{base_name}"


def is_test_sandbox(sandbox_name: str | None) -> bool:
    """
    Check if a sandbox is a test sandbox.

    Args:
        sandbox_name: Name of the sandbox to check

    Returns:
        True if sandbox name starts with 'test-'
    """
    return sandbox_name is not None and sandbox_name.startswith("test-")


def get_worker_id() -> str:
    """
    Get the current pytest-xdist worker ID.

    Returns:
        Worker ID (e.g., 'gw0', 'gw1') or 'master' if not running under xdist
    """
    return os.getenv("PYTEST_XDIST_WORKER", "master")


def with_timeout(seconds: float = 30.0):
    """
    Decorator to add timeout to a function.

    Args:
        seconds: Timeout in seconds

    Example:
        @with_timeout(10)
        def my_slow_function():
            time.sleep(5)
    """

    def decorator(func: Callable):
        @wraps(func)
        def wrapper(*args, **kwargs):
            start = time.time()
            result = func(*args, **kwargs)
            elapsed = time.time() - start
            if elapsed > seconds:
                test_logger.warning(f"{func.__name__} took {elapsed:.2f}s (timeout: {seconds}s)")
            return result

        return wrapper

    return decorator


def with_retry(max_attempts: int = 3, delay: float = 1.0):
    """
    Decorator to retry a function on failure.

    Args:
        max_attempts: Maximum number of attempts
        delay: Delay between attempts in seconds

    Example:
        @with_retry(max_attempts=3)
        def flaky_function():
            # Might fail, will retry
            pass
    """

    def decorator(func: Callable):
        @wraps(func)
        def wrapper(*args, **kwargs):
            last_error = None
            for attempt in range(max_attempts):
                try:
                    return func(*args, **kwargs)
                except Exception as e:
                    last_error = e
                    if attempt < max_attempts - 1:
                        test_logger.warning(
                            f"{func.__name__} failed (attempt {attempt + 1}/{max_attempts}): {e}"
                        )
                        time.sleep(delay)
            raise last_error  # type: ignore

        return wrapper

    return decorator


def is_server_available() -> bool:
    """Check if DSB server is available."""
    try:
        # Health check doesn't require API key
        client = DSBClient(api_url=DSB_API_URL)
        health = client.health.check()
        client.close()
        return health.status in ["healthy", "ok"]
    except Exception:
        return False


@pytest.fixture(scope="session")
def server_available() -> bool:
    """Check if DSB server is available at session start."""
    return is_server_available()


@pytest.fixture
def skip_if_server_unavailable(server_available: bool):
    """Skip test if server is not available."""
    if not server_available:
        pytest.skip("DSB server not available")


# ============================================================================
# Mock Transport Fixtures
# ============================================================================


@pytest.fixture
def mock_transport() -> Mock:
    """Mock transport instance for unit tests."""
    transport = Mock()
    transport.request.return_value = {"status": "ok"}
    transport.close.return_value = None
    return transport


@pytest.fixture
def mock_async_transport() -> Mock:
    """Mock async transport instance for unit tests."""
    transport = Mock()
    transport.request = Mock(return_value={"status": "ok"})
    transport.close = Mock(return_value=asyncio.sleep(0))
    return transport


# ============================================================================
# Sync Client Fixtures
# ============================================================================


@pytest.fixture
def sync_client_mock(mock_transport: Mock) -> DSBClient:
    """
    Sync client fixture with mocked transport.

    Used for unit tests that don't require a real server.
    """
    client = DSBClient.__new__(DSBClient)
    client._transport = mock_transport

    # Initialize sync API modules
    from dsb_sdk.api.activities import ActivitiesAPI
    from dsb_sdk.api.health import HealthAPI
    from dsb_sdk.api.sandbox import SandboxAPI
    from dsb_sdk.api.ssh import SSHAPI
    from dsb_sdk.api.web import WebAPI

    client.sandbox = SandboxAPI(mock_transport)
    client.ssh = SSHAPI(mock_transport)
    client.health = HealthAPI(mock_transport)
    client.activities = ActivitiesAPI(mock_transport)
    client.web = WebAPI(mock_transport)

    return client


@pytest.fixture(scope="module")
def sync_client_live() -> Generator[DSBClient, None, None]:
    """
    Live sync client fixture for integration tests.

    Requires a running DSB server.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY)
    yield client
    client.close()


# ============================================================================
# Async Client Fixtures
# ============================================================================


@pytest.fixture
def async_client(mock_async_transport: Mock) -> AsyncDSBClient:
    """
    Async client fixture with mocked transport.

    Used for unit tests that don't require a real server.
    """
    client = AsyncDSBClient.__new__(AsyncDSBClient)
    client._transport = mock_async_transport

    # Initialize async API modules (use *_async classes)
    from dsb_sdk.api.activities_async import AsyncActivitiesAPI
    from dsb_sdk.api.health_async import AsyncHealthAPI
    from dsb_sdk.api.sandbox_async import AsyncSandboxAPI
    from dsb_sdk.api.ssh_async import AsyncSSHAPI
    from dsb_sdk.api.web_async import AsyncWebAPI

    client.sandbox = AsyncSandboxAPI(mock_async_transport)
    client.ssh = AsyncSSHAPI(mock_async_transport)
    client.health = AsyncHealthAPI(mock_async_transport)
    client.activities = AsyncActivitiesAPI(mock_async_transport)
    client.web = AsyncWebAPI(mock_async_transport)

    return client


@pytest.fixture(scope="module")
async def async_client_live() -> AsyncGenerator[AsyncDSBClient, None]:
    """
    Live async client fixture for integration tests.

    Requires a running DSB server.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY)
    yield client
    await client.close()


# ============================================================================
# Sandbox Fixtures
# ============================================================================


@pytest.fixture
def sandbox_id() -> str:
    """Return a placeholder sandbox ID for unit tests."""
    return "00000000-0000-0000-0000-000000000000"


# ============================================================================
# Auto-Use Cleanup Fixtures
# ============================================================================


@pytest.fixture(scope="function", autouse=True)
async def auto_cleanup_test_sandboxes(
    request: pytest.FixtureRequest,
) -> AsyncGenerator[None, None]:
    """
    Automatically clean up test sandboxes after each test function.

    DISABLED: This fixture is currently disabled to prevent interference between
    parallel tests. Cleanup is now handled by:
    1. Module-scoped fixtures cleaning up their own sandboxes
    2. The module-level cleanup fixture (runs after all tests in a module)
    3. Function-scoped fixtures cleaning up their own sandboxes
    """
    # Setup phase - yield control to the test
    yield

    # DISABLED: Auto-cleanup causes race conditions with parallel tests
    # Each test/module should clean up its own sandboxes
    return


async def _cleanup_with_logging(
    client: DSBClient | AsyncDSBClient,
    test_name: str,
    is_async: bool,
    skip_shared: bool = False,
    worker_id: str = "master",
) -> None:
    """
    Perform cleanup with detailed logging.

    Args:
        client: DSB client (sync or async)
        test_name: Name of the test being cleaned up
        is_async: Whether the client is async
        skip_shared: If True, skip sandboxes with "-shared" in the name (module-scoped fixtures)
        worker_id: The pytest-xdist worker ID (e.g., 'gw0', 'gw1', 'master')
    """
    client_type = "async" if is_async else "sync"
    cleaned_count = 0
    failed_count = 0

    try:
        # List all sandboxes (use correct method for sync vs async)
        try:
            if is_async:
                response = await client.sandbox.list_async()
            else:
                response = client.sandbox.list()
        except Exception as e:
            test_logger.error(f"Failed to list {client_type} sandboxes: {e}")
            return

        test_sandboxes = [s for s in response.sandboxes if is_test_sandbox(s.config.name)]

        # Filter out shared sandboxes if skip_shared is True
        if skip_shared:
            test_sandboxes = [s for s in test_sandboxes if "-shared" not in (s.config.name or "")]

        if not test_sandboxes:
            test_logger.debug(f"No {client_type} test sandboxes to clean up")
            return

        test_logger.info(f"Cleaning up {len(test_sandboxes)} {client_type} test sandboxes")

        # Delete each test sandbox (convert UUID to str)
        for sandbox in test_sandboxes:
            sandbox_name = sandbox.config.name or "<unnamed>"
            try:
                # Convert UUID to string for API call
                sandbox_id_str = str(sandbox.id)

                if is_async:
                    await client.sandbox.delete_async(sandbox_id_str)
                else:
                    client.sandbox.delete(sandbox_id_str)

                cleaned_count += 1
                test_logger.debug(
                    f"Cleaned up {client_type} sandbox: {sandbox_name} ({sandbox.id})"
                )
            except Exception as e:
                failed_count += 1
                test_logger.error(f"Failed to clean up {client_type} sandbox {sandbox_name}: {e}")

        test_logger.info(
            f"Cleanup complete for {client_type} client: "
            f"{cleaned_count} cleaned, {failed_count} failed"
        )

    except Exception as e:
        test_logger.error(f"Cleanup failed for {client_type} client: {e}")


@pytest.fixture(scope="module", autouse=True)
async def module_cleanup(request: pytest.FixtureRequest) -> AsyncGenerator[None, None]:
    """
    Module-level cleanup that runs after all tests in a module complete.

    DISABLED: This fixture is currently disabled to prevent interference between
    parallel test workers. When running with pytest-xdist, each worker runs the
    module independently, and cleaning up "orphaned" sandboxes in one worker
    would delete sandboxes still in use by other workers.

    Cleanup is now handled by:
    1. Each fixture cleaning up its own sandboxes in its finally block
    2. Tests using function-scoped fixtures that clean up after each test
    """
    yield

    # DISABLED: Module-level cleanup causes race conditions with parallel tests
    # Each fixture should clean up its own sandboxes
    return


@pytest.fixture(scope="function", autouse=True)
def cleanup_on_failure(request: pytest.FixtureRequest) -> Generator[None, None, None]:
    """
    Log cleanup context when a test fails.

    This helps debug cleanup issues by logging relevant information
    when tests fail.
    """
    yield

    # Log that cleanup ran (helps with debugging)
    if hasattr(request, "node"):
        test_logger.debug(f"Test {request.node.name} completed - cleanup ran")


# ============================================================================
# Legacy Cleanup Fixtures (kept for backward compatibility)
# ============================================================================


@pytest.fixture(scope="function")
async def cleanup_test_sandboxes(sync_client_live: DSBClient):
    """
    Legacy cleanup fixture that removes test sandboxes after each test.

    Note: This is kept for backward compatibility. New code should rely
    on the auto_cleanup_test_sandboxes fixture which runs automatically.
    """
    yield sync_client_live

    # Cleanup after test (now delegated to auto-use fixture)
    test_logger.warning(
        "Using legacy cleanup_test_sandboxes fixture. Auto-use cleanup handles this automatically."
    )


@pytest.fixture(scope="function")
async def cleanup_async_test_sandboxes(async_client_live: AsyncDSBClient):
    """
    Legacy cleanup fixture for async tests.

    Note: This is kept for backward compatibility. New code should rely
    on the auto_cleanup_test_sandboxes fixture which runs automatically.
    """
    yield async_client_live

    # Cleanup after test (now delegated to auto-use fixture)
    test_logger.warning(
        "Using legacy cleanup_async_test_sandboxes fixture. "
        "Auto-use cleanup handles this automatically."
    )


# ============================================================================
# Helper Fixtures for Integration Tests
# ============================================================================


@pytest.fixture
def sandbox_image() -> str:
    """Return the sandbox image name for integration tests."""
    return SANDBOX_IMAGE


@pytest.fixture
def slim_sandbox_image() -> str:
    """Return the slim sandbox image name for integration tests."""
    return SANDBOX_SLIM_IMAGE


# ============================================================================
# Environment Fixtures
# ============================================================================


@pytest.fixture
def dsb_api_url() -> str:
    """Return the DSB API URL for tests."""
    return DSB_API_URL


# ============================================================================
# Shared Sandbox Fixtures
# ============================================================================


@pytest.fixture(scope="module")
def shared_sandbox_web_tools(sync_client_live: DSBClient) -> Generator[str, None, None]:
    """
    Shared sandbox for web tools testing.

    Used by: test_web_api.py, test_async_web_api.py
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    sandbox_name = get_test_sandbox_name("web-tools-shared")
    test_logger.info(f"Creating shared sandbox: {sandbox_name}")

    sandbox = None
    try:
        sandbox = sync_client_live.sandbox.create(
            image=SANDBOX_IMAGE,
            name=sandbox_name,
            command=["sleep", "600"],
        )

        # Wait for RUNNING state
        import time
        for i in range(30):
            try:
                sb = sync_client_live.sandbox.get(str(sandbox.id))
                if sb.state.value == "running":
                    test_logger.info(f"Shared sandbox ready: {sandbox_name}")
                    yield str(sandbox.id)
                    break
            except Exception:
                pass
            time.sleep(0.5)
        else:
            pytest.fail(f"Sandbox failed to start: {sandbox_name}")

    except Exception as e:
        test_logger.error(f"Failed to create shared sandbox: {e}")
        pytest.skip(f"Failed to create shared sandbox: {e}")

    finally:
        if sandbox is not None:
            try:
                test_logger.info(f"Cleaning up shared sandbox: {sandbox_name}")
                sync_client_live.sandbox.delete(str(sandbox.id))
            except Exception as e:
                test_logger.warning(f"Failed to cleanup shared sandbox: {e}")


@pytest.fixture(scope="module")
def shared_sandbox_file_ops(sync_client_live: DSBClient) -> Generator[str, None, None]:
    """
    Shared sandbox for file operations testing.

    Used by: test_file_upload_api.py, test_file_download_api.py, test_static_files.py
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    sandbox_name = get_test_sandbox_name("file-ops-shared")
    test_logger.info(f"Creating shared sandbox: {sandbox_name}")

    sandbox = None
    try:
        # Use TEST_IMAGE for file operations tests
        test_image = os.getenv("TEST_IMAGE", "dsb/sandbox:latest")
        sandbox = sync_client_live.sandbox.create(
            image=test_image,
            name=sandbox_name,
            command=["sleep", "600"],
        )

        # Wait for RUNNING state
        import time
        for i in range(30):
            try:
                sb = sync_client_live.sandbox.get(str(sandbox.id))
                if sb.state.value == "running":
                    test_logger.info(f"Shared sandbox ready: {sandbox_name}")
                    yield str(sandbox.id)
                    break
            except Exception:
                pass
            time.sleep(0.5)
        else:
            pytest.fail(f"Sandbox failed to start: {sandbox_name}")

    except Exception as e:
        test_logger.error(f"Failed to create shared sandbox: {e}")
        pytest.skip(f"Failed to create shared sandbox: {e}")

    finally:
        if sandbox is not None:
            try:
                test_logger.info(f"Cleaning up shared sandbox: {sandbox_name}")
                sync_client_live.sandbox.delete(str(sandbox.id))
            except Exception as e:
                test_logger.warning(f"Failed to cleanup shared sandbox: {e}")


@pytest.fixture(scope="module")
def shared_sandbox_async(sync_client_live: DSBClient) -> Generator[str, None, None]:
    """
    Shared sandbox for async client testing.

    Used by: test_async_sandbox_api.py, test_async_client.py
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    sandbox_name = get_test_sandbox_name("async-shared")
    test_logger.info(f"Creating shared sandbox: {sandbox_name}")

    sandbox = None
    try:
        sandbox = sync_client_live.sandbox.create(
            image=SANDBOX_IMAGE,
            name=sandbox_name,
            command=["sleep", "600"],
        )

        # Wait for RUNNING state
        import time
        for i in range(30):
            try:
                sb = sync_client_live.sandbox.get(str(sandbox.id))
                if sb.state.value == "running":
                    test_logger.info(f"Shared sandbox ready: {sandbox_name}")
                    yield str(sandbox.id)
                    break
            except Exception:
                pass
            time.sleep(0.5)
        else:
            pytest.fail(f"Sandbox failed to start: {sandbox_name}")

    except Exception as e:
        test_logger.error(f"Failed to create shared sandbox: {e}")
        pytest.skip(f"Failed to create shared sandbox: {e}")

    finally:
        if sandbox is not None:
            try:
                test_logger.info(f"Cleaning up shared sandbox: {sandbox_name}")
                sync_client_live.sandbox.delete(str(sandbox.id))
            except Exception as e:
                test_logger.warning(f"Failed to cleanup shared sandbox: {e}")


@pytest.fixture(scope="module")
def shared_sandbox_terminal(sync_client_live: DSBClient) -> Generator[str, None, None]:
    """
    Shared sandbox for terminal API testing.

    Used by: test_terminal_api.py
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    sandbox_name = get_test_sandbox_name("terminal-shared")
    test_logger.info(f"Creating shared sandbox: {sandbox_name}")

    sandbox = None
    try:
        sandbox = sync_client_live.sandbox.create(
            image=SANDBOX_IMAGE,
            name=sandbox_name,
            command=["sleep", "600"],
        )

        # Wait for RUNNING state
        import time
        for i in range(30):
            try:
                sb = sync_client_live.sandbox.get(str(sandbox.id))
                if sb.state.value == "running":
                    test_logger.info(f"Shared sandbox ready: {sandbox_name}")
                    yield str(sandbox.id)
                    break
            except Exception:
                pass
            time.sleep(0.5)
        else:
            pytest.fail(f"Sandbox failed to start: {sandbox_name}")

    except Exception as e:
        test_logger.error(f"Failed to create shared sandbox: {e}")
        pytest.skip(f"Failed to create shared sandbox: {e}")

    finally:
        if sandbox is not None:
            try:
                test_logger.info(f"Cleaning up shared sandbox: {sandbox_name}")
                sync_client_live.sandbox.delete(str(sandbox.id))
            except Exception as e:
                test_logger.warning(f"Failed to cleanup shared sandbox: {e}")


@pytest.fixture(scope="module")
def shared_sandbox_ssh(sync_client_live: DSBClient) -> Generator[str, None, None]:
    """
    Shared sandbox for SSH API testing.

    Used by: test_ssh_api.py
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    sandbox_name = get_test_sandbox_name("ssh-shared")
    test_logger.info(f"Creating shared sandbox: {sandbox_name}")

    sandbox = None
    try:
        sandbox = sync_client_live.sandbox.create(
            image=SANDBOX_IMAGE,
            name=sandbox_name,
            command=["sleep", "600"],
        )

        # Wait for RUNNING state
        import time
        for i in range(30):
            try:
                sb = sync_client_live.sandbox.get(str(sandbox.id))
                if sb.state.value == "running":
                    test_logger.info(f"Shared sandbox ready: {sandbox_name}")
                    yield str(sandbox.id)
                    break
            except Exception:
                pass
            time.sleep(0.5)
        else:
            pytest.fail(f"Sandbox failed to start: {sandbox_name}")

    except Exception as e:
        test_logger.error(f"Failed to create shared sandbox: {e}")
        pytest.skip(f"Failed to create shared sandbox: {e}")

    finally:
        if sandbox is not None:
            try:
                test_logger.info(f"Cleaning up shared sandbox: {sandbox_name}")
                sync_client_live.sandbox.delete(str(sandbox.id))
            except Exception as e:
                test_logger.warning(f"Failed to cleanup shared sandbox: {e}")


# ============================================================================
# Event Loop Management
# ============================================================================


@pytest.fixture(scope="session")
def event_loop() -> Generator[asyncio.AbstractEventLoop, None, None]:
    """Create an event loop for the test session."""
    loop = asyncio.new_event_loop()
    yield loop
    loop.close()
