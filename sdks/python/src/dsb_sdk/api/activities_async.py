"""
Activities API implementation (asynchronous)

Provides asynchronous methods for activity tracking.
Use with AsyncDSBClient.
"""

from __future__ import annotations

from typing import Any

from dsb_sdk.transport.async_transport import AsyncTransport
from dsb_sdk.types.common import ActivityListResponse


class AsyncActivitiesAPI:
    """
    API for activity tracking (asynchronous).

    Use with AsyncDSBClient for asynchronous operations.
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async activities API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    async def list_async(self, sandbox_id: str | None = None, activity_type: str | None = None, limit: int | None = None) -> ActivityListResponse:
        """
        List all activities.

        Returns:
            ActivityListResponse with list of activities

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     activities = await client.activities.list_async()
        """
        response = await self.transport.request(
            method="GET",
            path="/activities",
        )
        # API returns a list directly, wrap it in the expected response format
        if isinstance(response, list):
            return ActivityListResponse(activities=response, total=len(response))
        return ActivityListResponse(**response)

    async def cleanup_all_async(self) -> dict[str, Any]:
        """
        Cleanup all inactive sandboxes.

        Returns:
            Cleanup confirmation with count of cleaned up sandboxes

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.activities.cleanup_all_async()
        """
        return await self.transport.request(
            method="POST",
            path="/activities/cleanup-all",
        )

    async def cleanup_async(self, sandbox_id: str) -> dict[str, Any]:
        """
        Cleanup a specific sandbox by ID.

        Args:
            sandbox_id: Sandbox UUID to cleanup

        Returns:
            Cleanup confirmation response

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.activities.cleanup_async(sandbox_id)
        """
        return await self.transport.request(
            method="POST",
            path=f"/activities/{sandbox_id}/cleanup",
        )

