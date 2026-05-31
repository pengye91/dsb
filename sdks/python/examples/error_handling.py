"""
Error Handling Examples

This example demonstrates error handling patterns in the DSB SDK.
"""

import asyncio
import time

from dsb_sdk import AsyncDSBClient, DSBClient
from dsb_sdk.exceptions import (
    DSBAPIError,
    DSBConnectionError,
    DSBTimeoutError,
    DSBValidationError,
)


def example_validation_error():
    """Handle validation errors"""
    client = DSBClient(api_url="http://localhost:8080")

    try:
        # Missing required 'image' parameter
        sandbox = client.sandbox.create(name="test")
    except DSBValidationError as e:
        print(f"❌ Validation Error: {e}")
        print("   Error details: Check input parameters")

        # Fix: Provide required parameter
        sandbox = client.sandbox.create(image="python:3.12", name="test")
        print(f"✅ Created sandbox: {sandbox.id}")


def example_timeout_with_retry():
    """Handle timeout with retry logic"""
    client = DSBClient(api_url="http://localhost:8080")

    max_retries = 3
    base_timeout = 30.0

    for attempt in range(max_retries):
        try:
            timeout = base_timeout * (attempt + 1)  # Increase timeout each attempt
            print(f"Attempt {attempt + 1} with timeout {timeout}s")

            sandbox = client.sandbox.create(image="python:3.12", name="retry-test", timeout=timeout)
            print(f"✅ Created sandbox: {sandbox.id}")
            return sandbox

        except DSBTimeoutError as e:
            print(f"⏱️ Timeout on attempt {attempt + 1}: {e}")
            if attempt < max_retries - 1:
                print("   Retrying with longer timeout...")
                time.sleep(2)
            else:
                print("❌ All retries exhausted")
                raise


def example_connection_error():
    """Handle connection errors"""
    try:
        # Wrong URL
        client = DSBClient(api_url="http://localhost:9999")
        client.sandbox.create(image="python:3.12")
    except DSBConnectionError as e:
        print(f"❌ Connection Error: {e}")
        print("   Troubleshooting:")
        print("   1. Check if DSB server is running")
        print("   2. Verify the URL is correct")
        print("   3. Check firewall rules")


def example_api_error():
    """Handle API errors from server"""
    client = DSBClient(api_url="http://localhost:8080")

    try:
        # Try to delete non-existent sandbox
        import uuid

        fake_id = uuid.uuid4()
        client.sandbox.delete(fake_id)
    except DSBAPIError as e:
        print(f"❌ API Error: {e}")
        print(f"   Status code: {e.status_code if hasattr(e, 'status_code') else 'N/A'}")
        print(f"   Response data: {e.response_data if hasattr(e, 'response_data') else 'N/A'}")


def example_context_manager_cleanup():
    """Use context managers for automatic cleanup"""
    client = DSBClient(api_url="http://localhost:8080")

    sandbox = client.sandbox.create(image="python:3.12", name="cleanup-test")
    print(f"Created sandbox: {sandbox.id}")

    try:
        # Do work...
        time.sleep(2)
        result = client.sandbox.exec(sandbox.id, ["echo", "hello"])
        print(f"Output: {result.output.strip()}")
    finally:
        # Always cleanup
        print("Cleaning up...")
        client.sandbox.delete(sandbox.id)
        print("✅ Sandbox deleted")


async def example_async_error_handling():
    """Error handling in async context"""
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        try:
            sandbox = await client.sandbox.create_async(image="python:3.12")
            print(f"✅ Created sandbox: {sandbox.id}")

            # Simulate work
            await asyncio.sleep(2)

            # Cleanup
            await client.sandbox.delete_async(sandbox.id)
            print("✅ Sandbox deleted")

        except DSBConnectionError as e:
            print(f"❌ Connection failed: {e}")
        except DSBTimeoutError as e:
            print(f"❌ Operation timed out: {e}")
        except DSBAPIError as e:
            print(f"❌ Server error: {e}")


def example_graceful_degradation():
    """Handle errors with graceful degradation"""
    client = DSBClient(api_url="http://localhost:8080")

    # Try to use terminal, fall back to exec if unavailable
    sandbox = client.sandbox.create(image="python:3.12")
    time.sleep(3)

    try:
        # Try terminal first (interactive)
        terminal = client.terminal.connect(sandbox.id)
        terminal.send("echo 'Hello from terminal'\n")
        output = terminal.receive(timeout=5.0)
        print(f"✅ Terminal output: {output.strip()}")
        terminal.close()
    except Exception as e:
        print(f"⚠️ Terminal unavailable: {e}")
        print("   Falling back to exec...")

        # Fall back to exec (non-interactive)
        result = client.sandbox.exec(sandbox.id, ["echo", "Hello from exec"])
        print(f"✅ Exec output: {result.output.strip()}")

    finally:
        client.sandbox.delete(sandbox.id)


def example_custom_retry_logic():
    """Custom retry logic for specific operations"""
    client = DSBClient(api_url="http://localhost:8080")

    def execute_with_retry(sandbox_id, command, max_retries=3):
        """Execute command with retries on timeout"""
        for attempt in range(max_retries):
            try:
                return client.sandbox.exec(sandbox_id, command, timeout=30.0 * (attempt + 1))
            except DSBTimeoutError:
                if attempt == max_retries - 1:
                    raise
                print(f"Timeout, retrying ({attempt + 1}/{max_retries})...")
                time.sleep(1)

    # Usage
    sandbox = client.sandbox.create(image="python:3.12")
    time.sleep(3)

    try:
        result = execute_with_retry(
            sandbox.id, ["python", "-c", "import time; time.sleep(20); print('done')"]
        )
        print(f"✅ Output: {result.output.strip()}")
    except DSBTimeoutError:
        print("❌ Operation timed out after all retries")
    finally:
        client.sandbox.delete(sandbox.id)


def example_multiple_sandboxes_with_error_handling():
    """Create multiple sandboxes with individual error handling"""
    client = DSBClient(api_url="http://localhost:8080")

    images = ["python:3.12", "node:20", "ubuntu:22.04"]
    successful = []
    failed = []

    for image in images:
        try:
            sandbox = client.sandbox.create(image=image)
            successful.append(sandbox)
            print(f"✅ Created {image}: {sandbox.id}")
        except DSBAPIError as e:
            failed.append((image, str(e)))
            print(f"❌ Failed {image}: {e}")

    print(f"\n✅ Successful: {len(successful)}")
    print(f"❌ Failed: {len(failed)}")

    # Cleanup successful sandboxes
    for sandbox in successful:
        try:
            client.sandbox.delete(sandbox.id)
            print(f"✅ Deleted {sandbox.id}")
        except Exception as e:
            print(f"⚠️ Failed to delete {sandbox.id}: {e}")


if __name__ == "__main__":
    print("=== Error Handling Examples ===\n")

    print("1. Validation Error Handling")
    print("-" * 40)
    example_validation_error()
    print()

    print("2. Timeout with Retry")
    print("-" * 40)
    example_timeout_with_retry()
    print()

    print("3. Connection Error")
    print("-" * 40)
    example_connection_error()
    print()

    print("4. API Error")
    print("-" * 40)
    example_api_error()
    print()

    print("5. Context Manager Cleanup")
    print("-" * 40)
    example_context_manager_cleanup()
    print()

    print("6. Async Error Handling")
    print("-" * 40)
    asyncio.run(example_async_error_handling())
    print()

    print("7. Graceful Degradation")
    print("-" * 40)
    example_graceful_degradation()
    print()

    print("8. Custom Retry Logic")
    print("-" * 40)
    example_custom_retry_logic()
    print()

    print("9. Multiple Sandboxes with Error Handling")
    print("-" * 40)
    example_multiple_sandboxes_with_error_handling()
