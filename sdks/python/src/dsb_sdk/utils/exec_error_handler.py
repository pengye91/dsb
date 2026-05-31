"""
Centralized error handling for sandbox tool execution.

This module provides utilities for handling results from sandbox
tool execution.

With the new architecture:
- Tool functions return values directly (not via stdout)
- Exception-based error handling with SandboxError
- Direct FastAPI JSON serialization - no wrapper object
- Errors are handled by HTTP status codes before reaching this point
"""

from __future__ import annotations

import re
from typing import Any


def parse_exec_result(
    result: dict[str, Any],
    tool_name: str,
) -> Any:
    """
    Parse sandbox exec result.

    With the new architecture, the result is already the parsed JSON data
    from the HTTP response. HTTP errors are handled before this point.

    Args:
        result: Parsed exec response dict (direct result from tool)
        tool_name: Name of tool for error messages (e.g., 'databend_tools.py', 'web_tools.py')

    Returns:
        The result data directly (already parsed from JSON)

    Raises:
        DSBTimeoutError: If error_message indicates a timeout occurred
        DSBValidationError: If result contains an error response
    """
    from dsb_sdk.exceptions import DSBTimeoutError, DSBValidationError

    # Check for legacy wrapper format (for backward compatibility during migration)
    # If result has 'output' or 'exit_code' field, it's the old format - parse from stdout
    if "output" in result or "exit_code" in result:
        return _parse_legacy_result(result, tool_name)

    # Check for direct error response (from HTTP error handling)
    if isinstance(result, dict) and "error_message" in result:
        error_msg = result.get("error_message", "Unknown error")

        # Detect timeout errors
        timeout_pattern = r"timed out after (\d+) seconds?"
        if re.search(timeout_pattern, error_msg, re.IGNORECASE):
            raise DSBTimeoutError(error_msg, retryable=True)

        # Extract Databend errors from nested format
        # Format: "Tool execution failed: HTTP error XXX: {"detail":"failed to query: b'{JSON}'"}"
        if "Tool execution failed: HTTP error" in error_msg and '"detail":' in error_msg:
            databend_error = _extract_databend_error(error_msg)
            if databend_error:
                raise DSBValidationError(databend_error)

        raise DSBValidationError(error_msg)

    # New format: result is already the data - just return it
    return result


def _extract_databend_error(error_msg: str) -> str | None:
    """
    Extract clean Databend error message from nested error format.

    Databend errors come wrapped in:
    "Tool execution failed: HTTP error 400 Bad Request: {"detail":"failed to query: b'{JSON}'"}"

    This extracts the clean Databend error message from the nested structure.

    Args:
        error_msg: Raw error message from backend

    Returns:
        Clean Databend error message, or None if not a Databend error
    """
    import json

    try:
        # Find the JSON object start: {"detail":
        json_start = error_msg.find('{"detail":')
        if json_start == -1:
            return None

        # Extract from JSON start
        json_str = error_msg[json_start:]

        # Find "detail":"
        detail_key_pos = json_str.find('"detail":"')
        if detail_key_pos == -1:
            return None

        # Move past "detail":"
        value_start = detail_key_pos + 10  # len('"detail":"')

        # Find the end of the detail value
        # The detail value ends with '" followed by }
        # Scan from the end backwards to find the last quote
        for i in range(len(json_str) - 1, value_start - 1, -1):
            # Look for an unescaped quote (not preceded by backslash)
            if json_str[i] == '"' and (i == 0 or json_str[i-1] != '\\'):
                # Check if this is followed by }
                if i + 1 < len(json_str) and json_str[i+1] == '}':
                    # This is the end of the detail value
                    detail = json_str[value_start:i]
                    break
        else:
            # Didn't find the end
            return None

        # Check if this is a Databend query error
        if not detail.startswith("failed to query: b'"):
            return None

        # Extract the JSON content between b' and '
        # Format: failed to query: b'{JSON}'
        json_content = detail[19:-1]  # Remove "failed to query: b'" and trailing '

        # Find the matching closing brace for the opening brace
        open_braces = 0
        json_end = -1
        for i, char in enumerate(json_content):
            if char == '{':
                open_braces += 1
            elif char == '}':
                open_braces -= 1
                if open_braces == 0:
                    json_end = i + 1
                    break

        if json_end == -1:
            return None

        inner_json_str = json_content[:json_end]

        # Parse the inner JSON
        databend_error = json.loads(inner_json_str)
        if "error" in databend_error and "message" in databend_error["error"]:
            error_content = databend_error["error"]["message"]
            # Remove SQL formatting markers (common in Databend errors)
            error_content = re.sub(r'--> SQL:\d+:\d+', '', error_content)
            error_content = re.sub(r'\s*\|\s*', ' | ', error_content)
            error_content = error_content.strip()
            return error_content
    except (json.JSONDecodeError, KeyError, IndexError):
        # If extraction fails, return None
        pass

    return None


def _parse_legacy_result(
    result: dict[str, Any],
    tool_name: str,
) -> Any:
    """
    Parse legacy format result (for backward compatibility).

    Legacy format has 'output' field containing stdout JSON.

    Args:
        result: Raw exec response dict with 'output' field containing stdout
        tool_name: Name of tool for error messages

    Returns:
        Parsed JSON response from tool
    """
    import json

    from dsb_sdk.exceptions import DSBTimeoutError, DSBValidationError

    # Extract stdout from result
    stdout = result.get("output", "")

    # Handle empty or missing stdout
    if not stdout:
        raise DSBValidationError(
            f"Invalid JSON response from {tool_name}: empty output"
        )

    # Parse JSON response from stdout
    try:
        response = json.loads(stdout)
    except json.JSONDecodeError as e:
        raise DSBValidationError(
            f"Invalid JSON response from {tool_name}: {e}"
        )

    # Check for error response using status field
    if response.get("status") == "error":
        # Support both error_message (server format) and error (legacy) fields
        error_msg = response.get("error_message") or response.get("error", "Unknown error")

        # Detect timeout errors from server
        timeout_pattern = r"timed out after (\d+) seconds?"
        if re.search(timeout_pattern, error_msg, re.IGNORECASE):
            raise DSBTimeoutError(error_msg, retryable=True)

        raise DSBValidationError(error_msg)

    # Return result from success response
    if response.get("status") == "success":
        result_data = response.get("result")
        # If no "result" field, return the whole response
        if result_data is None:
            return response
        return result_data

    # Fallback: return the response as-is
    return response
