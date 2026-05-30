"""
Health check API implementation (synchronous)

Provides synchronous methods for health checks.
Use with DSBClient.
"""

from __future__ import annotations

from dsb_sdk.transport.sync import SyncTransport
from dsb_sdk.types.common import HealthStatus


class HealthAPI:
    """
    API for health checks (synchronous).

    Use with DSBClient for synchronous operations.
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize health API.

        Args:
            transport: SyncTransport instance
        """
        self.transport = transport

    def check(self) -> HealthStatus:
        """
        Check API health status.

        Returns:
            HealthStatus with server status, version, and uptime

        Example:
            >>> client = DSBClient()
            >>> status = client.health.check()
            >>> print(f"Server version: {status.version}")
        """
        response = self.transport.request(
            method="GET",
            path="/health",
        )
        return HealthStatus(**response)
