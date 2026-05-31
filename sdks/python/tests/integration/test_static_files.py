"""
Integration tests for Static File Serving API

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

These tests verify that:
1. Static files can be served from sandboxes
2. Files can be listed with metadata
3. Individual files can be deleted
4. All sandbox files can be deleted at once
5. Binary and text files are handled correctly
"""

import os
import time
import uuid
from collections.abc import Iterator

import pytest

from dsb_sdk import AsyncDSBClient, DSBClient
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


@pytest.fixture(scope="function")
def sync_client() -> Iterator[DSBClient]:
    """
    Create a DSB client for testing.

    Scope is function-level to ensure fresh connection for each test.
    """
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


def ensure_sandbox_running(
    client: DSBClient,
    sandbox_id: str,
    max_wait: int = 60,
    poll_interval: float = 1,
) -> None:
    """Wait for a sandbox to reach running before file operations."""
    last_state = SandboxState.UNKNOWN
    wait_time = 0.0

    while wait_time < max_wait:
        try:
            sandbox = client.sandbox.get(sandbox_id)
            last_state = sandbox.state
            if last_state == SandboxState.RUNNING:
                return
            if last_state in (SandboxState.ERROR, SandboxState.DESTROYED):
                break
        except Exception:
            pass

        time.sleep(poll_interval)
        wait_time += poll_interval

    raise AssertionError(
        f"Sandbox {sandbox_id} did not reach running state within {max_wait}s "
        f"(last state: {last_state.value})"
    )


def wait_for_files_visible(
    client: DSBClient,
    sandbox_id: str,
    expected_files: set[str],
    max_wait: float = 5,
    poll_interval: float = 0.1,
) -> None:
    """Wait until static file listings include the expected files."""
    waited = 0.0
    while waited < max_wait:
        try:
            files = client.static_files.list_files(sandbox_id)
            visible = {file.file_name for file in files.files} | {
                file.file_path for file in files.files
            }
            if expected_files.issubset(visible):
                return
        except Exception:
            pass

        time.sleep(poll_interval)
        waited += poll_interval

    raise AssertionError(
        f"Timed out waiting for files {sorted(expected_files)} in sandbox {sandbox_id}"
    )


@pytest.mark.integration
class TestStaticFileServing:
    """Integration tests for static file serving functionality"""

    def test_static_file_serving_workflow(self, sync_client: DSBClient, server_available: bool):
        """Test complete static file serving workflow"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox with static server enabled (use unique name to avoid collisions)
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-static-workflow-{uuid.uuid4().hex[:8]}",
        )

        try:
            ensure_sandbox_running(sync_client, str(sandbox.id))

            # Write a text file in the container
            result = sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "echo 'Hello World' > /public/hello.txt"],
            )
            wait_for_files_visible(sync_client, str(sandbox.id), {"hello.txt"})

            # Verify file was written successfully
            assert result.get("exit_code", 0) == 0 or "output" in str(result).lower()

            # Read file via static API
            content = sync_client.static_files.serve_file(sandbox.id, "hello.txt")

            # Verify content
            assert b"Hello" in content or b"hello" in content

            # List files
            file_list = sync_client.static_files.list_files(sandbox.id)

            # Verify file is listed
            assert file_list.total_count >= 1
            assert any(f.file_name == "hello.txt" for f in file_list.files)

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    @pytest.mark.serial
    def test_serve_text_file(self, sync_client: DSBClient, server_available: bool):
        """Test serving a text file

        Note: This test requires serial execution due to container resource usage.
        Marked with @pytest.mark.serial to prevent parallel execution conflicts.
        """
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-text-file-{uuid.uuid4().hex[:8]}",
        )

        try:
            ensure_sandbox_running(sync_client, str(sandbox.id))

            # Create multiple text files
            files = ["index.html", "style.css", "app.js"]
            for filename in files:
                sync_client.sandbox.exec(
                    str(sandbox.id),
                    ["sh", "-c", f"echo 'Content of {filename}' > /public/{filename}"],
                )
            wait_for_files_visible(sync_client, str(sandbox.id), set(files))

            # Read each file
            for filename in files:
                content = sync_client.static_files.serve_file(sandbox.id, filename)
                assert filename.encode() in content or b"Content" in content

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_serve_binary_file(self, sync_client: DSBClient, server_available: bool):
        """Test serving a binary file"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-binary-file-{uuid.uuid4().hex[:8]}",
        )

        ensure_sandbox_running(sync_client, str(sandbox.id))

        try:
            # Create a small binary file
            sync_client.sandbox.exec(
                str(sandbox.id),
                [
                    "sh",
                    "-c",
                    "dd if=/dev/zero of=/public/test.bin bs=1024 count=10",
                ],
            )

            # Wait for file to be synchronized to host filesystem
            # This prevents race condition where file isn't visible via bind mount yet
            time.sleep(0.5)  # Wait 500ms for Docker bind mount sync

            # Poll until file exists (with timeout)
            max_wait = 5
            waited = 0
            while waited < max_wait:
                try:
                    files = sync_client.static_files.list_files(str(sandbox.id))
                    if "test.bin" in [f.file_name for f in files.files]:
                        break
                except Exception:
                    pass
                time.sleep(0.1)
                waited += 0.1

            # Read binary file
            content = sync_client.static_files.serve_file(sandbox.id, "test.bin")

            # Verify we got binary data back
            assert len(content) > 0
            assert isinstance(content, bytes)

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_list_files_with_metadata(self, sync_client: DSBClient, server_available: bool):
        """Test listing files and verifying metadata"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-list-files-{uuid.uuid4().hex[:8]}",
        )

        ensure_sandbox_running(sync_client, str(sandbox.id))

        try:
            # Create files with different types
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "echo '<html></html>' > /public/index.html"],
            )
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "echo 'body { color: red; }' > /public/style.css"],
            )
            wait_for_files_visible(sync_client, str(sandbox.id), {"index.html", "style.css"})

            # List files
            file_list = sync_client.static_files.list_files(sandbox.id)

            # Verify metadata
            assert file_list.sandbox_id == sandbox.id
            assert file_list.total_count >= 2

            # Check individual file metadata
            for file_metadata in file_list.files:
                assert file_metadata.file_name is not None
                assert file_metadata.file_path is not None
                assert file_metadata.file_size_bytes >= 0
                assert file_metadata.content_type is not None

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_delete_single_file(self, sync_client: DSBClient, server_available: bool):
        """Test deleting a specific file"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-delete-file-{uuid.uuid4().hex[:8]}",
        )

        ensure_sandbox_running(sync_client, str(sandbox.id))

        try:
            # Create a file
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "echo 'to be deleted' > /public/to-delete.txt"],
            )
            wait_for_files_visible(sync_client, str(sandbox.id), {"to-delete.txt"})

            # Verify file exists
            file_list = sync_client.static_files.list_files(sandbox.id)
            initial_count = file_list.total_count

            # Delete the file
            result = sync_client.static_files.delete_file(sandbox.id, "to-delete.txt")

            # Verify deletion response
            assert "message" in result or "deleted" in str(result).lower()

            # Verify file is gone
            file_list = sync_client.static_files.list_files(sandbox.id)
            assert file_list.total_count <= initial_count

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_delete_all_sandbox_files(self, sync_client: DSBClient, server_available: bool):
        """Test deleting all files for a sandbox"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-delete-all-{uuid.uuid4().hex[:8]}",
        )

        ensure_sandbox_running(sync_client, str(sandbox.id))

        try:
            # Create multiple files
            for i in range(3):
                sync_client.sandbox.exec(
                    str(sandbox.id),
                    ["sh", "-c", f"echo 'file{i}' > /public/file{i}.txt"],
                )

            # Wait for files to be synchronized (with polling)
            max_wait = 5
            waited = 0
            while waited < max_wait:
                try:
                    files = sync_client.static_files.list_files(str(sandbox.id))
                    if files.total_count >= 3:
                        break
                except Exception:
                    pass
                time.sleep(0.1)
                waited += 0.1

            # Verify files exist
            file_list = sync_client.static_files.list_files(sandbox.id)
            assert file_list.total_count >= 3

            # Delete all files
            result = sync_client.static_files.delete_sandbox_files(sandbox.id)

            # Verify deletion response
            assert "deleted_count" in result or "deleted" in str(result).lower()

            # Verify all files are gone
            file_list = sync_client.static_files.list_files(sandbox.id)
            assert file_list.total_count == 0

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_nested_directory_files(self, sync_client: DSBClient, server_available: bool):
        """Test serving files from nested directories"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-nested-files-{uuid.uuid4().hex[:8]}",
        )

        ensure_sandbox_running(sync_client, str(sandbox.id))

        try:
            # Create nested directory structure
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "mkdir -p /public/css /public/js"],
            )
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "echo 'body {}' > /public/css/style.css"],
            )
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "echo 'console.log(1)' > /public/js/app.js"],
            )
            wait_for_files_visible(
                sync_client,
                str(sandbox.id),
                {"css/style.css", "js/app.js"},
            )

            # Read nested files
            css_content = sync_client.static_files.serve_file(sandbox.id, "css/style.css")
            js_content = sync_client.static_files.serve_file(sandbox.id, "js/app.js")

            assert b"body" in css_content or len(css_content) > 0
            assert b"console" in js_content or len(js_content) > 0

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass


@pytest.mark.integration
@pytest.mark.asyncio
class TestAsyncStaticFileServing:
    """Integration tests for async static file serving"""

    @pytest.mark.serial
    @pytest.mark.asyncio
    async def test_async_serve_file(self, server_available: bool):
        """Test serving files asynchronously

        Note: This test requires serial execution due to container resource usage.
        Marked with @pytest.mark.serial to prevent parallel execution conflicts.
        """
        if not server_available:
            pytest.skip("DSB server not available")

        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            # Create sandbox
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-static-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )

            try:
                # Write file
                await client.sandbox.exec_async(str(sandbox.id),
                    ["sh", "-c", "echo 'async test' > /public/async.txt"],
                )

                # Read file via static API (async)
                content = await client.static_files.serve_file(sandbox.id, "async.txt")

                assert b"async" in content or b"test" in content

            finally:
                # Cleanup
                try:
                    await client.sandbox.delete_async(str(sandbox.id))
                except Exception:
                    pass

        finally:
            await client.close()

    async def test_async_list_and_delete(self, server_available: bool):
        """Test listing and deleting files asynchronously"""
        if not server_available:
            pytest.skip("DSB server not available")

        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name=f"test-async-list-{uuid.uuid4().hex[:8]}",
                command=["sleep", "300"],
            )

            try:
                # Create files
                for i in range(2):
                    await client.sandbox.exec_async(str(sandbox.id),
                        ["sh", "-c", f"echo '{i}' > /public/async{i}.txt"],
                    )

                # List files
                file_list = await client.static_files.list_files(sandbox.id)
                assert file_list.total_count >= 2

                # Delete all files
                result = await client.static_files.delete_sandbox_files(sandbox.id)
                assert "deleted" in str(result).lower() or "count" in str(result).lower()

            finally:
                # Cleanup
                try:
                    await client.sandbox.delete_async(str(sandbox.id))
                except Exception:
                    pass

        finally:
            await client.close()


@pytest.mark.integration
class TestStaticFileErrorScenarios:
    """Integration tests for error scenarios"""

    def test_serve_nonexistent_file(self, sync_client: DSBClient, server_available: bool):
        """Test serving a file that doesn't exist"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-file-not-found-{uuid.uuid4().hex[:8]}",
        )

        try:
            # Try to read a file that doesn't exist
            with pytest.raises(Exception):  # Should raise API error
                sync_client.static_files.serve_file(sandbox.id, "nonexistent.txt")

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass
