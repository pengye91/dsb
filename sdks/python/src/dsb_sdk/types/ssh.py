"""
SSH session type definitions
"""

from __future__ import annotations

from datetime import datetime
from uuid import UUID

from pydantic import BaseModel, ConfigDict, Field, field_serializer


class SSHSession(BaseModel):
    """SSH session instance"""

    id: UUID = Field(..., description="Unique session identifier")
    sandbox_id: UUID = Field(..., description="Associated sandbox ID")
    username: str | None = Field(None, description="SSH username (not returned by API)")
    created_at: datetime = Field(..., alias="connected_at", description="Session creation time")
    last_activity: datetime | None = Field(
        None, alias="last_activity_at", description="Last activity timestamp"
    )
    status: str = Field(..., alias="state", description="Session status")

    model_config = ConfigDict(populate_by_name=True)

    @field_serializer("id", "sandbox_id")
    @classmethod
    def serialize_uuid(cls, v: UUID) -> str:
        """Serialize UUID to string."""
        return str(v)

    @field_serializer("created_at", "last_activity")
    @classmethod
    def serialize_datetime(cls, v: datetime | None) -> str | None:
        """Serialize datetime to ISO format string."""
        if v is None:
            return None
        return v.isoformat()


class SSHSessionConfig(BaseModel):
    """Configuration for creating SSH session"""

    sandbox_id: UUID = Field(..., description="Sandbox to connect to")
    client_ip: str = Field(..., description="Client IP address")
    auth_method: str = Field(
        default="api_key", description="Authentication method (api_key or certificate)"
    )
    username: str | None = Field(None, description="SSH username")
    public_key: str | None = Field(None, description="SSH public key")
    ssh_version: str | None = Field(None, description="SSH protocol version")

    @field_serializer("sandbox_id")
    @classmethod
    def serialize_uuid(cls, v: UUID) -> str:
        """Serialize UUID to string."""
        return str(v)


class SSHSessionListResponse(BaseModel):
    """Response containing list of SSH sessions"""

    sessions: list[SSHSession] = Field(default_factory=list, description="List of sessions")
    total: int = Field(..., description="Total count")
