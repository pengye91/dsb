"""
Performance benchmarks for DSB SDK.

Run with: pytest tests/benchmarks/test_performance.py --benchmark-only
"""

from datetime import datetime
from unittest.mock import Mock
from uuid import uuid4

import pytest

from dsb_sdk.api.sandbox import SandboxAPI
from dsb_sdk.types import Sandbox, SandboxState


@pytest.mark.benchmark
class TestSandboxCreationPerformance:
    """Benchmark sandbox creation operations."""

    def test_create_sandbox_mock(self, benchmark):
        """Benchmark sandbox creation (mocked)."""
        from datetime import datetime
        from uuid import uuid4

        mock_transport = Mock()
        mock_response = {
            "id": str(uuid4()),
            "config": {"image": "python:3.12", "environment": {}, "ports": {}},
            "state": "running",
            "created_at": datetime.now().isoformat(),
            "updated_at": datetime.now().isoformat(),
        }
        mock_transport.request.return_value = mock_response

        def create_sandbox():
            api = SandboxAPI(mock_transport)
            return api.create(image="python:3.12")

        result = benchmark(create_sandbox)
        assert result.config.image == "python:3.12"


@pytest.mark.benchmark
class TestCommandExecutionPerformance:
    """Benchmark command execution operations."""

    def test_exec_command_mock(self, benchmark):
        """Benchmark command execution (mocked)."""
        mock_transport = Mock()
        mock_transport.request.return_value = {
            "exit_code": 0,
            "output": "hello world",
        }

        def exec_command():
            api = SandboxAPI(mock_transport)
            return api.exec("test-id", ["echo", "hello"])

        result = benchmark(exec_command)
        assert result["exit_code"] == 0
        assert result["output"] == "hello world"


@pytest.mark.benchmark
class TestSerializationPerformance:
    """Benchmark data serialization/deserialization."""

    def test_sandbox_serialization(self, benchmark):
        """Benchmark Sandbox model serialization."""
        sandbox_data = {
            "id": str(uuid4()),
            "config": {
                "image": "python:3.12-slim",
                "name": "test-sandbox",
                "environment": {"DEBUG": "true"},
                "ports": {"8080": "8080"},
                "volumes": {"/tmp": "/tmp"},
                "command": ["python", "-m", "http.server", "8080"],
            },
            "state": "running",
            "created_at": datetime.now(),
            "updated_at": datetime.now(),
        }

        def deserialize():
            return Sandbox(**sandbox_data)

        result = benchmark(deserialize)
        assert result.state == SandboxState.RUNNING


@pytest.mark.benchmark
class TestRetryLogicPerformance:
    """Benchmark retry logic overhead."""

    def test_retry_decorator_overhead(self, benchmark):
        """Benchmark retry decorator overhead (successful call)."""
        from dsb_sdk.utils.retry import RetryStrategies

        @RetryStrategies.quick_operation
        def successful_call():
            return "success"

        result = benchmark(successful_call)
        assert result == "success"


@pytest.mark.benchmark
class TestCircuitBreakerPerformance:
    """Benchmark circuit breaker overhead."""

    def test_circuit_breaker_overhead_closed(self, benchmark):
        """Benchmark circuit breaker overhead in closed state."""
        from dsb_sdk.utils.circuit import get_circuit_breaker

        breaker = get_circuit_breaker("test-breaker", fail_max=10)

        @breaker
        def successful_call():
            return "success"

        result = benchmark(successful_call)
        assert result == "success"


@pytest.mark.benchmark
class TestLoggingPerformance:
    """Benchmark structured logging overhead."""

    def test_structured_logging(self, benchmark):
        """Benchmark structured logging."""
        from dsb_sdk.logging import get_logger

        logger = get_logger(__name__)

        def log_operation():
            logger.info(
                "test_operation",
                operation="create",
                sandbox_id="test-123",
                image="python:3.12",
                duration_ms=150.5,
            )

        benchmark(log_operation)


@pytest.mark.benchmark
class TestMetricsPerformance:
    """Benchmark metrics collection overhead."""

    def test_metrics_recording(self, benchmark):
        """Benchmark metrics recording."""
        from dsb_sdk.metrics import DSBMetrics

        metrics = DSBMetrics()

        def record_metric():
            metrics.record_request(
                operation="sandbox.create",
                duration_seconds=0.150,
                status="success",
                api="sandbox-api",
            )

        benchmark(record_metric)


@pytest.mark.benchmark(group="error_handling")
class TestErrorHandlingPerformance:
    """Benchmark error handling overhead."""

    def test_exception_creation(self, benchmark):
        """Benchmark exception creation."""
        from dsb_sdk.exceptions import DSBAPIError

        def create_exception():
            return DSBAPIError(
                message="Test error",
                status_code=500,
                response_data={"error": "test"},
            )

        result = benchmark(create_exception)
        assert result.status_code == 500

    def test_retryable_check(self, benchmark):
        """Benchmark retryable error check."""
        from dsb_sdk.exceptions import DSBAPIError, is_retryable_error

        error = DSBAPIError("Test error", status_code=500)

        def check_retryable():
            return is_retryable_error(error)

        result = benchmark(check_retryable)
        assert result is True
