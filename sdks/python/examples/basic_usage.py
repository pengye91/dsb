"""
Basic usage example for DSB SDK

This example demonstrates how to use the DSB SDK to manage sandboxes.
"""

from dsb_sdk import DSBClient


def main():
    # Initialize the client
    client = DSBClient(api_url="http://localhost:8080")

    try:
        # Check server health
        health = client.health.check()
        print(f"Server status: {health.status}")
        print(f"Server version: {health.version}")

        # List existing sandboxes
        sandboxes = client.sandbox.list()
        print(f"\nExisting sandboxes: {sandboxes.total}")

        # Create a new sandbox
        print("\nCreating sandbox...")
        sandbox = client.sandbox.create(
            image="python:3.12", name="example-sandbox", environment={"DEBUG": "true"}
        )
        print(f"Created sandbox: {sandbox.id}")
        print(f"State: {sandbox.state}")

        # Execute a command
        print("\nExecuting command...")
        result = client.sandbox.exec(sandbox.id, command=["echo", "hello from dsb sdk"])
        print(f"Output: {result.output}")
        print(f"Exit code: {result.exit_code}")

        # Get sandbox stats
        stats = client.sandbox.stats(sandbox.id)
        print("\nSandbox stats:")
        print(f"  CPU: {stats.cpu_percent}%")
        print(f"  Memory: {stats.memory_percent}%")

        # List sandboxes again
        sandboxes = client.sandbox.list()
        print(f"\nTotal sandboxes: {sandboxes.total}")

    finally:
        # Clean up: close the client
        client.close()


if __name__ == "__main__":
    main()
