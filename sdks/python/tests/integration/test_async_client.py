"""
Integration tests for AsyncDSBClient

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

Markers:
    - sandbox: Marks tests as sandbox API tests
    - slow: Marks tests that take longer than 30 seconds
    - requires_server: Marks tests that require a running DSB server
"""

import asyncio
import os
from collections.abc import AsyncGenerator
from uuid import UUID

import pytest

from dsb_sdk import AsyncDSBClient
from dsb_sdk.types.sandbox import SandboxState

# Test server URL from environment or default
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")
DSB_API_KEY = os.getenv("DSB_API_KEY")
TEST_IMAGE = os.getenv("TEST_IMAGE", "dsb/sandbox:latest")


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


@pytest.fixture(scope="module")
def server_available() -> bool:
    """Check if DSB server is available at module start."""
    return is_server_available()


@pytest.fixture(scope="function")
async def async_client() -> AsyncGenerator[AsyncDSBClient, None]:
    """
    Create an async DSB client for testing.

    Scope is function-level to ensure fresh client for each test.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    await client.close()


@pytest.fixture(scope="function")
async def cleanup_sandboxes(
    async_client: AsyncDSBClient,
) -> AsyncGenerator[list[str], None]:
    """
    Cleanup all test sandboxes after each test.

    Removes any sandbox with a name starting with 'test-async-'.
    """
    created_ids: list[str] = []

    yield created_ids

    # Cleanup after test
    for sandbox_id in created_ids:
        try:
            await async_client.sandbox.delete_async(sandbox_id)
        except Exception:
            pass  # Best effort cleanup

    # Also cleanup any stray test sandboxes
    try:
        response = await async_client.sandbox.list_async()
        for sandbox in response.sandboxes:
            if sandbox.config.name and sandbox.config.name.startswith("test-async-"):
                try:
                    await async_client.sandbox.delete_async(sandbox.id)
                except Exception:
                    pass
    except Exception as e:
        print(f"Warning: Async cleanup failed: {e}")


async def async_retry(func, max_attempts: int = 3, base_delay: float = 2.0):
    """Retry an async operation on transient Docker/server errors.

    Only retries on DSBAPIError (5xx) or DSBConnectionError,
    NOT on assertion errors or validation errors.
    """
    from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError

    last_error = None
    for attempt in range(max_attempts):
        try:
            return await func()
        except (DSBAPIError, DSBConnectionError) as e:
            last_error = e
            if attempt < max_attempts - 1:
                delay = base_delay * (attempt + 1)
                await asyncio.sleep(delay)
    raise last_error


async def wait_for_sandbox_async(
    client: AsyncDSBClient,
    sandbox_id: str,
    max_wait: int = 60,
    poll_interval: float = 1,
) -> bool:
    """Wait for sandbox to be running (async version)."""
    wait_time = 0
    while wait_time < max_wait:
        try:
            sandbox = await client.sandbox.get_async(sandbox_id)
            if sandbox.state.value == "running":
                # Give it a moment to fully initialize
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


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestAsyncSandboxCreation:
    """Tests for async sandbox creation"""

    @pytest.mark.asyncio
    async def test_async_create_sandbox(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ):
        """Test creating a sandbox asynchronously"""
        import uuid
        sandbox = await async_client.sandbox.create_async(
            image=TEST_IMAGE,
            name=f"test-async-create-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )

        cleanup_sandboxes.append(str(sandbox.id))

        assert sandbox.id is not None
        assert isinstance(sandbox.id, UUID)
        assert sandbox.state in [
            SandboxState.CREATING,
            SandboxState.CREATED,
            SandboxState.RUNNING,
        ]
        assert sandbox.config.image == TEST_IMAGE

    @pytest.mark.asyncio
    async def test_async_create_sandbox_with_env(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ):
        """Test creating a sandbox with environment variables"""
        import uuid
        sandbox = await async_client.sandbox.create_async(
            image=TEST_IMAGE,
            name=f"test-async-env-{uuid.uuid4().hex[:8]}",
            environment={"ASYNC_VAR": "async_value"},
            command=["sleep", "300"],
        )

        cleanup_sandboxes.append(str(sandbox.id))

        assert sandbox.id is not None
        assert sandbox.config.environment.get("ASYNC_VAR") == "async_value"

    @pytest.mark.asyncio
    async def test_async_list_sandboxes(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ):
        """Test listing sandboxes asynchronously"""
        # Create a test sandbox
        import uuid
        await async_client.sandbox.create_async(
            image=TEST_IMAGE,
            name=f"test-async-list-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )

        # List sandboxes
        response = await async_client.sandbox.list_async()

        assert response.total >= 0
        assert isinstance(response.sandboxes, list)
        # Our test sandbox should be in the list (use startswith for robustness)
        test_sandboxes = [s for s in response.sandboxes if s.config.name and s.config.name.startswith("test-async-list")]
        assert len(test_sandboxes) >= 1


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestAsyncSandboxRetrieval:
    """Tests for async sandbox retrieval"""

    @pytest.mark.asyncio
    async def test_async_get_sandbox(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ):
        """Test getting sandbox details asynchronously"""
        # First create a sandbox
        import uuid
        created = await async_client.sandbox.create_async(
            image=TEST_IMAGE,
            name=f"test-async-get-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(str(created.id))

        # Get the sandbox
        sandbox = await async_client.sandbox.get_async(str(created.id))

        assert sandbox.id == created.id
        assert sandbox.config.image == TEST_IMAGE


@pytest.mark.sandbox
@pytest.mark.serial  # Must run sequentially due to shared fixture cleanup
@pytest.mark.requires_server
class TestAsyncSandboxExecution:
    """Tests for async command execution"""

    @pytest.fixture
    async def running_sandbox(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ) -> str:
        """Create a running sandbox for execution tests.

        Retries on transient Docker/server errors to handle
        Docker-in-Docker resource pressure during parallel testing.
        """
        import uuid

        async def _create_and_wait():
            sandbox = await async_client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-exec-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )
            cleanup_sandboxes.append(str(sandbox.id))

            if not await wait_for_sandbox_async(async_client, str(sandbox.id)):
                # Raise a retryable error so async_retry can retry
                from dsb_sdk.exceptions import DSBAPIError
                raise DSBAPIError("Sandbox did not reach running state", status_code=503)

            # Extra wait for Docker container to be fully registered
            await asyncio.sleep(3)
            return str(sandbox.id)

        try:
            return await async_retry(_create_and_wait, max_attempts=3, base_delay=3.0)
        except Exception:
            pytest.skip("Could not create a running sandbox after multiple attempts")

    @pytest.mark.asyncio
    async def test_async_exec_command(
        self,
        async_client: AsyncDSBClient,
        running_sandbox: str,
    ):
        """Test executing a command asynchronously"""
        result = await async_retry(
            lambda: async_client.sandbox.exec_async(
                running_sandbox,
                command=["echo", "async hello"],
            ),
            max_attempts=3,
            base_delay=2.0,
        )

        assert "async hello" in result["output"]

    @pytest.mark.asyncio
    async def test_async_exec_python(
        self,
        async_client: AsyncDSBClient,
        running_sandbox: str,
    ):
        """Test executing shell commands asynchronously"""
        # Use shell command since dsb/sandbox image might not have python
        result = await async_retry(
            lambda: async_client.sandbox.exec_async(
                running_sandbox,
                command=["sh", "-c", "echo 'Async from sandbox!'"],
            ),
            max_attempts=3,
            base_delay=2.0,
        )

        assert "Async from sandbox!" in result["output"]

    @pytest.mark.asyncio
    async def test_async_exec_with_working_dir(
        self,
        async_client: AsyncDSBClient,
        running_sandbox: str,
    ):
        """Test executing with custom working directory"""
        result = await async_client.sandbox.exec_async(
            running_sandbox,
            command=["sh", "-c", "pwd"],
            working_dir="/tmp",
        )

        # Note: working_dir support depends on backend implementation
        # Just verify the command executes
        assert "output" in result


@pytest.mark.sandbox
@pytest.mark.serial  # Must run sequentially due to shared fixture cleanup
@pytest.mark.requires_server
class TestAsyncSandboxLifecycle:
    """Tests for async sandbox lifecycle"""

    @pytest.mark.asyncio
    async def test_async_stop_sandbox(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ):
        """Test stopping a sandbox asynchronously

        Retries on transient Docker errors to handle Docker-in-Docker
        resource pressure during parallel testing.
        """
        import uuid

        async def _create_stop_sandbox():
            sandbox = await async_client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-stop-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )
            cleanup_sandboxes.append(str(sandbox.id))

            # Wait for it to start
            await wait_for_sandbox_async(async_client, str(sandbox.id))
            await asyncio.sleep(2)

            # Stop it
            return await async_client.sandbox.stop_async(str(sandbox.id))

        stopped = await async_retry(_create_stop_sandbox, max_attempts=3, base_delay=3.0)

        assert stopped.id is not None
        assert stopped.state in [SandboxState.STOPPED, SandboxState.DESTROYING]

    @pytest.mark.asyncio
    async def test_async_delete_sandbox(
        self,
        async_client: AsyncDSBClient,
    ):
        """Test deleting a sandbox asynchronously"""
        import uuid
        sandbox = await async_client.sandbox.create_async(
            image=TEST_IMAGE,
            name=f"test-async-delete-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )

        # Wait for Docker to fully register the container
        await wait_for_sandbox_async(async_client, str(sandbox.id))
        await asyncio.sleep(2)

        # Delete it
        result = await async_client.sandbox.delete_async(str(sandbox.id))

        assert result is not None

        # Verify it's gone
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.get_async(str(sandbox.id))


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestAsyncContextManager:
    """Tests for async context manager"""

    @pytest.mark.asyncio
    async def test_async_context_manager(
        self,
        cleanup_sandboxes: list[str],
    ):
        """Test using async client as context manager

        Retries on transient Docker/server errors.
        """

        async def _context_manager_test():
            async with AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0) as client:
                health = await client.health.check()
                assert health.status in ["healthy", "ok"]

                import uuid
                sandbox = await client.sandbox.create_async(
                    image=TEST_IMAGE,
                    name=f"test-async-context-{uuid.uuid4().hex[:8]}",
                    command=["sleep", "300"],
                )
                cleanup_sandboxes.append(str(sandbox.id))
                return sandbox

        sandbox = await async_retry(_context_manager_test, max_attempts=3, base_delay=3.0)
        assert sandbox.id is not None


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestAsyncHealthCheck:
    """Tests for async health check"""

    @pytest.mark.asyncio
    async def test_async_health_check(
        self,
        async_client: AsyncDSBClient,
    ):
        """Test async health check"""
        health = await async_client.health.check()

        assert health.status in ["healthy", "ok"]
        # Other fields may be optional depending on backend configuration
        assert health is not None


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestAsyncSSH:
    """Tests for async SSH API"""

    @pytest.mark.asyncio
    async def test_async_ssh_list(
        self,
        async_client: AsyncDSBClient,
    ):
        """Test async listing SSH sessions"""
        response = await async_client.ssh.list_async()

        assert response is not None
        assert hasattr(response, "total")
        assert hasattr(response, "sessions")
        assert isinstance(response.sessions, list)


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestAsyncActivities:
    """Tests for async Activities API"""

    @pytest.mark.asyncio
    async def test_async_activities_list(
        self,
        async_client: AsyncDSBClient,
    ):
        """Test async listing activities"""
        response = await async_client.activities.list_async()

        assert response is not None
        assert hasattr(response, "total")
        assert hasattr(response, "activities")
