"""Integration tests for the Admin API (API key management)."""

import os
from collections.abc import Iterator

import pytest

from dsb_sdk import AsyncDSBClient, DSBClient

DSB_API_URL = os.environ.get("DSB_API_URL", "http://localhost:18080")
DSB_ADMIN_API_KEY = os.environ.get("DSB_API_KEY", "test-admin-key-for-testing-only")


def is_server_available() -> bool:
    """Check if the DSB server is reachable."""
    try:
        client = DSBClient(api_url=DSB_API_URL, api_key=DSB_ADMIN_API_KEY, timeout=120.0)
        client.health.check()
        client.close()
        return True
    except Exception:
        return False


@pytest.fixture(scope="module")
def sync_client() -> Iterator[DSBClient]:
    """Module-scoped synchronous client with admin API key."""
    if not is_server_available():
        pytest.skip("DSB server not available")
    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_ADMIN_API_KEY, timeout=120.0)
    yield client
    client.close()


@pytest.fixture
def async_client() -> Iterator[AsyncDSBClient]:
    """Async client with admin API key."""
    if not is_server_available():
        pytest.skip("DSB server not available")
    client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_ADMIN_API_KEY, timeout=120.0)
    yield client


# ---------------------------------------------------------------------------
# Synchronous tests
# ---------------------------------------------------------------------------


@pytest.mark.admin
@pytest.mark.requires_server
class TestAdminAPISyncListKeys:
    """Test listing API keys."""

    def test_list_api_keys_returns_list(self, sync_client: DSBClient):
        """List API keys returns a list."""
        keys = sync_client.admin.list_api_keys()
        assert isinstance(keys, list)


@pytest.mark.admin
@pytest.mark.requires_server
class TestAdminAPISyncCreateKey:
    """Test creating API keys."""

    def test_create_api_key(self, sync_client: DSBClient):
        """Create a new API key and verify response structure."""
        result = sync_client.admin.create_api_key(
            name="integration-test-key",
            description="Created by integration test",
        )
        assert "api_key" in result
        assert "key" in result
        assert result["key"]["name"] == "integration-test-key"
        assert result["key"]["description"] == "Created by integration test"
        # Cleanup
        sync_client.admin.delete_api_key(result["key"]["id"])

    def test_create_api_key_with_scopes(self, sync_client: DSBClient):
        """Create an API key with specific scopes."""
        result = sync_client.admin.create_api_key(
            name="scoped-test-key",
            scopes=["sandbox:read", "sandbox:write"],
        )
        assert result["key"]["scopes"] == ["sandbox:read", "sandbox:write"]
        sync_client.admin.delete_api_key(result["key"]["id"])

    def test_create_api_key_with_expiry(self, sync_client: DSBClient):
        """Create an API key with expiration."""
        result = sync_client.admin.create_api_key(
            name="expiring-test-key",
            expires_in_days=30,
        )
        assert result["key"]["expires_at"] is not None
        sync_client.admin.delete_api_key(result["key"]["id"])

    def test_create_api_key_with_created_by(self, sync_client: DSBClient):
        """Create an API key with created_by field."""
        result = sync_client.admin.create_api_key(
            name="created-by-test-key",
            created_by="test-admin",
        )
        assert result["key"]["created_by"] == "test-admin"
        sync_client.admin.delete_api_key(result["key"]["id"])


@pytest.mark.admin
@pytest.mark.requires_server
class TestAdminAPISyncGetKey:
    """Test fetching a specific API key."""

    def test_get_api_key(self, sync_client: DSBClient):
        """Get details of a specific API key by ID."""
        created = sync_client.admin.create_api_key(name="get-test-key")
        key_id = created["key"]["id"]

        fetched = sync_client.admin.get_api_key(key_id)
        assert fetched["name"] == "get-test-key"
        assert fetched["id"] == key_id

        sync_client.admin.delete_api_key(key_id)


@pytest.mark.admin
@pytest.mark.requires_server
class TestAdminAPISyncDeleteKey:
    """Test deleting an API key."""

    def test_delete_api_key(self, sync_client: DSBClient):
        """Delete an API key and verify it no longer exists."""
        created = sync_client.admin.create_api_key(name="delete-test-key")
        key_id = created["key"]["id"]

        sync_client.admin.delete_api_key(key_id)

        # Verify the key is gone
        with pytest.raises(Exception):
            sync_client.admin.get_api_key(key_id)


@pytest.mark.admin
@pytest.mark.requires_server
class TestAdminAPISyncRotateKey:
    """Test rotating an API key."""

    def test_rotate_api_key(self, sync_client: DSBClient):
        """Rotate an API key and verify a new key is returned."""
        created = sync_client.admin.create_api_key(name="rotate-test-key")
        key_id = created["key"]["id"]
        old_key = created["api_key"]

        rotated = sync_client.admin.rotate_api_key(key_id)
        assert "api_key" in rotated
        assert rotated["api_key"] != old_key  # New key is different
        assert rotated["key"]["id"] == key_id  # Same key ID

        sync_client.admin.delete_api_key(key_id)


@pytest.mark.admin
@pytest.mark.requires_server
class TestAdminAPISyncListVisibility:
    """Test that created keys appear in list results."""

    def test_create_and_list_shows_key(self, sync_client: DSBClient):
        """Creating a key should make it appear in the list."""
        created = sync_client.admin.create_api_key(name="list-visibility-test-key")
        key_id = created["key"]["id"]

        keys = sync_client.admin.list_api_keys()
        key_ids = [k["id"] for k in keys]
        assert key_id in key_ids

        sync_client.admin.delete_api_key(key_id)


# ---------------------------------------------------------------------------
# Asynchronous tests
# ---------------------------------------------------------------------------


@pytest.mark.admin
@pytest.mark.requires_server
@pytest.mark.asyncio
class TestAsyncAdminAPIListKeys:
    """Test async listing of API keys."""

    async def test_list_api_keys_returns_list(
        self, async_client: AsyncDSBClient
    ):
        """List API keys returns a list (async)."""
        keys = await async_client.admin.list_api_keys_async()
        assert isinstance(keys, list)


@pytest.mark.admin
@pytest.mark.requires_server
@pytest.mark.asyncio
class TestAsyncAdminAPICreateKey:
    """Test async creation of API keys."""

    async def test_create_api_key(self, async_client: AsyncDSBClient):
        """Create and verify an API key (async)."""
        result = await async_client.admin.create_api_key_async(
            name="async-integration-test-key",
            description="Created by async integration test",
        )
        assert "api_key" in result
        assert "key" in result
        assert result["key"]["name"] == "async-integration-test-key"
        await async_client.admin.delete_api_key_async(result["key"]["id"])


@pytest.mark.admin
@pytest.mark.requires_server
@pytest.mark.asyncio
class TestAsyncAdminAPIRotateKey:
    """Test async rotation of API keys."""

    async def test_rotate_api_key(self, async_client: AsyncDSBClient):
        """Rotate an API key and verify new key is returned (async)."""
        created = await async_client.admin.create_api_key_async(
            name="async-rotate-test-key"
        )
        key_id = created["key"]["id"]
        old_key = created["api_key"]

        rotated = await async_client.admin.rotate_api_key_async(key_id)
        assert "api_key" in rotated
        assert rotated["api_key"] != old_key
        assert rotated["key"]["id"] == key_id

        await async_client.admin.delete_api_key_async(key_id)
