"""
Metrics collection for DSB SDK using Prometheus.

Provides instrumentation for monitoring SDK performance and behavior.
"""

import time
from collections.abc import Callable
from functools import wraps
from typing import Any

from prometheus_client import (
    CONTENT_TYPE_LATEST,
    CollectorRegistry,
    Counter,
    Gauge,
    Histogram,
    Summary,
    generate_latest,
)


class DSBMetrics:
    """
    Metrics registry for DSB SDK operations.

    Tracks request latency, error rates, and operation counts.

    Example:
        ```python
        from dsb_sdk.metrics import DSBMetrics

        # Initialize metrics
        metrics = DSBMetrics()

        # Track a request
        with metrics.track_request("sandbox.create", "sandbox-api"):
            result = client.sandbox.create(image="python:3.12")

        # Record an error
        metrics.record_error("sandbox.create", "timeout", "sandbox-api")

        # Export metrics for Prometheus
        metrics_data = metrics.export()
        ```
    """

    def __init__(self, registry: CollectorRegistry | None = None):
        """
        Initialize metrics.

        Args:
            registry: Prometheus collector registry. If None, uses default registry.
        """
        self.registry = registry or CollectorRegistry()

        # Request latency histogram
        self.request_duration = Histogram(
            "dsb_request_duration_seconds",
            "DSB API request latency",
            ["operation", "api"],
            registry=self.registry,
        )

        # Request counter
        self.request_count = Counter(
            "dsb_requests_total",
            "Total DSB API requests",
            ["operation", "api", "status"],
            registry=self.registry,
        )

        # Error counter
        self.error_count = Counter(
            "dsb_errors_total",
            "Total DSB SDK errors",
            ["operation", "error_type", "api"],
            registry=self.registry,
        )

        # Retry counter
        self.retry_count = Counter(
            "dsb_retries_total",
            "Total retry attempts",
            ["operation"],
            registry=self.registry,
        )

        # Circuit breaker events
        self.circuit_breaker_events = Counter(
            "dsb_circuit_breaker_events_total",
            "Circuit breaker events",
            ["breaker_name", "event"],
            registry=self.registry,
        )

        # Active sandboxes gauge
        self.active_sandboxes = Gauge(
            "dsb_active_sandboxes",
            "Number of active sandboxes",
            registry=self.registry,
        )

        # Sandbox operations summary
        self.sandbox_operation_duration = Summary(
            "dsb_sandbox_operation_duration_seconds",
            "Sandbox operation duration",
            ["operation"],
            registry=self.registry,
        )

    def track_request(
        self,
        operation: str,
        api: str = "unknown",
    ):
        """
        Context manager for tracking request metrics.

        Args:
            operation: Operation name (e.g., "sandbox.create")
            api: API name (e.g., "sandbox-api")

        Returns:
            Context manager

        Example:
            ```python
            with metrics.track_request("sandbox.create", "sandbox-api"):
                result = client.sandbox.create(image="python:3.12")
            ```
        """
        return RequestTracker(self, operation, api)

    def record_request(
        self,
        operation: str,
        duration_seconds: float,
        status: str = "success",
        api: str = "unknown",
    ):
        """
        Record a request metric.

        Args:
            operation: Operation name
            duration_seconds: Request duration in seconds
            status: Request status (success, error, timeout)
            api: API name
        """
        self.request_duration.labels(operation=operation, api=api).observe(duration_seconds)
        self.request_count.labels(operation=operation, api=api, status=status).inc()

    def record_error(
        self,
        operation: str,
        error_type: str,
        api: str = "unknown",
    ):
        """
        Record an error metric.

        Args:
            operation: Operation name
            error_type: Error type (e.g., "ConnectionError", "Timeout")
            api: API name
        """
        self.error_count.labels(operation=operation, error_type=error_type, api=api).inc()

    def record_retry(self, operation: str):
        """
        Record a retry attempt.

        Args:
            operation: Operation being retried
        """
        self.retry_count.labels(operation=operation).inc()

    def record_circuit_breaker_event(self, breaker_name: str, event: str):
        """
        Record a circuit breaker event.

        Args:
            breaker_name: Circuit breaker name
            event: Event type (opened, closed, half_open, rejected)
        """
        self.circuit_breaker_events.labels(breaker_name=breaker_name, event=event).inc()

    def set_active_sandboxes(self, count: int):
        """
        Set the active sandbox count.

        Args:
            count: Number of active sandboxes
        """
        self.active_sandboxes.set(count)

    def record_sandbox_operation(
        self,
        operation: str,
        duration_seconds: float,
    ):
        """
        Record a sandbox operation duration.

        Args:
            operation: Operation type (create, delete, exec, etc.)
            duration_seconds: Duration in seconds
        """
        self.sandbox_operation_duration.labels(operation=operation).observe(duration_seconds)

    def export(self) -> bytes:
        """
        Export metrics in Prometheus text format.

        Returns:
            Metrics data as bytes
        """
        return generate_latest(self.registry)

    def get_content_type(self) -> str:
        """
        Get the content type for metrics export.

        Returns:
            Content type string
        """
        return CONTENT_TYPE_LATEST


class RequestTracker:
    """Context manager for tracking requests."""

    def __init__(self, metrics: DSBMetrics, operation: str, api: str):
        self.metrics = metrics
        self.operation = operation
        self.api = api
        self.start_time: float | None = None
        self.error: Exception | None = None

    def __enter__(self):
        self.start_time = time.time()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        if self.start_time is None:
            return

        duration = time.time() - self.start_time

        if exc_type is not None:
            # Error occurred
            error_type = exc_type.__name__
            self.metrics.record_error(self.operation, error_type, self.api)
            self.metrics.record_request(self.operation, duration, "error", self.api)
        else:
            # Success
            self.metrics.record_request(self.operation, duration, "success", self.api)

        return False


def track_metrics(metrics: DSBMetrics | None = None):
    """
    Decorator for automatically tracking function metrics.

    Args:
        metrics: DSBMetrics instance. If None, uses global metrics.

    Returns:
        Decorator function

    Example:
        ```python
        from dsb_sdk.metrics import track_metrics

        @track_metrics()
        def create_sandbox(image: str):
            return client.sandbox.create(image=image)
        ```
    """
    if metrics is None:
        metrics = get_global_metrics()

    def decorator(func: Callable) -> Callable:
        @wraps(func)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            operation = f"{func.__module__}.{func.__name__}"
            with metrics.track_request(operation):
                return func(*args, **kwargs)

        return wrapper

    return decorator


# Global metrics instance
_global_metrics: DSBMetrics | None = None


def get_global_metrics() -> DSBMetrics:
    """
    Get the global metrics instance.

    Returns:
        DSBMetrics instance
    """
    global _global_metrics
    if _global_metrics is None:
        _global_metrics = DSBMetrics()
    return _global_metrics


def reset_global_metrics():
    """Reset the global metrics instance."""
    global _global_metrics
    _global_metrics = None


class MetricsMiddleware:
    """
    Middleware for automatically tracking HTTP request metrics.

    Can be integrated with the transport layer for automatic instrumentation.

    Example:
        ```python
        from dsb_sdk.metrics import MetricsMiddleware

        middleware = MetricsMiddleware()

        # Track an HTTP request
        def send_request(method, url, **kwargs):
            with middleware.track_http_request(method, url):
                return httpx.request(method, url, **kwargs)
        ```
    """

    def __init__(self, metrics: DSBMetrics | None = None):
        """
        Initialize middleware.

        Args:
            metrics: DSBMetrics instance. If None, uses global metrics.
        """
        self.metrics = metrics or get_global_metrics()

    def track_http_request(self, method: str, url: str, api: str = "unknown"):
        """
        Track an HTTP request.

        Args:
            method: HTTP method
            url: Request URL
            api: API name

        Returns:
            Context manager
        """
        operation = f"{method} {url}"
        return self.metrics.track_request(operation, api)


def create_metrics_summary(metrics: DSBMetrics | None = None) -> dict[str, Any]:
    """
    Create a human-readable summary of metrics.

    Args:
        metrics: DSBMetrics instance. If None, uses global metrics.

    Returns:
        Dictionary with metrics summary

    Example:
        ```python
        from dsb_sdk.metrics import create_metrics_summary

        summary = create_metrics_summary()
        print(f"Total requests: {summary['total_requests']}")
        print(f"Total errors: {summary['total_errors']}")
        print(f"Error rate: {summary['error_rate']:.2%}")
        ```
    """
    if metrics is None:
        metrics = get_global_metrics()

    # Collect all metric samples
    summary: dict[str, Any] = {}

    for metric in metrics.registry.collect():
        for sample in metric.samples:
            name = sample.name

            if name == "dsb_requests_total":
                summary.setdefault("total_requests", 0)
                summary["total_requests"] += sample.value or 0

            elif name == "dsb_errors_total":
                summary.setdefault("total_errors", 0)
                summary["total_errors"] += sample.value or 0

            elif name == "dsb_retries_total":
                summary.setdefault("total_retries", 0)
                summary["total_retries"] += sample.value or 0

            elif name == "dsb_request_duration_seconds" and sample.name.endswith("_sum"):
                summary.setdefault("total_duration_seconds", 0)
                summary["total_duration_seconds"] += sample.value or 0

    # Calculate derived metrics
    if summary.get("total_requests", 0) > 0:
        summary["error_rate"] = summary.get("total_errors", 0) / summary["total_requests"]
        summary["avg_duration_seconds"] = (
            summary.get("total_duration_seconds", 0) / summary["total_requests"]
        )
    else:
        summary["error_rate"] = 0.0
        summary["avg_duration_seconds"] = 0.0

    return summary
