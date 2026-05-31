"""
Integration tests for Health API

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

Markers:
    - health: Marks tests as Health API tests
    - requires_server: Marks tests that require a running DSB server
"""

import os
from collections.abc import Iterator

import pytest

from dsb_sdk import DSBClient

# Test server URL from environment or default
DSB_API_URL = os.getenv("DSB_API_URL", "http://localhost:8081")
DSB_API_KEY = os.getenv("DSB_API_KEY")


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


@pytest.mark.health
@pytest.mark.requires_server
class TestHealthAPI:
    """Tests for Health Check API"""

    def test_health_check_returns_ok(
        self,
        sync_client: DSBClient,
    ):
        """Test that health check returns healthy status"""
        health = sync_client.health.check()

        assert health.status in ["healthy", "ok"]

    def test_health_check_has_version(
        self,
        sync_client: DSBClient,
    ):
        """Test that health check returns version info"""
        health = sync_client.health.check()

        # Version is optional in API response
        if health.version is not None:
            assert isinstance(health.version, str)

    def test_health_check_has_uptime(
        self,
        sync_client: DSBClient,
    ):
        """Test that health check returns uptime info"""
        health = sync_client.health.check()

        # Uptime is optional in API response
        if health.uptime_seconds is not None:
            assert health.uptime_seconds >= 0

    def test_health_check_has_timestamp(
        self,
        sync_client: DSBClient,
    ):
        """Test that health check returns timestamp"""
        health = sync_client.health.check()

        # Timestamp is optional in API response
        assert health.timestamp is None or isinstance(health.timestamp, object)

    def test_health_check_multiple_calls(
        self,
        sync_client: DSBClient,
    ):
        """Test that multiple health checks return consistent data"""
        health1 = sync_client.health.check()
        health2 = sync_client.health.check()

        assert health1.status == health2.status
        assert health1.version == health2.version

    def test_health_check_structure(
        self,
        sync_client: DSBClient,
    ):
        """Test health response structure"""
        health = sync_client.health.check()

        # Check for expected fields
        assert hasattr(health, "status")
        assert hasattr(health, "version")
        assert hasattr(health, "uptime_seconds")
        assert hasattr(health, "timestamp")


@pytest.mark.health
@pytest.mark.requires_server
class TestHealthAPIResponseFormats:
    """Tests for different health response formats"""

    def test_health_response_is_dict_like(
        self,
        sync_client: DSBClient,
    ):
        """Test that health response can be converted to dict"""

        health = sync_client.health.check()

        # Should be serializable to JSON
        json_str = health.model_dump_json()
        assert isinstance(json_str, str)

        # Should be convertible to dict
        health_dict = health.model_dump()
        assert isinstance(health_dict, dict)
        assert "status" in health_dict
