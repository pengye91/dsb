#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
"""
Unified error handling for sandbox tools.

Provides exception-based error handling for all sandbox tools.
Tools raise SandboxError which is caught by tool_proxy.py and converted to HTTP errors.

This module integrates with the unified error code system shared across:
- Rust backend (src/api/errors.rs)
- Python SDK (sdks/python/src/dsb_sdk/error_codes.py)
- Sandbox (docker/images/sandbox/error_codes.py)
"""

from typing import Optional

from error_codes import (
    INTERNAL_ERROR,
    TOOL_VALIDATION_ERROR,
    TOOL_EXECUTION_FAILED,
    TOOL_NOT_FOUND,
    get_http_status,
)


class SandboxError(Exception):
    """
    Base exception for sandbox tool errors.

    Provides structured error information with error codes for consistent
    error reporting across all sandbox tools. Integrates with the unified
    error code system.

    Attributes:
        message: Human-readable error message
        error_code: Machine-readable error code (e.g., "TOOL_VALIDATION_ERROR")
        status_code: HTTP status code (inferred from error_code if not provided)
    """

    def __init__(
        self,
        message: str,
        error_code: str = INTERNAL_ERROR,
        status_code: Optional[int] = None,
    ):
        self.message = message
        self.error_code = error_code
        # Infer status code from error code if not explicitly provided
        self.status_code = status_code if status_code is not None else get_http_status(error_code)
        super().__init__(message)

    def to_dict(self) -> dict:
        """
        Convert error to dictionary format for JSON serialization.

        Returns:
            Dictionary with error details following RFC 9457 format
        """
        return {
            "error_code": self.error_code,
            "message": self.message,
            "status": self.status_code,
        }


class ToolValidationError(SandboxError):
    """
    Exception raised when tool input validation fails.

    This is a convenience subclass for validation errors.
    """

    def __init__(self, message: str):
        super().__init__(
            message=message,
            error_code=TOOL_VALIDATION_ERROR,
        )


class ToolExecutionError(SandboxError):
    """
    Exception raised when tool execution fails.

    This is a convenience subclass for execution errors.
    """

    def __init__(self, message: str):
        super().__init__(
            message=message,
            error_code=TOOL_EXECUTION_FAILED,
        )


class ToolNotFoundError(SandboxError):
    """
    Exception raised when a requested tool is not found.

    This is a convenience subclass for not found errors.
    """

    def __init__(self, tool_name: str):
        super().__init__(
            message=f"Tool not found: {tool_name}",
            error_code=TOOL_NOT_FOUND,
        )
