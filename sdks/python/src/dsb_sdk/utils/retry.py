"""
Retry utilities with exponential backoff for resilient API calls.

Provides decorators and utilities for handling transient failures in API calls.
"""

import asyncio
import logging
from collections.abc import Callable
from functools import wraps
from typing import Any, TypeVar

from tenacity import (
    after_log,
    before_sleep_log,
    retry,
    stop_after_attempt,
    wait_exponential,
)

from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError, DSBTimeoutError

logger = logging.getLogger(__name__)

T = TypeVar("T")


# Retry configuration defaults
DEFAULT_MAX_ATTEMPTS = 3
DEFAULT_MIN_WAIT = 1.0  # seconds
DEFAULT_MAX_WAIT = 10.0  # seconds
DEFAULT_EXPONENTIAL_MULTIPLIER = 1.0


def is_retryable_error(error: Exception) -> bool:
    """
    Determine if an error is retryable.

    Transient errors that might resolve on retry:
    - Connection errors (network blips)
    - Timeout errors (temporary delays)
    - 5xx server errors (server might recover)
    - 429 rate limit errors (back off and retry)

    Non-retryable errors:
    - 4xx client errors (except 429)
    - Validation errors
    - Authentication errors

    Args:
        error: The exception to evaluate

    Returns:
        True if the error should be retried, False otherwise
    """
    # Connection and timeout errors are always retryable
    if isinstance(error, (DSBConnectionError, DSBTimeoutError)):
        return True

    # API errors with specific status codes
    if isinstance(error, DSBAPIError):
        if error.status_code is None:
            return True  # No status code, might be network error

        # Retry on server errors (5xx)
        if 500 <= error.status_code < 600:
            return True

        # Retry on rate limiting (429)
        if error.status_code == 429:
            return True

    # Don't retry on other errors (validation, auth, etc.)
    return False


def retry_with_exponential_backoff(
    max_attempts: int = DEFAULT_MAX_ATTEMPTS,
    min_wait: float = DEFAULT_MIN_WAIT,
    max_wait: float = DEFAULT_MAX_WAIT,
    multiplier: float = DEFAULT_EXPONENTIAL_MULTIPLIER,
) -> Callable:
    """
    Decorator for retrying functions with exponential backoff.

    Exponential backoff increases the wait time between retries:
    Attempt 1: wait 1s
    Attempt 2: wait 2s
    Attempt 3: wait 4s
    ... and so on, up to max_wait

    Args:
        max_attempts: Maximum number of retry attempts (default: 3)
        min_wait: Minimum wait time in seconds (default: 1.0)
        max_wait: Maximum wait time in seconds (default: 10.0)
        multiplier: Exponential multiplier (default: 1.0)

    Returns:
        Decorator function

    Example:
        ```python
        @retry_with_exponential_backoff(max_attempts=5)
        def create_sandbox(image: str):
            return client.sandbox.create(image=image)
        ```
    """

    def decorator(
        func: Callable[..., T],
    ) -> Callable[..., T]:
        # Determine if function is async
        is_async = asyncio.iscoroutinefunction(func)

        @wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> T:
            @retry(
                stop=stop_after_attempt(max_attempts),
                wait=wait_exponential(multiplier=multiplier, min=min_wait, max=max_wait),
                retry=retry_if_exception_type_and_retryable,
                before_sleep=before_sleep_log(logger, logging.WARNING),
                after=after_log(logger, logging.INFO),
                reraise=True,
            )
            def sync_func_with_retry():
                return func(*args, **kwargs)

            return sync_func_with_retry()

        @wraps(func)
        async def async_wrapper(*args: Any, **kwargs: Any) -> T:
            @retry(
                stop=stop_after_attempt(max_attempts),
                wait=wait_exponential(multiplier=multiplier, min=min_wait, max=max_wait),
                retry=retry_if_exception_type_and_retryable,
                before_sleep=before_sleep_log(logger, logging.WARNING),
                after=after_log(logger, logging.INFO),
                reraise=True,
            )
            async def async_func_with_retry():
                return await func(*args, **kwargs)  # type: ignore

            return await async_func_with_retry()

        return async_wrapper if is_async else wrapper  # type: ignore

    return decorator


def retry_if_exception_type_and_retryable(retry_state: Any) -> bool:
    """
    Retry predicate for tenacity that checks both exception type and retryability.

    Args:
        retry_state: Tenacity retry state object

    Returns:
        True if exception should be retried
    """
    if retry_state.outcome.failed:
        exception = retry_state.outcome.exception()
        return is_retryable_error(exception)  # type: ignore
    return False


class RetryConfig:
    """
    Configuration for retry behavior.

    Allows customizing retry logic without changing decorators.

    Example:
        ```python
        # Create custom retry config
        retry_config = RetryConfig(
            max_attempts=5,
            min_wait=2.0,
            max_wait=30.0,
        )

        # Use with decorator
        @retry_with_config(retry_config)
        def my_function():
            ...
        ```
    """

    def __init__(
        self,
        max_attempts: int = DEFAULT_MAX_ATTEMPTS,
        min_wait: float = DEFAULT_MIN_WAIT,
        max_wait: float = DEFAULT_MAX_WAIT,
        multiplier: float = DEFAULT_EXPONENTIAL_MULTIPLIER,
    ):
        self.max_attempts = max_attempts
        self.min_wait = min_wait
        self.max_wait = max_wait
        self.multiplier = multiplier


def retry_with_config(config: RetryConfig) -> Callable:
    """
    Decorator for retrying with a RetryConfig object.

    Args:
        config: RetryConfig instance with retry parameters

    Returns:
        Decorator function
    """
    return retry_with_exponential_backoff(
        max_attempts=config.max_attempts,
        min_wait=config.min_wait,
        max_wait=config.max_wait,
        multiplier=config.multiplier,
    )


# Pre-configured retry strategies
class RetryStrategies:
    """
    Pre-configured retry strategies for common scenarios.

    Example:
        ```python
        from dsb_sdk.utils.retry import RetryStrategies

        # For quick operations (API calls, health checks)
        @RetryStrategies.quick_operation
        def check_health():
            return client.health.check()

        # For long operations (sandbox creation, file uploads)
        @RetryStrategies.long_running
        def create_sandbox(image: str):
            return client.sandbox.create(image=image)
        ```
    """

    # Quick operations: 3 attempts, short wait times
    quick_operation = retry_with_exponential_backoff(
        max_attempts=3,
        min_wait=0.5,
        max_wait=5.0,
    )

    # Long-running operations: 5 attempts, longer wait times
    long_running = retry_with_exponential_backoff(
        max_attempts=5,
        min_wait=2.0,
        max_wait=30.0,
    )

    # Critical operations: 7 attempts, very patient
    critical = retry_with_exponential_backoff(
        max_attempts=7,
        min_wait=1.0,
        max_wait=60.0,
    )


def should_retry_exception(exception: Exception) -> bool:
    """
    Public API for checking if an exception should trigger a retry.

    Useful for custom retry logic outside of decorators.

    Args:
        exception: The exception to check

    Returns:
        True if the exception is retryable

    Example:
        ```python
        try:
            result = client.sandbox.create(image="python:3.12")
        except DSBAPIError as e:
            if should_retry_exception(e):
                # Log and retry
                logger.warning(f"Retryable error: {e}")
            else:
                # Don't retry, handle error
                logger.error(f"Non-retryable error: {e}")
                raise
        ```
    """
    return is_retryable_error(exception)
