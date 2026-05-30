"""
Terminal API for interactive shell sessions

This module provides the TerminalAPI for managing WebSocket terminal connections.
"""

from typing import TYPE_CHECKING
from uuid import UUID

from dsb_sdk.utils.websocket import (
    AsyncWebSocketTerminalClient,
    WebSocketTerminalClient,
)

if TYPE_CHECKING:
    from dsb_sdk.transport.async_transport import AsyncTransport
    from dsb_sdk.transport.sync import SyncTransport


class TerminalAPI:
    """
    API for interactive terminal sessions in sandboxes.

    Works with both sync and async transports.
    """

    def __init__(self, transport: "SyncTransport", api_url: str):
        """
        Initialize terminal API.

        Args:
            transport: SyncTransport instance
            api_url: Base API URL (for constructing WebSocket URLs)
        """
        self.transport = transport
        self.api_url = api_url.rstrip("/")

    def connect(self, sandbox_id: str | UUID, timeout: float = 30.0) -> WebSocketTerminalClient:
        """
        Connect to a sandbox's terminal via WebSocket.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Connection timeout in seconds

        Returns:
            WebSocketTerminalClient instance

        Example:
            >>> client = DSBClient(api_url="http://localhost:8080")
            >>> terminal = client.terminal.connect(sandbox.id)
            >>> with terminal:
            ...     terminal.send("ls -la\\n")
            ...     output = terminal.receive(timeout=5.0)
            ...     print(output)
        """
        # Validate sandbox_id
        if not sandbox_id or (isinstance(sandbox_id, str) and not sandbox_id.strip()):
            raise ValueError("sandbox_id cannot be empty")

        # Convert UUID to string if needed
        sandbox_id_str = str(sandbox_id) if isinstance(sandbox_id, UUID) else sandbox_id

        # Construct WebSocket URL
        ws_url = self.api_url.replace("http://", "ws://").replace("https://", "wss://")
        ws_url = f"{ws_url}/terminal/{sandbox_id_str}"

        return WebSocketTerminalClient(ws_url=ws_url, timeout=timeout)

    def get_websocket_url(
        self,
        sandbox_id: str | UUID,
        session_id: str | None = None,
        cols: int | None = None,
        rows: int | None = None,
    ) -> str:
        """
        Generate WebSocket URL for terminal connection without connecting.

        Args:
            sandbox_id: Sandbox UUID
            session_id: Optional SSH session ID
            cols: Optional terminal columns
            rows: Optional terminal rows

        Returns:
            WebSocket URL string

        Example:
            >>> client = DSBClient(api_url="http://localhost:8080")
            >>> ws_url = client.terminal.get_websocket_url(sandbox.id)
            >>> print(ws_url)
            ws://localhost:8080/terminal/123e4567-e89b-12d3-a456-426614174000
        """
        # Validate sandbox_id
        if not sandbox_id or (isinstance(sandbox_id, str) and not sandbox_id.strip()):
            raise ValueError("sandbox_id cannot be empty")

        # Convert UUID to string if needed
        sandbox_id_str = str(sandbox_id) if isinstance(sandbox_id, UUID) else sandbox_id

        # Construct WebSocket URL
        ws_url = self.api_url.replace("http://", "ws://").replace("https://", "wss://")
        ws_url = f"{ws_url}/terminal/{sandbox_id_str}"

        # Add query parameters if provided
        params = []
        if session_id:
            params.append(f"session_id={session_id}")
        if cols is not None:
            params.append(f"cols={cols}")
        if rows is not None:
            params.append(f"rows={rows}")

        if params:
            ws_url += "?" + "&".join(params)

        return ws_url

    def execute_interactive(
        self,
        sandbox_id: str | UUID,
        command: str,
        timeout: float = 5.0,
    ) -> str:
        """
        Execute a command and get its output interactively.

        Args:
            sandbox_id: Sandbox UUID
            command: Command to execute (should include \\n)
            timeout: How long to wait for output

        Returns:
            Command output

        Example:
            >>> output = client.terminal.execute_interactive(
            ...     sandbox.id,
            ...     "cat /etc/os-release\\n"
            ... )
        """
        with self.connect(sandbox_id) as terminal:
            terminal.send(command)
            return terminal.receive(timeout=timeout)

    def resize_terminal(
        self,
        sandbox_id: str | UUID,
        rows: int,
        cols: int,
    ) -> None:
        """
        Resize the terminal pseudo-TTY.

        Args:
            sandbox_id: Sandbox UUID
            rows: Number of rows
            cols: Number of columns
        """
        with self.connect(sandbox_id, timeout=5.0) as terminal:
            terminal.resize(rows, cols)


class AsyncTerminalAPI:
    """
    Async API for interactive terminal sessions in sandboxes.

    Provides identical API to TerminalAPI but with async methods.
    """

    def __init__(self, transport: "AsyncTransport", api_url: str):
        """
        Initialize async terminal API.

        Args:
            transport: AsyncTransport instance
            api_url: Base API URL (for constructing WebSocket URLs)
        """
        self.transport = transport
        self.api_url = api_url.rstrip("/")

    async def connect_async(
        self, sandbox_id: str | UUID, timeout: float = 30.0
    ) -> AsyncWebSocketTerminalClient:
        """
        Connect to a sandbox's terminal via WebSocket asynchronously.

        Args:
            sandbox_id: Sandbox UUID
            timeout: Connection timeout in seconds

        Returns:
            AsyncWebSocketTerminalClient instance

        Example:
            >>> async with AsyncDSBClient(api_url="http://localhost:8080") as client:
            ...     terminal = await client.terminal.connect_async(sandbox.id)
            ...     await terminal.send("ls -la\\n")
            ...     output = await terminal.receive(timeout=5.0)
        """
        sandbox_id_str = str(sandbox_id) if isinstance(sandbox_id, UUID) else sandbox_id
        ws_url = self.api_url.replace("http://", "ws://").replace("https://", "wss://")
        ws_url = f"{ws_url}/terminal/{sandbox_id_str}"

        client = AsyncWebSocketTerminalClient(ws_url=ws_url, timeout=timeout)
        await client.connect()
        return client

    async def execute_interactive_async(
        self,
        sandbox_id: str | UUID,
        command: str,
        timeout: float = 5.0,
    ) -> str:
        """
        Execute a command and get its output interactively (async).

        Args:
            sandbox_id: Sandbox UUID
            command: Command to execute (should include \\n)
            timeout: How long to wait for output

        Returns:
            Command output

        Example:
            >>> async with AsyncDSBClient(api_url="http://localhost:8080") as client:
            ...     output = await client.terminal.execute_interactive_async(
            ...         sandbox.id,
            ...         "cat /etc/os-release\\n"
            ...     )
        """
        terminal = await self.connect_async(sandbox_id)
        try:
            await terminal.send(command)
            return await terminal.receive(timeout=timeout)
        finally:
            await terminal.close()

    async def resize_terminal_async(
        self,
        sandbox_id: str | UUID,
        rows: int,
        cols: int,
    ) -> None:
        """
        Resize the terminal pseudo-TTY asynchronously.

        Args:
            sandbox_id: Sandbox UUID
            rows: Number of rows
            cols: Number of columns

        Example:
            >>> async with AsyncDSBClient(api_url="http://localhost:8080") as client:
            ...     await client.terminal.resize_terminal_async(sandbox.id, rows=24, cols=80)
        """
        terminal = await self.connect_async(sandbox_id)
        try:
            await terminal.resize(rows, cols)
        finally:
            await terminal.close()

    # Backward compatibility aliases for renamed methods
    async def connect(
        self, sandbox_id: str | UUID, timeout: float = 30.0
    ) -> AsyncWebSocketTerminalClient:
        """
        Backward compatibility alias for connect_async.

        Deprecated: Use connect_async instead.
        """
        return await self.connect_async(sandbox_id, timeout)

    async def execute_interactive(
        self, sandbox_id: str | UUID, command: str, timeout: float = 5.0
    ) -> str:
        """
        Backward compatibility alias for execute_interactive_async.

        Deprecated: Use execute_interactive_async instead.
        """
        return await self.execute_interactive_async(sandbox_id, command, timeout)

    async def resize_terminal(self, sandbox_id: str | UUID, rows: int, cols: int) -> None:
        """
        Backward compatibility alias for resize_terminal_async.

        Deprecated: Use resize_terminal_async instead.
        """
        return await self.resize_terminal_async(sandbox_id, rows, cols)
