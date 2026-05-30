"""
Integration tests for file download API

Tests run in docker-compose environment with DSB server available.
"""

import os
import tempfile
import uuid
from collections.abc import Iterator

import pytest

from dsb_sdk import AsyncDSBClient, DSBClient

# Test server URL from environment (set by docker-compose)
DSB_API_URL = os.getenv("DSB_API_URL", "http://dsb-server-test:8080")
DSB_API_KEY = os.getenv("DSB_API_KEY")
TEST_IMAGE = os.getenv("TEST_IMAGE", "dsb/sandbox:latest")


@pytest.fixture(scope="function")
def sync_client() -> Iterator[DSBClient]:
    """
    Create a DSB client for testing.

    Scope is function-level to create fresh client for each test.
    """
    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


@pytest.fixture(scope="function")
def test_sandbox_id(sync_client: DSBClient) -> Iterator[str]:
    """
    Create a sandbox for testing and cleanup after.

    Returns the sandbox ID (string) for use with client API methods.
    Uses unique naming to prevent conflicts in docker-compose environment.
    """
    import time
    unique_name = f"test-file-download-{uuid.uuid4().hex[:8]}"
    sandbox = sync_client.sandbox.create(
        image=TEST_IMAGE,
        name=unique_name,
        command=["sleep", "300"],
    )

    # Wait for sandbox to be in running state (max 30 seconds)
    sandbox_id = str(sandbox.id)
    for i in range(30):
        try:
            sb = sync_client.sandbox.get(sandbox_id)
            if sb.state == "running":
                break
        except Exception:
            pass
        time.sleep(1)

    yield sandbox_id

    # Cleanup - always delete the sandbox
    try:
        sync_client.sandbox.delete(sandbox_id)
    except Exception as e:
        print(f"Warning: Failed to cleanup sandbox {sandbox.id}: {e}")


@pytest.mark.serial
class TestSynchronousFileDownload:
    """Tests for synchronous file download

    Note: These tests require serial execution due to container resource usage.
    Marked with @pytest.mark.serial to prevent parallel execution conflicts.
    """

    def test_download_file_success(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test successful file download"""
        # Create a test file in the sandbox
        test_content = "Hello from DSB file download!"
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=["sh", "-c", f"echo '{test_content}' > /tmp/test.txt"]
        )

        # Download the file
        response = sync_client.sandbox.download_file(test_sandbox_id, "/tmp/test.txt")

        # Verify response
        assert response.name == "test.txt"
        assert response.path == "/tmp/test.txt"
        assert response.size > 0
        assert response.content_type == "text/plain"
        assert test_content in response.content.decode()

    def test_download_file_not_found(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test downloading non-existent file"""
        from dsb_sdk.exceptions import DSBAPIError

        # Try to download non-existent file
        with pytest.raises(DSBAPIError) as exc_info:
            sync_client.sandbox.download_file(test_sandbox_id, "/tmp/nonexistent.txt")

        # Verify error
        assert exc_info.value.status_code == 404

    def test_download_file_inline_disposition(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test download with inline disposition"""
        # Create test file
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=["sh", "-c", "echo 'inline test' > /tmp/inline.txt"]
        )

        # Download with inline disposition
        response = sync_client.sandbox.download_file(
            test_sandbox_id,
            "/tmp/inline.txt",
            disposition="inline"
        )

        # Verify download succeeded
        assert response.size > 0
        assert b"inline test" in response.content

    def test_download_file_json(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test downloading JSON file"""
        # Create JSON file
        json_content = '{"key": "value", "number": 42}'
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=["sh", "-c", f"echo '{json_content}' > /tmp/config.json"]
        )

        # Download JSON file
        response = sync_client.sandbox.download_file(test_sandbox_id, "/tmp/config.json")

        # Verify
        assert response.content_type == "application/json"
        assert "key" in response.content.decode()

    def test_download_file_binary(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test downloading binary file"""
        # Create binary file using base64
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=[
                "sh",
                "-c",
                "echo 'SGVsbG8gV29ybGQh' | base64 -d > /tmp/binary.bin",
            ]
        )

        # Download binary file
        response = sync_client.sandbox.download_file(test_sandbox_id, "/tmp/binary.bin")

        # Verify
        assert response.content_type == "application/octet-stream"
        assert response.content == b"Hello World!"

    def test_download_file_to_path(self, sync_client: DSBClient, test_sandbox_id: str, tmp_path):
        """Test downloading file directly to disk"""
        # Create test file in sandbox
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=["sh", "-c", "echo 'path test' > /tmp/file.txt"]
        )

        local_path = tmp_path / "downloaded.txt"

        # Download to path
        result = sync_client.sandbox.download_file_to_path(
            test_sandbox_id,
            "/tmp/file.txt",
            str(local_path)
        )

        # Verify result
        assert result["sandbox_path"] == "/tmp/file.txt"
        assert result["local_path"] == str(local_path)
        assert result["size"] > 0

        # Verify file was written
        assert local_path.exists()
        content = local_path.read_text()
        assert "path test" in content

    def test_download_file_creates_directories(self, sync_client: DSBClient, test_sandbox_id: str, tmp_path):
        """Test download creates parent directories if needed"""
        # Create test file (with directory creation)
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=["sh", "-c", "mkdir -p /tmp/nested && echo 'dir test' > /tmp/nested/file.txt"]
        )

        local_path = tmp_path / "subdir" / "nested" / "file.txt"

        # Download to path with nested directories
        sync_client.sandbox.download_file_to_path(test_sandbox_id, "/tmp/nested/file.txt", str(local_path))

        # Verify directories were created
        assert local_path.exists()
        assert "dir test" in local_path.read_text()

    def test_download_file_overwrites_existing(self, sync_client: DSBClient, test_sandbox_id: str, tmp_path):
        """Test download overwrites existing file"""
        # Create test file in sandbox
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=["sh", "-c", "echo 'new content' > /tmp/file.txt"]
        )

        local_path = tmp_path / "overwrite.txt"

        # Create existing file
        local_path.write_text("old content")

        # Download should overwrite
        sync_client.sandbox.download_file_to_path(test_sandbox_id, "/tmp/file.txt", str(local_path))

        # Verify file was overwritten
        assert "new content" in local_path.read_text()

    def test_download_and_upload_roundtrip(self, sync_client: DSBClient, test_sandbox_id: str, tmp_path):
        """Test upload then download to verify content integrity"""
        # Create a local test file
        test_content = "Round-trip test content with special chars: àéïôù"
        local_file = tmp_path / "upload.txt"
        local_file.write_text(test_content, encoding="utf-8")

        # Upload to sandbox
        sync_client.sandbox.upload_file(test_sandbox_id, "/tmp/roundtrip.txt", str(local_file))

        # Download back
        response = sync_client.sandbox.download_file(test_sandbox_id, "/tmp/roundtrip.txt")

        # Verify content integrity
        downloaded_content = response.content.decode("utf-8")
        assert downloaded_content == test_content

    def test_download_file_metadata_headers(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test that all metadata headers are correctly parsed"""
        # Create test file
        sync_client.sandbox.exec(
            test_sandbox_id,
            command=["sh", "-c", "echo 'metadata test' > /tmp/meta.txt"]
        )

        # Download and check all metadata
        response = sync_client.sandbox.download_file(test_sandbox_id, "/tmp/meta.txt")

        # Verify all fields are populated
        assert response.name == "meta.txt"
        assert response.path == "/tmp/meta.txt"
        assert response.size > 0
        assert response.content_type == "text/plain"
        assert len(response.content) > 0


class TestAsyncFileDownload:
    """Tests for asynchronous file download"""

    @pytest.mark.asyncio
    async def test_download_file_async_success(self):
        """Test successful async file download"""
        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            # Create sandbox with unique name
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-download-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )

            try:
                # Create test file
                await client.sandbox.exec_async(
                    str(sandbox.id),
                    command=["sh", "-c", "echo 'async test' > /tmp/async.txt"]
                )

                # Download file
                response = await client.sandbox.download_file_async(str(sandbox.id), "/tmp/async.txt")

                # Verify
                assert response.name == "async.txt"
                assert b"async test" in response.content

            finally:
                # Cleanup
                await client.sandbox.delete_async(str(sandbox.id))

        finally:
            await client.close()

    @pytest.mark.asyncio
    async def test_download_file_to_path_async(self):
        """Test async download to path"""

        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            # Create sandbox with unique name
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-path-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )

            try:
                # Create test file
                await client.sandbox.exec_async(
                    str(sandbox.id),
                    command=["sh", "-c", "echo 'async path' > /tmp/file.txt"]
                )

                # Download to temp file
                with tempfile.NamedTemporaryFile(delete=False) as tmp:
                    tmp_path = tmp.name

                try:
                    result = await client.sandbox.download_file_to_path_async(
                        str(sandbox.id),
                        "/tmp/file.txt",
                        tmp_path
                    )

                    # Verify
                    assert result["size"] > 0
                    assert "sandbox_path" in result

                    # Verify file was written
                    with open(tmp_path, "rb") as f:
                        content = f.read()
                    assert b"async path" in content

                finally:
                    # Cleanup temp file
                    if os.path.exists(tmp_path):
                        os.unlink(tmp_path)

            finally:
                # Cleanup sandbox
                await client.sandbox.delete_async(str(sandbox.id))

        finally:
            await client.close()

    @pytest.mark.asyncio
    async def test_download_and_upload_async_roundtrip(self):
        """Test async upload then download for content integrity"""

        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            # Create sandbox with unique name
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-roundtrip-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )

            try:
                # Create local test file
                test_content = "Async round-trip with unicode: 你好世界"
                with tempfile.NamedTemporaryFile(
                    mode="w", delete=False, encoding="utf-8"
                ) as tmp:
                    tmp.write(test_content)
                    tmp_path = tmp.name

                try:
                    # Upload
                    await client.sandbox.upload_file_async(str(sandbox.id), "/tmp/roundtrip.txt", tmp_path)

                    # Download
                    response = await client.sandbox.download_file_async(str(sandbox.id), "/tmp/roundtrip.txt")

                    # Verify
                    downloaded = response.content.decode("utf-8")
                    assert downloaded == test_content

                finally:
                    # Cleanup temp file
                    if os.path.exists(tmp_path):
                        os.unlink(tmp_path)

            finally:
                # Cleanup sandbox
                await client.sandbox.delete_async(str(sandbox.id))

        finally:
            await client.close()
