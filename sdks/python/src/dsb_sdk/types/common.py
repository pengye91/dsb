"""
Common type definitions
"""

from __future__ import annotations

from datetime import datetime
from typing import Any

from pydantic import BaseModel, ConfigDict, Field, field_serializer


class HealthStatus(BaseModel):
    """API health status"""

    status: str = Field(..., description="Health status (ok/unhealthy)")
    version: str | None = Field(None, description="DSB server version")
    uptime_seconds: float | None = Field(None, description="Server uptime in seconds")
    timestamp: datetime | None = Field(None, description="Status check timestamp")

    @field_serializer("timestamp")
    @classmethod
    def serialize_datetime(cls, v: datetime | None) -> str | None:
        """Serialize datetime to ISO format string."""
        if v is None:
            return None
        return v.isoformat()


class Activity(BaseModel):
    """Activity record"""

    id: str = Field(..., description="Activity ID")
    sandbox_id: str = Field(..., description="Associated sandbox ID")
    action: str = Field(..., alias="activity_type", description="Activity action")
    timestamp: datetime = Field(..., description="Activity timestamp")
    details: dict[str, Any] = Field(default_factory=dict, description="Activity details")

    model_config = ConfigDict(populate_by_name=True)

    @field_serializer("timestamp")
    @classmethod
    def serialize_datetime(cls, v: datetime) -> str:
        """Serialize datetime to ISO format string."""
        return v.isoformat()


class ActivityListResponse(BaseModel):
    """Response containing list of activities"""

    activities: list[Activity] = Field(default_factory=list, description="List of activities")
    total: int = Field(..., description="Total count")
