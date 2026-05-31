"""
Integration tests for file upload API

Tests run in docker-compose environment with DSB server available.
"""

import os
import tempfile
import uuid
from collections.abc import Iterator

import pytest

from dsb_sdk import DSBClient

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
    unique_name = f"test-file-upload-{uuid.uuid4().hex[:8]}"
    sandbox = sync_client.sandbox.create(
        image=TEST_IMAGE,
        name=unique_name,
        command=["sleep", "300"],
    )

    # Wait for sandbox to be in running state (max 30 seconds)
    sandbox_id = str(sandbox.id)
    for _ in range(30):
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


class TestFileUploadIntegration:
    """Integration tests for file upload functionality"""

    def test_upload_file_from_path(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test uploading a file from a path"""
        # Create a temporary file
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".txt") as f:
            f.write("Hello from DSB file upload!")
            temp_path = f.name

        try:
            # Upload the file
            response = sync_client.sandbox.upload_file(
                test_sandbox_id,
                "/tmp/uploaded.txt",
                temp_path,
            )

            # Verify response
            assert response.success is True
            assert response.file.name == "uploaded.txt" or response.file.name.endswith(".txt")
            assert response.file.path == "/tmp/uploaded.txt"
            assert response.file.size > 0

            # Verify file exists in sandbox
            result = sync_client.sandbox.exec(
                test_sandbox_id,
                ["cat", "/tmp/uploaded.txt"],
            )

            assert "Hello from DSB file upload!" in result["output"]

        finally:
            # Cleanup temp file
            os.unlink(temp_path)

    def test_upload_file_from_bytes(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test uploading a file from bytes"""
        # Upload from bytes
        data = b"Binary data from upload"
        response = sync_client.sandbox.upload_file(
            test_sandbox_id,
            "/tmp/binary.dat",
            data,
        )

        # Verify response
        assert response.success is True
        assert response.file.size == len(data)

        # Verify file content in sandbox
        result = sync_client.sandbox.exec(
            test_sandbox_id,
            ["cat", "/tmp/binary.dat"],
        )

        assert "Binary data from upload" in result["output"]

    def test_upload_file_from_file_object(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test uploading a file from a file object"""
        # Create file-like object with BytesIO
        from io import BytesIO

        content = b"Data from BytesIO"
        file_obj = BytesIO(content)

        # Upload
        response = sync_client.sandbox.upload_file(
            test_sandbox_id,
            "/tmp/from_bytesio.txt",
            file_obj,
        )

        # Verify
        assert response.success is True

        # Verify in sandbox
        result = sync_client.sandbox.exec(
            test_sandbox_id,
            ["cat", "/tmp/from_bytesio.txt"],
        )

        assert "Data from BytesIO" in result["output"]

    def test_upload_json_config(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test uploading a JSON configuration file"""
        # Create a JSON config
        config_content = '{"debug": true, "port": 8080, "hosts": ["localhost", "0.0.0.0"]}'

        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".json") as f:
            f.write(config_content)
            temp_path = f.name

        try:
            # Upload config
            response = sync_client.sandbox.upload_file(
                test_sandbox_id,
                "/etc/app/config.json",
                temp_path,
            )

            assert response.success is True

            # Verify JSON is valid in sandbox
            result = sync_client.sandbox.exec(
                test_sandbox_id,
                ["cat", "/etc/app/config.json"],
            )

            assert '"debug": true' in result["output"]
            assert '"port": 8080' in result["output"]

        finally:
            os.unlink(temp_path)

    def test_upload_creates_parent_directories(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test that upload creates parent directories automatically"""
        # Upload to a path with non-existent parent directories
        data = b"Deep nested file"
        response = sync_client.sandbox.upload_file(
            test_sandbox_id,
            "/opt/myapp/deeply/nested/config.txt",
            data,
        )

        assert response.success is True
        assert response.file.path == "/opt/myapp/deeply/nested/config.txt"

        # Verify file exists
        result = sync_client.sandbox.exec(
            test_sandbox_id,
            ["cat", "/opt/myapp/deeply/nested/config.txt"],
        )

        assert "Deep nested file" in result["output"]

    def test_upload_nonexistent_sandbox(self, sync_client: DSBClient):
        """Test uploading to a non-existent sandbox raises error"""
        fake_id = "00000000-0000-0000-0000-000000000000"

        with pytest.raises(Exception) as exc_info:
            sync_client.sandbox.upload_file(fake_id, "/tmp/file.txt", b"data")

        # Should get a 404 or similar error
        assert "not found" in str(exc_info.value).lower() or exc_info.value.args[0] == "Sandbox not found"

    def test_upload_to_stopped_sandbox(self, sync_client: DSBClient):
        """Test uploading to a stopped sandbox raises error"""
        # Create and stop sandbox
        unique_name = f"test-stopped-{uuid.uuid4().hex[:8]}"
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=unique_name,
            command=["sleep", "10"],
        )

        # Stop it
        sync_client.sandbox.stop(str(sandbox.id))

        try:
            # Try to upload - should fail
            with pytest.raises(Exception) as exc_info:
                sync_client.sandbox.upload_file(str(sandbox.id), "/tmp/file.txt", b"data")

            # Should get an error about sandbox not running
            error_msg = str(exc_info.value).lower()
            assert "not running" in error_msg or "conflict" in error_msg

        finally:
            sync_client.sandbox.delete(str(sandbox.id))

    def test_upload_large_file(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test uploading a larger file (~1MB)"""
        # Create a 1MB file
        large_data = b"x" * (1024 * 1024)

        response = sync_client.sandbox.upload_file(
            test_sandbox_id,
            "/tmp/large_file.bin",
            large_data,
        )

        assert response.success is True
        assert response.file.size == 1024 * 1024
        assert response.file.path == "/tmp/large_file.bin"

    def test_overwrite_existing_file(self, sync_client: DSBClient, test_sandbox_id: str):
        """Test that uploading overwrites existing files"""
        # Create initial file
        sync_client.sandbox.exec(
            test_sandbox_id,
            ["sh", "-c", "echo 'original content' > /tmp/overwrite.txt"],
        )

        # Upload new file to same path
        new_content = b"new content"
        response = sync_client.sandbox.upload_file(
            test_sandbox_id,
            "/tmp/overwrite.txt",
            new_content,
        )

        assert response.success is True

        # Verify file was overwritten
        result = sync_client.sandbox.exec(
            test_sandbox_id,
            ["cat", "/tmp/overwrite.txt"],
        )

        assert "new content" in result["output"]
        assert "original content" not in result["output"]
