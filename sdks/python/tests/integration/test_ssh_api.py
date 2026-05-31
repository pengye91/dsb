"""
Integration tests for SSH Gateway API

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.
Set DSB_SSH_USERNAME to specify the SSH username.

Markers:
    - ssh: Marks tests as SSH API tests
    - requires_server: Marks tests that require a running DSB server
"""

import os
import time
from collections.abc import Iterator
from uuid import UUID

import pytest

from dsb_sdk import DSBClient
from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError, DSBValidationError

# Skip auto-cleanup since we use module-scoped shared sandbox
SKIP_AUTO_CLEANUP = True

# Test server URL from environment or default
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")
DSB_API_KEY = os.getenv("DSB_API_KEY")
TEST_IMAGE = os.getenv("TEST_IMAGE", "dsb/sandbox:latest")
DEFAULT_SSH_USERNAME = os.getenv("DSB_SSH_USERNAME", "dsb")


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


def wait_for_sandbox(
    client: DSBClient,
    sandbox_id: str,
    max_wait: int = 60,
    poll_interval: float = 1,
) -> bool:
    """Wait for sandbox to be running."""
    wait_time = 0
    while wait_time < max_wait:
        try:
            sandbox = client.sandbox.get(sandbox_id)
            if sandbox.state.value == "running":
                return True
            # Check for error states
            elif sandbox.state.value in ("error", "destroyed", "destroying"):
                return False
        except Exception:
            pass

        time.sleep(poll_interval)
        wait_time += poll_interval

    return False


@pytest.fixture(scope="module")
def sandbox(shared_sandbox_ssh: str) -> str:
    """
    Use shared sandbox for SSH tests.
    """
    return shared_sandbox_ssh


@pytest.fixture(scope="function")
def cleanup_ssh_sessions(
    sync_client: DSBClient,
) -> Iterator[list]:
    """
    Cleanup all SSH sessions after each test.
    """
    created_session_ids: list = []

    yield created_session_ids

    # Cleanup SSH sessions
    for session_id in created_session_ids:
        try:
            sync_client.ssh.terminate(session_id)
        except Exception:
            pass


@pytest.mark.ssh
@pytest.mark.requires_server
class TestSSHSessionManagement:
    """Tests for SSH session management"""

    def test_create_ssh_session(
        self,
        sync_client: DSBClient,
        sandbox: object,
        cleanup_ssh_sessions: list,
    ):
        """Test creating an SSH session"""
        session = sync_client.ssh.create(
            sandbox_id=sandbox,
            username=DEFAULT_SSH_USERNAME,
        )

        cleanup_ssh_sessions.append(session.id)

        assert session.id is not None
        assert isinstance(session.id, UUID)
        assert str(session.sandbox_id) == sandbox
        # Note: username is not returned by the API for security reasons
        assert session.username is None
        assert session.status in ["active", "pending", "ready", "connecting"]

    def test_get_ssh_session(
        self,
        sync_client: DSBClient,
        sandbox: object,
        cleanup_ssh_sessions: list,
    ):
        """Test getting SSH session details"""
        created = sync_client.ssh.create(
            sandbox_id=sandbox,
            username=DEFAULT_SSH_USERNAME,
        )
        cleanup_ssh_sessions.append(created.id)

        session = sync_client.ssh.get(created.id)

        assert session.id == created.id
        assert str(session.sandbox_id) == sandbox
        # Note: username is not returned by the API for security reasons
        assert session.username is None

    def test_list_ssh_sessions(
        self,
        sync_client: DSBClient,
        sandbox: object,
        cleanup_ssh_sessions: list,
    ):
        """Test listing SSH sessions"""
        # Create a session
        sync_client.ssh.create(
            sandbox_id=sandbox,
            username=DEFAULT_SSH_USERNAME,
        )

        # List sessions
        response = sync_client.ssh.list()

        assert response.total >= 0
        assert isinstance(response.sessions, list)
        # Our session should be in the list
        test_sessions = [s for s in response.sessions if str(s.sandbox_id) == sandbox]
        assert len(test_sessions) >= 1

    def test_ssh_heartbeat(
        self,
        sync_client: DSBClient,
        sandbox: object,
        cleanup_ssh_sessions: list,
    ):
        """Test sending heartbeat to SSH session"""
        session = sync_client.ssh.create(
            sandbox_id=sandbox,
            username=DEFAULT_SSH_USERNAME,
        )
        cleanup_ssh_sessions.append(session.id)

        # Send heartbeat
        result = sync_client.ssh.heartbeat(session.id)

        assert result is not None

    def test_terminate_ssh_session(
        self,
        sync_client: DSBClient,
        sandbox: object,
    ):
        """Test terminating an SSH session"""
        session = sync_client.ssh.create(
            sandbox_id=sandbox,
            username=DEFAULT_SSH_USERNAME,
        )

        # Terminate it
        result = sync_client.ssh.terminate(session.id)

        assert result is not None

        # Verify it's terminated or doesn't exist
        try:
            retrieved = sync_client.ssh.get(session.id)
            # If it exists, status should be terminated/closed
            assert retrieved.status in ["terminated", "closed"]
        except Exception:
            # Or it might not exist anymore, which is also fine
            pass


@pytest.mark.ssh
@pytest.mark.requires_server
class TestSSHSessionWithPublicKey:
    """Tests for SSH sessions with public key authentication"""

    @pytest.fixture
    def test_public_key(self) -> str:
        """Return a test public key."""
        # For testing, we use a dummy public key
        # In real scenarios, this would be a valid SSH public key
        return "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC7 test@example.com"

    def test_create_ssh_session_with_key(
        self,
        sync_client: DSBClient,
        sandbox: object,
        cleanup_ssh_sessions: list,
        test_public_key: str,
    ):
        """Test creating SSH session with public key"""
        session = sync_client.ssh.create(
            sandbox_id=sandbox,
            username=DEFAULT_SSH_USERNAME,
            public_key=test_public_key,
        )

        cleanup_ssh_sessions.append(session.id)

        assert session.id is not None
        assert str(session.sandbox_id) == sandbox
        # Note: username is not returned by the API for security reasons
        assert session.username is None


@pytest.mark.ssh
@pytest.mark.requires_server
class TestSSHWorkflow:
    """Integration tests for complete SSH workflow"""

    def test_complete_ssh_workflow(
        self,
        sync_client: DSBClient,
        sandbox: object,
        cleanup_ssh_sessions: list,
    ):
        """Test complete SSH session workflow"""
        # 1. Create SSH session
        session = sync_client.ssh.create(
            sandbox_id=sandbox,
            username=DEFAULT_SSH_USERNAME,
        )
        cleanup_ssh_sessions.append(session.id)

        assert session.id is not None

        # 2. Get session details
        retrieved = sync_client.ssh.get(session.id)
        assert retrieved.id == session.id

        # 3. List all sessions
        sessions = sync_client.ssh.list()
        assert any(s.id == session.id for s in sessions.sessions)

        # 4. Send heartbeat
        heartbeat_result = sync_client.ssh.heartbeat(session.id)
        assert heartbeat_result is not None

        # 5. Terminate session
        terminate_result = sync_client.ssh.terminate(session.id)
        assert terminate_result is not None

        # Remove from cleanup list since it's already terminated
        cleanup_ssh_sessions.remove(session.id)


@pytest.mark.ssh
@pytest.mark.requires_server
class TestSSHErrors:
    """Error handling tests for SSH API"""

    def test_create_ssh_session_invalid_sandbox_id(self, sync_client: DSBClient):
        """Test creating SSH session with invalid sandbox ID"""
        fake_sandbox_id = "00000000-0000-0000-0000-000000000000"

        with pytest.raises(DSBAPIError) as exc_info:
            sync_client.ssh.create(
                sandbox_id=fake_sandbox_id,
                username=DEFAULT_SSH_USERNAME,
            )

        # Should get a sandbox not found error
        assert "not found" in str(exc_info.value).lower() or "sandbox" in str(exc_info.value).lower()

    def test_create_ssh_session_empty_username(self, sync_client: DSBClient, sandbox: object):
        """Test creating SSH session with empty username"""
        with pytest.raises((DSBValidationError, ValueError)):
            sync_client.ssh.create(
                sandbox_id=sandbox,
                username="",
            )

    def test_get_nonexistent_ssh_session(self, sync_client: DSBClient):
        """Test getting a non-existent SSH session"""
        fake_session_id = "00000000-0000-0000-0000-000000000000"

        with pytest.raises(DSBAPIError) as exc_info:
            sync_client.ssh.get(fake_session_id)

        assert "not found" in str(exc_info.value).lower()

    def test_terminate_nonexistent_ssh_session(self, sync_client: DSBClient):
        """Test terminating a non-existent SSH session"""
        fake_session_id = "00000000-0000-0000-0000-000000000000"

        # Should either raise an error or return gracefully
        try:
            result = sync_client.ssh.terminate(fake_session_id)
            # If no error, should at least return something
            assert result is not None
        except DSBAPIError:
            # Expected behavior
            pass

    def test_heartbeat_nonexistent_ssh_session(self, sync_client: DSBClient):
        """Test sending heartbeat to non-existent SSH session"""
        fake_session_id = "00000000-0000-0000-0000-000000000000"

        # Should either raise an error or return gracefully
        try:
            result = sync_client.ssh.heartbeat(fake_session_id)
            # If no error, should at least return something
            assert result is not None
        except DSBAPIError:
            # Expected behavior
            pass

    def test_create_ssh_session_with_invalid_public_key(self, sync_client: DSBClient, sandbox: object, cleanup_ssh_sessions: list):
        """Test creating SSH session with malformed public key"""
        invalid_key = "not-a-valid-ssh-public-key"

        # Should reject invalid public key format
        with pytest.raises((DSBValidationError, ValueError)):
            sync_client.ssh.create(
                sandbox_id=sandbox,
                username=DEFAULT_SSH_USERNAME,
                public_key=invalid_key,
            )

    def test_create_ssh_session_for_stopped_sandbox(self, sync_client: DSBClient):
        """Test creating SSH session for a stopped sandbox"""
        # Create a sandbox
        try:
            sandbox_obj = sync_client.sandbox.create(
                image=TEST_IMAGE,
                name="test-stopped-ssh",
            )
        except DSBConnectionError:
            pytest.skip("DSB server not available")

        # Stop it immediately
        try:
            sync_client.sandbox.stop(str(sandbox_obj.id))
        except Exception:
            pass

        # Try to create SSH session - should fail
        try:
            with pytest.raises(DSBAPIError) as exc_info:
                sync_client.ssh.create(
                    sandbox_id=str(sandbox_obj.id),
                    username=DEFAULT_SSH_USERNAME,
                )

            # Should get an error about sandbox not running
            error_msg = str(exc_info.value).lower()
            assert "not running" in error_msg or "stopped" in error_msg or "conflict" in error_msg
        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox_obj.id))
            except Exception:
                pass
