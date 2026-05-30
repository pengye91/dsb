# SSH API

The SSH API provides methods for creating and managing SSH sessions to sandboxes.

## Overview

SSH sessions allow interactive shell access to sandboxes through an SSH gateway. This is useful for debugging, development, and scenarios requiring persistent connections.

## Basic Usage

```python
from dsb_sdk import DSBClient

client = DSBClient()

# Create an SSH session
session = client.ssh.create(
    sandbox_id="sandbox-uuid",
)
print(f"Session ID: {session.id}")
print(f"SSH Command: ssh {session.host} -p {session.port} {session.username}")
```

## Creating Sessions

### Basic Session

```python
session = client.ssh.create(
    sandbox_id="sandbox-uuid",
)
```

### With Public Key

```python
with open("~/.ssh/id_rsa.pub") as f:
    public_key = f.read()

session = client.ssh.create(
    sandbox_id="sandbox-uuid",
    public_key=public_key,
)
```

### With Session Name

```python
session = client.ssh.create(
    sandbox_id="sandbox-uuid",
    name="my-debug-session",
)
```

## Managing Sessions

### List Sessions

```python
sessions = client.ssh.list()
for session in sessions:
    print(f"{session.id}: {session.state}")
```

### Get Session Details

```python
session = client.ssh.get(session_id)
print(f"State: {session.state}")
print(f"Connected at: {session.connected_at}")
```

### Heartbeat

```python
# Keep session alive
client.ssh.heartbeat(session_id)
```

### Terminate Session

```python
client.ssh.terminate(session_id)
```

## Session States

| State | Description |
|-------|-------------|
| `pending` | Session requested, not yet connected |
| `active` | Client connected, session active |
| `expired` | Session timed out |
| `terminated` | Session was terminated |

## Async Usage

```python
import asyncio
from dsb_sdk import AsyncDSBClient

async def main():
    async with AsyncDSBClient() as client:
        session = await client.ssh.create_async(
            sandbox_id="sandbox-uuid",
        )
        print(f"Session: {session.id}")

        # List sessions
        sessions = await client.ssh.list_async()

        # Terminate
        await client.ssh.terminate_async(session.id)

asyncio.run(main())
```

## API Reference

### Methods

| Method | Description |
|--------|-------------|
| `create(sandbox_id, ...)` | Create an SSH session |
| `get(session_id)` | Get session details |
| `list()` | List all sessions |
| `heartbeat(session_id)` | Send heartbeat to keep session alive |
| `terminate(session_id)` | Terminate a session |

### Async Methods

| Method | Description |
|--------|-------------|
| `create_async(sandbox_id, ...)` | Create an SSH session |
| `get_async(session_id)` | Get session details |
| `list_async()` | List all sessions |
| `heartbeat_async(session_id)` | Send heartbeat |
| `terminate_async(session_id)` | Terminate a session |
