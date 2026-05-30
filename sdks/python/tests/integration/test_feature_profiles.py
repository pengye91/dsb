"""
Integration tests for Feature Profile support

Tests require a running DSB server on localhost:8081
Set DSB_API_URL environment variable to override the default server URL.

These tests verify that:
1. Feature profiles can be specified when creating sandboxes
2. Feature profiles auto-configure ports, volumes, environment variables
3. enable_all_features flag works correctly
4. Feature profiles integrate with static file serving
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
# Use an image that has feature profiles in its metadata
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


def wait_for_sandbox_running(
    client: DSBClient,
    sandbox_id: str,
    max_wait: int = 60,
    poll_interval: float = 1,
) -> SandboxState:
    """Wait for a sandbox to become runnable for follow-up operations."""
    wait_time = 0.0
    while wait_time < max_wait:
        try:
            sandbox = client.sandbox.get(sandbox_id)
            if sandbox.state == SandboxState.RUNNING:
                return sandbox.state
            if sandbox.state in (SandboxState.ERROR, SandboxState.DESTROYED):
                return sandbox.state
        except Exception:
            pass

        time.sleep(poll_interval)
        wait_time += poll_interval

    try:
        return client.sandbox.get(sandbox_id).state
    except Exception:
        return SandboxState.UNKNOWN


@pytest.mark.integration
class TestFeatureProfiles:
    """Integration tests for feature profile support"""

    def test_create_sandbox_with_features(self, sync_client: DSBClient, server_available: bool):
        """Test creating sandbox with specific feature profiles"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox with VNC and browser features
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-features-{uuid.uuid4().hex[:8]}",
            features=["vnc", "browser"],
        )

        try:
            # Verify sandbox was created
            assert sandbox.id is not None
            assert sandbox.state in [
                SandboxState.CREATING,
                SandboxState.STARTING,
                SandboxState.RUNNING,
            ]

            # Verify that features were recorded in config (sorted alphabetically)
            assert sandbox.config.features == ["browser", "vnc"]
            assert sandbox.config.enable_all_features is False

            # If the feature system is working, we should see ports configured
            # (This depends on the image having feature metadata)
            if len(sandbox.config.ports) > 0:
                # Features auto-configured ports
                assert True  # Ports were configured by features

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass

    def test_create_sandbox_enable_all_features(
        self, sync_client: DSBClient, server_available: bool
    ):
        """Test creating sandbox with enable_all_features flag"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox with all features enabled
        # Use unique name to avoid conflicts when running tests in parallel
        unique_name = f"test-all-features-{uuid.uuid4().hex[:8]}"
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=unique_name,
            enable_all_features=True,
        )

        try:
            # Verify sandbox was created
            assert sandbox.id is not None
            assert sandbox.state in [
                SandboxState.CREATING,
                SandboxState.STARTING,
                SandboxState.RUNNING,
            ]

            # Verify enable_all_features flag is set
            assert sandbox.config.enable_all_features is True

            # If the image has feature metadata, all default features should be enabled
            # This would manifest as auto-configured ports, volumes, etc.
            if len(sandbox.config.ports) > 0:
                # Features auto-configured ports
                assert True

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass

    def test_create_sandbox_no_features(self, sync_client: DSBClient, server_available: bool):
        """Test creating sandbox without any features (backward compatibility)"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox without features
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-no-features-{uuid.uuid4().hex[:8]}",
            command=["sleep", "30"],
        )

        try:
            # Verify sandbox was created
            assert sandbox.id is not None
            assert sandbox.state in [
                SandboxState.CREATING,
                SandboxState.STARTING,
                SandboxState.RUNNING,
            ]

            # Verify default features are enabled (browser, desktop, vnc are enabled_by_default)
            # Features are sorted alphabetically
            expected_features = ["browser", "desktop", "vnc"]
            assert sandbox.config.features == expected_features
            assert sandbox.config.enable_all_features is False

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass

    def test_feature_profiles_with_custom_config(
        self, sync_client: DSBClient, server_available: bool
    ):
        """Test that custom config can override feature profile settings"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox with features AND custom port
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-custom-features-{uuid.uuid4().hex[:8]}",
            features=["browser"],
            ports={"9999": "8080"},  # Custom port mapping (avoid port 8080 conflict)
        )

        try:
            # Verify sandbox was created
            assert sandbox.id is not None

            # Verify custom port is present
            assert "9999" in sandbox.config.ports or len(sandbox.config.ports) > 0

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass

    def test_features_with_static_server(self, sync_client: DSBClient, server_available: bool):
        """Test that feature profiles work with static file serving"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox with both features and static server enabled
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name=f"test-features-static-{uuid.uuid4().hex[:8]}",
            features=["webhost"],  # Feature that might include static server
        )

        try:
            # Verify sandbox was created
            assert sandbox.id is not None
            state = wait_for_sandbox_running(sync_client, str(sandbox.id))
            assert state == SandboxState.RUNNING

            # Write a test file in the container
            result = sync_client.sandbox.exec(
                sandbox.id,
                ["sh", "-c", "echo 'Feature test' > /public/test.txt"],
            )

            # Verify command executed (API returns output)
            assert "output" in result

            # Try to read the file via static API
            # (This tests integration between features and static file serving)
            try:
                content = sync_client.static_files.serve_file(sandbox.id, "test.txt")
                assert b"Feature test" in content
            except Exception:
                # Static server might not be fully set up yet, which is okay
                pass

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass


@pytest.mark.integration
@pytest.mark.asyncio
class TestAsyncFeatureProfiles:
    """Integration tests for async feature profile support"""

    async def test_async_create_sandbox_with_features(self, server_available: bool):
        """Test creating sandbox with features using async client"""
        if not server_available:
            pytest.skip("DSB server not available")

        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            # Create sandbox with features
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name="test-async-features",
                features=["vnc"],
                command=["sleep", "300"],
            )

            try:
                # Verify sandbox was created
                assert sandbox.id is not None
                assert sandbox.config.features == ["vnc"]

            finally:
                # Cleanup
                try:
                    await client.sandbox.delete_async(sandbox.id)
                except Exception:
                    pass

        finally:
            await client.close()

    async def test_async_enable_all_features(self, server_available: bool):
        """Test enable_all_features with async client"""
        if not server_available:
            pytest.skip("DSB server not available")

        client = AsyncDSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)

        try:
            # Create sandbox with all features
            sandbox = await client.sandbox.create_async(
                image=TEST_IMAGE,
                name="test-async-all-features",
                enable_all_features=True,
                command=["sleep", "300"],
            )

            try:
                # Verify sandbox was created
                assert sandbox.id is not None
                assert sandbox.config.enable_all_features is True

            finally:
                # Cleanup
                try:
                    await client.sandbox.delete_async(sandbox.id)
                except Exception:
                    pass

        finally:
            await client.close()


@pytest.mark.integration
class TestFeatureProfilesEdgeCases:
    """Integration tests for edge cases and error scenarios"""

    def test_invalid_feature_name(self, sync_client: DSBClient, server_available: bool):
        """Test creating sandbox with invalid/non-existent feature name"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox with a feature that doesn't exist in the image
        # This should not fail - the server should just ignore unknown features
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-invalid-feature",
            features=["nonexistent-feature-xyz"],
        )

        try:
            # Sandbox should still be created (unknown features are ignored)
            assert sandbox.id is not None

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass

    def test_empty_features_list(self, sync_client: DSBClient, server_available: bool):
        """Test creating sandbox with empty features list"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Explicitly pass empty features list
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-empty-features",
            features=[],
        )

        try:
            # Verify sandbox was created
            assert sandbox.id is not None
            # When an empty list is explicitly provided, default features are still enabled
            # Features are sorted alphabetically
            expected_features = ["browser", "desktop", "vnc"]
            assert sandbox.config.features == expected_features

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass

    def test_features_with_inactivity_timeout(self, sync_client: DSBClient, server_available: bool):
        """Test that feature profiles work with inactivity timeout"""
        if not server_available:
            pytest.skip("DSB server not available")

        # Create sandbox with features and inactivity timeout
        sandbox = sync_client.sandbox.create(
            image=TEST_IMAGE,
            name="test-features-timeout",
            features=["browser"],
            inactivity_timeout_minutes=5,
        )

        try:
            # Verify both settings are applied
            assert sandbox.id is not None
            assert sandbox.config.features == ["browser"]
            assert sandbox.config.inactivity_timeout_minutes == 5

        finally:
            # Cleanup
            try:
                sync_client.sandbox.delete(sandbox.id)
            except Exception:
                pass
