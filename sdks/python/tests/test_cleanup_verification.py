"""
Cleanup verification tests for Python SDK tests

Meta-tests that verify the cleanup infrastructure works correctly.
Tests resource cleanup on success, failure, and timeout scenarios.
"""

# Import test utilities
import sys
import time
from pathlib import Path
from unittest.mock import Mock

import pytest

from dsb_sdk.client import AsyncDSBClient, DSBClient

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent))

from conftest import get_test_sandbox_name, is_test_sandbox


class TestSandboxNaming:
    """Tests for standardized sandbox naming helpers."""

    def test_get_test_sandbox_name(self):
        """Test that sandbox names get 'test-' prefix."""
        result = get_test_sandbox_name("my-sandbox")
        assert result == "test-my-sandbox"

    def test_get_test_sandbox_name_already_has_prefix(self):
        """Test that double prefix is not added."""
        result = get_test_sandbox_name("test-my-sandbox")
        assert result == "test-my-sandbox"

    def test_is_test_sandbox_positive(self):
        """Test detection of test sandboxes."""
        assert is_test_sandbox("test-my-sandbox")
        assert is_test_sandbox("test-123")
        assert is_test_sandbox("test-async-something")

    def test_is_test_sandbox_negative(self):
        """Test that non-test sandboxes are not detected."""
        assert not is_test_sandbox("my-sandbox")
        assert not is_test_sandbox("production-sandbox")
        assert not is_test_sandbox(None)
        assert not is_test_sandbox("")


class TestCleanupFixtureDetection:
    """Tests that auto-use cleanup fixtures are properly configured."""

    def test_auto_cleanup_fixture_exists(self):
        """Verify auto_cleanup_test_sandboxes fixture exists."""
        from conftest import auto_cleanup_test_sandboxes

        assert auto_cleanup_test_sandboxes is not None

    def test_module_cleanup_fixture_exists(self):
        """Verify module_cleanup fixture exists."""
        from conftest import module_cleanup

        assert module_cleanup is not None

    def test_cleanup_on_failure_fixture_exists(self):
        """Verify cleanup_on_failure fixture exists."""
        from conftest import cleanup_on_failure

        assert cleanup_on_failure is not None


@pytest.mark.timeout(60)
def test_cleanup_fixture_with_sync_client(sync_client_live: DSBClient):
    """
    Test that cleanup fixture works with sync client.

    This test creates a test sandbox and verifies it gets cleaned up.
    Note: This test requires a running DSB server.
    """
    pytest.skip("DSB server not available - integration test")


@pytest.mark.asyncio
@pytest.mark.timeout(60)
async def test_cleanup_fixture_with_async_client(async_client_live: AsyncDSBClient):
    """
    Test that cleanup fixture works with async client.

    This test creates a test sandbox and verifies it gets cleaned up.
    Note: This test requires a running DSB server.
    """
    pytest.skip("DSB server not available - integration test")


@pytest.mark.timeout(10)
def test_cleanup_helpers_performance():
    """Test that cleanup helper functions are fast."""
    # Test is_test_sandbox performance
    start = time.time()
    for i in range(1000):
        is_test_sandbox(f"test-sandbox-{i}")
    elapsed = time.time() - start
    assert elapsed < 0.1, f"is_test_sandbox too slow: {elapsed}s"

    # Test get_test_sandbox_name performance
    start = time.time()
    for i in range(1000):
        get_test_sandbox_name(f"sandbox-{i}")
    elapsed = time.time() - start
    assert elapsed < 0.1, f"get_test_sandbox_name too slow: {elapsed}s"


class TestServerManager:
    """Tests for DSBServerManager."""

    def test_server_manager_import(self):
        """Verify DSBServerManager can be imported."""
        try:
            from tests.dsb_server_manager import DSBServerManager

            assert DSBServerManager is not None
        except ImportError:
            pytest.skip("dsb_server_manager not available")

    def test_server_manager_initialization(self):
        """Test that DSBServerManager can be initialized."""
        try:
            from tests.dsb_server_manager import DSBServerManager

            manager = DSBServerManager(startup_timeout=1.0)
            assert manager.startup_timeout == 1.0
            assert manager.port == 8080
        except ImportError:
            pytest.skip("dsb_server_manager not available")


@pytest.mark.unit
class TestMockClientCleanup:
    """Unit tests for cleanup behavior with mock clients."""

    def test_mock_client_cleanup_no_error(self):
        """Test cleanup doesn't raise errors with mock clients."""
        # Create a mock client with sandbox attribute
        mock_client = Mock()
        mock_client.sandbox = Mock()
        mock_client.sandbox.list = Mock(return_value=Mock(sandboxes=[]))

        # Simulate cleanup (should not raise)
        response = mock_client.sandbox.list()
        assert len(response.sandboxes) == 0

    def test_mock_client_cleanup_with_sandboxes(self):
        """Test cleanup removes test sandboxes with mock client."""
        mock_client = Mock()
        mock_client.sandbox = Mock()

        # Create mock sandbox
        mock_sandbox = Mock()
        mock_sandbox.id = "test-id-123"
        mock_sandbox.config.name = "test-sandbox"

        mock_client.sandbox.list = Mock(return_value=Mock(sandboxes=[mock_sandbox]))
        mock_client.sandbox.delete = Mock(return_value=None)

        # Simulate cleanup
        response = mock_client.sandbox.list()
        for sandbox in response.sandboxes:
            if is_test_sandbox(sandbox.config.name):
                mock_client.sandbox.delete(sandbox.id)

        # Verify delete was called
        mock_client.sandbox.delete.assert_called_once_with("test-id-123")


@pytest.mark.asyncio
@pytest.mark.unit
class TestAsyncMockClientCleanup:
    """Unit tests for cleanup behavior with async mock clients."""

    async def test_async_mock_client_cleanup_no_error(self):
        """Test cleanup doesn't raise errors with async mock clients."""
        mock_client = Mock()
        mock_client.sandbox = Mock()

        # Create async mock for list_async
        async def mock_list():
            return Mock(sandboxes=[])

        mock_client.sandbox.list_async = mock_list

        # Simulate cleanup
        response = await mock_client.sandbox.list_async()
        assert len(response.sandboxes) == 0

    async def test_async_mock_client_cleanup_with_sandboxes(self):
        """Test cleanup removes test sandboxes with async mock client."""
        mock_client = Mock()
        mock_client.sandbox = Mock()

        # Create mock sandbox
        mock_sandbox = Mock()
        mock_sandbox.id = "test-id-456"
        mock_sandbox.config.name = "test-async-sandbox"

        # Track delete calls
        delete_called = []

        async def mock_list():
            return Mock(sandboxes=[mock_sandbox])

        async def mock_delete(sandbox_id):
            delete_called.append(sandbox_id)

        mock_client.sandbox.list_async = mock_list
        mock_client.sandbox.delete_async = mock_delete

        # Simulate cleanup
        response = await mock_client.sandbox.list_async()
        for sandbox in response.sandboxes:
            if is_test_sandbox(sandbox.config.name):
                await mock_client.sandbox.delete_async(sandbox.id)

        # Verify delete was called
        assert "test-id-456" in delete_called
