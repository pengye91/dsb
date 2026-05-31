"""
SSE Streaming Examples

This example demonstrates how to use SSE (Server-Sent Events) streaming
in the DSB SDK for real-time progress updates.
"""

import asyncio
import json

from dsb_sdk import AsyncDSBClient, DSBClient


def example_sandbox_creation_stream():
    """Stream sandbox creation progress"""
    client = DSBClient(api_url="http://localhost:8080")

    print("Creating sandbox with streaming...")
    print("-" * 40)

    sandbox = None
    try:
        # Stream creation progress
        for event in client.sandbox.create_stream(image="python:3.12", name="streaming-example"):
            # event is a dict with 'data' field containing the event payload
            event_data = event.get("data", {})

            if event.get("event") == "progress":
                # Progress update during creation
                print(f"  ⏳ Progress: {event_data}")
            elif event.get("event") == "complete":
                # Creation complete, sandbox data
                from dsb_sdk.types import Sandbox

                sandbox = Sandbox.model_validate(event_data)
                print(f"  ✅ Created sandbox: {sandbox.id}")
                print(f"     State: {sandbox.state}")
                break

        print("\nSandbox created successfully!")

        # Use the sandbox
        result = client.sandbox.exec(sandbox.id, ["echo", "Hello from streaming!"])
        print(f"Output: {result.output.strip()}")

    finally:
        if sandbox:
            client.sandbox.delete(sandbox.id)
            print("\n✅ Sandbox cleaned up")


async def example_async_sandbox_creation_stream():
    """Async version of sandbox creation streaming"""
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        print("Creating sandbox with async streaming...")
        print("-" * 40)

        sandbox = None
        try:
            # Stream creation progress
            async for event in client.sandbox.create_stream(
                image="python:3.12", name="async-streaming-example"
            ):
                # event is a dict with 'data' field containing the event payload
                event_data = event.get("data", {})

                if event.get("event") == "complete":
                    from dsb_sdk.types import Sandbox

                    sandbox = Sandbox.model_validate(event_data)
                    print(f"  ✅ Created sandbox: {sandbox.id}")
                    break

            print("\nSandbox created successfully!")

        finally:
            if sandbox:
                await client.sandbox.delete(sandbox.id)
                print("\n✅ Sandbox cleaned up")


def example_stats_stream():
    """Stream sandbox statistics in real-time"""
    client = DSBClient(api_url="http://localhost:8080")

    # Create sandbox first
    sandbox = client.sandbox.create(image="python:3.12", name="stats-stream-example")
    print(f"Created sandbox: {sandbox.id}\n")

    import time

    time.sleep(3)  # Wait for sandbox to be ready

    try:
        print("Streaming statistics (5 updates)...")
        print("-" * 40)

        # Stream statistics
        count = 0
        for stats in client.sandbox.stats_stream(sandbox.id):
            print(f"\nUpdate #{count + 1}:")
            print(f"  CPU: {stats.cpu_percent:.2f}%")
            print(f"  Memory: {stats.memory_usage_mb:.2f} MB")
            print(f"  Disk I/O: {stats.disk_io}")
            print(f"  Network I/O: {stats.network_io}")

            count += 1
            if count >= 5:
                print("\n✅ Received 5 updates, stopping...")
                break

            time.sleep(2)  # Wait between updates

    finally:
        client.sandbox.delete(sandbox.id)
        print("\n✅ Sandbox cleaned up")


async def example_multi_stream_monitoring():
    """Monitor multiple sandboxes simultaneously via streaming"""
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        # Create multiple sandboxes
        print("Creating 3 sandboxes...\n")
        sandboxes = []
        for i in range(3):
            sandbox = await client.sandbox.create(image="python:3.12", name=f"multi-stream-{i}")
            sandboxes.append(sandbox)
            print(f"Created: {sandbox.id}")

        await asyncio.sleep(3)  # Wait for sandboxes to be ready

        try:
            print("\nStarting concurrent monitoring...")
            print("-" * 40)

            # Create tasks to monitor each sandbox
            async def monitor_sandbox(sandbox, duration=10):
                """Monitor a single sandbox"""
                start_time = asyncio.get_event_loop().time()
                updates = 0

                async for stats in client.sandbox.stats_stream(sandbox.id):
                    elapsed = asyncio.get_event_loop().time() - start_time
                    print(
                        f"[{sandbox.id[:8]}] CPU: {stats.cpu_percent:.1f}% | "
                        f"Memory: {stats.memory_usage_mb:.0f}MB | "
                        f"Elapsed: {elapsed:.1f}s"
                    )

                    updates += 1
                    if elapsed >= duration:
                        break

                    await asyncio.sleep(1)

                return updates

            # Monitor all sandboxes concurrently
            results = await asyncio.gather(
                *[monitor_sandbox(sandbox, duration=5) for sandbox in sandboxes]
            )

            print(f"\n✅ Monitoring complete. Total updates: {sum(results)}")

        finally:
            # Cleanup all sandboxes
            print("\nCleaning up...")
            for sandbox in sandboxes:
                await client.sandbox.delete(sandbox.id)
            print("✅ All sandboxes cleaned up")


def example_stream_with_callback():
    """Use streaming with custom callback functions"""
    client = DSBClient(api_url="http://localhost:8080")

    def on_progress(data):
        """Called for each progress event"""
        print(f"  ⏳ Progress: {data}")

    def on_complete(sandbox):
        """Called when creation is complete"""
        print(f"  ✅ Complete! Sandbox ID: {sandbox.id}")
        print(f"     State: {sandbox.state}")
        return sandbox

    def on_error(error):
        """Called on error"""
        print(f"  ❌ Error: {error}")

    print("Creating sandbox with callbacks...")
    print("-" * 40)

    sandbox = None
    try:
        for event in client.sandbox.create_stream(image="python:3.12", name="callback-example"):
            event_data = event.get("data", {})

            if event.get("event") == "progress":
                on_progress(event_data)
            elif event.get("event") == "complete":
                from dsb_sdk.types import Sandbox

                sandbox = on_complete(Sandbox.model_validate(event_data))
                break
            elif event.get("event") == "error":
                on_error(event_data)
                break

        if sandbox:
            print("\n✅ Sandbox ready!")

    finally:
        if sandbox:
            client.sandbox.delete(sandbox.id)
            print("✅ Sandbox cleaned up")


def example_stream_to_file():
    """Stream progress to a file for logging"""
    import tempfile
    from datetime import datetime

    client = DSBClient(api_url="http://localhost:8080")

    # Create temporary log file
    with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".log") as log_file:
        log_path = log_file.name
        log_file.write(f"# Sandbox Creation Log - {datetime.now()}\n")
        log_file.write("=" * 50 + "\n")

    print(f"Streaming progress to: {log_path}")
    print("-" * 40)

    sandbox = None
    try:
        for event in client.sandbox.create_stream(image="python:3.12", name="logged-example"):
            timestamp = datetime.now().isoformat()
            event_data = event.get("data", {})

            # Append to log file
            with open(log_path, "a") as f:
                f.write(f"[{timestamp}] {event.get('event', 'unknown')}: {event_data}\n")

            # Also print to console
            print(f"[{timestamp}] {event.get('event', 'unknown')}: {event_data}")

            if event.get("event") == "complete":
                from dsb_sdk.types import Sandbox

                sandbox = Sandbox.model_validate(event_data)
                break

        print(f"\n✅ Log saved to: {log_path}")

        # Show log contents
        print("\nLog contents:")
        print("-" * 40)
        with open(log_path) as f:
            print(f.read())

    finally:
        if sandbox:
            client.sandbox.delete(sandbox.id)

        import os

        os.unlink(log_path)
        print("\n✅ Log file cleaned up")


async def example_stream_with_timeout():
    """Streaming with timeout protection"""
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        print("Creating sandbox with 60s timeout...")
        print("-" * 40)

        try:
            # Add timeout to the stream
            async for event in asyncio.wait_for(
                client.sandbox.create_stream(
                    image="python:3.12", name="timeout-example"
                ).__aiter__(),
                timeout=60.0,
            ):
                print(f"Event: {event.get('event', 'unknown')}")

                if event.get("event") == "complete":
                    from dsb_sdk.types import Sandbox

                    sandbox = Sandbox.model_validate(event.get("data", {}))
                    print(f"✅ Created: {sandbox.id}")

                    # Cleanup
                    await client.sandbox.delete_async(sandbox.id)
                    break

        except TimeoutError:
            print("❌ Timeout: Sandbox creation took too long")
        except Exception as e:
            print(f"❌ Error: {e}")


def example_stream_parsing():
    """Parse and handle different stream event types"""
    client = DSBClient(api_url="http://localhost:8080")

    print("Creating sandbox with detailed event parsing...")
    print("-" * 40)

    event_counts = {"progress": 0, "complete": 0, "error": 0}

    try:
        for event in client.sandbox.create_stream(image="python:3.12", name="parsing-example"):
            event_type = event.get("event", "unknown")

            # Count event types
            if event_type in event_counts:
                event_counts[event_type] += 1

            # Get event data
            event_data = event.get("data", {})

            # Parse JSON data if available
            try:
                data = json.loads(event_data) if isinstance(event_data, str) else event_data
            except (json.JSONDecodeError, TypeError):
                data = event_data

            # Handle different event types
            if event_type == "progress":
                if isinstance(data, dict):
                    if "stage" in data:
                        print(f"  Stage: {data['stage']}")
                    if "percent" in data:
                        print(f"  Progress: {data['percent']}%")
                else:
                    print(f"  {data}")

            elif event_type == "complete":
                from dsb_sdk.types import Sandbox

                sandbox = Sandbox.model_validate(data)
                print(f"  ✅ Sandbox created: {sandbox.id}")
                print(f"     Config: {sandbox.config.image}")
                break

            elif event_type == "error":
                print(f"  ❌ Error: {data}")
                break

        print("\n✅ Stream complete")
        print(f"Event counts: {event_counts}")

    finally:
        if "sandbox" in locals():
            client.sandbox.delete(sandbox.id)
            print("✅ Sandbox cleaned up")


if __name__ == "__main__":
    print("=== SSE Streaming Examples ===\n")

    print("1. Sandbox Creation Stream (Sync)")
    print("=" * 50)
    example_sandbox_creation_stream()
    print("\n")

    print("2. Sandbox Creation Stream (Async)")
    print("=" * 50)
    asyncio.run(example_async_sandbox_creation_stream())
    print("\n")

    print("3. Statistics Stream")
    print("=" * 50)
    example_stats_stream()
    print("\n")

    print("4. Multi-Stream Monitoring (Async)")
    print("=" * 50)
    asyncio.run(example_multi_stream_monitoring())
    print("\n")

    print("5. Stream with Callbacks")
    print("=" * 50)
    example_stream_with_callback()
    print("\n")

    print("6. Stream to File")
    print("=" * 50)
    example_stream_to_file()
    print("\n")

    print("7. Stream with Timeout (Async)")
    print("=" * 50)
    asyncio.run(example_stream_with_timeout())
    print("\n")

    print("8. Stream Event Parsing")
    print("=" * 50)
    example_stream_parsing()
