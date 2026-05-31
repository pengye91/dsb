"""
Activities API implementation (synchronous)

Provides synchronous methods for activity tracking.
Use with DSBClient.
"""

from __future__ import annotations

from typing import Any

from dsb_sdk.transport.sync import SyncTransport
from dsb_sdk.types.common import ActivityListResponse


class ActivitiesAPI:
    """
    API for activity tracking (synchronous).

    Use with DSBClient for synchronous operations.
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize activities API.

        Args:
            transport: SyncTransport instance
        """
        self.transport = transport

    def list(self, sandbox_id: str | None = None, activity_type: str | None = None, limit: int | None = None) -> ActivityListResponse:
        """
        List all activities.

        Returns:
            ActivityListResponse with list of activities
        """
        response = self.transport.request(
            method="GET",
            path="/activities",
        )
        # API returns a list directly, wrap it in the expected response format
        if isinstance(response, list):
            return ActivityListResponse(activities=response, total=len(response))
        return ActivityListResponse(**response)

    def cleanup_all(self) -> dict[str, Any]:
        """
        Cleanup all inactive sandboxes.

        Returns:
            Cleanup confirmation with count of cleaned up sandboxes
        """
        return self.transport.request(
            method="POST",
            path="/activities/cleanup-all",
        )

    def cleanup(self, sandbox_id: str) -> dict[str, Any]:
        """
        Cleanup a specific sandbox by ID.

        Args:
            sandbox_id: Sandbox UUID to cleanup

        Returns:
            Cleanup confirmation response
        """
        return self.transport.request(
            method="POST",
            path=f"/activities/{sandbox_id}/cleanup",
        )

