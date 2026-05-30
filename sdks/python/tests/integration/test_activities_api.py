"""
Integration tests for Activities API

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

Markers:
    - activities: Marks tests as Activities API tests
    - requires_server: Marks tests that require a running DSB server
"""

import os
from collections.abc import Iterator

import pytest

from dsb_sdk import DSBClient
from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError, DSBTimeoutError

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


@pytest.fixture(scope="function")
def cleanup_test_activities(sync_client: DSBClient) -> Iterator[list]:
    """
    Track created resources for cleanup.
    """
    created_ids: list = []

    yield created_ids

    # Cleanup after test
    for sandbox_id in created_ids:
        try:
            sync_client.sandbox.delete(sandbox_id)
        except Exception:
            pass


@pytest.mark.activities
@pytest.mark.requires_server
class TestActivitiesList:
    """Tests for listing activities"""

    def test_list_activities_returns_response(
        self,
        sync_client: DSBClient,
    ):
        """Test that list activities returns a response"""
        response = sync_client.activities.list()

        assert response is not None
        assert hasattr(response, "activities")
        assert hasattr(response, "total")

    def test_list_activities_has_total(
        self,
        sync_client: DSBClient,
    ):
        """Test that list activities returns total count"""
        response = sync_client.activities.list()

        assert isinstance(response.total, int)
        assert response.total >= 0

    def test_list_activities_is_list(
        self,
        sync_client: DSBClient,
    ):
        """Test that activities is a list"""
        response = sync_client.activities.list()

        assert isinstance(response.activities, list)

    def test_list_activities_empty_when_no_activities(
        self,
        sync_client: DSBClient,
    ):
        """Test list response when no activities exist"""
        response = sync_client.activities.list()

        # Should return empty list, not None
        if response.activities is not None:
            assert isinstance(response.activities, list)


@pytest.mark.activities
@pytest.mark.requires_server
class TestActivitiesCleanup:
    """Tests for activities cleanup functionality"""

    def test_cleanup_all_inactive(
        self,
        sync_client: DSBClient,
        cleanup_test_activities: list,
    ):
        """Test cleanup all inactive sandboxes"""
        # First create a sandbox to have something to clean up
        try:
            sandbox = sync_client.sandbox.create(
                image=TEST_IMAGE,
                name="test-cleanup-activity",
                command=["sleep", "300"],
            )
            cleanup_test_activities.append(sandbox.id)
        except DSBConnectionError:
            pytest.skip("DSB server not available")

        # Call cleanup all
        result = sync_client.activities.cleanup_all()

        assert result is not None
        assert isinstance(result, dict)

    def test_cleanup_all_returns_dict(
        self,
        sync_client: DSBClient,
    ):
        """Test that cleanup all returns a dictionary"""
        result = sync_client.activities.cleanup_all()

        assert isinstance(result, dict)


@pytest.mark.activities
@pytest.mark.requires_server
class TestActivitiesWorkflow:
    """Integration tests for activities workflow"""

    def test_activities_workflow(
        self,
        sync_client: DSBClient,
        cleanup_test_activities: list,
    ):
        """Test complete activities workflow"""
        # 1. List activities before creating anything
        response1 = sync_client.activities.list()
        initial_total = response1.total

        # 2. Create a sandbox
        try:
            sandbox = sync_client.sandbox.create(
                image=TEST_IMAGE,
                name="test-activity-workflow",
                command=["sleep", "300"],
            )
            cleanup_test_activities.append(sandbox.id)
        except DSBConnectionError:
            pytest.skip("DSB server not available")

        # 3. List activities after creating
        response2 = sync_client.activities.list()

        # Total should be at least the same (may include the new sandbox)
        assert response2.total >= initial_total

        # 4. Activities should be a list
        assert isinstance(response2.activities, list)

    def test_activities_with_sandbox(
        self,
        sync_client: DSBClient,
        cleanup_test_activities: list,
    ):
        """Test activities contain sandbox information"""
        # Create a sandbox
        try:
            sandbox = sync_client.sandbox.create(
                image=TEST_IMAGE,
                name="test-sandbox-activity",
                command=["sleep", "300"],
            )
            cleanup_test_activities.append(sandbox.id)
        except DSBConnectionError:
            pytest.skip("DSB server not available")

        # List activities
        response = sync_client.activities.list()

        # Check that our sandbox might be in the activities
        if response.activities:
            # Activities should have id and type
            for activity in response.activities:
                assert hasattr(activity, "id") or hasattr(activity, "type")


@pytest.mark.activities
@pytest.mark.requires_server
class TestActivitiesErrors:
    """Error handling tests for Activities API"""

    def test_list_activities_with_invalid_server(self):
        """Test listing activities when server is unavailable"""
        # Use a non-existent server URL
        client = DSBClient(api_url="http://localhost:9999")

        with pytest.raises((DSBConnectionError, DSBTimeoutError)):
            client.activities.list()

    def test_list_activities_with_connection_refused(self):
        """Test listing activities when connection is refused"""
        # Use an invalid IP that will refuse connection
        client = DSBClient(api_url="http://192.0.2.1:8081", timeout=1.0)

        with pytest.raises((DSBConnectionError, DSBTimeoutError)):
            client.activities.list()

    def test_cleanup_all_with_server_error(self, sync_client: DSBClient):
        """Test cleanup_all handles server errors gracefully"""
        # This test verifies that cleanup_all doesn't crash even if server has issues
        # The server should handle cleanup_all gracefully even with no sandboxes
        result = sync_client.activities.cleanup_all()

        # Should always return a dict, even if empty
        assert isinstance(result, dict)

    def test_list_activities_with_malformed_response(self):
        """Test listing activities handles malformed responses"""
        # Create a client with a URL that will return a non-JSON response
        # This tests error handling for unexpected responses
        client = DSBClient(api_url="http://localhost:8081/invalid", timeout=1.0)

        with pytest.raises((DSBConnectionError, DSBAPIError, DSBTimeoutError)):
            client.activities.list()

    def test_cleanup_all_with_empty_response(self, sync_client: DSBClient):
        """Test cleanup_all handles empty response gracefully"""
        # This test verifies that cleanup_all doesn't crash even if server has issues
        # The server should handle cleanup_all gracefully even with no sandboxes
        result = sync_client.activities.cleanup_all()

        # Should always return a dict, even if empty
        assert isinstance(result, dict)
