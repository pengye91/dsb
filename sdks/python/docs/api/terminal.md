# Terminal API

The Terminal API provides WebSocket-based interactive terminal sessions.

## Overview

Interactive terminals allow real-time command execution with PTY support, including features like terminal resizing and streaming output.

## Basic Usage

```python
from dsb_sdk import DSBClient

client = DSBClient()

# Connect to terminal
terminal = client.terminal.connect(sandbox_id)
print(f"Connected to terminal")

# Execute command
terminal.send("echo Hello\n")
output = terminal.receive()
print(output)
```

## Interactive Terminal

### Connect and Execute

```python
terminal = client.terminal.connect(
    sandbox_id="sandbox-uuid",
)

# Send commands
terminal.send("python --version\n")

# Receive output
while True:
    data = terminal.receive(timeout=5.0)
    if not data:
        break
    print(data, end="")
```

### Execute with Context Manager

```python
with client.terminal.connect(sandbox_id) as terminal:
    terminal.send("ls -la\n")
    output = terminal.receive()
    print(output)
```

### Execute Interactively

```python
terminal = client.terminal.connect(sandbox_id)

# Execute a command and get output
output = terminal.execute_interactive(
    command="echo Hello World",
    timeout=10.0,
)
print(output)
```

## Terminal Resize

```python
terminal = client.terminal.connect(
    sandbox_id="sandbox-uuid",
    cols=120,
    rows=40,
)

# Resize terminal
terminal.resize(cols=160, rows=50)
```

## Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `cols` | Number of columns | 80 |
| `rows` | Number of rows | 24 |
| `session_id` | Session identifier | Auto-generated |

## Async Usage

```python
import asyncio
from dsb_sdk import AsyncDSBClient

async def main():
    async with AsyncDSBClient() as client:
        terminal = await client.terminal.connect(sandbox_id)

        terminal.send("echo Hello\n")
        output = await terminal.receive()
        print(output)

        await terminal.close()

asyncio.run(main())
```

## WebSocket Connection

The terminal uses WebSocket for real-time communication:

```python
import websocket

def on_message(ws, message):
    print(message)

def on_error(ws, error):
    print(f"Error: {error}")

ws = websocket.WebSocketApp(
    "ws://localhost:8080/terminal/sandbox-uuid",
    on_message=on_message,
    on_error=on_error,
)
ws.run_forever()
```

## API Reference

### Synchronous Methods

| Method | Description |
|--------|-------------|
| `connect(sandbox_id, cols, rows, session_id)` | Connect to terminal |
| `execute_interactive(command, timeout)` | Execute command and get output |
| `resize(cols, rows)` | Resize terminal |

### Async Methods

| Method | Description |
|--------|-------------|
| `connect(sandbox_id, ...)` | Connect to terminal |
| `execute_interactive(command, timeout)` | Execute command |
| `resize(cols, rows)` | Resize terminal |

### Terminal Methods

| Method | Description |
|--------|-------------|
| `send(data)` | Send data to terminal |
| `receive(timeout)` | Receive data from terminal |
| `resize(cols, rows)` | Resize terminal |
| `close()` | Close terminal connection |

## Examples

### Simple Shell Session

```python
with client.terminal.connect(sandbox_id) as terminal:
    terminal.send("python\n")
    terminal.send("print('Hello from Python')\n")
    output = terminal.receive()
    print(output)
```

### Run Shell Script

```python
script = """
#!/bin/bash
echo "Running script..."
for i in {1..5}; do
  echo "Iteration $i"
done
echo "Done"
"""

terminal = client.terminal.connect(sandbox_id)
terminal.send(script + "\n")
output = terminal.receive(timeout=30.0)
print(output)
terminal.close()
```

### Interactive Python REPL

```python
terminal = client.terminal.connect(sandbox_id)

commands = [
    "import sys\n",
    "print(f'Python {sys.version}')\n",
    "exit()\n",
]

for cmd in commands:
    terminal.send(cmd)
    terminal.receive(timeout=5.0)

terminal.close()
```
