# DSB Python SDK Documentation

## Overview

The DSB Python SDK provides a complete interface for interacting with the DSB server, including sandbox management, SSH sessions, web scraping, and terminal access.

## Installation

```bash
pip install dsb-sdk
```

## Quick Start

```python
from dsb_sdk import DSBClient

client = DSBClient()

# Create a sandbox
sandbox = client.sandbox.create(image="python:3.12")

# Execute a command
result = client.sandbox.exec(
    sandbox.id,
    ["python", "--version"],
)
print(f"Output: {result['output']}")

# Clean up
client.sandbox.delete(sandbox.id)
```

## Features

- **Sandbox Management**: Create, manage, and delete Docker-based sandboxes
- **SSH Sessions**: Interactive SSH access to sandboxes
- **Terminal Sessions**: WebSocket-based terminal with PTY support
- **Web Scraping**: Extract content from web pages
- **Browser Automation**: Automated browser interactions
- **Type Safety**: Full type hints with Pydantic v2 validation
- **Async Support**: Complete async/await API parity
- **Resilience**: Built-in retry, circuit breaker, and error handling

## Documentation Structure

### API Reference

- [Sandbox API](api/sandbox.md) - Create and manage sandboxes
- [SSH API](api/ssh.md) - SSH session management
- [Web API](api/web.md) - Web scraping and browser automation
- [Terminal API](api/terminal.md) - Interactive terminal sessions

### Guides

- [Configuration](guides/configuration.md) - SDK configuration options
- [Async Usage](guides/async_usage.md) - Asynchronous programming patterns
- [Error Handling](guides/error_handling.md) - Exception handling and retry logic
- [Best Practices](guides/best_practices.md) - Recommendations and patterns

## Examples

See the [examples directory](../../examples/) for comprehensive examples:

- `basic_usage.py` - Basic sandbox operations
- `async_usage.py` - Asynchronous patterns
- `authenticated_usage.py` - Authentication setup
- `error_handling.py` - Error handling patterns
- `streaming.py` - SSE streaming examples
- `config_from_env.py` - Configuration loading

## Configuration

Configure the SDK using environment variables:

```bash
export DSB_API_URL="http://localhost:8080"
export DSB_TIMEOUT="60.0"
export DSB_API_KEY="your-api-key"
```

Or using a configuration file:

```yaml
# dsb.yaml
dsb:
  api_url: "http://localhost:8080"
  timeout: 60.0
  api_key: "your-api-key"
```

## Async Usage

```python
import asyncio
from dsb_sdk import AsyncDSBClient

async def main():
    async with AsyncDSBClient() as client:
        sandbox = await client.sandbox.create_async(
            image="python:3.12",
        )
        result = await client.sandbox.exec_async(
            sandbox.id,
            ["echo", "Hello async!"],
        )
        print(result['output'])
        await client.sandbox.delete_async(sandbox.id)

asyncio.run(main())
```

## Error Handling

```python
from dsb_sdk.exceptions import (
    DSBAPIError,
    DSBConnectionError,
    DSBTimeoutError,
    DSBValidationError,
)

try:
    result = client.sandbox.create(image="python:3.12")
except DSBValidationError as e:
    print(f"Invalid parameters: {e}")
except DSBAPIError as e:
    print(f"API error: {e.status_code}")
except DSBConnectionError as e:
    print(f"Connection error: {e}")
except DSBTimeoutError as e:
    print(f"Timeout: {e}")
```

## Resources

- [GitHub Repository](https://github.com/dsb/dsb)
- [Issue Tracker](https://github.com/dsb/dsb/issues)
- [Changelog](../../CHANGELOG.md)
