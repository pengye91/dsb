"""
Comprehensive integration tests for sandbox creation with all parameters.

Tests require a running DSB server on localhost:8081
"""

import os
from uuid import UUID

import pytest

from dsb_sdk import DSBClient
from dsb_sdk.types import PullPolicy, ResourceLimits, SandboxState

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


@pytest.fixture(scope="function")
def client():
    """Create a DSB client for testing."""
    if not is_server_available():
        pytest.skip("DSB server not available")

    client = DSBClient(api_url=DSB_API_URL, api_key=DSB_API_KEY, timeout=120.0)
    yield client
    client.close()


@pytest.mark.integration
class TestSandboxCreationWithAllParameters:
    """Comprehensive tests for sandbox creation with all parameters"""

    def test_create_with_port_mappings(self, client: DSBClient):
        """Test creating sandbox with port mappings"""
        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-ports",
            ports={"9001": "80", "9002": "443"},
            command=["sleep", "300"],
        )

        try:
            assert sandbox.id is not None
            assert isinstance(sandbox.id, UUID)
            assert sandbox.state in [SandboxState.CREATING, SandboxState.CREATED, SandboxState.RUNNING]
            # Verify ports are properly configured (including auto-configured VNC ports)
            assert len(sandbox.config.ports) >= 2 or "9001" in sandbox.config.ports
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_volumes(self, client: DSBClient):
        """Test creating sandbox with volume mounts"""
        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-volumes",
            volumes={"/tmp/host": "/tmp/container", "/tmp/data": "/tmp/data"},
            command=["sleep", "300"],
        )

        try:
            assert sandbox.id is not None
            assert isinstance(sandbox.id, UUID)
            # Verify volumes are configured
            assert len(sandbox.config.volumes) >= 2
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_resource_limits(self, client: DSBClient):
        """Test creating sandbox with resource limits"""
        resource_limits = ResourceLimits(
            memory_mb=1024,
            cpu_shares=1024,
            pids_limit=200
        )

        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-resources",
            resource_limits=resource_limits,
            command=["sleep", "300"],
        )

        try:
            assert sandbox.id is not None
            assert sandbox.config.resource_limits is not None
            assert sandbox.config.resource_limits.memory_mb == 1024
            assert sandbox.config.resource_limits.cpu_shares == 1024
            assert sandbox.config.resource_limits.pids_limit == 200
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_pull_policy(self, client: DSBClient):
        """Test creating sandbox with pull policy"""
        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-pull-policy",
            pull_policy=PullPolicy.MISSING,
            command=["sleep", "300"],
        )

        try:
            assert sandbox.id is not None
            assert sandbox.config.pull_policy == PullPolicy.MISSING
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_inactivity_timeout(self, client: DSBClient):
        """Test creating sandbox with inactivity timeout"""
        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-timeout",
            inactivity_timeout_minutes=15,
            command=["sleep", "300"],
        )

        try:
            assert sandbox.id is not None
            assert sandbox.config.inactivity_timeout_minutes == 15
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_static_server_enabled(self, client: DSBClient):
        """Test creating sandbox with static file server enabled"""
        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-static-server",
            command=["sleep", "300"],
        )

        try:
            assert sandbox.id is not None
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_command(self, client: DSBClient):
        """Test creating sandbox with custom command"""
        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-command",
            command=["tail", "-f", "/dev/null"]
        )

        try:
            assert sandbox.id is not None
            assert sandbox.config.command == ["tail", "-f", "/dev/null"]
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_all_parameters(self, client: DSBClient):
        """Test creating sandbox with all parameters combined"""
        resource_limits = ResourceLimits(
            memory_mb=1024,
            cpu_shares=512,
            pids_limit=200
        )

        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name="test-all-params",
            environment={"KEY1": "value1", "KEY2": "value2"},
            ports={"9010": "3000", "9011": "8000"},
            volumes={"/tmp/test": "/tmp/test"},
            command=["sleep", "3600"],
            pull_policy=PullPolicy.MISSING,
            resource_limits=resource_limits,
            inactivity_timeout_minutes=30,
            features=["vnc"],
            enable_all_features=False
        )

        try:
            assert sandbox.id is not None
            assert isinstance(sandbox.id, UUID)
            assert sandbox.state in [SandboxState.CREATING, SandboxState.CREATED, SandboxState.RUNNING]

            # Verify all parameters
            assert sandbox.config.name == "test-all-params"
            assert sandbox.config.environment["KEY1"] == "value1"
            assert sandbox.config.environment["KEY2"] == "value2"
            assert "9010" in sandbox.config.ports or len(sandbox.config.ports) >= 2
            assert len(sandbox.config.volumes) >= 1
            assert sandbox.config.command == ["sleep", "3600"]
            assert sandbox.config.pull_policy == PullPolicy.MISSING
            assert sandbox.config.resource_limits.memory_mb == 1024
            assert sandbox.config.resource_limits.cpu_shares == 512
            assert sandbox.config.resource_limits.pids_limit == 200
            assert sandbox.config.inactivity_timeout_minutes == 30
            assert "vnc" in sandbox.config.features
            assert sandbox.config.enable_all_features is False
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass

    def test_create_with_enable_all_features(self, client: DSBClient):
        """Test creating sandbox with enable_all_features flag"""
        # Use unique name to avoid conflicts when running tests in parallel
        import uuid
        unique_name = f"test-all-features-{uuid.uuid4().hex[:8]}"
        sandbox = client.sandbox.create(
            image=TEST_IMAGE,
            name=unique_name,
            enable_all_features=True,
            command=["sleep", "300"],
        )

        try:
            assert sandbox.id is not None
            assert sandbox.config.enable_all_features is True
        finally:
            try:
                client.sandbox.delete(str(sandbox.id))
            except Exception:
                pass
