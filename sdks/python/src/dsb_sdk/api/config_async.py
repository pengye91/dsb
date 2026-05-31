"""
Config API implementation (asynchronous).

Provides asynchronous methods for retrieving server configuration.
Use with AsyncDSBClient.
"""

from __future__ import annotations

from typing import Any

from dsb_sdk.transport.async_transport import AsyncTransport


class AsyncConfigAPI:
    """
    API for server configuration (asynchronous).

    Use with AsyncDSBClient for asynchronous operations.
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async config API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    async def get(self) -> dict[str, Any]:
        """
        Get frontend/server configuration.

        Returns:
            Dict with default_sandbox_image, default_inactivity_timeout,
            and authentication_required fields.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     config = await client.config.get()
            ...     print(f"Default image: {config['default_sandbox_image']}")
        """
        return await self.transport.request(
            method="GET",
            path="/config",
        )
