"""
WebSocket Terminal Example

This example demonstrates how to use interactive terminal sessions
in DSB sandboxes using the WebSocket terminal API.
"""

import time

from dsb_sdk import DSBClient


def example_basic_terminal():
    """Basic terminal usage"""
    client = DSBClient(api_url="http://localhost:8080")

    # Create a sandbox
    sandbox = client.sandbox.create(image="python:3.12", name="terminal-example")
    print(f"Created sandbox: {sandbox.id}")

    # Wait for sandbox to be ready
    time.sleep(3)

    # Execute command via terminal
    output = client.terminal.execute_interactive(
        sandbox.id,
        "cat /etc/os-release\n",
        timeout=5.0,
    )
    print("OS Release Info:")
    print(output)


def example_interactive_terminal():
    """Interactive terminal session with multiple commands"""
    client = DSBClient(api_url="http://localhost:8080")

    sandbox = client.sandbox.create(image="python:3.12", name="interactive-term")
    print(f"Created sandbox: {sandbox.id}")

    # Wait for sandbox to be ready
    time.sleep(3)

    # Connect to terminal
    terminal = client.terminal.connect(sandbox.id)

    try:
        # Send multiple commands
        terminal.send("pwd\n")
        output1 = terminal.receive(timeout=2.0)
        print(f"Current directory: {output1}")

        terminal.send("ls -la\n")
        output2 = terminal.receive(timeout=2.0)
        print(f"Directory listing:\n{output2}")

        terminal.send("python --version\n")
        output3 = terminal.receive(timeout=2.0)
        print(f"Python version: {output3}")
    finally:
        terminal.close()


def example_terminal_resize():
    """Resize terminal to custom dimensions"""
    client = DSBClient(api_url="http://localhost:8080")

    sandbox = client.sandbox.create(image="python:3.12", name="resize-example")
    print(f"Created sandbox: {sandbox.id}")

    # Wait for sandbox to be ready
    time.sleep(3)

    # Connect and resize terminal
    terminal = client.terminal.connect(sandbox.id)

    try:
        # Note: Terminal resize may not be supported in all implementations
        # terminal.resize(rows=40, cols=120)
        # print("Terminal resized to 40x120")

        # Run a command that benefits from larger terminal
        terminal.send("ps aux\n")
        output = terminal.receive(timeout=3.0)
        print(f"Process list:\n{output}")
    finally:
        terminal.close()


def example_context_manager():
    """Using terminal as context manager"""
    client = DSBClient(api_url="http://localhost:8080")

    sandbox = client.sandbox.create(image="python:3.12", name="context-term")
    print(f"Created sandbox: {sandbox.id}")

    # Wait for sandbox to be ready
    time.sleep(3)

    # Terminal is automatically closed when exiting context
    with client.terminal.connect(sandbox.id) as terminal:
        terminal.send("echo 'Hello from terminal!'\n")
        output = terminal.receive(timeout=2.0)
        print(output)


if __name__ == "__main__":
    print("=== Basic Terminal Example ===")
    example_basic_terminal()

    print("\n=== Interactive Terminal Example ===")
    example_interactive_terminal()

    print("\n=== Terminal Resize Example ===")
    example_terminal_resize()

    print("\n=== Context Manager Example ===")
    example_context_manager()
