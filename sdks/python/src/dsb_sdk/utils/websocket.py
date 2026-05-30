"""
WebSocket client for terminal connections

Provides a WebSocket client for interactive shell sessions in DSB sandboxes.
"""

import asyncio
import json

import websocket
from pydantic import BaseModel

from dsb_sdk.exceptions import DSBConnectionError


class TerminalMessage(BaseModel):
    """Message sent/received over terminal WebSocket"""

    message_type: str  # "input", "output", "resize", "error"
    data: str
    timestamp: float | None = None


class WebSocketTerminalClient:
    """
    WebSocket client for interactive terminal sessions.

    Example:
        async with WebSocketTerminalClient(
            ws_url="ws://localhost:8080/terminal/123e4567-..."
        ) as terminal:
            # Send input
            await terminal.send("ls -la\\n")

            # Receive output (with timeout)
            output = await terminal.receive(timeout=5.0)
            print(output)
    """

    def __init__(self, ws_url: str, timeout: float = 30.0):
        """
        Initialize WebSocket terminal client.

        Args:
            ws_url: WebSocket URL (e.g., "ws://localhost:8080/terminal/{sandbox_id}")
            timeout: Connection timeout in seconds
        """
        self.ws_url = ws_url
        self.timeout = timeout
        self._ws: websocket.WebSocket | None = None
        self._received_buffer: list[str] = []

    def connect(self) -> None:
        """
        Establish WebSocket connection.

        Raises:
            DSBConnectionError: If connection fails
        """
        try:
            self._ws = websocket.create_connection(
                self.ws_url,
                timeout=self.timeout,
            )
        except Exception as e:
            raise DSBConnectionError(f"WebSocket connection failed: {e}") from e

    def send(self, input_text: str) -> None:
        """
        Send input to terminal.

        Args:
            input_text: Input text to send (e.g., "ls -la\\n")

        Raises:
            DSBConnectionError: If not connected or send fails
        """
        if not self._ws:
            raise DSBConnectionError("WebSocket not connected")

        message = TerminalMessage(message_type="input", data=input_text)
        try:
            self._ws.send(message.model_dump_json())
        except Exception as e:
            raise DSBConnectionError(f"Failed to send message: {e}") from e

    def receive(self, timeout: float | None = None) -> str:
        """
        Receive output from terminal.

        Args:
            timeout: Receive timeout in seconds (None for blocking)

        Returns:
            Received output text

        Raises:
            DSBConnectionError: If receive fails or times out
        """
        if not self._ws:
            raise DSBConnectionError("WebSocket not connected")

        try:
            message_str = self._ws.recv()  # type: ignore
            message = TerminalMessage.model_validate_json(message_str)

            if message.message_type == "error":
                raise DSBConnectionError(f"Terminal error: {message.data}")

            return message.data
        except websocket.WebSocketTimeoutException:
            return ""  # Timeout = no more output
        except Exception as e:
            raise DSBConnectionError(f"Failed to receive message: {e}") from e

    def resize(self, rows: int, cols: int) -> None:
        """
        Resize terminal pseudo-TTY.

        Args:
            rows: Number of rows
            cols: Number of columns
        """
        if not self._ws:
            raise DSBConnectionError("WebSocket not connected")

        message_data = json.dumps({"rows": rows, "cols": cols})
        message = TerminalMessage(message_type="resize", data=message_data)

        try:
            self._ws.send(message.model_dump_json())
        except Exception as e:
            raise DSBConnectionError(f"Failed to resize terminal: {e}") from e

    def close(self) -> None:
        """Close WebSocket connection."""
        if self._ws:
            try:
                self._ws.close()
            except Exception:
                pass  # Ignore errors during close
            self._ws = None

    def __enter__(self):
        """Context manager entry."""
        self.connect()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()
        return False


class AsyncWebSocketTerminalClient:
    """
    Async WebSocket client for interactive terminal sessions.

    Example:
        client = AsyncWebSocketTerminalClient(
            ws_url="ws://localhost:8080/terminal/123e4567-..."
        )
        await client.connect()

        # Send input
        await client.send("ls -la\\n")

        # Receive output
        output = await client.receive(timeout=5.0)
        print(output)

        await client.close()
    """

    def __init__(self, ws_url: str, timeout: float = 30.0):
        """
        Initialize async WebSocket terminal client.

        Args:
            ws_url: WebSocket URL
            timeout: Connection timeout in seconds
        """
        self.ws_url = ws_url
        self.timeout = timeout
        self._ws: websocket.WebSocket | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._receive_queue: asyncio.Queue[str] | None = None

    async def connect(self) -> None:
        """
        Establish WebSocket connection asynchronously.

        Raises:
            DSBConnectionError: If connection fails
        """
        loop = asyncio.get_event_loop()
        self._loop = loop
        self._receive_queue = asyncio.Queue()

        try:
            # Run websocket connection in thread pool to avoid blocking
            self._ws = await loop.run_in_executor(
                None,
                lambda: websocket.create_connection(self.ws_url, timeout=self.timeout),
            )
        except Exception as e:
            raise DSBConnectionError(f"WebSocket connection failed: {e}") from e

    async def send(self, input_text: str) -> None:
        """
        Send input to terminal asynchronously.

        Args:
            input_text: Input text to send
        """
        if not self._ws:
            raise DSBConnectionError("WebSocket not connected")

        message = TerminalMessage(message_type="input", data=input_text)
        ws = self._ws  # type: ignore

        # Send in thread pool to avoid blocking
        await asyncio.get_event_loop().run_in_executor(
            None,
            lambda: ws.send(message.model_dump_json()),
        )

    async def receive(self, timeout: float | None = None) -> str:
        """
        Receive output from terminal asynchronously.

        Args:
            timeout: Receive timeout in seconds

        Returns:
            Received output text
        """
        if not self._ws:
            raise DSBConnectionError("WebSocket not connected")

        # Receive in thread pool with timeout
        try:
            message_str = await asyncio.wait_for(
                asyncio.get_event_loop().run_in_executor(
                    None,
                    lambda: self._ws.recv(),  # type: ignore
                ),
                timeout=timeout or self.timeout,
            )

            message = TerminalMessage.model_validate_json(message_str)

            if message.message_type == "error":
                raise DSBConnectionError(f"Terminal error: {message.data}")

            return message.data

        except TimeoutError:
            return ""  # Timeout = no more output
        except Exception as e:
            raise DSBConnectionError(f"Failed to receive message: {e}") from e

    async def resize(self, rows: int, cols: int) -> None:
        """
        Resize terminal pseudo-TTY asynchronously.

        Args:
            rows: Number of rows
            cols: Number of columns
        """
        if not self._ws:
            raise DSBConnectionError("WebSocket not connected")

        message_data = json.dumps({"rows": rows, "cols": cols})
        message = TerminalMessage(message_type="resize", data=message_data)
        ws = self._ws  # type: ignore

        await asyncio.get_event_loop().run_in_executor(
            None,
            lambda: ws.send(message.model_dump_json()),
        )

    async def close(self) -> None:
        """Close WebSocket connection asynchronously."""
        if self._ws:
            ws = self._ws  # type: ignore
            try:
                await asyncio.get_event_loop().run_in_executor(
                    None,
                    lambda: ws.close(),
                )
            except Exception:
                pass  # Ignore errors during close
            self._ws = None

    async def __aenter__(self):
        """Async context manager entry."""
        await self.connect()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        """Async context manager exit."""
        await self.close()
        return False
