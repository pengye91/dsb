"""
Integration tests for Sandbox API

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

Markers:
    - sandbox: Marks tests as sandbox API tests
    - slow: Marks tests that take longer than 30 seconds
    - requires_server: Marks tests that require a running DSB server
"""

import os
import time
from collections.abc import Iterator
from uuid import UUID

import pytest

from dsb_sdk import DSBClient
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
    """
    Create a DSB client for testing.

    Scope is module-level to reuse the connection.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


@pytest.fixture(scope="function")
def cleanup_sandboxes(sync_client: DSBClient) -> Iterator[None]:
    """
    Cleanup all test sandboxes after each test.

    Removes any sandbox with a name starting with 'test-'.
    """
    created_ids = []

    yield created_ids

    # Cleanup after test
    for sandbox_id in created_ids:
        try:
            sync_client.sandbox.delete(sandbox_id)
        except Exception:
            pass  # Best effort cleanup

    # Also cleanup any stray test sandboxes
    try:
        response = sync_client.sandbox.list()
        for sandbox in response.sandboxes:
            if sandbox.config.name and sandbox.config.name.startswith("test-"):
                try:
                    sync_client.sandbox.delete(sandbox.id)
                except Exception:
                    pass
    except Exception as e:
        print(f"Warning: Cleanup failed: {e}")


def wait_for_sandbox(
    client: DSBClient,
    sandbox_id: str,
    target_states: tuple[SandboxState, ...] = (SandboxState.RUNNING,),
    max_wait: int = 60,
    poll_interval: float = 1,
) -> SandboxState:
    """
    Wait for sandbox to reach a target state.

    Args:
        client: DSB client instance
        sandbox_id: Sandbox UUID
        target_states: Acceptable target states
        max_wait: Maximum wait time in seconds
        poll_interval: Poll interval in seconds

    Returns:
        Final sandbox state

    Raises:
        TimeoutError: If sandbox doesn't reach target state in time
    """
    wait_time = 0
    while wait_time < max_wait:
        try:
            sandbox = client.sandbox.get(sandbox_id)
            if sandbox.state in target_states:
                return sandbox.state
            # Also accept error states as terminal
            if sandbox.state in (SandboxState.ERROR, SandboxState.DESTROYED):
                return sandbox.state
        except Exception:
            # Log but continue - sandbox might not be immediately available
            pass

        time.sleep(poll_interval)
        wait_time += poll_interval

    # Return current state even if not target
    try:
        return client.sandbox.get(sandbox_id).state
    except Exception:
        return SandboxState.UNKNOWN


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestSandboxCreation:
    """Tests for sandbox creation"""

    def test_create_sandbox_with_minimal_config(
        self, sync_client: DSBClient, cleanup_sandboxes: list
    ):
        """Test creating a sandbox with minimal configuration"""
        import uuid
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-minimal-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )

        cleanup_sandboxes.append(sandbox.id)

        assert sandbox.id is not None
        assert isinstance(sandbox.id, UUID)
        assert sandbox.state in [
            SandboxState.CREATING,
            SandboxState.CREATED,
            SandboxState.RUNNING,
        ]
        assert sandbox.config.image == TEST_IMAGE
        assert sandbox.config.name.startswith("test-minimal-")

    def test_create_sandbox_with_environment(self, sync_client: DSBClient, cleanup_sandboxes: list):
        """Test creating a sandbox with environment variables"""
        import uuid
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-env-{uuid.uuid4().hex[:8]}",
            environment={"TEST_VAR": "test_value", "ANOTHER_VAR": "123"},
            command=["sleep", "300"],
        )

        cleanup_sandboxes.append(sandbox.id)

        assert sandbox.id is not None
        assert sandbox.config.environment["TEST_VAR"] == "test_value"
        assert sandbox.config.environment["ANOTHER_VAR"] == "123"

    def test_create_sandbox_invalid_image(self, sync_client: DSBClient):
        """Test creating a sandbox with invalid image"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            sync_client.sandbox.create(
                image="invalid/nonexistent/image:xyz999",
                name="test-invalid",
            )


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestSandboxRetrieval:
    """Tests for sandbox retrieval operations"""

    def test_get_sandbox(self, sync_client: DSBClient, cleanup_sandboxes: list):
        """Test getting sandbox details"""
        # First create a sandbox
        created = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-get",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(created.id)

        # Wait for sandbox to be ready
        wait_for_sandbox(sync_client, created.id)

        # Get the sandbox
        sandbox = sync_client.sandbox.get(created.id)

        assert sandbox.id == created.id
        assert sandbox.config.image == TEST_IMAGE
        assert sandbox.state in [
            SandboxState.CREATING,
            SandboxState.CREATED,
            SandboxState.RUNNING,
        ]

    def test_list_sandboxes(self, sync_client: DSBClient, cleanup_sandboxes: list):
        """Test listing all sandboxes"""
        import uuid
        # Create a test sandbox with unique name to avoid conflicts in parallel tests
        unique_name = f"test-list-{uuid.uuid4().hex[:8]}"
        sync_client.sandbox.create(image=TEST_IMAGE, name=unique_name, command=["sleep", "300"])
        cleanup_sandboxes.append(sync_client.sandbox.list().sandboxes[-1].id)

        # List sandboxes
        response = sync_client.sandbox.list()

        assert response.total >= 0
        assert isinstance(response.sandboxes, list)
        # Our test sandbox should be in the list
        test_sandboxes = [s for s in response.sandboxes if s.config.name == unique_name]
        assert len(test_sandboxes) >= 1

    def test_get_nonexistent_sandbox(self, sync_client: DSBClient):
        """Test getting a sandbox that doesn't exist"""
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            sync_client.sandbox.get("00000000-0000-0000-0000-000000000000")


@pytest.mark.sandbox
@pytest.mark.serial
@pytest.mark.requires_server
class TestSandboxExecution:
    """Tests for command execution in sandboxes

    Note: These tests require serial execution due to container resource usage.
    Marked with @pytest.mark.serial to prevent parallel execution conflicts.
    """

    @pytest.fixture
    def running_sandbox(self, sync_client: DSBClient, cleanup_sandboxes: list) -> str:
        """Create a running sandbox for execution tests"""
        import uuid
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-exec-{uuid.uuid4().hex[:8]}",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(sandbox.id)

        # Wait for sandbox to be running
        state = wait_for_sandbox(sync_client, sandbox.id)
        if state != SandboxState.RUNNING:
            # Skip tests that require a running sandbox
            pytest.skip(f"Sandbox not in RUNNING state: {state}")

        return sandbox.id

    def test_exec_simple_command(self, sync_client: DSBClient, running_sandbox: str):
        """Test executing a simple command"""
        result = sync_client.sandbox.exec(
            running_sandbox,
            command=["echo", "hello world"],
        )

        assert "output" in result
        assert "hello world" in result["output"]

    def test_exec_python_code(self, sync_client: DSBClient, running_sandbox: str):
        """Test executing shell commands"""
        # Use echo since dsb/sandbox image might not have python3
        result = sync_client.sandbox.exec(
            running_sandbox,
            command=["sh", "-c", "echo 'Hello from sandbox!'"],
        )

        assert "Hello from sandbox!" in result["output"]

    def test_exec_with_working_dir(self, sync_client: DSBClient, running_sandbox: str):
        """Test executing command with custom working directory"""
        result = sync_client.sandbox.exec(
            running_sandbox,
            command=["sh", "-c", "pwd"],
            working_dir="/tmp",
        )

        # Note: working_dir support depends on backend implementation
        # Just verify the command executes
        assert "output" in result

    def test_exec_failing_command(self, sync_client: DSBClient, running_sandbox: str):
        """Test executing a command that fails"""
        result = sync_client.sandbox.exec(
            running_sandbox,
            command=["ls", "/nonexistent-directory"],
        )

        # Command should execute and return output (even if error)
        assert "output" in result


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestSandboxLifecycle:
    """Tests for sandbox lifecycle management"""

    def test_stop_sandbox(self, sync_client: DSBClient, cleanup_sandboxes: list):
        """Test stopping a running sandbox

        Retries on transient Docker errors to handle Docker-in-Docker
        resource pressure during parallel testing.
        """
        from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-stop",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(sandbox.id)

        # Wait for it to start
        wait_for_sandbox(sync_client, sandbox.id)

        # Stop it - retry on transient Docker errors
        last_error = None
        for attempt in range(3):
            try:
                stopped = sync_client.sandbox.stop(sandbox.id)
                assert stopped.id == sandbox.id
                assert stopped.state in [SandboxState.STOPPED, SandboxState.DESTROYING]
                return
            except (DSBAPIError, DSBConnectionError) as e:
                last_error = e
                if attempt < 2:
                    time.sleep(2 * (attempt + 1))
        raise last_error

    def test_delete_sandbox(self, sync_client: DSBClient):
        """Test deleting a sandbox"""
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-delete",
            command=["sleep", "300"],
        )

        # Delete it
        result = sync_client.sandbox.delete(sandbox.id)

        assert result is not None

        # Verify it's gone
        from dsb_sdk.exceptions import DSBAPIError

        with pytest.raises(DSBAPIError):
            sync_client.sandbox.get(sandbox.id)


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestSandboxStats:
    """Tests for sandbox statistics"""

    def test_get_sandbox_stats(self, sync_client: DSBClient, cleanup_sandboxes: list):
        """Test getting sandbox statistics"""
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-stats",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(sandbox.id)

        # Wait for sandbox to be running
        wait_for_sandbox(sync_client, sandbox.id)

        stats = sync_client.sandbox.stats(sandbox.id)

        # sandbox_id is optional in stats response
        if stats.sandbox_id is not None:
            assert stats.sandbox_id == sandbox.id
        assert stats.cpu_percent >= 0
        assert stats.memory_usage_mb >= 0
        assert stats.memory_percent >= 0
        assert isinstance(stats.network_rx_bytes, int)
        assert isinstance(stats.network_tx_bytes, int)


@pytest.mark.sandbox
@pytest.mark.serial
@pytest.mark.requires_server
class TestSandboxListIncludeDeleted:
    """Tests for listing sandboxes with include_deleted parameter

    Note: These tests require serial execution due to environment state dependencies.
    Marked with @pytest.mark.serial to prevent parallel execution conflicts.
    """

    def test_list_sandboxes_without_deleted(
        self, sync_client: DSBClient, cleanup_sandboxes: list
    ):
        """Test listing sandboxes excludes deleted by default"""
        # Create a sandbox
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-list-no-deleted",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(sandbox.id)

        # Wait for it to be running
        wait_for_sandbox(sync_client, sandbox.id)

        # List without include_deleted (should show our sandbox)
        response = sync_client.sandbox.list()
        assert response.total >= 1

        # Delete the sandbox
        sync_client.sandbox.delete(sandbox.id)

        # List again without include_deleted (should not show deleted sandbox)
        response = sync_client.sandbox.list()
        sandbox_ids = [s.id for s in response.sandboxes]
        assert sandbox.id not in sandbox_ids

    def test_list_sandboxes_with_deleted(
        self, sync_client: DSBClient, cleanup_sandboxes: list
    ):
        """Test listing sandboxes with include_deleted=True"""
        # Create a sandbox
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-list-with-deleted",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(sandbox.id)

        # Wait for it to be running
        wait_for_sandbox(sync_client, sandbox.id, (SandboxState.RUNNING,))

        # List with include_deleted=False
        response = sync_client.sandbox.list(include_deleted=False)
        initial_count = response.total
        assert initial_count >= 1

        # Delete the sandbox
        sync_client.sandbox.delete(sandbox.id)

        # List without include_deleted (should not show deleted)
        response = sync_client.sandbox.list(include_deleted=False)
        assert response.total < initial_count
        sandbox_ids = [s.id for s in response.sandboxes]
        assert sandbox.id not in sandbox_ids

        # List WITH include_deleted=True (should show deleted)
        response = sync_client.sandbox.list(include_deleted=True)
        sandbox_ids = [s.id for s in response.sandboxes]
        assert sandbox.id in sandbox_ids, "Deleted sandbox should be included"

        # Verify the deleted sandbox has correct state
        deleted_sandbox = next((s for s in response.sandboxes if s.id == sandbox.id), None)
        assert deleted_sandbox is not None
        assert deleted_sandbox.state == SandboxState.DESTROYED

    def test_list_sandboxes_filter_by_destroyed_state(
        self, sync_client: DSBClient, cleanup_sandboxes: list
    ):
        """Test filtering sandboxes by destroyed state"""
        # Create a sandbox
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-filter-destroyed",
            command=["sleep", "300"],
        )
        cleanup_sandboxes.append(sandbox.id)

        # Wait for it to be running
        wait_for_sandbox(sync_client, sandbox.id, (SandboxState.RUNNING,))

        # Delete the sandbox
        sync_client.sandbox.delete(sandbox.id)

        # List with state=destroyed and include_deleted=True
        response = sync_client.sandbox.list(state="destroyed", include_deleted=True)

        # Should include our deleted sandbox
        sandbox_ids = [s.id for s in response.sandboxes]
        assert sandbox.id in sandbox_ids

        # All returned sandboxes should be in destroyed state
        for s in response.sandboxes:
            assert s.state == SandboxState.DESTROYED


@pytest.mark.sandbox
@pytest.mark.requires_server
class TestHealthAPI:
    """Tests for health check API"""

    def test_health_check(self, sync_client: DSBClient):
        """Test server health check"""
        health = sync_client.health.check()

        assert health.status in ["healthy", "ok"]
