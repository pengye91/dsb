"""
Health check API implementation (asynchronous)

Provides asynchronous methods for health checks.
Use with AsyncDSBClient.
"""

from __future__ import annotations

from dsb_sdk.transport.async_transport import AsyncTransport
from dsb_sdk.types.common import HealthStatus


class AsyncHealthAPI:
    """
    API for health checks (asynchronous).

    Use with AsyncDSBClient for asynchronous operations.
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async health API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    async def check_async(self) -> HealthStatus:
        """
        Check API health status.

        Returns:
            HealthStatus with server status, version, and uptime

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     status = await client.health.check_async()
            ...     print(f"Server version: {status.version}")
        """
        response = await self.transport.request(
            method="GET",
            path="/health",
        )
        return HealthStatus(**response)

    # Backward compatibility alias
    async def check(self) -> HealthStatus:
        """
        Backward compatibility alias for check_async.

        Deprecated: Use check_async instead.
        """
        return await self.check_async()
