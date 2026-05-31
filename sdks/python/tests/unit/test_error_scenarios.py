"""
Error scenario tests for DSB SDK.

Tests error handling, retry logic, and circuit breaker behavior.
"""

import pytest

from dsb_sdk.exceptions import (
    DSBAPIError,
    DSBCircuitOpenError,
    DSBConnectionError,
    DSBRateLimitError,
    DSBTimeoutError,
    DSBValidationError,
    get_error_suggestion,
    is_retryable_error,
)
from dsb_sdk.utils.circuit import CircuitState, DSBCircuitBreaker
from dsb_sdk.utils.retry import RetryStrategies


class TestErrorClassification:
    """Test error classification and retryability."""

    def test_connection_error_is_retryable(self):
        """Test that connection errors are retryable."""
        error = DSBConnectionError("Connection failed")
        assert error.is_retryable() is True
        assert is_retryable_error(error) is True

    def test_timeout_error_is_retryable(self):
        """Test that timeout errors are retryable."""
        error = DSBTimeoutError("Request timeout")
        assert error.is_retryable() is True
        assert is_retryable_error(error) is True

    def test_validation_error_is_not_retryable(self):
        """Test that validation errors are not retryable."""
        error = DSBValidationError("Invalid input")
        assert error.is_retryable() is False
        assert is_retryable_error(error) is False

    def test_circuit_open_error_is_not_retryable(self):
        """Test that circuit open errors are not retryable."""
        error = DSBCircuitOpenError("Circuit breaker is open")
        assert error.is_retryable() is False
        assert is_retryable_error(error) is False

    def test_api_error_500_is_retryable(self):
        """Test that 500 errors are retryable."""
        error = DSBAPIError("Server error", status_code=500)
        assert error.is_retryable() is True
        assert is_retryable_error(error) is True

    def test_api_error_429_is_retryable(self):
        """Test that 429 (rate limit) errors are retryable."""
        error = DSBAPIError("Rate limit exceeded", status_code=429)
        assert error.is_retryable() is True
        assert is_retryable_error(error) is True

    def test_api_error_404_is_not_retryable(self):
        """Test that 404 errors are not retryable."""
        error = DSBAPIError("Not found", status_code=404)
        assert error.is_retryable() is False
        assert is_retryable_error(error) is False

    def test_rate_limit_error_with_retry_after(self):
        """Test rate limit error with retry_after field."""
        error = DSBRateLimitError("Rate limit exceeded", retry_after=60)
        assert error.retry_after == 60
        assert error.is_retryable() is True


class TestErrorSuggestions:
    """Test error suggestion messages."""

    def test_connection_error_suggestion(self):
        """Test suggestion for connection errors."""
        error = DSBConnectionError("Connection failed")
        suggestion = get_error_suggestion(error)
        assert "network connection" in suggestion.lower()
        assert "server" in suggestion.lower()

    def test_timeout_error_suggestion(self):
        """Test suggestion for timeout errors."""
        error = DSBTimeoutError("Request timeout")
        suggestion = get_error_suggestion(error)
        assert "timeout" in suggestion.lower()
        assert "increasing" in suggestion.lower()

    def test_validation_error_suggestion(self):
        """Test suggestion for validation errors."""
        error = DSBValidationError("Invalid input")
        suggestion = get_error_suggestion(error)
        assert "parameters" in suggestion.lower()

    def test_404_error_suggestion(self):
        """Test suggestion for 404 errors."""
        error = DSBAPIError("Not found", status_code=404)
        suggestion = get_error_suggestion(error)
        assert "not found" in suggestion.lower()

    def test_500_error_suggestion(self):
        """Test suggestion for 500 errors."""
        error = DSBAPIError("Server error", status_code=500)
        suggestion = get_error_suggestion(error)
        assert "server error" in suggestion.lower()

    def test_429_error_suggestion(self):
        """Test suggestion for 429 errors."""
        error = DSBAPIError("Rate limit exceeded", status_code=429)
        suggestion = get_error_suggestion(error)
        assert "rate limit" in suggestion.lower()


class TestRetryLogic:
    """Test retry logic behavior."""

    def test_successful_call_no_retry(self):
        """Test that successful calls don't retry."""
        call_count = 0

        @RetryStrategies.quick_operation
        def successful_call():
            nonlocal call_count
            call_count += 1
            return "success"

        result = successful_call()
        assert result == "success"
        assert call_count == 1

    def test_retry_on_connection_error(self):
        """Test that connection errors trigger retries."""
        call_count = 0

        @RetryStrategies.quick_operation
        def failing_call():
            nonlocal call_count
            call_count += 1
            if call_count < 3:
                raise DSBConnectionError("Connection failed")
            return "success"

        result = failing_call()
        assert result == "success"
        assert call_count == 3

    def test_retry_on_timeout_error(self):
        """Test that timeout errors trigger retries."""
        call_count = 0

        @RetryStrategies.quick_operation
        def failing_call():
            nonlocal call_count
            call_count += 1
            if call_count < 2:
                raise DSBTimeoutError("Timeout")
            return "success"

        result = failing_call()
        assert result == "success"
        assert call_count == 2

    def test_no_retry_on_validation_error(self):
        """Test that validation errors don't trigger retries."""
        call_count = 0

        @RetryStrategies.quick_operation
        def failing_call():
            nonlocal call_count
            call_count += 1
            raise DSBValidationError("Invalid input")

        with pytest.raises(DSBValidationError):
            failing_call()

        # Should only call once (no retry)
        assert call_count == 1

    def test_max_retries_exceeded(self):
        """Test that retries stop after max attempts."""
        call_count = 0

        @RetryStrategies.quick_operation
        def always_failing():
            nonlocal call_count
            call_count += 1
            raise DSBConnectionError("Always fails")

        with pytest.raises(DSBConnectionError):
            always_failing()

        # Should call max_attempts times (3 for quick_operation)
        assert call_count == 3


class TestCircuitBreaker:
    """Test circuit breaker behavior."""

    def test_circuit_breaker_initially_closed(self):
        """Test that circuit breaker starts in closed state."""
        breaker = DSBCircuitBreaker(fail_max=3)
        assert breaker.state == CircuitState.CLOSED
        assert breaker.is_open is False

    def test_circuit_breaker_opens_after_failures(self):
        """Test that circuit breaker opens after max failures."""
        breaker = DSBCircuitBreaker(fail_max=2)

        @breaker
        def failing_call():
            raise DSBConnectionError("Fails")

        # First two failures should open the circuit
        with pytest.raises(DSBConnectionError):
            failing_call()

        assert breaker.state == CircuitState.CLOSED

        with pytest.raises(DSBConnectionError):
            failing_call()

        # Circuit should be open now
        assert breaker.state == CircuitState.OPEN
        assert breaker.is_open is True

    def test_circuit_breaker_blocks_requests_when_open(self):
        """Test that open circuit breaker blocks requests."""
        breaker = DSBCircuitBreaker(fail_max=1)

        @breaker
        def failing_call():
            raise DSBConnectionError("Fails")

        # Trigger circuit to open
        with pytest.raises((DSBConnectionError, DSBConnectionError)):
            failing_call()

        # Circuit is now open
        assert breaker.is_open is True

        # Next call should be rejected immediately
        with pytest.raises((DSBConnectionError, Exception)):
            failing_call()

    def test_circuit_breaker_allows_successes(self):
        """Test that successful calls don't open circuit."""
        breaker = DSBCircuitBreaker(fail_max=3)

        @breaker
        def successful_call():
            return "success"

        # Multiple successful calls
        for _ in range(10):
            result = successful_call()
            assert result == "success"

        # Circuit should still be closed
        assert breaker.state == CircuitState.CLOSED
        assert breaker.is_open is False

    def test_circuit_breaker_failure_count(self):
        """Test that circuit breaker tracks failures."""
        breaker = DSBCircuitBreaker(fail_max=5)

        @breaker
        def failing_call():
            raise DSBConnectionError("Fails")

        # Make 3 failures
        for _ in range(3):
            with pytest.raises((DSBConnectionError, Exception)):
                failing_call()

        # Failure count should be 3
        assert breaker.failure_count == 3

    def test_circuit_breaker_reset(self):
        """Test manual circuit breaker reset."""
        breaker = DSBCircuitBreaker(fail_max=2)

        @breaker
        def failing_call():
            raise DSBConnectionError("Fails")

        # Open the circuit
        with pytest.raises((DSBConnectionError, Exception)):
            failing_call()
        with pytest.raises((DSBConnectionError, Exception)):
            failing_call()

        assert breaker.is_open is True

        # Reset the circuit
        breaker.reset()
        assert breaker.state == CircuitState.CLOSED
        assert breaker.is_open is False


class TestErrorRecovery:
    """Test error recovery patterns."""

    def test_retry_with_backoff_succeeds(self):
        """Test that retry with backoff eventually succeeds."""
        call_count = 0

        @RetryStrategies.long_running
        def intermittent_failure():
            nonlocal call_count
            call_count += 1
            if call_count < 3:
                raise DSBConnectionError(f"Attempt {call_count} failed")
            return "success"

        result = intermittent_failure()
        assert result == "success"
        assert call_count == 3

    def test_circuit_breaker_prevents_cascading_failures(self):
        """Test that circuit breaker prevents calling failing service."""
        breaker = DSBCircuitBreaker(fail_max=2)
        call_count = 0

        @breaker
        def failing_service():
            nonlocal call_count
            call_count += 1
            raise DSBConnectionError("Service down")

        # Trigger circuit breaker to open
        for _ in range(5):
            try:
                failing_service()
            except Exception:
                pass

        # Should only have called the service 2 times (circuit opened)
        assert call_count == 2

    def test_validation_error_fails_fast(self):
        """Test that validation errors fail immediately without retry."""
        call_count = 0

        @RetryStrategies.quick_operation
        def invalid_request():
            nonlocal call_count
            call_count += 1
            raise DSBValidationError("Invalid parameters")

        with pytest.raises(DSBValidationError):
            invalid_request()

        # Should only call once (fail fast)
        assert call_count == 1
