"""
Config API implementation (synchronous).

Provides synchronous methods for retrieving server configuration.
Use with DSBClient.
"""

from __future__ import annotations

from typing import Any

from dsb_sdk.transport.sync import SyncTransport


class ConfigAPI:
    """
    API for server configuration (synchronous).

    Use with DSBClient for synchronous operations.
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize config API.

        Args:
            transport: SyncTransport instance
        """
        self.transport = transport

    def get(self) -> dict[str, Any]:
        """
        Get frontend/server configuration.

        Returns:
            Dict with default_sandbox_image, default_inactivity_timeout,
            and authentication_required fields.

        Example:
            >>> client = DSBClient()
            >>> config = client.config.get()
            >>> print(f"Default image: {config['default_sandbox_image']}")
        """
        return self.transport.request(
            method="GET",
            path="/config",
        )
