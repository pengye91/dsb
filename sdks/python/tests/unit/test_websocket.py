"""
Unit tests for WebSocket terminal client
"""

from unittest.mock import Mock, patch

import pytest

from dsb_sdk.utils.websocket import (
    AsyncWebSocketTerminalClient,
    TerminalMessage,
    WebSocketTerminalClient,
)


class TestTerminalMessage:
    """Tests for TerminalMessage model"""

    def test_create_message(self):
        """Test creating a terminal message"""
        msg = TerminalMessage(message_type="input", data="ls -la")
        assert msg.message_type == "input"
        assert msg.data == "ls -la"
        assert msg.timestamp is None

    def test_message_with_timestamp(self):
        """Test message with timestamp"""
        import time

        msg = TerminalMessage(message_type="output", data="test", timestamp=time.time())
        assert msg.timestamp is not None


class TestWebSocketTerminalClient:
    """Tests for synchronous WebSocket terminal client"""

    def test_init(self):
        """Test client initialization"""
        client = WebSocketTerminalClient("ws://localhost:8081/terminal/123")
        assert client.ws_url == "ws://localhost:8081/terminal/123"
        assert client.timeout == 30.0
        assert client._ws is None

    def test_init_custom_timeout(self):
        """Test initialization with custom timeout"""
        client = WebSocketTerminalClient("ws://localhost/term/456", timeout=60.0)
        assert client.timeout == 60.0

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_connect(self, mock_ws):
        """Test connecting to WebSocket"""
        mock_ws_client = Mock()
        mock_ws.return_value = mock_ws_client

        client = WebSocketTerminalClient("ws://localhost/term/123")
        client.connect()

        assert client._ws is not None
        mock_ws.assert_called_once_with("ws://localhost/term/123", timeout=30.0)

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_connect_failure(self, mock_ws):
        """Test connection failure"""
        mock_ws.side_effect = Exception("Connection failed")

        client = WebSocketTerminalClient("ws://localhost/term/123")
        with pytest.raises(Exception):  # DSBConnectionError
            client.connect()

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_send(self, mock_ws):
        """Test sending data"""
        mock_ws_client = Mock()
        mock_ws.return_value = mock_ws_client

        client = WebSocketTerminalClient("ws://localhost/term/123")
        client.connect()

        client.send("ls -la\n")

        # Verify send was called with JSON message
        assert mock_ws_client.send.called
        sent_data = mock_ws_client.send.call_args[0][0]
        msg = TerminalMessage.model_validate_json(sent_data)
        assert msg.message_type == "input"
        assert "ls -la" in msg.data

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_send_not_connected(self, mock_ws):
        """Test sending when not connected"""
        client = WebSocketTerminalClient("ws://localhost/term/123")
        # Don't connect

        with pytest.raises(Exception):  # DSBConnectionError
            client.send("test")

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_receive(self, mock_ws):
        """Test receiving data"""
        mock_ws_client = Mock()
        mock_ws.return_value = mock_ws_client

        # Mock receive to return a message
        response_msg = TerminalMessage(message_type="output", data="output text")
        mock_ws_client.recv.return_value = response_msg.model_dump_json()

        client = WebSocketTerminalClient("ws://localhost/term/123")
        client.connect()

        output = client.receive()

        assert output == "output text"
        mock_ws_client.recv.assert_called_once()

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_resize(self, mock_ws):
        """Test resizing terminal"""
        mock_ws_client = Mock()
        mock_ws.return_value = mock_ws_client

        client = WebSocketTerminalClient("ws://localhost/term/123")
        client.connect()

        client.resize(rows=24, cols=80)

        assert mock_ws_client.send.called
        sent_data = mock_ws_client.send.call_args[0][0]
        msg = TerminalMessage.model_validate_json(sent_data)
        assert msg.message_type == "resize"
        assert "rows" in msg.data
        assert "cols" in msg.data

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_close(self, mock_ws):
        """Test closing connection"""
        mock_ws_client = Mock()
        mock_ws.return_value = mock_ws_client

        client = WebSocketTerminalClient("ws://localhost/term/123")
        client.connect()

        client.close()

        assert client._ws is None
        mock_ws_client.close.assert_called_once()

    @patch("dsb_sdk.utils.websocket.websocket.create_connection")
    def test_context_manager(self, mock_ws):
        """Test using client as context manager"""
        mock_ws_client = Mock()
        mock_ws.return_value = mock_ws_client

        with WebSocketTerminalClient("ws://localhost/term/123") as client:
            assert client._ws is not None
            mock_ws_client.send("test")

        # Should be closed after exiting context
        assert client._ws is None
        mock_ws_client.close.assert_called_once()


class TestAsyncWebSocketTerminalClient:
    """Tests for async WebSocket terminal client"""

    def test_init(self):
        """Test async client initialization"""
        client = AsyncWebSocketTerminalClient("ws://localhost:8081/terminal/123")
        assert client.ws_url == "ws://localhost:8081/terminal/123"
        assert client.timeout == 30.0

    @pytest.mark.asyncio
    async def test_connect(self):
        """Test async connection"""
        client = AsyncWebSocketTerminalClient("ws://localhost/term/123")
        # Mock websocket.create_connection
        with patch("dsb_sdk.utils.websocket.websocket.create_connection") as mock_ws_create:
            mock_ws = Mock()
            mock_ws_create.return_value = mock_ws

            await client.connect()

            assert client._ws is not None
            mock_ws_create.assert_called_once()

    @pytest.mark.asyncio
    async def test_send(self):
        """Test async send"""
        client = AsyncWebSocketTerminalClient("ws://localhost/term/123")

        # Create a mock websocket
        mock_ws = Mock()
        client._ws = mock_ws

        # Send should work via executor
        await client.send("test input")

        # Since send runs in executor, we need to check it was called
        assert mock_ws.send.called

    @pytest.mark.asyncio
    async def test_close(self):
        """Test async close"""
        client = AsyncWebSocketTerminalClient("ws://localhost/term/123")

        # Create a mock websocket
        mock_ws = Mock()
        client._ws = mock_ws

        await client.close()

        assert client._ws is None

    @pytest.mark.asyncio
    async def test_receive(self):
        """Test async receive"""
        client = AsyncWebSocketTerminalClient("ws://localhost/term/123")

        # Create a mock websocket
        mock_ws = Mock()
        response_msg = TerminalMessage(message_type="output", data="test output")
        mock_ws.recv.return_value = response_msg.model_dump_json()
        client._ws = mock_ws

        output = await client.receive()

        assert output == "test output"
        mock_ws.recv.assert_called_once()

    @pytest.mark.asyncio
    async def test_context_manager(self):
        """Test async context manager"""
        with patch("dsb_sdk.utils.websocket.websocket.create_connection") as mock_ws_create:
            mock_ws = Mock()
            mock_ws_create.return_value = mock_ws

            async with AsyncWebSocketTerminalClient("ws://localhost/term/123") as client:
                assert client._ws is not None

            # Should be closed after exiting context
            assert client._ws is None
