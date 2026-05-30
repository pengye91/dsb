"""
Async usage example for DSB SDK

Demonstrates proper async/await patterns using AsyncDSBClient with the new
async methods (*_async) that properly await transport requests.
"""

import asyncio

from dsb_sdk import AsyncDSBClient


async def main():
    """Main async function demonstrating proper async SDK usage."""
    # Initialize the async client
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        # Check server health (using async method)
        health = await client.health.check_async()
        print(f"Server status: {health.status}")
        print(f"Server version: {health.version}")

        # List existing sandboxes (using async method)
        sandboxes = await client.sandbox.list_async()
        print(f"Existing sandboxes: {sandboxes.total}")

        # Create a sandbox (using async method)
        print("\nCreating sandbox...")
        sandbox = await client.sandbox.create_async(image="node:20", name="async-sandbox")
        print(f"Sandbox created: {sandbox.id}")
        print(f"Sandbox state: {sandbox.state.value}")

        # Stream progress updates (using async iterator)
        print("\nWaiting for sandbox to be ready...")
        async for event in client.sandbox.create_stream_async(
            image="node:20", name="async-sandbox"
        ):
            print(f"  Progress: {event.get('stage', 'unknown')} - {event.get('message', '')}")

        # Get sandbox details (using async method)
        sandbox = await client.sandbox.get_async(sandbox.id)
        print(f"Sandbox state: {sandbox.state.value}")

        # Execute a command (using async method)
        result = await client.sandbox.exec_async(sandbox.id, command=["node", "--version"])
        print(f"Node version: {result.output.strip()}")

        # Get sandbox stats (using async method)
        stats = await client.sandbox.stats_async(sandbox.id)
        print(f"CPU: {stats.cpu_percent}%")

        # Create SSH session (using async method)
        session = await client.ssh.create_async(sandbox_id=sandbox.id, username="dsb")
        print(f"\nSSH session created: {session.id}")

        # List SSH sessions (using async method)
        sessions = await client.ssh.list_async()
        print(f"Total SSH sessions: {sessions.total}")

        # Send heartbeat (using async method)
        await client.ssh.heartbeat_async(session.id)
        print("Heartbeat sent")

        # Terminate session (using async method)
        await client.ssh.terminate_async(session.id)
        print("SSH session terminated")

        # List activities (using async method)
        activities = await client.activities.list_async()
        print(f"Total activities: {activities.total}")

        # Stop sandbox (using async method)
        sandbox = await client.sandbox.stop_async(sandbox.id)
        print(f"\nSandbox stopped: {sandbox.state.value}")

        # Delete sandbox (using async method)
        await client.sandbox.delete_async(sandbox.id)
        print("Sandbox deleted")


if __name__ == "__main__":
    asyncio.run(main())
