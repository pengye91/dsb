"""
SSE (Server-Sent Events) streaming utilities
"""

from typing import Any


class SSEDecoder:
    """
    Decoder for SSE (Server-Sent Events) streams.

    Handles parsing of SSE format:
    - Lines starting with "data: " contain event data
    - Lines with "[DONE]" indicate end of stream
    - Empty lines separate events
    """

    @staticmethod
    def decode(line: str) -> dict[str, Any] | None:
        """
        Decode a single SSE line.

        Args:
            line: Raw SSE line

        Returns:
            Parsed JSON data or None if not a data line

        Examples:
            >>> SSEDecoder.decode("data: {\"status\": \"ok\"}")
            {"status": "ok"}
            >>> SSEDecoder.decode("event: message")
            None
            >>> SSEDecoder.decode("data: [DONE]")
            None
        """
        if line.startswith("data: "):
            data = line[6:].strip()  # Remove "data: " prefix
            if data == "[DONE]":
                return None
            if not data:
                return None

            import json

            try:
                return json.loads(data)
            except json.JSONDecodeError:
                return None

        return None
