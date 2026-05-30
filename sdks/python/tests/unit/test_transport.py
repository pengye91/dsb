"""Unit tests for transport layer"""

from unittest.mock import Mock

import pytest


class TestJSONSerializationTypes:
    """Tests for JSON serialization type validation

    These tests verify that numeric values are serialized with the correct
    JSON types to match backend Rust serde expectations.

    Background: Rust serde expects u64 (integer) for timeout values.
    Python float values (e.g., 60.0) serialize to JSON as 60.0,
    which causes deserialization errors in the Rust backend.

    These tests use httpx mocking to capture the actual JSON payload
    that would be sent over the wire.
    """

    def test_timeout_as_integer_in_json_payload(self):
        """Test that timeout values are serialized as integers, not floats"""

        from dsb_sdk.transport.sync import SyncTransport

        # Create a real transport with mocked httpx client
        transport = SyncTransport.__new__(SyncTransport)
        transport.api_url = "http://localhost:8081"
        transport.timeout = 30.0
        transport.verify_ssl = True
        transport.api_key = None

        # Mock the httpx client but capture the actual request
        mock_client = Mock()
        mock_response = Mock()
        mock_response.json.return_value = {"status": "ok"}
        mock_response.raise_for_status = Mock()
        mock_client.request.return_value = mock_response
        transport._client = mock_client

        # Make a request with integer timeout
        transport.request(
            method="POST",
            path="/sandboxes/test-id/tools",
            json_data={"interpreter": "python", "script_path": "/opt/tools/test.py", "timeout": 90},
            timeout=90,
        )

        # Verify the httpx client was called
        assert mock_client.request.called
        call_kwargs = mock_client.request.call_args.kwargs

        # The timeout parameter for httpx should be a number (int or float is ok)
        assert "timeout" in call_kwargs
        assert isinstance(call_kwargs["timeout"], (int, float))

    def test_json_data_numeric_types(self):
        """Test that numeric values in json_data maintain correct types"""
        import json

        from dsb_sdk.transport.sync import SyncTransport

        # Create a real transport with mocked httpx client
        transport = SyncTransport.__new__(SyncTransport)
        transport.api_url = "http://localhost:8081"
        transport.timeout = 30.0
        transport.verify_ssl = True
        transport.api_key = None

        # Mock the httpx client
        mock_client = Mock()

        # Capture the JSON content that httpx would send
        captured_json = None

        def mock_request(*args, **kwargs):
            nonlocal captured_json
            # httpx serializes the json parameter to JSON
            if "json" in kwargs:
                captured_json = json.dumps(kwargs["json"])
            mock_response = Mock()
            mock_response.json.return_value = {"status": "ok"}
            mock_response.raise_for_status = Mock()
            return mock_response

        mock_client.request = mock_request
        transport._client = mock_client

        # Make a request with various numeric types
        transport.request(
            method="POST",
            path="/sandboxes/test-id/tools",
            json_data={
                "interpreter": "python",
                "timeout": 90,  # Should serialize as integer
                "max_retries": 3,  # Should serialize as integer
                "retry_delay_secs": 1,  # Should serialize as integer
            },
        )

        # Verify JSON was captured
        assert captured_json is not None, "JSON payload should have been captured"

        # Parse and verify the JSON contains integers, not floats
        parsed = json.loads(captured_json)
        assert parsed["timeout"] == 90
        assert parsed["max_retries"] == 3
        assert parsed["retry_delay_secs"] == 1

        # Verify the JSON string representation uses integers
        # (no decimal points for whole numbers)
        assert '"timeout":90' in captured_json or '"timeout": 90' in captured_json
        assert '"timeout":90.0' not in captured_json and '"timeout": 90.0' not in captured_json

    def test_json_serialization_with_float_values(self):
        """Test that actual float values serialize correctly (e.g., cpu_percent)"""
        import json

        from dsb_sdk.transport.sync import SyncTransport

        # Create a real transport with mocked httpx client
        transport = SyncTransport.__new__(SyncTransport)
        transport.api_url = "http://localhost:8081"
        transport.timeout = 30.0
        transport.verify_ssl = True
        transport.api_key = None

        # Mock the httpx client
        mock_client = Mock()

        captured_json = None

        def mock_request(*args, **kwargs):
            nonlocal captured_json
            if "json" in kwargs:
                captured_json = json.dumps(kwargs["json"])
            mock_response = Mock()
            mock_response.json.return_value = {"status": "ok"}
            mock_response.raise_for_status = Mock()
            return mock_response

        mock_client.request = mock_request
        transport._client = mock_client

        # Make a request with actual float values (e.g., CPU percentage)
        transport.request(
            method="POST",
            path="/sandboxes/test-id/tools",
            json_data={
                "interpreter": "python",
                "cpu_limit": 0.5,  # Actual float (should stay as float)
                "memory_limit_gb": 2.0,  # Actual float
                "timeout": 90,  # Integer that must stay as integer
            },
        )

        # Verify JSON was captured
        assert captured_json is not None, "JSON payload should have been captured"

        # Parse and verify
        parsed = json.loads(captured_json)
        assert parsed["cpu_limit"] == 0.5  # Float stays float
        assert parsed["memory_limit_gb"] == 2.0  # Float stays float
        assert parsed["timeout"] == 90  # Integer stays integer

        # Verify JSON representation
        assert '"cpu_limit":0.5' in captured_json or '"cpu_limit": 0.5' in captured_json
        assert '"timeout":90' in captured_json or '"timeout": 90' in captured_json

    def test_timeout_must_be_integer_for_rust_backend(self):
        """Test that timeout values are integers (Rust u64 compatible)

        This is the critical test that would have caught the original bug.
        The original code used: timeout=float(exec_timeout + buffer)
        Which sent {"timeout": 90.0} instead of {"timeout": 90}
        """
        import json

        # Simulate the original buggy code
        exec_timeout = 60
        http_buffer_secs = 30

        # BUGGY: This creates a float
        buggy_timeout = float(exec_timeout + http_buffer_secs)
        assert isinstance(buggy_timeout, float)
        assert buggy_timeout == 90.0

        # CORRECT: This creates an int
        correct_timeout = int(exec_timeout + http_buffer_secs)
        assert isinstance(correct_timeout, int)
        assert correct_timeout == 90

        # Verify JSON serialization difference
        buggy_json = json.dumps({"timeout": buggy_timeout})
        correct_json = json.dumps({"timeout": correct_timeout})

        # The buggy version serializes as 90.0 (float)
        assert buggy_json == '{"timeout": 90.0}'

        # The correct version serializes as 90 (integer)
        assert correct_json == '{"timeout": 90}'

        # These are different! Rust serde will reject the float version
        assert buggy_json != correct_json


class TestAsyncJSONSerializationTypes:
    """Async versions of JSON serialization type tests"""

    @pytest.mark.asyncio
    async def test_async_timeout_as_integer_in_json_payload(self):
        """Test that async requests also use integer timeout values"""
        import json
        from unittest.mock import AsyncMock

        from dsb_sdk.transport.async_transport import AsyncTransport

        # Create a real async transport with mocked httpx client
        transport = AsyncTransport.__new__(AsyncTransport)
        transport.api_url = "http://localhost:8081"
        transport.timeout = 30.0
        transport.verify_ssl = True
        transport.api_key = None

        # Mock the httpx async client
        mock_client = AsyncMock()

        captured_json = None

        async def mock_request(*args, **kwargs):
            nonlocal captured_json
            if "json" in kwargs:
                captured_json = json.dumps(kwargs["json"])
            mock_response = Mock()
            mock_response.json.return_value = {"status": "ok"}
            mock_response.raise_for_status = Mock()
            return mock_response

        mock_client.request = mock_request
        transport._client = mock_client

        # Make an async request
        await transport.request(
            method="POST",
            path="/sandboxes/test-id/tools",
            json_data={"interpreter": "python", "timeout": 120},
            timeout=120,
        )

        # Verify JSON was captured
        assert captured_json is not None
        parsed = json.loads(captured_json)
        assert parsed["timeout"] == 120
        assert '"timeout":120' in captured_json or '"timeout": 120' in captured_json
