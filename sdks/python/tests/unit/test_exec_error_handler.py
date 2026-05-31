"""Unit tests for exec_error_handler module"""

import json

import pytest

from dsb_sdk.exceptions import DSBTimeoutError, DSBValidationError
from dsb_sdk.utils.exec_error_handler import parse_exec_result


class TestParseExecResult:
    """Test parse_exec_result function"""

    def test_new_format_direct_result(self):
        """Test new format - direct result without wrapper"""
        result = {"data": "test", "count": 42}
        response = parse_exec_result(
            result=result,
            tool_name="test_tool.py",
        )
        assert response == {"data": "test", "count": 42}

    def test_new_format_error_response(self):
        """Test new format - direct error response"""
        result = {
            "error_message": "Invalid parameter",
            "status_code": 400
        }
        with pytest.raises(DSBValidationError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="test_tool.py",
            )
        assert "Invalid parameter" in str(exc_info.value)

    def test_new_format_timeout_error(self):
        """Test new format - timeout error response"""
        result = {
            "error_message": "Command timed out after 60 seconds",
            "status_code": 500
        }
        with pytest.raises(DSBTimeoutError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="test_tool.py",
            )
        assert "timed out after 60 seconds" in str(exc_info.value)
        assert exc_info.value.is_retryable() is True

    def test_legacy_format_success_response(self):
        """Test legacy format - successful response with result field"""
        result = {
            "output": json.dumps({"status": "success", "result": {"data": "test"}}),
            "exit_code": 0,
        }
        response = parse_exec_result(
            result=result,
            tool_name="test_tool.py",
        )
        assert response == {"data": "test"}

    def test_legacy_format_success_without_result_field(self):
        """Test legacy format - successful response without result field (returns whole response)"""
        result = {
            "output": json.dumps({"status": "success", "data": "test"}),
            "exit_code": 0,
        }
        response = parse_exec_result(
            result=result,
            tool_name="test_tool.py",
        )
        assert response == {"status": "success", "data": "test"}

    def test_legacy_format_timeout_scenario(self):
        """Test legacy format - timeout error raises DSBTimeoutError"""
        result = {
            "output": json.dumps({
                "status": "error",
                "error_message": "Command timed out after 60 seconds"
            }),
            "exit_code": 0,
        }
        with pytest.raises(DSBTimeoutError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="test_tool.py",
            )
        assert "timed out after 60 seconds" in str(exc_info.value)
        assert exc_info.value.is_retryable() is True

    def test_legacy_format_custom_timeout_value(self):
        """Test legacy format - timeout with custom timeout value"""
        result = {
            "output": json.dumps({
                "status": "error",
                "error_message": "Command timed out after 120 seconds"
            }),
            "exit_code": 0,
        }
        with pytest.raises(DSBTimeoutError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="browser_tools.js",
            )
        assert "timed out after 120 seconds" in str(exc_info.value)
        assert exc_info.value.is_retryable() is True

    def test_legacy_format_timeout_case_insensitive(self):
        """Test legacy format - timeout detection is case-insensitive"""
        result = {
            "output": json.dumps({
                "status": "error",
                "error_message": "Command TIMED OUT after 30 seconds"
            }),
            "exit_code": 0,
        }
        with pytest.raises(DSBTimeoutError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="test_tool.py",
            )
        assert "TIMED OUT after 30 seconds" in str(exc_info.value)

    def test_legacy_format_error_status(self):
        """Test legacy format - error status in stdout"""
        result = {
            "output": json.dumps({
                "status": "error",
                "error_message": "Invalid parameter"
            }),
            "exit_code": 1,
        }
        with pytest.raises(DSBValidationError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="test_tool.py",
            )
        assert "Invalid parameter" in str(exc_info.value)
        assert exc_info.value.is_retryable() is False

    def test_legacy_format_invalid_json(self):
        """Test legacy format - invalid JSON in stdout"""
        result = {
            "output": "Some text output",
            "exit_code": 1,
        }
        with pytest.raises(DSBValidationError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="test_tool.py",
            )
        assert "Invalid JSON response from test_tool.py" in str(exc_info.value)

    def test_legacy_format_missing_output_field(self):
        """Test legacy format - result with missing output field"""
        result = {
            "exit_code": 0,
        }
        with pytest.raises(DSBValidationError) as exc_info:
            parse_exec_result(
                result=result,
                tool_name="test_tool.py",
            )
        assert "Invalid JSON response" in str(exc_info.value)
