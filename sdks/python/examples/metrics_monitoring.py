"""
Example: Monitoring DSB SDK metrics with Prometheus.

This example demonstrates how to use the metrics module to expose
Prometheus metrics for your DSB SDK usage.
"""

from dsb_sdk import DSBClient
from dsb_sdk.metrics import (
    DSBMetrics,
    create_metrics_summary,
    get_global_metrics,
    track_metrics,
)


def example_basic_metrics():
    """Example: Basic metrics setup and usage."""
    print("=" * 60)
    print("Basic Metrics Example")
    print("=" * 60)

    # Create metrics instance
    metrics = DSBMetrics()

    # Create client
    client = DSBClient()

    print("\n1. Tracking sandbox creation...")
    try:
        with metrics.track_request("sandbox.create", "sandbox-api"):
            sandbox = client.sandbox.create(
                image="python:3.12",
                name="metrics-test",
            )
        print(f"   Sandbox created: {sandbox.id}")

        # Track execution with manual timing
        import time

        start = time.monotonic()
        result = client.sandbox.exec(str(sandbox.id), ["echo", "hello"])
        duration = time.monotonic() - start
        metrics.record_request("sandbox.exec", duration, "success", "sandbox-api")
        print(f"   Command output: {result.get('output', '').strip()}")

        # Record sandbox operation
        metrics.record_sandbox_operation("create", duration)
        metrics.set_active_sandboxes(1)

        # Get metrics summary
        print("\n2. Metrics summary:")
        summary = create_metrics_summary(metrics)
        print(f"   Total requests: {summary.get('total_requests', 0)}")
        print(f"   Total errors: {summary.get('total_errors', 0)}")
        print(f"   Error rate: {summary.get('error_rate', 0):.2%}")

        # Export metrics
        print("\n3. Exported metrics:")
        metrics_data = metrics.export().decode()
        for line in metrics_data.split("\n")[:10]:
            if line and not line.startswith("#"):
                print(f"   {line}")

        # Cleanup
        client.sandbox.delete(str(sandbox.id))
        print("\n4. Sandbox deleted")

    except Exception as e:
        print(f"   Note: {e}")
        print("   (Expected without running DSB server)")


def example_global_metrics():
    """Example: Using global metrics instance."""
    print("\n" + "=" * 60)
    print("Global Metrics Example")
    print("=" * 60)

    # Get global metrics instance
    metrics = get_global_metrics()

    print("\n1. Using global metrics instance...")
    print(f"   Metrics registry has {len(list(metrics.registry.collect()))} collectors")

    # Track a request
    with metrics.track_request("test.operation", "test-api"):
        import time

        time.sleep(0.01)  # Simulate work

    print("\n2. Metrics summary:")
    summary = create_metrics_summary(metrics)
    print(f"   Total requests: {summary.get('total_requests', 0)}")


def example_custom_metrics():
    """Example: Custom metrics with labels."""
    print("\n" + "=" * 60)
    print("Custom Metrics Example")
    print("=" * 60)

    from prometheus_client import Counter, Gauge, Histogram

    # Create custom metrics
    custom_counter = Counter(
        "my_app_sandbox_operations_total",
        "Total number of sandbox operations",
        ["operation", "status"],
    )

    custom_histogram = Histogram(
        "my_app_operation_duration_seconds",
        "Duration of sandbox operations",
        ["operation"],
        buckets=[0.1, 0.5, 1.0, 2.0, 5.0],
    )

    custom_gauge = Gauge(
        "my_app_active_sandboxes",
        "Number of currently active sandboxes",
    )

    # Use the metrics
    print("\n1. Recording custom metrics...")
    custom_counter.labels(operation="create", status="success").inc()
    custom_counter.labels(operation="delete", status="success").inc()

    import time

    with custom_histogram.labels(operation="exec").time():
        time.sleep(0.1)  # Simulate operation

    custom_gauge.set(5)

    print("   Custom metrics recorded")
    print(f"   Sandbox operations: {custom_counter._value.get()}")
    print(f"   Active sandboxes: {custom_gauge._value.get()}")


def example_metrics_decorator():
    """Example: Using the metrics decorator."""
    print("\n" + "=" * 60)
    print("Metrics Decorator Example")
    print("=" * 60)

    # Create a metrics-tracked function
    metrics = DSBMetrics()

    @track_metrics(metrics)
    def my_sandbox_operation(image: str):
        """A sample sandbox operation."""
        # In real usage, this would call the SDK
        import time

        time.sleep(0.05)
        return {"status": "success"}

    print("\n1. Calling tracked function...")
    result = my_sandbox_operation("python:3.12")
    print(f"   Result: {result}")

    # Check metrics
    print("\n2. Metrics summary:")
    summary = create_metrics_summary(metrics)
    print(f"   Total requests: {summary.get('total_requests', 0)}")


def example_error_tracking():
    """Example: Tracking errors with metrics."""
    print("\n" + "=" * 60)
    print("Error Tracking Example")
    print("=" * 60)

    metrics = DSBMetrics()

    print("\n1. Recording errors...")
    metrics.record_error("sandbox.create", "ConnectionError", "sandbox-api")
    metrics.record_error("sandbox.create", "TimeoutError", "sandbox-api")
    metrics.record_error("sandbox.delete", "NotFoundError", "sandbox-api")

    print("\n2. Metrics summary:")
    summary = create_metrics_summary(metrics)
    print(f"   Total requests: {summary.get('total_requests', 0)}")
    print(f"   Total errors: {summary.get('total_errors', 0)}")
    print(f"   Error rate: {summary.get('error_rate', 0):.2%}")

    # Export and show error metrics
    print("\n3. Error metrics:")
    metrics_data = metrics.export().decode()
    for line in metrics_data.split("\n"):
        if "dsb_errors_total" in line:
            print(f"   {line}")


def main():
    """Run all examples."""
    print("\n" + "#" * 60)
    print("# DSB SDK Metrics Examples")
    print("#" * 60)

    examples = [
        ("Basic Metrics", example_basic_metrics),
        ("Global Metrics", example_global_metrics),
        ("Custom Metrics", example_custom_metrics),
        ("Metrics Decorator", example_metrics_decorator),
        ("Error Tracking", example_error_tracking),
    ]

    for name, func in examples:
        try:
            func()
        except Exception as e:
            print(f"\n   Error in {name}: {e}")

    print("\n" + "#" * 60)
    print("# Examples completed!")
    print("#" * 60)


if __name__ == "__main__":
    main()
