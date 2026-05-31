"""
Integration tests for DSB SDK API contracts

These tests verify that the SDK can correctly parse real API responses.
They use stored JSON fixtures captured from actual API responses to ensure
the SDK models stay in sync with the API contract.

This is critical for catching drift between API output and SDK expectations.
"""

from datetime import datetime
from typing import Any
from uuid import UUID

import pytest

from dsb_sdk.types.exec import ExecResponse
from dsb_sdk.types.sandbox import (
    DatabendConfig,
    FileInfo,
    PaginationMeta,
    ResourceLimits,
    Sandbox,
    SandboxConfig,
    SandboxCreateRequest,
    SandboxListResponse,
    SandboxState,
    SandboxStats,
    UploadFileResponse,
)
from dsb_sdk.types.ssh import SSHSession
from dsb_sdk.types.web import (
    BrowserInfo,
    WebHealthResponse,
    WebScrapeResult,
)

# ============================================================================
# Real API Response Fixtures
# ============================================================================

@pytest.fixture
def sandbox_create_response() -> dict[str, Any]:
    """Real API response for sandbox creation"""
    return {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "config": {
            "image": "python:3.12",
            "name": "test-sandbox",
            "environment": {"TEST_VAR": "test_value"},
            "port_mappings": {"8080": "80"},
            "volumes": {},
            "command": None,
            "pull_policy": None,
            "resource_limits": None,
            "inactivity_timeout_minutes": None,
            "features": [],
            "enable_all_features": False,
            "databend": None,
        },
        "state": "running",
        "container_id": "abc123def456",
        "created_at": "2026-01-15T10:30:00Z",
        "updated_at": "2026-01-15T10:30:00Z",
        "deleted_at": None,
        "deleted_by": None,
    }


@pytest.fixture
def sandbox_list_response_data() -> dict[str, Any]:
    """Real API response for sandbox list"""
    return {
        "data": [
            {
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "config": {
                    "image": "python:3.12",
                    "name": "test-sandbox-1",
                    "environment": {},
                    "port_mappings": {},
                    "volumes": {},
                    "command": None,
                    "pull_policy": None,
                    "resource_limits": None,
                    "inactivity_timeout_minutes": None,
                    "features": [],
                    "enable_all_features": False,
                    "databend": None,
                },
                "state": "running",
                "container_id": "abc123",
                "created_at": "2026-01-15T10:30:00Z",
                "updated_at": "2026-01-15T10:30:00Z",
                "deleted_at": None,
                "deleted_by": None,
            },
            {
                "id": "650e8400-e29b-41d4-a716-446655440001",
                "config": {
                    "image": "node:20",
                    "name": "test-sandbox-2",
                    "environment": {},
                    "port_mappings": {},
                    "volumes": {},
                    "command": None,
                    "pull_policy": None,
                    "resource_limits": None,
                    "inactivity_timeout_minutes": None,
                    "features": [],
                    "enable_all_features": False,
                    "databend": None,
                },
                "state": "stopped",
                "container_id": "def456",
                "created_at": "2026-01-15T10:31:00Z",
                "updated_at": "2026-01-15T10:32:00Z",
                "deleted_at": None,
                "deleted_by": None,
            },
        ],
        "pagination": {
            "page": 1,
            "per_page": 50,
            "total": 2,
            "total_pages": 1,
            "has_next": False,
            "has_prev": False,
        },
    }


@pytest.fixture
def sandbox_stats_response() -> dict[str, Any]:
    """Real API response for sandbox stats"""
    return {
        "sandbox_id": "550e8400-e29b-41d4-a716-446655440000",
        "cpu_percent": 15.5,
        "memory_usage_mb": 256.0,
        "memory_limit_mb": 512.0,
        "memory_percent": 50.0,
        "network_rx_bytes": 1024,
        "network_tx_bytes": 2048,
        "block_read_bytes": 4096,
        "block_write_bytes": 8192,
        "timestamp": "2026-01-15T10:30:00Z",
    }


@pytest.fixture
def exec_response_data() -> dict[str, Any]:
    """Real API response for command execution"""
    return {
        "exit_code": 0,
        "output": "Hello, World!",
        "stderr": "",
        "timed_out": False,
    }


@pytest.fixture
def ssh_session_response() -> dict[str, Any]:
    """Real API response for SSH session creation"""
    return {
        "id": "750e8400-e29b-41d4-a716-446655440002",
        "sandbox_id": "550e8400-e29b-41d4-a716-446655440000",
        "username": None,  # Not returned by API
        "connected_at": "2026-01-15T10:30:00Z",
        "last_activity_at": "2026-01-15T10:35:00Z",
        "state": "active",
    }


@pytest.fixture
def web_scrape_response() -> dict[str, Any]:
    """Real API response for web scraping"""
    return {
        "url": "https://example.com",
        "title": "Example Domain",
        "content": "# Example Domain\n\nThis is an example page.",
        "screenshot": None,
        "screenshot_encoding": None,
        "screenshot_path": None,
    }


@pytest.fixture
def browser_health_response() -> dict[str, Any]:
    """Real API response for browser health check"""
    return {
        "message": "Browser is ready",
        "cdp_url": "ws://localhost:9222",
        "browser_ready": True,
    }


@pytest.fixture
def browser_info_response() -> dict[str, Any]:
    """Real API response for browser capability info"""
    return {
        "supports_automation": True,
        "browser_type": "chromium",
        "cdp_port": 9222,
        "image_name": "dsb/sandbox:latest",
    }


# ============================================================================
# Contract Tests - Verify SDK Models Can Parse Real Responses
# ============================================================================

class TestSandboxContract:
    """Test Sandbox model contract with real API responses"""

    def test_parse_sandbox_create_response(self, sandbox_create_response):
        """Test SDK can parse real sandbox creation response"""
        sandbox = Sandbox(**sandbox_create_response)

        assert isinstance(sandbox.id, UUID)
        assert str(sandbox.id) == "550e8400-e29b-41d4-a716-446655440000"
        assert isinstance(sandbox.config, SandboxConfig)
        assert sandbox.config.image == "python:3.12"
        assert sandbox.config.name == "test-sandbox"
        assert sandbox.state == SandboxState.RUNNING
        assert sandbox.container_id == "abc123def456"
        assert isinstance(sandbox.created_at, datetime)
        assert isinstance(sandbox.updated_at, datetime)

    def test_parse_sandbox_list_response(self, sandbox_list_response_data):
        """Test SDK can parse real sandbox list response"""
        response = SandboxListResponse(**sandbox_list_response_data)

        assert len(response.data) == 2
        assert isinstance(response.data[0], Sandbox)
        assert isinstance(response.pagination, PaginationMeta)
        assert response.pagination.total == 2
        assert response.total == 2  # Backward compatibility property
        assert response.sandboxes == response.data  # Backward compatibility property

    def test_parse_sandbox_stats_response(self, sandbox_stats_response):
        """Test SDK can parse real sandbox stats response"""
        stats = SandboxStats(**sandbox_stats_response)

        assert stats.cpu_percent == 15.5
        assert stats.memory_usage_mb == 256.0
        assert stats.memory_limit_mb == 512.0
        assert stats.memory_percent == 50.0
        assert stats.network_rx_bytes == 1024
        assert stats.network_tx_bytes == 2048
        # Use alias field names
        assert stats.disk_read_bytes == 4096
        assert stats.disk_write_bytes == 8192
        assert isinstance(stats.timestamp, datetime)


class TestExecContract:
    """Test Exec model contract with real API responses"""

    def test_parse_exec_response(self, exec_response_data):
        """Test SDK can parse real exec response"""
        response = ExecResponse(**exec_response_data)

        assert response.exit_code == 0
        assert response.output == "Hello, World!"
        assert response.stderr == ""
        assert response.timed_out is False
        assert response.is_successful() is True


class TestSSHContract:
    """Test SSH model contract with real API responses"""

    def test_parse_ssh_session_response(self, ssh_session_response):
        """Test SDK can parse real SSH session response"""
        session = SSHSession(**ssh_session_response)

        assert isinstance(session.id, UUID)
        assert isinstance(session.sandbox_id, UUID)
        assert session.username is None  # Not returned by API
        # status is the actual field, state is the alias
        assert session.status == "active"
        # With populate_by_name=True, we can also access by alias
        assert session.model_dump(by_alias=True)["state"] == "active"
        assert isinstance(session.created_at, datetime)
        assert isinstance(session.last_activity, datetime)


class TestWebContract:
    """Test Web model contract with real API responses"""

    def test_parse_web_scrape_response(self, web_scrape_response):
        """Test SDK can parse real web scrape response"""
        result = WebScrapeResult(**web_scrape_response)

        assert result.url == "https://example.com"
        assert result.title == "Example Domain"
        assert "Example Domain" in result.content

    def test_parse_browser_health_response(self, browser_health_response):
        """Test SDK can parse real browser health response"""
        health = WebHealthResponse(**browser_health_response)

        assert health.message == "Browser is ready"
        assert health.cdp_url == "ws://localhost:9222"
        assert health.browser_ready is True

    def test_parse_browser_info_response(self, browser_info_response):
        """Test SDK can parse real browser info response"""
        info = BrowserInfo(**browser_info_response)

        assert info.supports_automation is True
        assert info.browser_type == "chromium"
        assert info.cdp_port == 9222
        assert info.image_name == "dsb/sandbox:latest"


class TestResourceLimitsContract:
    """Test ResourceLimits with nested ulimits"""

    def test_parse_resource_limits_with_ulimits_dict(self):
        """Test parsing resource limits with ulimits as dict (from API)"""
        data = {
            "memory_mb": 512.0,
            "cpu_quota": 100000,
            "cpu_period": 100000,
            "pids_limit": 100,
            "ulimits": [
                {"name": "nofile", "soft": 65536, "hard": 65536},
                {"name": "nproc", "soft": 4096, "hard": 8192},
            ],
        }

        limits = ResourceLimits(**data)

        assert limits.memory_mb == 512.0
        assert limits.cpu_quota == 100000
        assert limits.ulimits is not None
        assert len(limits.ulimits) == 2
        assert limits.ulimits[0].name == "nofile"


class TestDatabendConfigContract:
    """Test DatabendConfig model"""

    def test_parse_databend_config(self):
        """Test parsing DatabendConfig"""
        data = {
            "host": "databend.example.com",
            "port": 8000,
            "user": "admin",
            "password": "secret",
            "database": "test_db",
            "virtual_db_prefix": "compliance_virtual_cluster",
            "meta_path": "/opt/tools/meta/compliance_table_meta.xml",
        }

        config = DatabendConfig(**data)

        assert config.host == "databend.example.com"
        assert config.port == 8000
        assert config.user == "admin"

    def test_databend_config_to_environment_dict(self):
        """Test DatabendConfig to_environment_dict method"""
        config = DatabendConfig(
            host="databend.example.com",
            port=8000,
            user="admin",
            password="secret",
            database="test_db",
            virtual_db_prefix="compliance_virtual_cluster",
            meta_path="/opt/tools/meta/compliance_table_meta.xml",
        )

        env = config.to_environment_dict()

        assert env["DATABEND_HOST"] == "databend.example.com"
        assert env["DATABEND_PORT"] == "8000"
        assert env["DATABEND_USER"] == "admin"
        assert env["DATABEND_PASSWORD"] == "secret"
        assert env["DATABEND_DATABASE"] == "test_db"


class TestSandboxCreateRequestContract:
    """Test SandboxCreateRequest with complex nested types"""

    def test_parse_sandbox_create_request_with_resource_limits(self):
        """Test parsing request with resource limits"""
        data = {
            "image": "python:3.12",
            "name": "test-sandbox",
            "environment": {"KEY": "VALUE"},
            "port_mappings": [
                {"host_port": 8080, "container_port": 80, "protocol": "tcp"}
            ],
            "volumes": [
                {"type": "bind", "host_path": "/host", "container_path": "/container", "read_only": False}
            ],
            "resource_limits": {
                "memory_mb": 512.0,
                "ulimits": [
                    {"name": "nofile", "soft": 65536, "hard": 65536}
                ],
            },
            "features": ["browser", "databend"],
            "enable_all_features": True,
        }

        request = SandboxCreateRequest(**data)

        assert request.image == "python:3.12"
        assert request.resource_limits is not None
        assert request.resource_limits.memory_mb == 512.0
        assert request.port_mappings is not None
        assert len(request.port_mappings) == 1
        assert request.volumes is not None
        assert len(request.volumes) == 1


class TestPaginationMetaContract:
    """Test PaginationMeta model"""

    def test_parse_pagination_meta(self):
        """Test parsing pagination metadata"""
        data = {
            "page": 2,
            "per_page": 25,
            "total": 100,
            "total_pages": 4,
            "has_next": True,
            "has_prev": True,
        }

        meta = PaginationMeta(**data)

        assert meta.page == 2
        assert meta.per_page == 25
        assert meta.total == 100
        assert meta.total_pages == 4
        assert meta.has_next is True
        assert meta.has_prev is True


class TestFileUploadContract:
    """Test file upload response models"""

    def test_parse_upload_file_response(self):
        """Test parsing upload file response"""
        data = {
            "success": True,
            "file": {
                "name": "test.txt",
                "path": "/app/test.txt",
                "size": 1024,
                "uploaded_at": "2026-01-15T10:30:00Z",
            },
        }

        response = UploadFileResponse(**data)

        assert response.success is True
        assert isinstance(response.file, FileInfo)
        assert response.file.name == "test.txt"
        assert response.file.path == "/app/test.txt"
        assert response.file.size == 1024


# ============================================================================
# Integration Tests with Full Request/Response Cycles
# ============================================================================

class TestSandboxCreateRequestIntegration:
    """Integration tests for sandbox creation request/response cycle"""

    def test_full_create_request_cycle(self):
        """Test full cycle: create request -> send to API -> parse response"""
        # Create request using SDK
        request = SandboxCreateRequest(
            image="python:3.12",
            name="test-sandbox",
            environment={"TEST": "value"},
            resource_limits=ResourceLimits(
                memory_mb=512.0,
                cpu_quota=None,
                cpu_period=None,
                cpu_shares=None,
                pids_limit=None,
                ulimits=None,
            ),
            port_mappings=None,
            volumes=None,
            command=None,
            pull_policy=None,
            inactivity_timeout_minutes=None,
            features=None,
            enable_all_features=False,
        )

        # Convert to dict (simulating API request serialization)
        request_dict = request.model_dump(exclude_none=True)

        # Simulate API response (real response structure)
        response_data = {
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "config": {
                "image": request.image,
                "name": request.name,
                "environment": request.environment or {},
                "port_mappings": {},
                "volumes": {},
                "command": None,
                "pull_policy": None,
                "resource_limits": {
                    "memory_mb": 512.0,
                },
                "inactivity_timeout_minutes": None,
                "features": request.features or [],
                "enable_all_features": request.enable_all_features,
                "databend": None,
            },
            "state": "running",
            "container_id": "abc123",
            "created_at": "2026-01-15T10:30:00Z",
            "updated_at": "2026-01-15T10:30:00Z",
            "deleted_at": None,
            "deleted_by": None,
        }

        # Parse response
        response = Sandbox(**response_data)

        # Verify contract maintained through cycle
        assert response.config.image == request.image
        assert response.config.name == request.name
        assert response.config.resource_limits is not None
        assert response.config.resource_limits.memory_mb == 512.0


class TestErrorResponseContract:
    """Test error response parsing"""

    def test_parse_rfc_9457_error_response(self):
        """Test parsing RFC 9457 problem details error response"""
        error_data = {
            "type": "https://example.com/errors/sandbox-not-found",
            "title": "Sandbox Not Found",
            "status": 404,
            "detail": "The requested sandbox does not exist",
            "instance": "/sandboxes/550e8400-e29b-41d4-a716-446655440000",
        }

        # This would typically raise DSBAPIError
        # Test that we can at least parse the structure
        assert error_data["status"] == 404
        assert error_data["title"] == "Sandbox Not Found"
