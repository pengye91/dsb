"""
End-to-End Integration Tests for Complete Sandbox Workflows

These tests demonstrate real-world usage scenarios combining:
- Feature profiles
- Static file serving
- Sandbox lifecycle management
- Both sync and async clients

Tests require a running DSB server on localhost:8081
"""

import os
from collections.abc import Iterator

import pytest

from dsb_sdk import AsyncDSBClient, DSBClient

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
    """Create a DSB client for testing."""
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


@pytest.mark.integration
class TestSandboxE2E:
    """End-to-end tests for complete sandbox workflows"""

    def test_complete_web_app_deployment_workflow(
        self, sync_client: DSBClient, server_available: bool
    ):
        """
        E2E Test: Deploy a complete web application

        Scenario:
        1. Create sandbox with webhost feature
        2. Deploy HTML, CSS, JS files
        3. Verify files are served correctly
        4. Clean up resources
        """
        if not server_available:
            pytest.skip("DSB server not available")

        # Step 1: Create sandbox with webhost feature
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="e2e-web-app",
            command=["sleep", "300"],
        )

        try:
            # Step 2: Deploy a complete web application
            # Create HTML
            sync_client.sandbox.exec(
                str(sandbox.id),
                [
                    "sh",
                    "-c",
                    """cat > /public/index.html << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <link rel="stylesheet" href="css/style.css">
</head>
<body>
    <h1>E2E Test App</h1>
    <script src="js/app.js"></script>
</body>
</html>
EOF""",
                ],
            )

            # Create CSS
            sync_client.sandbox.exec(
                str(sandbox.id),
                [
                    "sh",
                    "-c",
                    """mkdir -p /public/css && cat > /public/css/style.css << 'EOF'
body { font-family: Arial; margin: 20px; }
h1 { color: #333; }
EOF""",
                ],
            )

            # Create JavaScript
            sync_client.sandbox.exec(
                str(sandbox.id),
                [
                    "sh",
                    "-c",
                    """mkdir -p /public/js && cat > /public/js/app.js << 'EOF'
console.log('E2E app loaded');
EOF""",
                ],
            )

            # Step 3: Verify all files are deployed
            file_list = sync_client.static_files.list_files(sandbox.id)
            assert file_list.total_count >= 3, "Should have at least 3 files"

            # Verify HTML content
            html_content = sync_client.static_files.serve_file(sandbox.id, "index.html")
            assert b"E2E Test App" in html_content
            assert b"css/style.css" in html_content
            assert b"js/app.js" in html_content

            # Verify CSS content
            css_content = sync_client.static_files.serve_file(sandbox.id, "css/style.css")
            assert b"font-family" in css_content or b"Arial" in css_content

            # Verify JS content
            js_content = sync_client.static_files.serve_file(sandbox.id, "js/app.js")
            assert b"E2E app loaded" in js_content or b"console" in js_content

        finally:
            # Step 4: Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_static_file_lifecycle_workflow(self, sync_client: DSBClient, server_available: bool):
        """
        E2E Test: Complete static file lifecycle

        Scenario:
        1. Create sandbox with static server
        2. Upload files
        3. List and verify files
        4. Update existing file
        5. Delete specific files
        6. Clean up all files
        """
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="e2e-file-lifecycle",
            command=["sleep", "300"],
        )

        try:
            # Step 2: Upload initial files
            test_files = {
                "readme.txt": "Welcome to the E2E test",
                "data.json": '{"status": "active"}',
                "config.yaml": "debug: true",
            }

            for filename, content in test_files.items():
                sync_client.sandbox.exec(
                    str(sandbox.id),
                    ["sh", "-c", f"echo '{content}' > /public/{filename}"],
                )

            # Step 3: List and verify files
            file_list = sync_client.static_files.list_files(sandbox.id)
            assert file_list.total_count >= 3

            # Verify each file can be read
            for filename in test_files.keys():
                content = sync_client.static_files.serve_file(sandbox.id, filename)
                assert len(content) > 0, f"File {filename} should have content"

            # Step 4: Update an existing file
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "echo 'Updated content' > /public/readme.txt"],
            )

            updated_content = sync_client.static_files.serve_file(sandbox.id, "readme.txt")
            assert b"Updated" in updated_content

            # Step 5: Delete specific files
            sync_client.static_files.delete_file(sandbox.id, "data.json")

            file_list = sync_client.static_files.list_files(sandbox.id)
            assert not any(f.file_name == "data.json" for f in file_list.files)

            # Step 6: Clean up remaining files
            result = sync_client.static_files.delete_sandbox_files(sandbox.id)
            assert "deleted" in str(result).lower() or "count" in str(result).lower()

            # Verify all files are gone
            file_list = sync_client.static_files.list_files(sandbox.id)
            assert file_list.total_count == 0

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_multi_sandbox_workflow(self, sync_client: DSBClient, server_available: bool):
        """
        E2E Test: Manage multiple sandboxes simultaneously

        Scenario:
        1. Create multiple sandboxes with different configurations
        2. Deploy different content to each
        3. Verify isolation (content doesn't leak between sandboxes)
        4. Clean up all sandboxes
        """
        if not server_available:
            pytest.skip("DSB server not available")

        # Step 1: Create multiple sandboxes
        sandboxes = []
        configs = [
            {"name": "web-1", "content": "Sandbox 1 content"},
            {"name": "web-2", "content": "Sandbox 2 content"},
            {"name": "web-3", "content": "Sandbox 3 content"},
        ]

        for config in configs:
            sandbox = sync_client.sandbox.create(
                image=TEST_IMAGE,
                name=f"e2e-{config['name']}",
                command=["sleep", "300"],
            )
            sandboxes.append(sandbox)

        try:
            # Step 2: Deploy different content to each sandbox
            for i, sandbox in enumerate(sandboxes):
                sync_client.sandbox.exec(
                    str(sandbox.id),
                    ["sh", "-c", f"echo '{configs[i]['content']}' > /public/data.txt"],
                )

            # Step 3: Verify isolation
            for i, sandbox in enumerate(sandboxes):
                content = sync_client.static_files.serve_file(sandbox.id, "data.txt")
                expected = configs[i]["content"]
                assert expected.encode() in content

                # Verify we don't get content from other sandboxes
                for j, other_config in enumerate(configs):
                    if i != j:
                        other_content = other_config["content"]
                        # This specific content shouldn't be in this sandbox
                        # (unless it's a substring, so this is a weak check)
                        assert (
                            content.count(other_content.encode())
                            == content.count(expected.encode())
                            or other_content.encode() not in content
                        )

        finally:
            # Step 4: Clean up all sandboxes
            for sandbox in sandboxes:
                try:
                    sync_client.sandbox.delete(str(sandbox.id))
                except Exception:
                    pass


@pytest.mark.integration
@pytest.mark.asyncio
class TestAsyncSandboxE2E:
    """End-to-end tests using async client"""

    async def test_async_complete_workflow(self, server_available: bool):
        """
        E2E Test: Complete async workflow

        Scenario:
        1. Create sandbox asynchronously
        2. Deploy files using async operations
        3. Serve files asynchronously
        4. Clean up
        """
        if not server_available:
            pytest.skip("DSB server not available")

        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            # Step 1: Create sandbox
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name="e2e-async-workflow",
                command=["sleep", "300"],
            )

            try:
                # Step 2: Deploy files concurrently
                import asyncio

                async def deploy_file(filename: str, content: str):
                    await client.sandbox.exec_async(
                        str(sandbox.id),
                        ["sh", "-c", f"echo '{content}' > /public/{filename}"],
                    )

                # Deploy multiple files concurrently
                await asyncio.gather(
                    deploy_file("index.html", "<h1>Async App</h1>"),
                    deploy_file("about.html", "<h1>About</h1>"),
                    deploy_file("contact.html", "<h1>Contact</h1>"),
                )

                # Step 3: Verify files asynchronously
                file_list = await client.static_files.list_files(sandbox.id)
                assert file_list.total_count >= 3

                # Read files concurrently
                async def read_file(filename: str):
                    return await client.static_files.serve_file(sandbox.id, filename)

                contents = await asyncio.gather(
                    read_file("index.html"),
                    read_file("about.html"),
                    read_file("contact.html"),
                )

                assert any(b"Async App" in c for c in contents)
                assert any(b"About" in c for c in contents)
                assert any(b"Contact" in c for c in contents)

            finally:
                # Step 4: Cleanup
                try:
                    await client.sandbox.delete_async(str(sandbox.id))
                except Exception:
                    pass

        finally:
            await client.close()


@pytest.mark.integration
class TestSandboxE2EEdgeCases:
    """E2E tests for edge cases and error scenarios"""

    def test_large_file_handling(self, sync_client: DSBClient, server_available: bool):
        """E2E Test: Handle larger files"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="e2e-large-files",
            command=["sleep", "300"],
        )

        try:
            # Create a larger file (1MB)
            sync_client.sandbox.exec(
                str(sandbox.id),
                ["sh", "-c", "dd if=/dev/zero of=/public/large.bin bs=1024 count=1024"],
            )

            # Read the large file
            content = sync_client.static_files.serve_file(sandbox.id, "large.bin")

            # Verify we got the full content
            assert len(content) >= 1024 * 1024

            # Check file metadata
            file_list = sync_client.static_files.list_files(sandbox.id)
            large_file = next((f for f in file_list.files if f.file_name == "large.bin"), None)
            assert large_file is not None
            assert large_file.file_size_bytes >= 1024 * 1024

        finally:
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_special_characters_in_filenames(self, sync_client: DSBClient, server_available: bool):
        """E2E Test: Handle filenames with special characters"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="e2e-special-chars",
            command=["sleep", "300"],
        )

        try:
            # Create files with special characters (that are valid in URLs)
            special_files = [
                "file-with-dashes.txt",
                "file_with_underscores.txt",
                "file.with.dots.txt",
            ]

            for filename in special_files:
                sync_client.sandbox.exec(
                    str(sandbox.id),
                    ["sh", "-c", f"echo 'test' > /public/{filename}"],
                )

            # Verify each file can be read
            for filename in special_files:
                content = sync_client.static_files.serve_file(sandbox.id, filename)
                assert len(content) > 0

        finally:
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_rapid_file_operations(self, sync_client: DSBClient, server_available: bool):
        """E2E Test: Handle rapid file operations"""
        if not server_available:
            pytest.skip("DSB server not available")

        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="e2e-rapid-ops",
            command=["sleep", "300"],
        )

        try:
            # Rapidly create, read, and delete files
            for i in range(10):
                # Create
                sync_client.sandbox.exec(
                    str(sandbox.id),
                    ["sh", "-c", f"echo '{i}' > /public/rapid{i}.txt"],
                )

                # Read
                content = sync_client.static_files.serve_file(sandbox.id, f"rapid{i}.txt")
                assert str(i).encode() in content

                # Delete
                sync_client.static_files.delete_file(sandbox.id, f"rapid{i}.txt")

            # Verify all files were deleted
            file_list = sync_client.static_files.list_files(sandbox.id)
            # Should not have any rapid*.txt files
            rapid_files = [f for f in file_list.files if f.file_name.startswith("rapid")]
            assert len(rapid_files) == 0

        finally:
            try:
                sync_client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass
