"""
Structured logging configuration for DSB SDK.

Provides JSON-formatted structured logging with context tracking.
"""

import logging
import sys
from typing import Any

import structlog

from dsb_sdk.exceptions import DSBError


def configure_logging(
    level: int = logging.INFO,
    json_format: bool = True,
    include_timestamp: bool = True,
    include_log_level: bool = True,
    include_logger_name: bool = True,
    include_caller_info: bool = False,
) -> None:
    """
    Configure structured logging for the DSB SDK.

    Args:
        level: Logging level (default: logging.INFO)
        json_format: Use JSON format (default: True). Set False for readable text.
        include_timestamp: Include timestamp in logs (default: True)
        include_log_level: Include log level (default: True)
        include_logger_name: Include logger name (default: True)
        include_caller_info: Include file/function/line info (default: False)

    Example:
        ```python
        from dsb_sdk.logging import configure_logging
        import logging

        # Configure JSON logging for production
        configure_logging(level=logging.INFO, json_format=True)

        # Configure text logging for development
        configure_logging(level=logging.DEBUG, json_format=False)
        ```
    """

    # Configure structlog
    processors = [
        structlog.stdlib.filter_by_level,
        structlog.stdlib.add_logger_name,
        structlog.stdlib.add_log_level,
        structlog.stdlib.add_log_level_number,
        structlog.processors.StackInfoRenderer(),
        structlog.processors.format_exc_info,
        structlog.processors.UnicodeDecoder(),
    ]

    # Add timestamp processor if requested
    if include_timestamp:
        processors.append(structlog.processors.TimeStamper(fmt="iso"))

    if json_format:
        # Production: JSON format
        processors.append(structlog.processors.JSONRenderer())
    else:
        # Development: readable text format
        processors.append(
            structlog.dev.ConsoleRenderer(
                colors=True,
                exception_formatter=structlog.dev.plain_traceback,
            )
        )

    structlog.configure(
        processors=processors,
        wrapper_class=structlog.stdlib.BoundLogger,
        context_class=dict,
        logger_factory=structlog.stdlib.LoggerFactory(),
        cache_logger_on_first_use=True,
    )

    # Configure standard logging to forward to structlog
    logging.basicConfig(
        format="%(message)s",
        stream=sys.stdout,
        level=level,
    )

    # Set log level for DSB SDK
    logging.getLogger("dsb_sdk").setLevel(level)


def get_logger(name: str | None = None) -> structlog.stdlib.BoundLogger:
    """
    Get a structured logger instance.

    Args:
        name: Logger name (usually __name__). If None, uses "dsb_sdk"

    Returns:
        Structured logger instance

    Example:
        ```python
        from dsb_sdk.logging import get_logger

        logger = get_logger(__name__)
        logger.info("sandbox_created", sandbox_id=sandbox.id, image=image)
        ```
    """
    if name is None:
        name = "dsb_sdk"
    return structlog.get_logger(name)


class LoggingContext:
    """
    Context manager for adding contextual information to logs.

    Example:
        ```python
        from dsb_sdk.logging import LoggingContext, get_logger

        logger = get_logger(__name__)

        # All logs within this context will include sandbox_id
        with LoggingContext(sandbox_id="abc-123"):
            logger.info("creating_sandbox", image="python:3.12")
            logger.info("sandbox_ready")

        # Outside context, sandbox_id is not included
        logger.info("operation_complete")
        ```
    """

    def __init__(self, **kwargs):
        """
        Initialize logging context.

        Args:
            **kwargs: Key-value pairs to include in all log messages
        """
        self.context = kwargs
        self.token = None

    def __enter__(self):
        """Bind context to current logger."""
        self.token = structlog.contextvars.bind_contextvars(**self.context)
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Unbind context from current logger."""
        if self.token is not None:
            structlog.contextvars.unbind_contextvars(*self.context.keys())
        return False


def log_exception(
    logger: structlog.stdlib.BoundLogger,
    exception: Exception,
    message: str = "error_occurred",
    level: str = "error",
    **kwargs: Any,
) -> None:
    """
    Log an exception with structured context.

    Args:
        logger: Structured logger instance
        exception: Exception to log
        message: Log message (default: "error_occurred")
        level: Log level (default: "error")
        **kwargs: Additional context to log

    Example:
        ```python
        from dsb_sdk.logging import get_logger, log_exception

        logger = get_logger(__name__)

        try:
            result = client.sandbox.create(image="python:3.12")
        except Exception as e:
            log_exception(logger, e, "sandbox_creation_failed",
                         image="python:3.12", attempt=3)
        ```
    """
    context = {
        "exception_type": type(exception).__name__,
        "exception_message": str(exception),
        **kwargs,
    }

    if isinstance(exception, DSBError):
        context["dsb_error_type"] = exception.__class__.__name__

    log_func = getattr(logger, level, logger.error)
    log_func(message, **context)


# Pre-configured loggers for common SDK operations
class SDKLoggers:
    """Pre-configured loggers for SDK components."""

    client = get_logger("dsb_sdk.client")
    sandbox = get_logger("dsb_sdk.api.sandbox")
    ssh = get_logger("dsb_sdk.api.ssh")
    terminal = get_logger("dsb_sdk.api.terminal")
    web = get_logger("dsb_sdk.api.web")
    activities = get_logger("dsb_sdk.api.activities")
    health = get_logger("dsb_sdk.api.health")
    transport = get_logger("dsb_sdk.transport")
    retry = get_logger("dsb_sdk.utils.retry")
    circuit = get_logger("dsb_sdk.utils.circuit")


def log_request(
    logger: structlog.stdlib.BoundLogger,
    method: str,
    url: str,
    status_code: int | None = None,
    duration_ms: float | None = None,
    **kwargs: Any,
) -> None:
    """
    Log an HTTP request with structured context.

    Args:
        logger: Structured logger instance
        method: HTTP method (GET, POST, etc.)
        url: Request URL
        status_code: HTTP response status code
        duration_ms: Request duration in milliseconds
        **kwargs: Additional context
    """
    context = {
        "method": method,
        "url": url,
        "status_code": status_code,
        "duration_ms": duration_ms,
        **kwargs,
    }

    if status_code and status_code >= 400:
        logger.warning("http_request_failed", **context)
    else:
        logger.info("http_request", **context)


def log_sandbox_operation(
    logger: structlog.stdlib.BoundLogger,
    operation: str,
    sandbox_id: str,
    **kwargs: Any,
) -> None:
    """
    Log a sandbox operation with structured context.

    Args:
        logger: Structured logger instance
        operation: Operation type (create, delete, exec, etc.)
        sandbox_id: Sandbox ID
        **kwargs: Additional context
    """
    context = {
        "operation": operation,
        "sandbox_id": sandbox_id,
        **kwargs,
    }

    logger.info("sandbox_operation", **context)


def log_retry_attempt(
    logger: structlog.stdlib.BoundLogger,
    func_name: str,
    attempt: int,
    max_attempts: int,
    wait_time: float,
    last_error: Exception,
    **kwargs: Any,
) -> None:
    """
    Log a retry attempt with structured context.

    Args:
        logger: Structured logger instance
        func_name: Name of function being retried
        attempt: Current attempt number
        max_attempts: Maximum attempts
        wait_time: Wait time before next attempt
        last_error: Last exception that occurred
        **kwargs: Additional context
    """
    context = {
        "function": func_name,
        "attempt": attempt,
        "max_attempts": max_attempts,
        "wait_time_seconds": wait_time,
        "error_type": type(last_error).__name__,
        "error_message": str(last_error),
        **kwargs,
    }

    logger.warning("retry_attempt", **context)


def log_circuit_breaker_event(
    logger: structlog.stdlib.BoundLogger,
    event: str,
    breaker_name: str,
    state: str,
    **kwargs: Any,
) -> None:
    """
    Log a circuit breaker event with structured context.

    Args:
        logger: Structured logger instance
        event: Event type (opened, closed, half_open, rejected)
        breaker_name: Circuit breaker name
        state: Current state
        **kwargs: Additional context
    """
    context = {
        "event": event,
        "breaker_name": breaker_name,
        "state": state,
        **kwargs,
    }

    if event in ("opened", "rejected"):
        logger.error("circuit_breaker_event", **context)
    else:
        logger.info("circuit_breaker_event", **context)
