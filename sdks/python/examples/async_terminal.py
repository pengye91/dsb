"""
Async WebSocket Terminal Example

This example demonstrates async usage of interactive terminal sessions.
"""

import asyncio

from dsb_sdk import AsyncDSBClient


async def example_async_terminal():
    """Async terminal usage"""
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        # Create sandbox
        sandbox = await client.sandbox.create(image="python:3.12", name="async-terminal-example")
        print(f"Created sandbox: {sandbox.id}")

        # Wait for sandbox to be ready
        await asyncio.sleep(3)

        # Connect to terminal
        terminal = await client.terminal.connect(sandbox.id)

        try:
            # Send command
            await terminal.send("pwd\n")
            output = await terminal.receive(timeout=2.0)
            print(f"Current directory: {output}")

            # Another command
            await terminal.send("ls -la /tmp\n")
            output = await terminal.receive(timeout=2.0)
            print(f"Temp directory:\n{output}")
        finally:
            # Note: terminal may not have async close, use sync close
            terminal.close()


async def example_async_interactive():
    """Interactive session with multiple commands"""
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        sandbox = await client.sandbox.create(image="python:3.12", name="async-interactive")
        print(f"Created sandbox: {sandbox.id}")

        await asyncio.sleep(3)

        terminal = await client.terminal.connect(sandbox.id)

        try:
            # Execute Python code interactively
            commands = [
                "python3\n",
                "import sys\n",
                "print(f'Python version: {sys.version}')\n",
                "exit()\n",
            ]

            for cmd in commands:
                await terminal.send(cmd)
                if cmd != "python3\n" and cmd != "import sys\n":
                    # Get output after each command
                    output = await terminal.receive(timeout=2.0)
                    if output:
                        print(output)
        finally:
            # Note: terminal may not have async close, use sync close
            terminal.close()


async def example_async_quick_command():
    """Quick single command execution"""
    async with AsyncDSBClient(api_url="http://localhost:8080") as client:
        sandbox = await client.sandbox.create(image="python:3.12", name="async-quick")
        print(f"Created sandbox: {sandbox.id}")

        await asyncio.sleep(3)

        # Execute command and get output in one call
        output = await client.terminal.execute_interactive_async(
            sandbox.id, "uname -a\n", timeout=3.0
        )
        print(f"System info: {output}")


if __name__ == "__main__":
    print("=== Async Terminal Example ===")
    asyncio.run(example_async_terminal())

    print("\n=== Async Interactive Example ===")
    asyncio.run(example_async_interactive())

    print("\n=== Async Quick Command Example ===")
    asyncio.run(example_async_quick_command())
