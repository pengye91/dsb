"""
Admin API implementation (asynchronous)

Provides asynchronous methods for managing API keys.
Use with AsyncDSBClient.
"""

from __future__ import annotations

from typing import Any

from dsb_sdk.transport.async_transport import AsyncTransport


class AsyncAdminAPI:
    """
    API for admin operations including API key management (asynchronous).

    Use with AsyncDSBClient for asynchronous operations.
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async admin API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    async def list_api_keys_async(self) -> list[dict[str, Any]]:
        """
        List all API keys.

        Returns:
            List of API key objects with id, name, scopes, etc.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     keys = await client.admin.list_api_keys_async()
        """
        return await self.transport.request(
            method="GET",
            path="/admin/api-keys",
        )

    async def get_api_key_async(self, key_id: str) -> dict[str, Any]:
        """
        Get a specific API key by ID.

        Args:
            key_id: UUID of the API key

        Returns:
            API key object with id, name, scopes, is_active, etc.

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     key = await client.admin.get_api_key_async(key_id)
        """
        return await self.transport.request(
            method="GET",
            path=f"/admin/api-keys/{key_id}",
        )

    async def create_api_key_async(
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

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.admin.create_api_key_async(
            ...         name="my-key", scopes=["sandbox:read"]
            ...     )
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
        return await self.transport.request(
            method="POST",
            path="/admin/api-keys",
            json_data=data,
        )

    async def delete_api_key_async(self, key_id: str) -> None:
        """
        Delete an API key.

        Args:
            key_id: UUID of the API key to delete

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     await client.admin.delete_api_key_async(key_id)
        """
        await self.transport.request(
            method="DELETE",
            path=f"/admin/api-keys/{key_id}",
        )

    async def rotate_api_key_async(self, key_id: str) -> dict[str, Any]:
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

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     result = await client.admin.rotate_api_key_async(key_id)
        """
        return await self.transport.request(
            method="POST",
            path=f"/admin/api-keys/{key_id}/rotate",
        )
