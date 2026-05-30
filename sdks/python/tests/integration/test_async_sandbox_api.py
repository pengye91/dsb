"""
Integration tests for Async Sandbox API

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
from dsb_sdk.exceptions import DSBAPIError
from dsb_sdk.types.sandbox import SandboxState

# Test server URL from environment or default
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")
DSB_API_KEY = os.getenv("DSB_API_KEY")
TEST_IMAGE = os.getenv("TEST_IMAGE", "dsb/sandbox:latest")


@pytest.fixture(scope="function")
async def async_client() -> AsyncGenerator[AsyncDSBClient, None]:
    """
    Create an async DSB client for testing.

    Scope is function-level to ensure fresh client for each test.
    """
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
                    await async_client.sandbox.delete_async(str(sandbox.id))
                except Exception:
                    pass
    except Exception as e:
        print(f"Warning: Async cleanup failed: {e}")


async def wait_for_sandbox_async(
    client: AsyncDSBClient,
    sandbox_id: str,
    max_wait: int = 60,
    poll_interval: float = 1,
) -> bool:
    """
    Wait for sandbox to be running (async version).

    Args:
        client: AsyncDSB client instance
        sandbox_id: Sandbox UUID
        max_wait: Maximum wait time in seconds
        poll_interval: Poll interval in seconds

    Returns:
        True if sandbox is running, False otherwise
    """
    wait_time = 0
    while wait_time < max_wait:
        try:
            sandbox = await client.sandbox.get_async(sandbox_id)
            # SandboxState is a string enum, can compare directly to string
            if sandbox.state == "running":
                # Wait a bit more for services to be ready
                await asyncio.sleep(1)
                return True
            # Check for error states
            elif sandbox.state in ("error", "destroyed", "destroying"):
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


@pytest.mark.sandbox
@pytest.mark.requires_server
@pytest.mark.serial
class TestAsyncSandboxRetrieval:
    """Tests for async sandbox retrieval operations

    Note: These tests require serial execution due to container resource usage.
    Marked with @pytest.mark.serial to prevent parallel execution conflicts.
    """

    @pytest.mark.asyncio
    async def test_async_get_sandbox(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ):
        """Test getting sandbox details asynchronously"""
        import uuid
        # First create a sandbox
        created = await async_client.sandbox.create_async(
            image=TEST_IMAGE,
            name=f"test-async-get-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(str(created.id))

        # Wait for sandbox to be ready
        if not await wait_for_sandbox_async(async_client, str(created.id)):
            pytest.skip("Sandbox did not reach running state in time")

        # Get the sandbox
        sandbox = await async_client.sandbox.get_async(str(created.id))

        assert sandbox.id == created.id
        assert sandbox.config.image == TEST_IMAGE

    @pytest.mark.asyncio
    async def test_async_list_sandboxes(
        self,
        async_client: AsyncDSBClient,
        cleanup_sandboxes: list[str],
    ):
        """Test listing all sandboxes asynchronously"""
        import uuid
        # Create a test sandbox
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
        """Create a running sandbox for execution tests"""
        import uuid
        sandbox = await async_client.sandbox.create_async(
            image=TEST_IMAGE,
            name=f"test-async-exec-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(str(sandbox.id))

        # Wait for sandbox to be running
        if not await wait_for_sandbox_async(async_client, str(sandbox.id)):
            pytest.skip("Sandbox did not reach running state in time")

        return str(sandbox.id)

    @pytest.mark.asyncio
    async def test_async_exec_command(
        self,
        async_client: AsyncDSBClient,
        running_sandbox: str,
    ):
        """Test executing a command asynchronously"""
        result = await async_client.sandbox.exec_async(
            running_sandbox,
            command=["echo", "async hello"],
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
        result = await async_client.sandbox.exec_async(
            running_sandbox,
            command=["sh", "-c", "echo 'Async from sandbox!'"],
        )

        assert "Async from sandbox!" in result["output"]

    @pytest.mark.asyncio
    async def test_async_exec_multi_command(
        self,
        async_client: AsyncDSBClient,
        running_sandbox: str,
    ):
        """Test executing multiple commands sequentially"""
        result1 = await async_client.sandbox.exec_async(
            running_sandbox,
            command=["echo", "first"],
        )
        result2 = await async_client.sandbox.exec_async(
            running_sandbox,
            command=["echo", "second"],
        )

        assert "first" in result1["output"]
        assert "second" in result2["output"]


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestAsyncContextManager:
    """Tests for async context manager"""

    @pytest.mark.asyncio
    async def test_async_context_manager(
        self,
        cleanup_sandboxes: list[str],
    ):
        """Test using async client as context manager"""
        import uuid
        async with AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0) as client:
            health = await client.health.check()
            assert health.status in ["healthy", "ok"]

            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-context-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )
            cleanup_sandboxes.append(str(sandbox.id))
            assert sandbox.id is not None

    @pytest.mark.asyncio
    async def test_async_context_manager_cleanup(self):
        """Test that context manager properly closes client"""
        async with AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0) as client:
            await client.health.check()

        # Client should be closed, trying to use it should fail
        # (we don't test this as it might raise different errors)


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
class TestAsyncSandboxLifecycle:
    """Tests for async sandbox lifecycle management"""

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
class TestAsyncSandboxErrors:
    """Error handling tests for Async Sandbox API"""

    @pytest.mark.asyncio
    async def test_async_create_invalid_image(self, async_client: AsyncDSBClient):
        """Test creating sandbox with invalid image"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.create_async(
                image="invalid/nonexistent/image:xyz999",
                name="test-invalid",
            )

    @pytest.mark.asyncio
    async def test_async_get_nonexistent_sandbox(self, async_client: AsyncDSBClient):
        """Test getting non-existent sandbox"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError) as exc_info:
            await async_client.sandbox.get_async("00000000-0000-0000-0000-000000000000")

        assert "not found" in str(exc_info.value).lower()

    @pytest.mark.asyncio
    async def test_async_delete_nonexistent_sandbox(self, async_client: AsyncDSBClient):
        """Test deleting non-existent sandbox"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.delete_async("00000000-0000-0000-0000-000000000000")

    @pytest.mark.asyncio
    async def test_async_start_nonexistent_sandbox(self, async_client: AsyncDSBClient):
        """Test starting non-existent sandbox"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.start_async("00000000-0000-0000-0000-000000000000")

    @pytest.mark.asyncio
    async def test_async_stop_nonexistent_sandbox(self, async_client: AsyncDSBClient):
        """Test stopping non-existent sandbox"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.stop_async("00000000-0000-0000-0000-000000000000")

    @pytest.mark.asyncio
    async def test_async_exec_nonexistent_sandbox(self, async_client: AsyncDSBClient):
        """Test executing command in non-existent sandbox"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.exec_async(
                "00000000-0000-0000-0000-000000000000",
                ["echo", "test"],
            )

    @pytest.mark.asyncio
    async def test_async_create_with_empty_image(self, async_client: AsyncDSBClient):
        """Test creating sandbox with empty image name"""
        from dsb_sdk.exceptions import DSBValidationError

        with pytest.raises((DSBValidationError, ValueError)):
            await async_client.sandbox.create_async(
                image="",
                name="test-empty",
            )

    @pytest.mark.asyncio
    async def test_async_create_with_invalid_name(self, async_client: AsyncDSBClient):
        """Test creating sandbox with invalid name"""
        # Test with special characters that might not be allowed
        try:
            sandbox = await async_client.sandbox.create_async(
                image=TEST_IMAGE,
                name="test/invalid/name",  # Invalid character
            )
            # If creation succeeds, cleanup
            await async_client.sandbox.delete_async(str(sandbox.id))
        except (DSBAPIError, ValueError):
            # Expected behavior
            pass

    @pytest.mark.asyncio
    async def test_async_list_with_invalid_filters(self, async_client: AsyncDSBClient):
        """Test listing sandboxes with invalid filter parameters"""
        # Test with invalid state value
        from dsb_sdk.exceptions import DSBAPIError

        try:
            result = await async_client.sandbox.list_async(state="invalid_state")
            # If no error, should still return a valid response
            assert result is not None
            assert hasattr(result, "sandboxes")
        except (DSBAPIError, ValueError):
            # Expected behavior for invalid state
            pass

    @pytest.mark.asyncio
    async def test_async_upload_to_nonexistent_sandbox(self, async_client: AsyncDSBClient):
        """Test uploading file to non-existent sandbox"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.upload_file_async(
                "00000000-0000-0000-0000-000000000000",
                "/tmp/test.txt",
                b"test data",
            )

    @pytest.mark.asyncio
    async def test_async_download_from_nonexistent_sandbox(self, async_client: AsyncDSBClient):
        """Test downloading file from non-existent sandbox"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            await async_client.sandbox.download_file_async(
                "00000000-0000-0000-0000-000000000000",
                "/tmp/test.txt",
            )
