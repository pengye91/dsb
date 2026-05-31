"""
SSH Gateway API implementation (asynchronous)

Provides asynchronous methods for managing SSH sessions.
Use with AsyncDSBClient.
"""

from __future__ import annotations

from typing import Any
from uuid import UUID

from dsb_sdk.transport.async_transport import AsyncTransport
from dsb_sdk.types.ssh import (
    SSHSession,
    SSHSessionConfig,
    SSHSessionListResponse,
)


class AsyncSSHAPI:
    """
    API for managing SSH sessions through the SSH Gateway (asynchronous).

    Use with AsyncDSBClient for asynchronous operations.
    """

    def __init__(self, transport: AsyncTransport):
        """
        Initialize async SSH API.

        Args:
            transport: AsyncTransport instance
        """
        self.transport = transport

    async def create_async(
        self,
        sandbox_id: str | UUID,
        username: str,
        public_key: str | None = None,
        client_ip: str | None = None,
    ) -> SSHSession:
        """
        Create a new SSH session.

        Args:
            sandbox_id: Target sandbox UUID
            username: SSH username
            public_key: Optional SSH public key
            client_ip: Optional client IP address (defaults to "127.0.0.1")

        Returns:
            Created SSHSession instance

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     session = await client.ssh.create_async(sandbox_id, "user")
        """
        request_data = SSHSessionConfig(
            sandbox_id=sandbox_id if isinstance(sandbox_id, UUID) else UUID(sandbox_id),
            client_ip=client_ip or "127.0.0.1",
            username=username,
            public_key=public_key,
            ssh_version=None,
        )

        response = await self.transport.request(
            method="POST",
            path="/ssh-sessions",
            json_data=request_data.model_dump(exclude_none=True),
        )

        return SSHSession(**response)

    async def get_async(self, session_id: str | UUID) -> SSHSession:
        """
        Get SSH session details.

        Args:
            session_id: SSH session UUID

        Returns:
            SSHSession instance

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     session = await client.ssh.get_async(session_id)
        """
        response = await self.transport.request(
            method="GET",
            path=f"/ssh-sessions/{session_id}",
        )
        return SSHSession(**response)

    async def list_async(self, sandbox_id: str | None = None, state: str | None = None, limit: int | None = None, offset: int | None = None) -> SSHSessionListResponse:
        """
        List all SSH sessions.

        Returns:
            SSHSessionListResponse with list of sessions

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     response = await client.ssh.list_async()
            ...     for session in response.sessions:
            ...         print(f"{session.id}: {session.username}")
        """
        response = await self.transport.request(
            method="GET",
            path="/ssh-sessions",
        )
        # API returns a list directly, wrap it in the expected response format
        if isinstance(response, list):
            return SSHSessionListResponse(sessions=response, total=len(response))
        return SSHSessionListResponse(**response)

    async def heartbeat_async(
        self,
        session_id: str | UUID,
        bytes_sent: int = 0,
        bytes_received: int = 0,
    ) -> dict[str, Any]:
        """
        Update session activity (send heartbeat).

        Args:
            session_id: SSH session UUID
            bytes_sent: Cumulative bytes sent (default: 0)
            bytes_received: Cumulative bytes received (default: 0)

        Returns:
            Heartbeat confirmation

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     await client.ssh.heartbeat_async(session_id)
        """
        return await self.transport.request(
            method="POST",
            path=f"/ssh-sessions/{session_id}/heartbeat",
            json_data={
                "bytes_sent": bytes_sent,
                "bytes_received": bytes_received,
            },
        )

    async def terminate_async(
        self,
        session_id: str | UUID,
        reason: str | None = None,
    ) -> dict[str, Any]:
        """
        Terminate an SSH session.

        Args:
            session_id: SSH session UUID
            reason: Optional reason for termination

        Returns:
            Termination confirmation

        Example:
            >>> async with AsyncDSBClient() as client:
            ...     await client.ssh.terminate_async(session_id)
        """
        json_data = {}
        if reason:
            json_data["reason"] = reason

        return await self.transport.request(
            method="POST",
            path=f"/ssh-sessions/{session_id}/terminate",
            json_data=json_data,
        )
