"""
SSH Gateway API implementation (synchronous)

Provides synchronous methods for managing SSH sessions.
Use with DSBClient.
"""

from __future__ import annotations

from typing import Any
from uuid import UUID

from dsb_sdk.transport.sync import SyncTransport
from dsb_sdk.types.ssh import (
    SSHSession,
    SSHSessionConfig,
    SSHSessionListResponse,
)


class SSHAPI:
    """
    API for managing SSH sessions through the SSH Gateway (synchronous).

    Use with DSBClient for synchronous operations.
    """

    def __init__(self, transport: SyncTransport):
        """
        Initialize SSH API.

        Args:
            transport: SyncTransport instance
        """
        self.transport = transport

    def create(
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

        Raises:
            DSBAPIError: API error
            DSBConnectionError: Connection error
        """
        request_data = SSHSessionConfig(
            sandbox_id=sandbox_id if isinstance(sandbox_id, UUID) else UUID(sandbox_id),
            client_ip=client_ip or "127.0.0.1",
            username=username,
            public_key=public_key,
            ssh_version=None,
        )

        response = self.transport.request(
            method="POST",
            path="/ssh-sessions",
            json_data=request_data.model_dump(exclude_none=True),
        )

        return SSHSession(**response)

    def get(self, session_id: str | UUID) -> SSHSession:
        """
        Get SSH session details.

        Args:
            session_id: SSH session UUID

        Returns:
            SSHSession instance
        """
        response = self.transport.request(
            method="GET",
            path=f"/ssh-sessions/{session_id}",
        )
        return SSHSession(**response)

    def list(self, sandbox_id: str | None = None, state: str | None = None, limit: int | None = None, offset: int | None = None) -> SSHSessionListResponse:
        """
        List all SSH sessions.

        Returns:
            SSHSessionListResponse with list of sessions
        """
        response = self.transport.request(
            method="GET",
            path="/ssh-sessions",
        )
        # API returns a list directly, wrap it in the expected response format
        if isinstance(response, list):
            return SSHSessionListResponse(sessions=response, total=len(response))
        return SSHSessionListResponse(**response)

    def heartbeat(
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
        """
        return self.transport.request(
            method="POST",
            path=f"/ssh-sessions/{session_id}/heartbeat",
            json_data={
                "bytes_sent": bytes_sent,
                "bytes_received": bytes_received,
            },
        )

    def terminate(
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
        """
        json_data = {}
        if reason:
            json_data["reason"] = reason

        return self.transport.request(
            method="POST",
            path=f"/ssh-sessions/{session_id}/terminate",
            json_data=json_data,
        )
