# DSB Python SDK

Python SDK for interacting with Distributed Sandboxes (DSB), including sandbox management, SSH sessions, terminals, and web automation.

## Installation

```bash
pip install dsb-sdk
```

## Initialization

The SDK provides both synchronous and asynchronous clients. Always use context managers (`with` or `async with`) to ensure proper resource cleanup.

```python
from dsb_sdk import DSBClient, AsyncDSBClient

# Synchronous client
with DSBClient(api_url="http://localhost:8080/api") as client:
    pass # Use the client

# Asynchronous client
async with AsyncDSBClient(api_url="http://localhost:8080/api") as async_client:
    pass # Use the async client
```

## Quickstart: Sandbox Management

Create, check, and delete a sandbox easily.

```python
from dsb_sdk import DSBClient

with DSBClient(api_url="http://localhost:8080/api") as client:
    # Create a sandbox
    sandbox = client.sandbox.create(image="python:3.12", name="my-sandbox")
    print(f"Created sandbox: {sandbox.id}")
    
    # Check status
    status = client.sandbox.get(sandbox.id)
    print(f"Sandbox status: {status.state}")
    
    # Delete the sandbox
    client.sandbox.delete(sandbox.id)
    print("Sandbox deleted")
```

## Executing Code

Run commands directly inside a running sandbox.

```python
from dsb_sdk import DSBClient
from dsb_sdk.types.exec import ExecRequest

with DSBClient(api_url="http://localhost:8080/api") as client:
    sandbox = client.sandbox.create(image="python:3.12")
    
    # Execute a command
    request = ExecRequest(cmd=["echo", "Hello from DSB!"])
    response = client.sandbox.exec(sandbox.id, request)
    print(response.stdout)
    
    client.sandbox.delete(sandbox.id)
```

## Other Features

### SSH Access
Retrieve SSH connection details for a sandbox.
```python
with DSBClient(api_url="http://localhost:8080/api") as client:
    ssh_session = client.ssh.create_session(sandbox.id)
    print(f"Connect via: ssh -p {ssh_session.port} {ssh_session.user}@{ssh_session.host}")
```

### Web Automation
Scrape a webpage.
```python
with DSBClient(api_url="http://localhost:8080/api") as client:
    result = client.web.scrape("https://example.com")
    print(result.markdown)
```

## Async Support

All synchronous methods have an asynchronous equivalent when using `AsyncDSBClient`. Simply append `_async` to the method name.

```python
async with AsyncDSBClient(api_url="http://localhost:8080/api") as client:
    sandbox = await client.sandbox.create_async(image="python:3.12")
    await client.sandbox.delete_async(sandbox.id)
```
