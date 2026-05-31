"""
DSB SDK Exception hierarchy with retryable classification.
"""

from typing import Any

from .error_codes import RETRYABLE_ERROR_CODES


class DSBError(Exception):
    """Base exception for all DSB SDK errors"""

    def __init__(self, message: str, retryable: bool | None = False):
        """
        Initialize DSB error.

        Args:
            message: Error message
            retryable: Whether the operation that caused this error is retryable
        """
        super().__init__(message)
        self.retryable = retryable

    def is_retryable(self) -> bool:
        """Check if this error is retryable."""
        return bool(self.retryable) if self.retryable is not None else False


class DSBAPIError(DSBError):
    """Exception raised when the DSB API returns an error response"""

    def __init__(
        self,
        message: str,
        status_code: int | None = None,
        response_data: dict[str, Any] | None = None,
        retryable: bool | None = None,
        error_code: str | None = None,
        request_id: str | None = None,
        suggestions: list | None = None,
    ):
        """
        Initialize API error.

        Args:
            message: Error message
            status_code: HTTP status code
            response_data: Response data from API
            retryable: Whether the error is retryable (auto-detected if None)
            error_code: Machine-readable error code (RFC 9457)
            request_id: Unique request identifier for troubleshooting
            suggestions: List of remediation suggestions
        """
        super().__init__(message, retryable=retryable)
        self.status_code = status_code
        self.response_data = response_data
        self.error_code = error_code
        self.request_id = request_id
        self.suggestions = suggestions or []

        # Auto-determine retryability based on status code or error_code
        if retryable is None:
            if error_code is not None:
                # Use error_code for retryability detection
                self.retryable = self._infer_retryability_from_code(error_code)
            elif status_code is not None:
                # Fall back to status code detection
                self.retryable = status_code >= 500 or status_code == 429

    def _infer_retryability_from_code(self, error_code: str) -> bool:
        """
        Infer retryability from RFC 9457 error code.

        Args:
            error_code: Machine-readable error code

        Returns:
            True if the error is retryable
        """
        return error_code in RETRYABLE_ERROR_CODES

    @classmethod
    def from_problem_details(cls, data: dict[str, Any]) -> "DSBAPIError":
        """
        Create DSBAPIError from RFC 9457 Problem Details format.

        Args:
            data: Error response data from API

        Returns:
            DSBAPIError instance

        Example:
            ```python
            error_data = {
                "error_code": "SANDBOX_NOT_FOUND",
                "title": "Sandbox Not Found",
                "status": 404,
                "detail": "Sandbox not found",
                "request_id": "req-123",
                "retryable": False,
                "suggestions": ["Check the ID", "List all sandboxes"]
            }
            error = DSBAPIError.from_problem_details(error_data)
            ```
        """
        return cls(
            message=data.get("detail", data.get("title", "Unknown error")),
            status_code=data.get("status"),
            error_code=data.get("error_code"),
            request_id=data.get("request_id"),
            retryable=data.get("retryable", False),
            response_data=data,
            suggestions=data.get("suggestions", []),
        )

    @classmethod
    def from_legacy_format(cls, data: dict[str, Any], status_code: int) -> "DSBAPIError":
        """
        Create DSBAPIError from legacy error format (backward compatibility).

        Args:
            data: Error response data from API
            status_code: HTTP status code

        Returns:
            DSBAPIError instance

        Example:
            ```python
            error_data = {
                "error": "Something went wrong",
                "status": 500,
                "hint": "Try again"
            }
            error = DSBAPIError.from_legacy_format(error_data, 500)
            ```
        """
        return cls(
            message=data.get("error", "Unknown error"),
            status_code=status_code,
            response_data=data,
        )


class DSBConnectionError(DSBError):
    """Exception raised when there's a connection error with the DSB server"""

    def __init__(self, message: str = "Connection error", retryable: bool = True):
        """
        Initialize connection error.

        Args:
            message: Error message
            retryable: Whether the error is retryable (default: True)
        """
        super().__init__(message, retryable=retryable)


class DSBTimeoutError(DSBError):
    """Exception raised when a request times out"""

    def __init__(self, message: str = "Request timeout", retryable: bool = True):
        """
        Initialize timeout error.

        Args:
            message: Error message
            retryable: Whether the error is retryable (default: True)
        """
        super().__init__(message, retryable=retryable)


class DSBValidationError(DSBError):
    """Exception raised when request validation fails"""

    def __init__(self, message: str = "Validation error", retryable: bool = False):
        """
        Initialize validation error.

        Args:
            message: Error message
            retryable: Whether the error is retryable (default: False)
        """
        super().__init__(message, retryable=retryable)


class DSBCircuitOpenError(DSBError):
    """Exception raised when circuit breaker is open"""

    def __init__(self, message: str = "Circuit breaker is open", retryable: bool = False):
        """
        Initialize circuit breaker error.

        Args:
            message: Error message
            retryable: Whether the error is retryable (default: False)
        """
        super().__init__(message, retryable=retryable)


class DSBAuthenticationError(DSBError):
    """Exception raised when authentication fails"""

    def __init__(self, message: str = "Authentication failed", retryable: bool = False):
        """
        Initialize authentication error.

        Args:
            message: Error message
            retryable: Whether the error is retryable (default: False)
        """
        super().__init__(message, retryable=retryable)


class DSBRateLimitError(DSBAPIError):
    """Exception raised when rate limit is exceeded"""

    def __init__(self, message: str = "Rate limit exceeded", retry_after: int | None = None):
        """
        Initialize rate limit error.

        Args:
            message: Error message
            retry_after: Seconds to wait before retrying
        """
        super().__init__(
            message,
            status_code=429,
            response_data={"retry_after": retry_after} if retry_after else None,
            retryable=True,
        )
        self.retry_after = retry_after


def is_retryable_error(error: Exception) -> bool:
    """
    Check if an error is retryable.

    Args:
        error: Exception to check

    Returns:
        True if the error should be retried

    Example:
        ```python
        try:
            result = client.sandbox.create(image="python:3.12")
        except DSBAPIError as e:
            if is_retryable_error(e):
                # Retry the operation
                logger.warning(f"Retryable error: {e}")
            else:
                # Don't retry, handle the error
                logger.error(f"Non-retryable error: {e}")
                raise
        ```
    """
    # DSB errors with explicit retryable flag
    if isinstance(error, DSBError):
        return error.is_retryable()

    # Connection and timeout errors are generally retryable
    if isinstance(error, (DSBConnectionError, DSBTimeoutError)):
        return True

    # Non-DSB errors - be conservative and don't retry
    return False


def get_error_suggestion(error: Exception) -> str:
    """
    Get a helpful suggestion for handling an error.

    Args:
        error: Exception to get suggestion for

    Returns:
        Helpful suggestion message

    Example:
        ```python
        try:
            result = client.sandbox.create(image="python:3.12")
        except DSBAPIError as e:
            print(f"Error: {e}")
            print(f"Suggestion: {get_error_suggestion(e)}")
        ```
    """
    if isinstance(error, DSBConnectionError):
        return (
            "Check your network connection and ensure the DSB server is running. "
            "If using a custom API URL, verify it's correct."
        )
    elif isinstance(error, DSBTimeoutError):
        return (
            "The request timed out. Try increasing the timeout parameter or "
            "check if the server is under heavy load."
        )
    elif isinstance(error, DSBCircuitOpenError):
        return (
            "The service is temporarily unavailable due to repeated failures. "
            "Wait a moment and try again."
        )
    elif isinstance(error, DSBRateLimitError):
        if error.retry_after:
            return f"Rate limit exceeded. Wait {error.retry_after} seconds before retrying."
        return "Rate limit exceeded. Wait a moment before retrying."
    elif isinstance(error, DSBValidationError):
        return "Check your request parameters and ensure they match the API requirements."
    elif isinstance(error, DSBAuthenticationError):
        return "Check your API credentials and authentication configuration."
    elif isinstance(error, DSBAPIError):
        if error.status_code == 404:
            return "The requested resource was not found. Check your resource ID."
        elif error.status_code == 500:
            return "Server error. Try again later or contact support if the issue persists."
        elif error.status_code == 429:
            return "Too many requests. Implement rate limiting or wait before retrying."
        else:
            return f"API error with status code {error.status_code}. Check the error details."
    else:
        return "An unexpected error occurred. Check the error details and logs."
