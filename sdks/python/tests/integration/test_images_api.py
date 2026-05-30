"""
Integration tests for Images API

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

Markers:
    - images: Marks tests as Images API tests
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
def sync_client() -> Iterator[DSBClient]:
    """Create a DSB client for testing."""
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


@pytest.mark.images
@pytest.mark.requires_server
class TestImagesListAPI:
    """Tests for listing images."""

    def test_list_images_returns_list(self, sync_client: DSBClient):
        """List images returns a list."""
        images = sync_client.images.list()
        assert isinstance(images, list)

    def test_list_images_non_empty(self, sync_client: DSBClient):
        """At least the sandbox image should be present."""
        images = sync_client.images.list()
        assert len(images) > 0

    def test_list_images_contains_sandbox_image(self, sync_client: DSBClient):
        """The sandbox image should appear in the image list."""
        images = sync_client.images.list()
        tags = [tag for img in images for tag in img.get("repo_tags", []) or []]
        assert any("dsb/sandbox" in tag for tag in tags)

    def test_image_has_expected_fields(self, sync_client: DSBClient):
        """Each image should have id, repo_tags, size, created."""
        images = sync_client.images.list()
        img = images[0]
        assert "id" in img
        assert "repo_tags" in img
        assert "size" in img
        assert "created" in img


@pytest.mark.images
@pytest.mark.requires_server
class TestImagesGetAPI:
    """Tests for getting image details."""

    def test_get_image_details(self, sync_client: DSBClient):
        """Get details of a specific image by ID."""
        images = sync_client.images.list()
        image_id = images[0]["id"]
        details = sync_client.images.get(image_id)
        assert details["id"] == image_id

    def test_get_image_has_architecture(self, sync_client: DSBClient):
        """Image details should include architecture."""
        images = sync_client.images.list()
        image_id = images[0]["id"]
        details = sync_client.images.get(image_id)
        assert "architecture" in details

    def test_get_image_has_os(self, sync_client: DSBClient):
        """Image details should include OS."""
        images = sync_client.images.list()
        image_id = images[0]["id"]
        details = sync_client.images.get(image_id)
        assert "os" in details

    def test_get_image_has_extended_fields(self, sync_client: DSBClient):
        """Image details should have fields beyond the summary."""
        images = sync_client.images.list()
        image_id = images[0]["id"]
        details = sync_client.images.get(image_id)
        assert "virtual_size" in details or "size" in details
        assert "architecture" in details


@pytest.mark.images
@pytest.mark.requires_server
class TestImagesDeleteAPI:
    """Tests for deleting images."""

    def test_delete_nonexistent_image(self, sync_client: DSBClient):
        """Deleting a nonexistent image should return an error."""
        fake_id = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        with pytest.raises(DSBAPIError):
            sync_client.images.delete(fake_id)


@pytest.mark.images
@pytest.mark.requires_server
class TestImagesErrors:
    """Error handling tests for Images API."""

    def test_list_images_with_invalid_server(self):
        """Test listing images when server is unavailable."""
        client = DSBClient(api_url="http://localhost:9999")
        with pytest.raises((DSBConnectionError, DSBTimeoutError)):
            client.images.list()

    def test_get_nonexistent_image(self, sync_client: DSBClient):
        """Getting a nonexistent image should raise an error."""
        fake_id = "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        with pytest.raises(DSBAPIError):
            sync_client.images.get(fake_id)
