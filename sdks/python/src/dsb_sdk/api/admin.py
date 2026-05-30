"""
Admin API implementation (synchronous)

Provides synchronous methods for managing API keys.
Use with DSBClient.
"""

from __future__ import annotations

from typing import Any

from dsb_sdk.transport.sync import SyncTransport


class AdminAPI:
    """
    API for admin operations including API key management (synchronous).

    Use with DSBClient for synchronous operations.
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize admin API.

        Args:
            transport: SyncTransport instance
        """
        self.transport = transport

    def list_api_keys(self) -> list[dict[str, Any]]:
        """
        List all API keys.

        Returns:
            List of API key objects with id, name, scopes, etc.
        """
        return self.transport.request(
            method="GET",
            path="/admin/api-keys",
        )

    def get_api_key(self, key_id: str) -> dict[str, Any]:
        """
        Get a specific API key by ID.

        Args:
            key_id: UUID of the API key

        Returns:
            API key object with id, name, scopes, is_active, etc.
        """
        return self.transport.request(
            method="GET",
            path=f"/admin/api-keys/{key_id}",
        )

    def create_api_key(
        self,
        name: str,
        description: str | None = None,
        scopes: list[str] | None = None,
        expires_in_days: int | None = None,
        created_by: str | None = None,
    ) -> dict[str, Any]:
        """
        Create a new API key.

        The full key value is only returned in this response and cannot
        be retrieved again.

        Args:
            name: Human-readable name for the key
            description: Optional description of the key's purpose
            scopes: Optional list of permission scopes (e.g. ["sandbox:read"])
            expires_in_days: Optional number of days until the key expires
            created_by: Optional identifier of who created the key

        Returns:
            ApiKeyResponse with 'api_key' (full key, shown only once) and
            'key' (ApiKey metadata object)
        """
        data: dict[str, Any] = {"name": name}
        if description is not None:
            data["description"] = description
        if scopes is not None:
            data["scopes"] = scopes
        if expires_in_days is not None:
            data["expires_in_days"] = expires_in_days
        if created_by is not None:
            data["created_by"] = created_by
        return self.transport.request(
            method="POST",
            path="/admin/api-keys",
            json_data=data,
        )

    def delete_api_key(self, key_id: str) -> None:
        """
        Delete an API key.

        Args:
            key_id: UUID of the API key to delete
        """
        self.transport.request(
            method="DELETE",
            path=f"/admin/api-keys/{key_id}",
        )

    def rotate_api_key(self, key_id: str) -> dict[str, Any]:
        """
        Rotate an API key, generating a new secret.

        The old key is invalidated and a new key is issued.
        The full key value is only returned in this response and cannot
        be retrieved again.

        Args:
            key_id: UUID of the API key to rotate

        Returns:
            ApiKeyResponse with 'api_key' (new full key, shown only once) and
            'key' (ApiKey metadata object)
        """
        return self.transport.request(
            method="POST",
            path=f"/admin/api-keys/{key_id}/rotate",
        )
