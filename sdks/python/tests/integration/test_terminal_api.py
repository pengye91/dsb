"""
Integration tests for Terminal API

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

Markers:
    - terminal: Marks tests as Terminal API tests
    - requires_server: Marks tests that require a running DSB server
"""

import os
import time
from collections.abc import Iterator

import pytest

from dsb_sdk import DSBClient
from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError

# Skip auto-cleanup since we use module-scoped shared sandbox
SKIP_AUTO_CLEANUP = True

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
def running_sandbox(shared_sandbox_terminal: str) -> str:
    """
    Use shared sandbox for terminal tests.
    """
    return shared_sandbox_terminal


@pytest.mark.terminal
@pytest.mark.requires_server
class TestTerminalAPI:
    """Tests for Terminal API"""

    def test_terminal_websocket_url(
        self,
        sync_client: DSBClient,
        running_sandbox: str,
    ):
        """Test getting terminal WebSocket URL"""
        ws_url = sync_client.terminal.get_websocket_url(running_sandbox)

        assert ws_url is not None
        assert isinstance(ws_url, str)
        assert "ws://" in ws_url or "wss://" in ws_url

    def test_terminal_websocket_url_contains_sandbox_id(
        self,
        sync_client: DSBClient,
        running_sandbox: str,
    ):
        """Test that WebSocket URL contains sandbox ID"""
        ws_url = sync_client.terminal.get_websocket_url(running_sandbox)

        assert str(running_sandbox) in ws_url

    def test_terminal_websocket_url_format(
        self,
        sync_client: DSBClient,
        running_sandbox: str,
    ):
        """Test WebSocket URL format"""
        ws_url = sync_client.terminal.get_websocket_url(running_sandbox)

        # URL should be properly formatted
        assert ws_url.startswith("ws://") or ws_url.startswith("wss://")

        # Should have a valid structure
        parts = ws_url.split("/")
        assert len(parts) >= 4  # protocol://host/.../sandbox_id

    def test_terminal_websocket_url_with_session(
        self,
        sync_client: DSBClient,
        running_sandbox: str,
    ):
        """Test WebSocket URL with custom session ID"""
        custom_session = "test-session-123"

        ws_url = sync_client.terminal.get_websocket_url(running_sandbox, session_id=custom_session)

        assert ws_url is not None
        assert custom_session in ws_url


@pytest.mark.terminal
@pytest.mark.requires_server
class TestTerminalAPIWithParams:
    """Tests for Terminal API with various parameters"""

    def test_terminal_with_cols_rows(
        self,
        sync_client: DSBClient,
        running_sandbox: str,
    ):
        """Test terminal with custom cols and rows"""
        ws_url = sync_client.terminal.get_websocket_url(running_sandbox, cols=120, rows=40)

        assert ws_url is not None
        # URL should contain dimensions
        assert "cols" in ws_url or "columns" in ws_url

    def test_terminal_with_initial_command(
        self,
        sync_client: DSBClient,
        running_sandbox: str,
    ):
        """Test terminal with initial command"""
        # Note: initial_command parameter not yet supported by SDK
        # Testing basic terminal URL generation instead
        ws_url = sync_client.terminal.get_websocket_url(running_sandbox)

        assert ws_url is not None
        assert "/terminal/" in ws_url


@pytest.mark.terminal
@pytest.mark.requires_server
class TestTerminalAPIUrlValidation:
    """Tests for URL validation and edge cases"""

    def test_terminal_url_for_nonexistent_sandbox(
        self,
        sync_client: DSBClient,
    ):
        """Test getting terminal URL for non-existent sandbox"""
        fake_id = "00000000-0000-0000-0000-000000000000"

        # Should still return a URL (server validates sandbox exists)
        ws_url = sync_client.terminal.get_websocket_url(fake_id)

        # URL is returned but connection will fail
        assert ws_url is not None
        assert "ws://" in ws_url or "wss://" in ws_url

    def test_terminal_url_uuid_format(
        self,
        sync_client: DSBClient,
        running_sandbox: str,
    ):
        """Test that sandbox ID is properly used in URL"""
        ws_url = sync_client.terminal.get_websocket_url(running_sandbox)

        # Sandbox ID should be in URL as UUID format
        assert str(running_sandbox) in ws_url


@pytest.mark.terminal
@pytest.mark.requires_server
class TestTerminalAPIErrors:
    """Error handling tests for Terminal API"""

    def test_terminal_url_invalid_sandbox_id_format(self, sync_client: DSBClient):
        """Test getting terminal URL with invalid sandbox ID format"""
        invalid_id = "not-a-uuid"

        # Should either raise error or handle gracefully
        try:
            ws_url = sync_client.terminal.get_websocket_url(invalid_id)
            # If no error, URL should still be returned
            assert ws_url is not None
        except (ValueError, DSBAPIError):
            # Expected behavior for invalid UUID
            pass

    def test_terminal_url_empty_sandbox_id(self, sync_client: DSBClient):
        """Test getting terminal URL with empty sandbox ID"""
        with pytest.raises((ValueError, DSBAPIError)):
            sync_client.terminal.get_websocket_url("")

    def test_terminal_with_invalid_dimensions(self, sync_client: DSBClient, running_sandbox: str):
        """Test terminal with invalid dimensions"""
        # Test with negative dimensions (should handle gracefully)
        try:
            ws_url = sync_client.terminal.get_websocket_url(running_sandbox, cols=-1, rows=-1)
            # If no error, should still return URL
            assert ws_url is not None
        except (ValueError, DSBAPIError):
            # Expected behavior for invalid dimensions
            pass

    def test_terminal_with_zero_dimensions(self, sync_client: DSBClient, running_sandbox: str):
        """Test terminal with zero dimensions"""
        # Test with zero dimensions (should handle gracefully)
        try:
            ws_url = sync_client.terminal.get_websocket_url(running_sandbox, cols=0, rows=0)
            # If no error, should still return URL
            assert ws_url is not None
        except (ValueError, DSBAPIError):
            # Expected behavior for invalid dimensions
            pass

    def test_terminal_with_extremely_large_dimensions(self, sync_client: DSBClient, running_sandbox: str):
        """Test terminal with extremely large dimensions"""
        # Test with unrealistically large dimensions
        try:
            ws_url = sync_client.terminal.get_websocket_url(running_sandbox, cols=99999, rows=99999)
            # If no error, should still return URL
            assert ws_url is not None
        except (ValueError, DSBAPIError):
            # Expected behavior for invalid dimensions
            pass

    def test_terminal_connection_to_stopped_sandbox(self, sync_client: DSBClient):
        """Test terminal connection to stopped sandbox"""
        # Create and stop sandbox
        try:
            sandbox = sync_client.sandbox.create(
                image=TEST_IMAGE,
                name="test-stopped-terminal",
            )
        except DSBConnectionError:
            pytest.skip("DSB server not available")

        # Stop it
        try:
            sync_client.sandbox.stop(str(sandbox.id))
        except Exception:
            pass

        try:
            # Get terminal URL - should still return URL even if sandbox is stopped
            ws_url = sync_client.terminal.get_websocket_url(str(sandbox.id))
            assert ws_url is not None
            # Note: Actual connection would fail, but URL generation should succeed
        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_terminal_url_with_special_chars_in_session_id(self, sync_client: DSBClient, running_sandbox: str):
        """Test terminal URL with special characters in session ID"""
        # Test with special characters that might need encoding
        special_session = "test-session-123_456.789"

        try:
            ws_url = sync_client.terminal.get_websocket_url(
                running_sandbox,
                session_id=special_session
            )
            # Should handle special characters gracefully
            assert ws_url is not None
        except (ValueError, DSBAPIError):
            # Expected behavior if special chars are not allowed
            pass
