"""Unit tests for streaming utilities"""

from dsb_sdk.utils.streaming import SSEDecoder


class TestSSEDecoder:
    """Tests for SSEDecoder class"""

    def test_decode_valid_json_data(self):
        """Test decoding valid JSON data"""
        result = SSEDecoder.decode('data: {"status": "ok"}')

        assert result == {"status": "ok"}

    def test_decode_done_marker(self):
        """Test decoding [DONE] marker returns None"""
        result = SSEDecoder.decode("data: [DONE]")

        assert result is None

    def test_decode_empty_data(self):
        """Test decoding empty data returns None"""
        result = SSEDecoder.decode("data: ")

        assert result is None

    def test_decode_empty_line(self):
        """Test decoding empty line returns None"""
        result = SSEDecoder.decode("")

        assert result is None

    def test_decode_event_line(self):
        """Test decoding event line returns None"""
        result = SSEDecoder.decode("event: message")

        assert result is None

    def test_decode_non_data_line(self):
        """Test decoding non-data line returns None"""
        result = SSEDecoder.decode("id: 123")

        assert result is None

    def test_decode_whitespace_data(self):
        """Test decoding whitespace-only data returns None"""
        result = SSEDecoder.decode("data:   ")

        assert result is None

    def test_decode_multiple_objects(self):
        """Test decoding multiple JSON objects"""
        result1 = SSEDecoder.decode('data: {"chunk": 1}')
        result2 = SSEDecoder.decode('data: {"chunk": 2}')
        result3 = SSEDecoder.decode("data: [DONE]")

        assert result1 == {"chunk": 1}
        assert result2 == {"chunk": 2}
        assert result3 is None

    def test_decode_invalid_json(self):
        """Test decoding invalid JSON returns None"""
        result = SSEDecoder.decode("data: not valid json")

        assert result is None

    def test_decode_nested_json(self):
        """Test decoding nested JSON objects"""
        result = SSEDecoder.decode('data: {"outer": {"inner": "value"}}')

        assert result == {"outer": {"inner": "value"}}

    def test_decode_array_json(self):
        """Test decoding JSON arrays"""
        result = SSEDecoder.decode('data: ["item1", "item2"]')

        assert result == ["item1", "item2"]

    def test_decode_with_spaces_after_prefix(self):
        """Test decoding with extra spaces after data prefix"""
        result = SSEDecoder.decode('data:   {"key": "value"}')

        assert result == {"key": "value"}

    def test_decode_line_without_data_prefix(self):
        """Test decoding line without data prefix"""
        result = SSEDecoder.decode("this is not a data line")

        assert result is None

    def test_decode_with_special_characters(self):
        """Test decoding JSON with special characters"""
        result = SSEDecoder.decode('data: {"message": "Hello\\nWorld\\t!\\""}')

        assert result == {"message": 'Hello\nWorld\t!"'}
