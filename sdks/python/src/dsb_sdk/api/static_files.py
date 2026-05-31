"""
Static file serving API implementation (synchronous)

Provides synchronous methods for serving static files from sandboxes.
Use with DSBClient.
"""

from __future__ import annotations

from uuid import UUID

import httpx

from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError, DSBTimeoutError
from dsb_sdk.transport.sync import SyncTransport
from dsb_sdk.types.sandbox import StaticFileList


class StaticFilesAPI:
    """
    API for static file serving (synchronous).

    Use with DSBClient for synchronous operations.
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize static files API.

        Args:
            transport: SyncTransport instance
        """
        self.transport = transport
        self._client = transport._client  # Access underlying httpx client

    def serve_file(self, sandbox_id: UUID, file_path: str) -> bytes:
        """
        Serve a static file from sandbox.

        Args:
            sandbox_id: Sandbox identifier
            file_path: File path relative to sandbox mount

        Returns:
            File contents as bytes

        Raises:
            DSBConnectionError: Connection failure
            DSBAPIError: File not found or error occurs

        Example:
            >>> client = DSBClient()
            >>> sandbox = client.sandbox.create(...)
            >>> content = client.static_files.serve_file(sandbox.id, "index.html")
        """
        # Make raw HTTP request for binary content
        headers = {"Accept": "application/octet-stream"}
        if self.transport.api_key:
            headers["X-API-Key"] = self.transport.api_key

        try:
            response = self._client.request(
                method="GET",
                url=f"/static/{sandbox_id}/{file_path}",
                headers=headers,
            )
            response.raise_for_status()
            return response.content

        except httpx.TimeoutException as e:
            raise DSBTimeoutError(f"Request timed out: {e}") from e

        except httpx.HTTPStatusError as e:
            status_code = e.response.status_code
            try:
                error_detail = e.response.json().get("detail", str(e))
            except Exception:
                error_detail = str(e)
            raise DSBAPIError(f"API error ({status_code}): {error_detail}") from e

        except httpx.ConnectError as e:
            raise DSBConnectionError(f"Connection failed: {e}") from e

    def list_files(self, sandbox_id: UUID) -> StaticFileList:
        """
        List all published files for a sandbox.

        Args:
            sandbox_id: Sandbox identifier

        Returns:
            StaticFileList with file metadata

        Example:
            >>> client = DSBClient()
            >>> sandbox = client.sandbox.create(...)
            >>> files = client.static_files.list_files(sandbox.id)
            >>> print(f"Total files: {files.total_count}")
        """
        response = self.transport.request(
            method="GET",
            path=f"/static/files/{sandbox_id}",
        )
        return StaticFileList(**response)

    def delete_file(self, sandbox_id: UUID, file_path: str) -> dict:
        """
        Delete a specific file.

        Args:
            sandbox_id: Sandbox identifier
            file_path: File path to delete

        Returns:
            Deletion response message

        Example:
            >>> client = DSBClient()
            >>> result = client.static_files.delete_file(sandbox.id, "old.html")
            >>> print(result["message"])
        """
        response = self.transport.request(
            method="DELETE",
            path=f"/static/file/{sandbox_id}/{file_path}",
        )
        return response

    def delete_sandbox_files(self, sandbox_id: UUID) -> dict:
        """
        Delete all files for a sandbox.

        Args:
            sandbox_id: Sandbox identifier

        Returns:
            Deletion response with count

        Example:
            >>> client = DSBClient()
            >>> result = client.static_files.delete_sandbox_files(sandbox.id)
            >>> print(f"Deleted {result['deleted_count']} files")
        """
        response = self.transport.request(
            method="DELETE",
            path=f"/static/sandbox/{sandbox_id}",
        )
        return response
