# Best Practices Guide

This guide covers best practices for using the DSB SDK effectively.

## Client Management

### Reuse Clients

Create one client and reuse it for multiple operations:

```python
# Good: Reuse client
client = DSBClient()
for i in range(10):
    sandbox = client.sandbox.create(image="python:3.12")
    # ...

# Bad: Create new client each time
for i in range(10):
    client = DSBClient()  # Wasteful
    sandbox = client.sandbox.create(image="python:3.12")
```

### Use Context Managers

```python
# Good: Use context manager
with DSBClient() as client:
    sandbox = client.sandbox.create(image="python:3.12")
    # Automatic cleanup

# Async
async with AsyncDSBClient() as client:
    sandbox = await client.sandbox.create_async(image="python:3.12")
```

## Sandbox Management

### Wait for Readiness

```python
# Good: Wait for sandbox to be ready
sandbox = client.sandbox.create(image="python:3.12")
ready = client.sandbox.wait_until_ready(sandbox.id)
result = client.sandbox.exec(ready.id, ["python", "script.py"])

# Bad: Assume immediate readiness
sandbox = client.sandbox.create(image="python:3.12")
result = client.sandbox.exec(sandbox.id, ["python", "script.py"])  # Might fail!
```

### Clean Up Resources

```python
# Good: Always clean up
try:
    sandbox = client.sandbox.create(image="python:3.12")
    result = client.sandbox.exec(sandbox.id, ["python", "script.py"])
finally:
    client.sandbox.delete(sandbox.id)

# Better: Use context manager
with client.terminal.connect(sandbox.id) as terminal:
    terminal.send("command\n")
    # Auto-cleanup
```

### Name Your Sandboxes

```python
# Good: Use descriptive names
sandbox = client.sandbox.create(
    image="python:3.12",
    name="data-processing-worker-001",
)

# Bad: Random names make debugging hard
sandbox = client.sandbox.create(image="python:3.12")
```

## Error Handling

### Use Specific Exception Types

```python
# Good: Catch specific exceptions
try:
    result = client.sandbox.create(image="python:3.12")
except DSBValidationError as e:
    print(f"Invalid input: {e}")
except DSBAPIError as e:
    print(f"Server error: {e}")

# Bad: Catch-all
try:
    result = client.sandbox.create(image="python:3.12")
except Exception as e:
    print(f"Error: {e}")
```

### Implement Retry Logic

```python
from dsb_sdk.utils.retry import RetryStrategies

@RetryStrategies.long_running
def create_sandbox_with_retry(image):
    return client.sandbox.create(image=image)
```

### Use Circuit Breakers

```python
from dsb_sdk.utils.circuit import CircuitBreakers

@CircuitBreakers.sandbox
def critical_operation(image):
    return client.sandbox.create(image=image)
```

## Performance

### Batch Operations

```python
# Good: Use asyncio.gather for parallel operations
import asyncio
from dsb_sdk import AsyncDSBClient

async def create_multiple():
    async with AsyncDSBClient() as client:
        tasks = [
            client.sandbox.create_async(image=f"python:{ver}")
            for ver in ["3.10", "3.11", "3.12"]
        ]
        return await asyncio.gather(*tasks)

asyncio.run(create_multiple())
```

### Set Appropriate Timeouts

```python
# Quick API calls - short timeout
client = DSBClient(timeout=10.0)

# Long-running operations - longer timeout
client.sandbox.exec(
    sandbox.id,
    ["python", "long_job.py"],
    timeout=600,  # 10 minutes
)
```

### Reuse Connections

```python
# Good: Single client for multiple operations
client = DSBClient()
for i in range(100):
    result = client.sandbox.exec(sandbox.id, ["python", "quick_task.py"])
```

## Security

### Use API Keys

```python
# Good: Use API key for authenticated requests
client = DSBClient(
    api_url="https://api.example.com",
    api_key="your-secure-api-key",
)

# Set via environment
import os
client = DSBClient(api_key=os.environ["DSB_API_KEY"])
```

### Avoid Hardcoding Credentials

```python
# Good: Load from environment
import os
api_key = os.environ.get("DSB_API_KEY")
if not api_key:
    raise ValueError("DSB_API_KEY not set")
client = DSBClient(api_key=api_key)

# Bad: Hardcoded credentials
client = DSBClient(api_key="secret-key-in-code")  # Never do this!
```

### Use SSL Verification

```python
# Good: Verify SSL in production
client = DSBClient(verify_ssl=True)

# Only disable for local testing
client = DSBClient(verify_ssl=False)  # Development only!
```

## Logging

### Configure Logging

```python
from dsb_sdk.logging import configure_logging

configure_logging(
    level=logging.INFO,
    json_format=True,
)
```

### Log Important Operations

```python
import logging

logger = logging.getLogger(__name__)

def create_sandbox(image):
    logger.info(f"Creating sandbox", extra={"image": image})
    try:
        sandbox = client.sandbox.create(image=image)
        logger.info(f"Sandbox created", extra={"sandbox_id": sandbox.id})
        return sandbox
    except Exception as e:
        logger.error(f"Failed to create sandbox", extra={"image": image, "error": str(e)})
        raise
```

## Testing

### Use Mock Transport

```python
from unittest.mock import Mock
from dsb_sdk.transport.sync import SyncTransport

def test_sandbox_creation():
    transport = Mock()
    transport.request.return_value = {
        "id": "test-id",
        "state": "running",
        "config": {"image": "python:3.12"},
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
    }

    client = DSBClient()
    client.sandbox.transport = transport

    sandbox = client.sandbox.create(image="python:3.12")
    assert sandbox.id == "test-id"
```

### Use Test Configuration

```python
from dsb_sdk import DSBConfig

def test_with_config():
    config = DSBConfig.load_for_tests()
    assert config.timeout > 0
```

## Resource Management

### Set Resource Limits

```python
from dsb_sdk.types.sandbox import ResourceLimits, PullPolicy

limits = ResourceLimits(
    memory_mb=512.0,
    cpu_quota=50000,
    pids_limit=100,
)

sandbox = client.sandbox.create(
    image="python:3.12",
    resource_limits=limits,
    pull_policy=PullPolicy.MISSING,
)
```

### Monitor Resource Usage

```python
# Check sandbox stats
stats = client.sandbox.stats(sandbox.id)
print(f"CPU: {stats.cpu_percent}%")
print(f"Memory: {stats.memory_mb} MB")
```

### Use Inactivity Timeout

```python
sandbox = client.sandbox.create(
    image="python:3.12",
    inactivity_timeout_minutes=30,  # Auto-stop after 30 min
)
```

## Code Organization

### Use API Classes Directly

```python
from dsb_sdk.api.sandbox import SandboxAPI

# Direct usage
sandbox_api = SandboxAPI(transport)
sandbox = sandbox_api.create(image="python:3.12")
```

### Import What You Need

```python
# Good: Specific imports
from dsb_sdk import DSBClient
from dsb_sdk.types.sandbox import PullPolicy, ResourceLimits

# Bad: Import everything
from dsb_sdk import *  # Avoid this
```
