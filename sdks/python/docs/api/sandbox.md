# Sandbox API

The Sandbox API provides methods for creating, managing, and interacting with sandboxed environments.

## Overview

Sandboxes are Docker containers that provide isolated environments for running code and commands. The API supports both synchronous and asynchronous operations.

## Basic Usage

```python
from dsb_sdk import DSBClient

client = DSBClient()

# Create a sandbox
sandbox = client.sandbox.create(
    image="python:3.12",
    name="my-sandbox",
)
print(f"Created sandbox: {sandbox.id}")

# Execute a command
result = client.sandbox.exec(
    sandbox.id,
    ["python", "--version"],
)
print(f"Output: {result['output']}")

# Delete the sandbox
client.sandbox.delete(sandbox.id)
```

## Async Usage

```python
import asyncio
from dsb_sdk import AsyncDSBClient

async def main():
    async with AsyncDSBClient() as client:
        sandbox = await client.sandbox.create_async(
            image="python:3.12",
            name="async-sandbox",
        )

        result = await client.sandbox.exec_async(
            sandbox.id,
            ["echo", "Hello from async!"],
        )
        print(f"Output: {result['output']}")

        await client.sandbox.delete_async(sandbox.id)

asyncio.run(main())
```

## Creating Sandboxes

### Basic Creation

```python
sandbox = client.sandbox.create(
    image="python:3.12",  # Docker image
    name="my-sandbox",    # Optional name
)
```

### With Environment Variables

```python
sandbox = client.sandbox.create(
    image="python:3.12",
    environment={
        "API_KEY": "your-key",
        "DEBUG": "true",
    },
)
```

### With Resource Limits

```python
from dsb_sdk.types.sandbox import ResourceLimits, PullPolicy

limits = ResourceLimits(
    memory_mb=512.0,    # 512 MB memory
    cpu_quota=50000,    # 50% CPU
    cpu_shares=512,
    pids_limit=100,
)

sandbox = client.sandbox.create(
    image="python:3.12",
    resource_limits=limits,
    pull_policy=PullPolicy.MISSING,  # Only pull if not present
)
```

### With Volume Mounts

```python
sandbox = client.sandbox.create(
    image="python:3.12",
    volumes={
        "/host/path": "/container/path",  # Read-write
        "/host/config": "/etc/config:ro",  # Read-only
    },
)
```

### With Port Mapping

```python
sandbox = client.sandbox.create(
    image="python:3.12",
    ports={
        "8080": "80",      # HTTP
        "8443": "443",     # HTTPS
    },
)
```

## Waiting for Sandboxes

### Wait Until Running

```python
# Block until sandbox reaches RUNNING state
running = client.sandbox.wait_until_running(
    sandbox.id,
    timeout=120.0,    # Maximum wait time
    poll_interval=2.0,  # Polling interval
)
```

### Wait Until Ready

```python
# Block until sandbox is fully ready
ready = client.sandbox.wait_until_ready(
    sandbox.id,
    timeout=300.0,
)
```

## Executing Commands

### Simple Command

```python
result = client.sandbox.exec(
    sandbox.id,
    ["python", "-c", "print('Hello World')"],
)
print(f"Output: {result['output']}")
print(f"Exit code: {result['exit_code']}")
```

### With Working Directory

```python
result = client.sandbox.exec(
    sandbox.id,
    ["ls", "-la"],
    working_dir="/app",
)
```

### With Environment

```python
result = client.sandbox.exec(
    sandbox.id,
    ["python", "script.py"],
    environment={
        "ENV_VAR": "value",
    },
)
```

### With Timeout

```python
from dsb_sdk.types.sandbox import ExecRequest

request = ExecRequest(
    command=["python", "slow_script.py"],
    timeout=30,  # 30 seconds
)
result = client.sandbox.exec(sandbox.id, request)
```

## Uploading Files

### Simple File Upload

```python
# Upload a file to the sandbox
with open('config.json', 'rb') as f:
    result = client.sandbox.upload_file(
        sandbox.id,
        path='/app/config.json',
        file=f,
    )

print(f"Success: {result['success']}")
print(f"File: {result['file']['name']}")
print(f"Path: {result['file']['path']}")
print(f"Size: {result['file']['size']} bytes")
```

### Upload with Path Sanitization

```python
# Path traversal is automatically prevented
with open('data.txt', 'rb') as f:
    result = client.sandbox.upload_file(
        sandbox.id,
        path='/etc/../tmp/safe.txt',  # Will be sanitized to /etc/tmp/safe.txt
        file=f,
    )
```

### Upload from Bytes

```python
from io import BytesIO

# Upload file from memory
data = b'Hello from Python SDK!'
file_obj = BytesIO(data)

result = client.sandbox.upload_file(
    sandbox.id,
    path='/app/message.txt',
    file=file_obj,
)
```

## Downloading Files

### Simple File Download

```python
# Download a file from the sandbox
response = client.sandbox.download_file(
    sandbox.id,
    path='/app/config.json',
)

print(f"Filename: {response.name}")
print(f"Path: {response.path}")
print(f"Size: {response.size} bytes")
print(f"Type: {response.content_type}")
print(f"Content: {response.content.decode()}")
```

### Download to Local File

```python
# Download directly to a local file
result = client.sandbox.download_file_to_path(
    sandbox_id=sandbox.id,
    sandbox_path='/app/output.txt',
    local_path='./downloaded_output.txt',
)

print(f"Downloaded: {result['size']} bytes")
print(f"Saved to: {result['local_path']}")
```

### Download Binary Files

```python
# Download binary data
response = client.sandbox.download_file(
    sandbox.id,
    path='/app/data.bin',
)

# Access raw bytes
binary_data = response.content
print(f"Binary data length: {len(binary_data)}")

# Save to file
with open('data.bin', 'wb') as f:
    f.write(binary_data)
```

### Download with Inline Disposition

```python
# Download file for inline viewing (e.g., in browser)
response = client.sandbox.download_file(
    sandbox.id,
    path='/app/report.html',
    disposition='inline',  # Content-Disposition: inline
)
```

### Download Different File Types

```python
# JSON files
config = client.sandbox.download_file(sandbox.id, '/app/config.json')
import json
data = json.loads(config.content.decode())

# Text files
logs = client.sandbox.download_file(sandbox.id, '/var/log/app.log')
log_content = logs.content.decode()

# Images
image = client.sandbox.download_file(sandbox.id, '/app/screenshot.png')
with open('screenshot.png', 'wb') as f:
    f.write(image.content)
```

### Async File Download

```python
import asyncio

async def download_file_async():
    async with AsyncDSBClient() as client:
        sandbox = await client.sandbox.create_async(
            image="python:3.12"
        )

        # Download file asynchronously
        response = await client.sandbox.download_file_async(
            sandbox.id,
            '/app/data.txt'
        )

        print(f"Downloaded: {response.name}")

        # Download to path asynchronously
        await client.sandbox.download_file_to_path_async(
            sandbox.id,
            '/app/data.txt',
            './local_data.txt'
        )

asyncio.run(download_file_async())
```

### Error Handling

```python
from dsb_sdk.exceptions import DSBAPIError

try:
    response = client.sandbox.download_file(
        sandbox.id,
        '/app/nonexistent.txt'
    )
except DSBAPIError as e:
    if e.status_code == 404:
        print("File not found in sandbox")
    elif e.status_code == 409:
        print("Sandbox is not running")
    else:
        print(f"Download failed: {e}")
```

## Streaming Progress

```python
# Stream sandbox creation progress
for event in client.sandbox.create_stream(
    image="python:3.12",
):
    print(f"Stage: {event.get('stage')}")
    print(f"Message: {event.get('message')}")
    print(f"Progress: {event.get('progress')}%")
```

## Statistics

### Get Stats

```python
stats = client.sandbox.stats(sandbox.id)
print(f"CPU: {stats.cpu_percent}%")
print(f"Memory: {stats.memory_mb} MB")
```

### Stream Stats

```python
for stat in client.sandbox.stats_stream(sandbox.id):
    print(f"CPU: {stat.get('cpu_percent')}%")
    print(f"Memory: {stat.get('memory_mb')} MB")
```

## Cleanup

### Force Cleanup

```python
result = client.sandbox.cleanup(sandbox.id)
print(f"Cleanup status: {result}")
```

## API Reference

### Methods

| Method | Description |
|--------|-------------|
| `create(image, ...)` | Create a new sandbox |
| `create_stream(image, ...)` | Create with streaming progress |
| `get(sandbox_id)` | Get sandbox details |
| `list()` | List all sandboxes |
| `stop(sandbox_id)` | Stop a running sandbox |
| `delete(sandbox_id)` | Delete a sandbox |
| `exec(sandbox_id, command, ...)` | Execute a command |
| `upload_file(sandbox_id, path, file, ...)` | Upload file to sandbox |
| `stats(sandbox_id)` | Get resource statistics |
| `stats_stream(sandbox_id)` | Stream statistics |
| `wait_until_running(sandbox_id, ...)` | Wait for RUNNING state |
| `wait_until_ready(sandbox_id, ...)` | Wait for ready state |
| `cleanup(sandbox_id)` | Force cleanup resources |

### Async Methods

All methods have async equivalents with `_async` suffix:

- `create_async()`
- `get_async()`
- `list_async()`
- `stop_async()`
- `delete_async()`
- `exec_async()`
- `stats_async()`
- `create_stream_async()`
- `stats_stream_async()`
- `wait_until_running()`
- `wait_until_ready()`
- `cleanup_async()`
