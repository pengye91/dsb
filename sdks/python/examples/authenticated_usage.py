"""
Example: Using API Key Authentication with DSB SDK

This example demonstrates how to use API key authentication when connecting
to a DSB server that requires authentication.
"""

import os

from dsb_sdk import DSBClient


def example_with_api_key():
    """Example: Using API key for authentication"""

    # Option 1: Pass API key directly
    client = DSBClient(api_url="http://localhost:8080", api_key="your-secret-api-key-here")

    # Use the client normally - API key is sent automatically
    try:
        # Create a sandbox
        sandbox = client.sandbox.create(image="alpine:latest", name="authenticated-sandbox")
        print(f"Created sandbox: {sandbox.id}")

        # Execute a command
        result = client.sandbox.exec(sandbox.id, ["echo", "Hello from authenticated client!"])
        print(f"Output: {result.output}")

        # Clean up
        client.sandbox.delete(sandbox.id)
        print("Sandbox deleted")

    except Exception as e:
        print(f"Error: {e}")

    finally:
        client.close()


def example_with_env_var():
    """Example: Loading API key from environment variable"""

    # Option 2: Load API key from environment variable
    api_key = os.getenv("DSB_API_KEY")
    api_url = os.getenv("DSB_API_URL", "http://localhost:8080")

    if not api_key:
        print("Warning: DSB_API_KEY not set, using unauthenticated client")
        client = DSBClient(api_url=api_url)
    else:
        client = DSBClient(api_url=api_url, api_key=api_key)

    try:
        # Health check to verify authentication
        health = client.health.check()
        print(f"Server status: {health.status}")

    except Exception as e:
        print(f"Authentication failed: {e}")

    finally:
        client.close()


def example_async_with_api_key():
    """Example: Async client with API key authentication"""
    import asyncio

    from dsb_sdk import AsyncDSBClient

    async def main():
        # Async client also supports API key authentication
        async with AsyncDSBClient(
            api_url="http://localhost:8080", api_key="your-async-api-key"
        ) as client:
            # Create a sandbox
            sandbox = await client.sandbox.create_async(
                image="python:3.12", name="async-auth-sandbox"
            )
            print(f"Created sandbox: {sandbox.id}")

            # Execute command
            result = await client.sandbox.exec_async(
                sandbox.id, ["python", "-c", "print('Hello from async auth!')"]
            )
            print(f"Output: {result.output}")

            # Clean up
            await client.sandbox.delete_async(sandbox.id)
            print("Sandbox deleted")

    asyncio.run(main())


if __name__ == "__main__":
    print("=" * 60)
    print("Example 1: Using API Key Directly")
    print("=" * 60)
    example_with_api_key()

    print("\n" + "=" * 60)
    print("Example 2: Loading API Key from Environment")
    print("=" * 60)
    example_with_env_var()

    print("\n" + "=" * 60)
    print("Example 3: Async Client with API Key")
    print("=" * 60)
    example_async_with_api_key()
