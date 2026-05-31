"""
Example: Using wait helpers for sandbox lifecycle management.

This example demonstrates how to use wait_until_running_async() and wait_until_ready_async()
to properly handle sandbox creation and ensure the sandbox is ready before executing commands.
"""

import time

from dsb_sdk import AsyncDSBClient, DSBClient


def example_sync_wait_helpers():
    """Example: Using wait helpers with synchronous client."""
    print("=" * 60)
    print("Synchronous Wait Helpers Example")
    print("=" * 60)

    client = DSBClient()

    # Create a sandbox (this starts the creation process)
    print("\n1. Creating sandbox...")
    sandbox = client.sandbox.create(
        image="python:3.12",
        name="wait-test-sync",
    )
    print(f"   Sandbox created: {sandbox.id}")
    print(f"   Initial state: {sandbox.state.value}")

    # Wait for sandbox to be running
    print("\n2. Waiting for sandbox to be running...")
    start_time = time.monotonic()
    running_sandbox = client.sandbox.wait_until_running(
        sandbox.id,
        timeout=120.0,  # 2 minutes timeout
        poll_interval=2.0,  # Poll every 2 seconds
    )
    elapsed = time.monotonic() - start_time
    print(f"   Sandbox is running after {elapsed:.1f}s")
    print(f"   State: {running_sandbox.state.value}")

    # Wait for sandbox to be fully ready
    print("\n3. Waiting for sandbox to be fully ready...")
    ready_sandbox = client.sandbox.wait_until_ready(sandbox.id, timeout=30.0)
    print(f"   Sandbox is ready: {ready_sandbox.state.value}")

    # Execute a command
    print("\n4. Executing command...")
    result = client.sandbox.exec(sandbox.id, ["python", "--version"])
    print(f"   Output: {result.get('output', '').strip()}")

    # Cleanup
    print("\n5. Cleaning up...")
    client.sandbox.delete(sandbox.id)
    print("   Sandbox deleted")

    return True


def example_async_wait_helpers():
    """Example: Using wait helpers with asynchronous client."""
    import asyncio

    print("\n" + "=" * 60)
    print("Asynchronous Wait Helpers Example")
    print("=" * 60)

    async def run_async_example():
        async with AsyncDSBClient() as client:
            # Create a sandbox
            print("\n1. Creating sandbox...")
            sandbox = await client.sandbox.create_async(
                image="python:3.12",
                name="wait-test-async",
            )
            print(f"   Sandbox created: {sandbox.id}")
            print(f"   Initial state: {sandbox.state.value}")

            # Wait for sandbox to be running
            print("\n2. Waiting for sandbox to be running...")
            start_time = asyncio.get_event_loop().time()
            running_sandbox = await client.sandbox.wait_until_running_async(
                sandbox.id,
                timeout=120.0,
                poll_interval=2.0,
            )
            elapsed = asyncio.get_event_loop().time() - start_time
            print(f"   Sandbox is running after {elapsed:.1f}s")
            print(f"   State: {running_sandbox.state.value}")

            # Wait for sandbox to be fully ready
            print("\n3. Waiting for sandbox to be fully ready...")
            ready_sandbox = await client.sandbox.wait_until_ready_async(sandbox.id, timeout=30.0)
            print(f"   Sandbox is ready: {ready_sandbox.state.value}")

            # Execute a command
            print("\n4. Executing command...")
            result = await client.sandbox.exec_async(
                sandbox.id, ["python", "-c", "print('Hello from async!')"]
            )
            print(f"   Output: {result.get('output', '').strip()}")

            # Cleanup
            print("\n5. Cleaning up...")
            await client.sandbox.delete_async(sandbox.id)
            print("   Sandbox deleted")

    asyncio.run(run_async_example())
    return True


def example_with_timeout_handling():
    """Example: Handling timeouts gracefully."""
    print("\n" + "=" * 60)
    print("Timeout Handling Example")
    print("=" * 60)

    client = DSBClient()

    # Create a sandbox with a very short timeout
    print("\n1. Creating sandbox with short timeout...")
    sandbox = client.sandbox.create(
        image="python:3.12",
        name="timeout-test",
    )
    print(f"   Sandbox created: {sandbox.id}")

    # Try to wait with a very short timeout
    print("\n2. Attempting to wait with 1 second timeout...")
    try:
        # This will likely timeout since sandbox creation takes time
        # Using a mock sandbox_id to demonstrate timeout
        client.sandbox.wait_until_running(
            "00000000-0000-0000-0000-000000000000",  # Non-existent ID
            timeout=1.0,
            poll_interval=0.5,
        )
    except Exception as e:
        print(f"   Expected error (non-existent sandbox): {type(e).__name__}")
        print(f"   Message: {str(e)[:80]}...")

    # Cleanup
    try:
        client.sandbox.delete(sandbox.id)
    except Exception:
        pass

    return True


def example_cleanup_sandbox():
    """Example: Force cleanup a sandbox."""
    print("\n" + "=" * 60)
    print("Force Cleanup Example")
    print("=" * 60)

    client = DSBClient()

    # Create a sandbox
    print("\n1. Creating sandbox...")
    sandbox = client.sandbox.create(
        image="python:3.12",
        name="cleanup-test",
    )
    print(f"   Sandbox created: {sandbox.id}")

    # Force cleanup (for testing or stuck sandboxes)
    print("\n2. Force cleanup sandbox...")
    try:
        result = client.sandbox.cleanup(sandbox.id)
        print(f"   Cleanup result: {result}")
    except Exception as e:
        print(f"   Note: Cleanup endpoint may not be available: {e}")

    # Delete normally
    print("\n3. Deleting sandbox...")
    try:
        client.sandbox.delete(sandbox.id)
        print("   Sandbox deleted")
    except Exception:
        pass

    return True


def example_activity_list():
    """Example: Listing recent activities."""
    print("\n" + "=" * 60)
    print("Activity List Example")
    print("=" * 60)

    client = DSBClient()

    # List recent activities
    print("\n1. Listing recent activities...")
    try:
        activities = client.activities.list()
        print(f"   Total activities: {activities.total}")
        for activity in activities.activities[:5]:
            print(f"   - {activity.get('action', '?')} on sandbox {activity.get('sandbox_id', '?')[:12]}...")
    except Exception as e:
        print(f"   Note: Activity endpoint may not be available: {e}")

    return True


def main():
    """Run all examples."""
    print("\n" + "#" * 60)
    print("# DSB SDK Wait Helpers Examples")
    print("#" * 60)

    examples = [
        ("Synchronous Wait Helpers", example_sync_wait_helpers),
        ("Asynchronous Wait Helpers", example_async_wait_helpers),
        ("Timeout Handling", example_with_timeout_handling),
        ("Force Cleanup", example_cleanup_sandbox),
        ("Activity List", example_activity_list),
    ]

    for name, func in examples:
        try:
            func()
        except Exception as e:
            print(f"\n   Error in {name}: {e}")
            print("   (This is expected without a running DSB server)")

    print("\n" + "#" * 60)
    print("# Examples completed!")
    print("#" * 60)


if __name__ == "__main__":
    main()
