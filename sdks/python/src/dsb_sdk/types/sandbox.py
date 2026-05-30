"""
Sandbox type definitions using Pydantic v2
"""

from __future__ import annotations

from datetime import datetime
from enum import Enum
from typing import Any
from uuid import UUID

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    field_serializer,
    field_validator,
)


class PullPolicy(str, Enum):
    """Docker image pull policy"""

    ALWAYS = "always"
    MISSING = "missing"
    NEVER = "never"


class Ulimit(BaseModel):
    """Container resource limit (ulimit)."""

    name: str = Field(..., description="Limit name (e.g., 'nofile', 'nproc')")
    soft: int = Field(..., ge=0, description="Soft limit")
    hard: int = Field(..., ge=0, description="Hard limit")


class ResourceLimits(BaseModel):
    """Resource limits for a sandbox"""

    memory_mb: float | None = Field(None, ge=0, description="Memory limit in MB")
    cpu_quota: int | None = Field(None, ge=0, description="CPU quota (microseconds)")
    cpu_period: int | None = Field(None, ge=0, description="CPU period (microseconds)")
    cpu_shares: int | None = Field(None, ge=0, description="CPU shares")
    pids_limit: int | None = Field(None, ge=1, description="Max number of processes")
    ulimits: list[Ulimit] | None = Field(
        None, description="Resource limits (ulimits)"
    )

    @field_validator("ulimits", mode="before")
    @classmethod
    def convert_empty_list(cls, v):
        """Convert empty list to None for ulimits"""
        if v == []:
            return None
        return v


class DatabendConfig(BaseModel):
    """
    Databend database configuration for sandbox.

    When provided in sandbox config, these credentials will be automatically
    injected as environment variables into the sandbox, making them available
    to databend_tools.py scripts.
    """

    host: str = Field(..., description="Databend server hostname")
    port: int = Field(8000, ge=1, le=65535, description="Databend server port")
    user: str | None = Field(None, description="Databend username")
    password: str | None = Field(None, description="Databend password")
    database: str = Field("default", description="Default database name")
    virtual_db_prefix: str = Field(
        "my_virtual_db",
        description="Virtual database prefix for metadata"
    )
    meta_path: str = Field(
        "/opt/tools/meta/table_meta.xml",
        description="Path to virtual database XML schema"
    )

    def to_environment_dict(self) -> dict[str, str]:
        """
        Convert DatabendConfig to environment variables dictionary.

        Returns dictionary with keys:
        - DATABEND_HOST
        - DATABEND_PORT
        - DATABEND_USER
        - DATABEND_PASSWORD
        - DATABEND_DATABASE
        - DATABEND_VIRTUAL_DB_PREFIX
        - DATABEND_META_PATH

        Returns:
            Dictionary of environment variables suitable for sandbox.exec()
        """
        env = {
            "DATABEND_HOST": self.host,
            "DATABEND_PORT": str(self.port),
            "DATABEND_DATABASE": self.database,
            "DATABEND_VIRTUAL_DB_PREFIX": self.virtual_db_prefix,
            "DATABEND_META_PATH": self.meta_path,
        }

        if self.user is not None:
            env["DATABEND_USER"] = self.user

        if self.password is not None:
            env["DATABEND_PASSWORD"] = self.password

        return env


class SandboxState(str, Enum):
    """Possible states of a sandbox"""

    UNKNOWN = "unknown"
    CREATING = "creating"
    CREATED = "created"
    STARTING = "starting"
    RUNNING = "running"
    STOPPED = "stopped"
    ERROR = "error"
    DESTROYING = "destroying"
    DESTROYED = "destroyed"


class SandboxConfig(BaseModel):
    """Sandbox configuration"""

    image: str = Field(..., description="Docker image to use")
    name: str | None = Field(None, description="Optional sandbox name")
    environment: dict[str, str] = Field(default_factory=dict, description="Environment variables")
    ports: dict[str, str] = Field(
        default_factory=dict, alias="port_mappings", description="Port mappings (host:container)"
    )
    volumes: dict[str, str] = Field(
        default_factory=dict, description="Volume mounts (host:container)"
    )
    command: list[str] | None = Field(None, description="Override default command")
    pull_policy: PullPolicy | None = Field(
        None, description="Image pull policy (always, missing, never)"
    )
    resource_limits: ResourceLimits | None = Field(None, description="Resource limits")
    inactivity_timeout_minutes: int | None = Field(
        None, ge=0, description="Auto-stop after inactivity (minutes)"
    )
    features: list[str] = Field(
        default_factory=list, description="Feature profiles to enable from image metadata"
    )
    enable_all_features: bool = Field(
        default=False, description="Enable all available features from image metadata"
    )
    databend: DatabendConfig | None = Field(
        None, description="Databend database configuration (auto-injects credentials)"
    )

    model_config = ConfigDict(populate_by_name=True)

    @field_validator("image")
    @classmethod
    def validate_image(cls, v: str) -> str:
        """Validate image name is not empty"""
        if not v or not v.strip():
            raise ValueError("image cannot be empty")
        return v.strip()

    @field_validator("volumes", mode="before")
    @classmethod
    def convert_volumes(cls, v):
        """Convert volumes from API format to SDK dict format"""
        if v == []:
            return {}
        if isinstance(v, list):
            # Convert from API format: [{"type": "bind", "host_path": "/host", "container_path": "/container", "read_only": false}]
            # To SDK format: {"/host": "/container"}
            result = {}
            for volume in v:
                if isinstance(volume, dict):
                    # For bind mounts, use host_path and container_path
                    if volume.get("type") == "bind":
                        host_path = volume.get("host_path")
                        container_path = volume.get("container_path")
                        if host_path and container_path:
                            result[host_path] = container_path
                    # For named volumes, use name and container_path
                    elif "name" in volume:
                        name = volume.get("name")
                        container_path = volume.get("container_path")
                        if name and container_path:
                            result[name] = container_path
            return result
        return v

    @field_validator("ports", mode="before")
    @classmethod
    def convert_port_mappings(cls, v):
        """Convert port_mappings list to dict"""
        if isinstance(v, list):
            # Convert from API format: [{"host_port": 9222, "container_port": 9222, "protocol": "tcp"}]
            # To SDK format: {"9222": "9222"}
            result = {}
            for mapping in v:
                if isinstance(mapping, dict):
                    host_port = mapping.get("host_port")
                    container_port = mapping.get("container_port")
                    if host_port is not None and container_port is not None:
                        result[str(host_port)] = str(container_port)
            return result
        return v


class Sandbox(BaseModel):
    """Sandbox instance"""

    id: UUID = Field(..., description="Unique sandbox identifier")
    config: SandboxConfig = Field(..., description="Sandbox configuration")
    state: SandboxState = Field(..., description="Current sandbox state")
    container_id: str | None = Field(None, description="Backend instance ID (Docker container ID or K8s Pod name)")
    created_at: datetime = Field(..., description="Creation timestamp")
    updated_at: datetime = Field(..., description="Last update timestamp")
    deleted_at: datetime | None = Field(
        None, description="Soft delete timestamp (None if not deleted)"
    )
    deleted_by: str | None = Field(
        None, description="User/system that performed the deletion"
    )

    @field_serializer("created_at", "updated_at", "deleted_at")
    @classmethod
    def serialize_datetime(cls, v: datetime | None) -> str | None:
        """Serialize datetime to ISO format string."""
        if v is None:
            return None
        return v.isoformat()


class SandboxStats(BaseModel):
    """Sandbox resource usage statistics"""

    sandbox_id: UUID | None = Field(
        None, description="Sandbox identifier (optional in API response)"
    )
    cpu_percent: float = Field(..., description="CPU usage percentage")
    memory_usage_mb: float = Field(..., description="Memory usage in MB")
    memory_limit_mb: float = Field(..., description="Memory limit in MB")
    memory_percent: float = Field(..., description="Memory usage percentage")
    network_rx_bytes: int = Field(..., description="Network bytes received")
    network_tx_bytes: int = Field(..., description="Network bytes transmitted")
    disk_read_bytes: int = Field(..., alias="block_read_bytes", description="Disk bytes read")
    disk_write_bytes: int = Field(..., alias="block_write_bytes", description="Disk bytes written")
    timestamp: datetime = Field(..., description="Statistics timestamp")

    model_config = ConfigDict(populate_by_name=True)

    @field_serializer("timestamp")
    @classmethod
    def serialize_datetime(cls, v: datetime) -> str:
        """Serialize datetime to ISO format string."""
        return v.isoformat()


class SandboxCreateRequest(BaseModel):
    """Request to create a sandbox"""

    image: str = Field(..., description="Docker image to use")
    name: str | None = Field(None, description="Optional sandbox name")
    environment: dict[str, str] | None = Field(None, description="Environment variables")
    port_mappings: list[dict[str, Any]] | None = Field(None, description="Port mappings (API format)")
    volumes: list[dict[str, Any]] | None = Field(None, description="Volume mounts (API format)")
    command: list[str] | None = Field(None, description="Override command")
    pull_policy: str | None = Field(None, description="Image pull policy")
    resource_limits: ResourceLimits | dict[str, Any] | None = Field(None, description="Resource limits")
    inactivity_timeout_minutes: int | None = Field(None, description="Auto-stop after inactivity")
    features: list[str] | None = Field(None, description="Feature profiles to enable from image metadata")
    enable_all_features: bool = Field(False, description="Enable all available features from image metadata")

    @field_validator("resource_limits", mode="before")
    @classmethod
    def convert_resource_limits(cls, v):
        """Convert ResourceLimits object to dict and transform ulimits"""
        if isinstance(v, ResourceLimits):
            limits_dict = v.model_dump(exclude_none=True)
            # Transform ulimits from dict format to list format for Rust compatibility
            if "ulimits" in limits_dict and isinstance(limits_dict["ulimits"], dict):
                ulimits_dict = limits_dict.pop("ulimits")
                limits_dict["ulimits"] = [
                    {"name": name, "soft": values["soft"], "hard": values["hard"]}
                    for name, values in ulimits_dict.items()
                ]
            # No longer force ulimits to empty array - let it be None if not provided
            return limits_dict
        return v


class SandboxProgressEvent(BaseModel):
    """Sandbox creation progress event"""

    event: str = Field(..., description="Event type: pulling, creating, starting, ready, error")
    message: str = Field(default="", description="Human-readable message")
    progress: int = Field(default=0, ge=0, le=100, description="Progress percentage")
    details: str | None = Field(None, description="Additional details")


class SandboxListResponse(BaseModel):
    """Response containing list of sandboxes with pagination"""

    data: list[Sandbox] = Field(default_factory=list, description="List of sandboxes")
    pagination: PaginationMeta = Field(
        default_factory=lambda: PaginationMeta(),
        description="Pagination metadata"
    )

    @property
    def total(self) -> int:
        """Backward compatibility property for total count"""
        return self.pagination.total

    @property
    def sandboxes(self) -> list[Sandbox]:
        """Backward compatibility property for accessing sandboxes"""
        return self.data


class PaginationMeta(BaseModel):
    """Pagination metadata for list responses"""

    page: int = Field(default=1, description="Current page number")
    per_page: int = Field(default=50, ge=1, le=200, description="Items per page")
    total: int = Field(default=0, ge=0, description="Total number of items")
    total_pages: int = Field(default=0, ge=0, description="Total number of pages")
    has_next: bool = Field(default=False, description="Whether there's a next page")
    has_prev: bool = Field(default=False, description="Whether there's a previous page")


class StaticFileMetadata(BaseModel):
    """Metadata for a single static file"""

    file_name: str = Field(..., description="File name")
    file_path: str = Field(..., description="File path relative to mount")
    file_size_bytes: int = Field(..., ge=0, description="File size in bytes")
    content_type: str = Field(..., description="MIME content type")


class StaticFileList(BaseModel):
    """List of static files for a sandbox"""

    sandbox_id: UUID = Field(..., description="Sandbox identifier")
    files: list[StaticFileMetadata] = Field(default_factory=list, description="File metadata list")
    total_count: int = Field(..., ge=0, description="Total number of files")
    total_size_bytes: int = Field(..., ge=0, description="Total size in bytes")


class FileInfo(BaseModel):
    """Information about an uploaded file"""

    name: str = Field(..., description="Original filename")
    path: str = Field(..., description="Sanitized destination path")
    size: int = Field(..., ge=0, description="File size in bytes")
    uploaded_at: datetime = Field(..., description="Upload timestamp")

    @field_serializer("uploaded_at")
    @classmethod
    def serialize_datetime(cls, v: datetime) -> str:
        """Serialize datetime to ISO format string."""
        return v.isoformat()


class UploadFileResponse(BaseModel):
    """Response from file upload endpoint"""

    success: bool = Field(..., description="Whether upload succeeded")
    file: FileInfo = Field(..., description="Uploaded file metadata")


class FileDownloadResponse(BaseModel):
    """Response from file download endpoint"""

    name: str = Field(..., description="Filename from container")
    path: str = Field(..., description="Full path in container")
    size: int = Field(..., ge=0, description="File size in bytes")
    content_type: str = Field(..., description="MIME content type")
    content: bytes = Field(..., description="File content as bytes")
