"""
Unified error codes shared with Rust backend.

This module provides error code constants that MUST stay in sync with the Rust
ErrorCode enum in `src/api/errors.rs` and the sandbox error_codes.py.

Error codes follow RFC 9457 (Problem Details for HTTP APIs) and use snake_case
format (e.g., SANDBOX_NOT_FOUND).

# When adding new error codes:
1. Add to Rust ErrorCode enum in src/api/errors.rs
2. Add to sandbox error_codes.py
3. Add to this file
4. Run scripts/verify_error_codes.py to verify sync
"""

# =============================================================================
# Sandbox Errors (404, 409, 500)
# =============================================================================
SANDBOX_NOT_FOUND = "SANDBOX_NOT_FOUND"
SANDBOX_INVALID_STATE = "SANDBOX_INVALID_STATE"
SANDBOX_ALREADY_EXISTS = "SANDBOX_ALREADY_EXISTS"
SANDBOX_CREATION_FAILED = "SANDBOX_CREATION_FAILED"
SANDBOX_EXECUTION_FAILED = "SANDBOX_EXECUTION_FAILED"

# =============================================================================
# Tool Execution Errors (400, 404, 408, 500)
# =============================================================================
TOOL_NOT_FOUND = "TOOL_NOT_FOUND"
TOOL_EXECUTION_FAILED = "TOOL_EXECUTION_FAILED"
TOOL_VALIDATION_ERROR = "TOOL_VALIDATION_ERROR"
TOOL_TIMEOUT = "TOOL_TIMEOUT"

# =============================================================================
# Backend Errors (502, 503)
# =============================================================================
BACKEND_IMAGE_PULL_FAILED = "BACKEND_IMAGE_PULL_FAILED"
BACKEND_CONTAINER_CREATE_FAILED = "BACKEND_CONTAINER_CREATE_FAILED"
BACKEND_CONTAINER_START_FAILED = "BACKEND_CONTAINER_START_FAILED"
BACKEND_VOLUME_ERROR = "BACKEND_VOLUME_ERROR"
BACKEND_CONTAINER_NOT_FOUND = "BACKEND_CONTAINER_NOT_FOUND"
BACKEND_EXEC_FAILED = "BACKEND_EXEC_FAILED"

# =============================================================================
# SSH/Terminal Errors (400, 404, 500)
# =============================================================================
SSH_SESSION_NOT_FOUND = "SSH_SESSION_NOT_FOUND"
SSH_AUTHENTICATION_FAILED = "SSH_AUTHENTICATION_FAILED"
SSH_CONNECTION_FAILED = "SSH_CONNECTION_FAILED"
TERMINAL_OPERATION_FAILED = "TERMINAL_OPERATION_FAILED"

# =============================================================================
# Validation Errors (400)
# =============================================================================
VALIDATION_ERROR = "VALIDATION_ERROR"
VALIDATION_INVALID_PORT = "VALIDATION_INVALID_PORT"
VALIDATION_MISSING_FIELD = "VALIDATION_MISSING_FIELD"
VALIDATION_INVALID_IMAGE_NAME = "VALIDATION_INVALID_IMAGE_NAME"
VALIDATION_INVALID_REQUEST = "VALIDATION_INVALID_REQUEST"

# =============================================================================
# Authentication/Authorization (401, 403)
# =============================================================================
AUTHENTICATION_MISSING = "AUTHENTICATION_MISSING"
AUTHENTICATION_INVALID_API_KEY = "AUTHENTICATION_INVALID_API_KEY"
AUTHORIZATION_INSUFFICIENT_PERMISSIONS = "AUTHORIZATION_INSUFFICIENT_PERMISSIONS"

# =============================================================================
# Database Errors (503)
# =============================================================================
DATABASE_CONNECTION_FAILED = "DATABASE_CONNECTION_FAILED"
DATABASE_QUERY_FAILED = "DATABASE_QUERY_FAILED"

# =============================================================================
# Infrastructure/Service Errors (429, 502, 503)
# =============================================================================
SERVICE_UNAVAILABLE = "SERVICE_UNAVAILABLE"
RATE_LIMIT_EXCEEDED = "RATE_LIMIT_EXCEEDED"
UPSTREAM_ERROR = "UPSTREAM_ERROR"
REQUEST_TIMEOUT = "REQUEST_TIMEOUT"

# =============================================================================
# Internal Errors (500)
# =============================================================================
INTERNAL_ERROR = "INTERNAL_ERROR"
CONFIGURATION_ERROR = "CONFIGURATION_ERROR"

# =============================================================================
# Retryable Error Codes
# =============================================================================
# These error codes indicate transient failures that may resolve with retry
RETRYABLE_ERROR_CODES = frozenset([
    SERVICE_UNAVAILABLE,
    RATE_LIMIT_EXCEEDED,
    DATABASE_CONNECTION_FAILED,
    BACKEND_IMAGE_PULL_FAILED,
    BACKEND_CONTAINER_CREATE_FAILED,
    BACKEND_CONTAINER_START_FAILED,
    BACKEND_EXEC_FAILED,
    UPSTREAM_ERROR,
    REQUEST_TIMEOUT,
    TOOL_TIMEOUT,
])

# =============================================================================
# HTTP Status Code Mapping
# =============================================================================
HTTP_STATUS_MAP = {
    # 400 Bad Request
    VALIDATION_ERROR: 400,
    VALIDATION_INVALID_PORT: 400,
    VALIDATION_MISSING_FIELD: 400,
    VALIDATION_INVALID_IMAGE_NAME: 400,
    VALIDATION_INVALID_REQUEST: 400,
    TOOL_VALIDATION_ERROR: 400,

    # 401 Unauthorized
    AUTHENTICATION_MISSING: 401,
    AUTHENTICATION_INVALID_API_KEY: 401,

    # 403 Forbidden
    AUTHORIZATION_INSUFFICIENT_PERMISSIONS: 403,

    # 404 Not Found
    SANDBOX_NOT_FOUND: 404,
    TOOL_NOT_FOUND: 404,
    BACKEND_CONTAINER_NOT_FOUND: 404,
    SSH_SESSION_NOT_FOUND: 404,

    # 408 Request Timeout
    TOOL_TIMEOUT: 408,
    REQUEST_TIMEOUT: 408,

    # 409 Conflict
    SANDBOX_ALREADY_EXISTS: 409,
    SANDBOX_INVALID_STATE: 409,

    # 429 Too Many Requests
    RATE_LIMIT_EXCEEDED: 429,

    # 502 Bad Gateway
    BACKEND_IMAGE_PULL_FAILED: 502,
    BACKEND_CONTAINER_CREATE_FAILED: 502,
    BACKEND_CONTAINER_START_FAILED: 502,
    BACKEND_VOLUME_ERROR: 502,
    BACKEND_EXEC_FAILED: 502,
    SSH_CONNECTION_FAILED: 502,
    UPSTREAM_ERROR: 502,

    # 503 Service Unavailable
    DATABASE_CONNECTION_FAILED: 503,
    DATABASE_QUERY_FAILED: 503,
    SERVICE_UNAVAILABLE: 503,

    # 500 Internal Server Error
    SANDBOX_CREATION_FAILED: 500,
    SANDBOX_EXECUTION_FAILED: 500,
    TOOL_EXECUTION_FAILED: 500,
    SSH_AUTHENTICATION_FAILED: 500,
    TERMINAL_OPERATION_FAILED: 500,
    INTERNAL_ERROR: 500,
    CONFIGURATION_ERROR: 500,
}


def is_retryable_error_code(error_code: str) -> bool:
    """
    Check if an error code indicates a retryable error.

    Retryable errors are typically transient failures that may resolve
    with subsequent attempts (e.g., network issues, temporary unavailability).

    Args:
        error_code: The error code string to check

    Returns:
        True if the error is retryable

    Example:
        >>> is_retryable_error_code("SERVICE_UNAVAILABLE")
        True
        >>> is_retryable_error_code("SANDBOX_NOT_FOUND")
        False
    """
    return error_code in RETRYABLE_ERROR_CODES


def get_http_status(error_code: str) -> int:
    """
    Get the HTTP status code for an error code.

    Args:
        error_code: The error code string

    Returns:
        The HTTP status code (defaults to 500 if unknown)

    Example:
        >>> get_http_status("SANDBOX_NOT_FOUND")
        404
        >>> get_http_status("VALIDATION_ERROR")
        400
    """
    return HTTP_STATUS_MAP.get(error_code, 500)


def get_all_error_codes() -> list:
    """
    Get a list of all error codes defined in this module.

    Returns:
        List of all error code strings

    Example:
        >>> codes = get_all_error_codes()
        >>> "SANDBOX_NOT_FOUND" in codes
        True
    """
    return [
        # Sandbox errors
        SANDBOX_NOT_FOUND,
        SANDBOX_INVALID_STATE,
        SANDBOX_ALREADY_EXISTS,
        SANDBOX_CREATION_FAILED,
        SANDBOX_EXECUTION_FAILED,

        # Tool errors
        TOOL_NOT_FOUND,
        TOOL_EXECUTION_FAILED,
        TOOL_VALIDATION_ERROR,
        TOOL_TIMEOUT,

        # Backend errors
        BACKEND_IMAGE_PULL_FAILED,
        BACKEND_CONTAINER_CREATE_FAILED,
        BACKEND_CONTAINER_START_FAILED,
        BACKEND_VOLUME_ERROR,
        BACKEND_CONTAINER_NOT_FOUND,
        BACKEND_EXEC_FAILED,

        # SSH errors
        SSH_SESSION_NOT_FOUND,
        SSH_AUTHENTICATION_FAILED,
        SSH_CONNECTION_FAILED,
        TERMINAL_OPERATION_FAILED,

        # Validation errors
        VALIDATION_ERROR,
        VALIDATION_INVALID_PORT,
        VALIDATION_MISSING_FIELD,
        VALIDATION_INVALID_IMAGE_NAME,
        VALIDATION_INVALID_REQUEST,

        # Auth errors
        AUTHENTICATION_MISSING,
        AUTHENTICATION_INVALID_API_KEY,
        AUTHORIZATION_INSUFFICIENT_PERMISSIONS,

        # Database errors
        DATABASE_CONNECTION_FAILED,
        DATABASE_QUERY_FAILED,

        # Infrastructure errors
        SERVICE_UNAVAILABLE,
        RATE_LIMIT_EXCEEDED,
        UPSTREAM_ERROR,
        REQUEST_TIMEOUT,

        # Internal errors
        INTERNAL_ERROR,
        CONFIGURATION_ERROR,
    ]


def is_validation_error_code(error_code: str) -> bool:
    """
    Check if an error code is a validation error.

    All validation error codes (both generic and specific) should raise
    DSBValidationError for better error handling in client code.

    Args:
        error_code: The error code string to check

    Returns:
        True if the error is a validation error

    Example:
        >>> is_validation_error_code("VALIDATION_ERROR")
        True
        >>> is_validation_error_code("VALIDATION_INVALID_REQUEST")
        True
        >>> is_validation_error_code("SANDBOX_NOT_FOUND")
        False
    """
    validation_error_codes = frozenset([
        VALIDATION_ERROR,
        VALIDATION_INVALID_PORT,
        VALIDATION_MISSING_FIELD,
        VALIDATION_INVALID_IMAGE_NAME,
        VALIDATION_INVALID_REQUEST,
        TOOL_VALIDATION_ERROR,
    ])
    return error_code in validation_error_codes


# =============================================================================
# Backward compatibility aliases (DOCKER_* -> BACKEND_*)
# =============================================================================
DOCKER_IMAGE_PULL_FAILED = BACKEND_IMAGE_PULL_FAILED
DOCKER_CONTAINER_CREATE_FAILED = BACKEND_CONTAINER_CREATE_FAILED
DOCKER_CONTAINER_START_FAILED = BACKEND_CONTAINER_START_FAILED
DOCKER_VOLUME_ERROR = BACKEND_VOLUME_ERROR
DOCKER_CONTAINER_NOT_FOUND = BACKEND_CONTAINER_NOT_FOUND
DOCKER_EXEC_FAILED = BACKEND_EXEC_FAILED
