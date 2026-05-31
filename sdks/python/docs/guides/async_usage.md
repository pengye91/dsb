# Async Usage Guide

This guide covers asynchronous patterns for the DSB SDK.

## Overview

The SDK provides both synchronous and asynchronous APIs with full feature parity. Use the async API for:

- High-concurrency applications
- I/O-bound operations
- Long-running operations
- Web servers and services

## Async Client

```python
import asyncio
from dsb_sdk import AsyncDSBClient

async def main():
    async with AsyncDSBClient() as client:
        # Create sandbox
        sandbox = await client.sandbox.create_async(
            image="python:3.12",
            name="async-sandbox",
        )

        # Execute command
        result = await client.sandbox.exec_async(
            sandbox.id,
            ["echo", "Hello async!"],
        )
        print(f"Output: {result['output']}")

        # Delete sandbox
        await client.sandbox.delete_async(sandbox.id)

asyncio.run(main())
```

## Context Manager

Use async context managers for automatic cleanup:

```python
async with AsyncDSBClient() as client:
    sandbox = await client.sandbox.create_async(
        image="python:3.12",
    )
    # Operations...
# Connection automatically closed
```

## Concurrent Operations

Run multiple operations concurrently:

```python
async def create_and_exec(client, image):
    sandbox = await client.sandbox.create_async(image=image)
    result = await client.sandbox.exec_async(
        sandbox.id,
        ["echo", f"Created {image}"],
    )
    await client.sandbox.delete_async(sandbox.id)
    return result

async def main():
    async with AsyncDSBClient() as client:
        images = ["python:3.12", "node:20", "golang:1.21"]

        # Run concurrently
        tasks = [
            create_and_exec(client, image)
            for image in images
        ]
        results = await asyncio.gather(*tasks)

        for result in results:
            print(result)

asyncio.run(main())
```

## Streaming

```python
async def create_with_progress(client):
    async for event in client.sandbox.create_stream_async(
        image="python:3.12",
    ):
        print(f"Stage: {event.get('stage')}")
        print(f"Progress: {event.get('progress')}%")

async def main():
    async with AsyncDSBClient() as client:
        await create_with_progress(client)

asyncio.run(main())
```

## Awaiting Sandboxes

```python
async def create_sandbox_wait_ready(client):
    sandbox = await client.sandbox.create_async(
        image="python:3.12",
        name="wait-test",
    )

    # Wait for sandbox to be ready
    ready = await client.sandbox.wait_until_ready_async(
        sandbox.id,
        timeout=120.0,
    )
    print(f"Sandbox ready: {ready.state}")

    return ready

asyncio.run(create_sandbox_wait_ready(AsyncDSBClient()))
```

## Error Handling

```python
import asyncio
from dsb_sdk import AsyncDSBClient
from dsb_sdk.exceptions import DSBAPIError, DSBTimeoutError

async def main():
    async with AsyncDSBClient() as client:
        try:
            result = await client.sandbox.exec_async(
                "invalid-id",
                ["echo", "test"],
            )
        except DSBAPIError as e:
            print(f"API Error: {e}")
        except DSBTimeoutError as e:
            print(f"Timeout: {e}")

asyncio.run(main())
```

## Parallel Sandbox Operations

```python
async def main():
    async with AsyncDSBClient() as client:
        # Create multiple sandboxes in parallel
        create_tasks = []
        for i in range(5):
            task = client.sandbox.create_async(
                image="python:3.12",
                name=f"sandbox-{i}",
            )
            create_tasks.append(task)

        sandboxes = await asyncio.gather(*create_tasks)

        # Execute commands in parallel
        exec_tasks = []
        for sandbox in sandboxes:
            task = client.sandbox.exec_async(
                sandbox.id,
                ["hostname"],
            )
            exec_tasks.append(task)

        results = await asyncio.gather(*exec_tasks)

        # Cleanup in parallel
        delete_tasks = [
            client.sandbox.delete_async(s.id)
            for s in sandboxes
        ]
        await asyncio.gather(*delete_tasks)

asyncio.run(main())
```

## Running in Web Servers

Use with FastAPI:

```python
from fastapi import FastAPI
from dsb_sdk import AsyncDSBClient

app = FastAPI()

@app.get("/sandbox/{image}")
async def create_sandbox(image: str):
    async with AsyncDSBClient() as client:
        sandbox = await client.sandbox.create_async(image=image)
        return {"sandbox_id": sandbox.id}
```

## Performance Tips

1. **Reuse the client** - Create one client and reuse it
2. **Use context managers** - Let the SDK handle cleanup
3. **Batch operations** - Use `asyncio.gather()` for parallel operations
4. **Set timeouts** - Always set reasonable timeouts
5. **Handle errors** - Use try/except for error recovery
