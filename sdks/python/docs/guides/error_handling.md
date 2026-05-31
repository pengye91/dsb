# Error Handling Guide

This guide covers error handling patterns for the DSB SDK.

## Exception Types

The SDK defines a hierarchy of exceptions:

```python
from dsb_sdk import DSBError
from dsb_sdk.exceptions import (
    DSBAPIError,
    DSBConnectionError,
    DSBTimeoutError,
    DSBValidationError,
)
```

### Exception Hierarchy

```
DSBError (base exception)
├── DSBAPIError (API returned error)
├── DSBConnectionError (network issues)
├── DSBTimeoutError (request timed out)
└── DSBValidationError (invalid parameters)
```

## Basic Error Handling

```python
from dsb_sdk import DSBClient
from dsb_sdk.exceptions import DSBAPIError, DSBConnectionError, DSBTimeoutError

client = DSBClient()

try:
    sandbox = client.sandbox.create(image="python:3.12")
except DSBValidationError as e:
    print(f"Invalid parameters: {e}")
except DSBAPIError as e:
    print(f"API error: {e.status_code} - {e}")
except DSBConnectionError as e:
    print(f"Connection error: {e}")
except DSBTimeoutError as e:
    print(f"Timeout: {e}")
```

## Retry Logic

The SDK provides built-in retry with exponential backoff:

```python
from dsb_sdk.utils.retry import retry_with_exponential_backoff, RetryStrategies

@RetryStrategies.long_running
def create_sandbox_with_retry(image):
    return client.sandbox.create(image=image)

# Or custom retry
@retry_with_exponential_backoff(max_attempts=5, min_wait=1.0, max_wait=30.0)
def create_sandbox(image):
    return client.sandbox.create(image=image)
```

## Custom Retry Logic

```python
from dsb_sdk.utils.retry import is_retryable_error, should_retry_exception

try:
    result = client.sandbox.create(image="python:3.12")
except DSBAPIError as e:
    if should_retry_exception(e):
        print("Retryable error, will retry...")
    else:
        print("Non-retryable error")
```

## Circuit Breaker

Use circuit breakers to prevent cascading failures:

```python
from dsb_sdk.utils.circuit import CircuitBreakers

@CircuitBreakers.sandbox
def create_sandbox(image):
    return client.sandbox.create(image=image)

# Check circuit breaker status
from dsb_sdk.utils.circuit import get_all_circuit_breaker_status

status = get_all_circuit_breaker_status()
print(status)
```

## Error Context

```python
try:
    sandbox = client.sandbox.create(
        image="invalid-image",
        name="test",
    )
except DSBAPIError as e:
    print(f"Error: {e}")
    print(f"Status: {e.status_code}")
    print(f"Response: {e.response_data}")
```

## Validation Errors

```python
from dsb_sdk.exceptions import DSBValidationError

try:
    # Empty image name
    sandbox = client.sandbox.create(image="")
except DSBValidationError as e:
    print(f"Validation error: {e}")

try:
    # Invalid timeout
    client = DSBClient(timeout=-1)
except DSBValidationError as e:
    print(f"Invalid timeout: {e}")
```

## Connection Errors

```python
from dsb_sdk.exceptions import DSBConnectionError

try:
    client = DSBClient(api_url="http://invalid:8080")
    client.health.check()
except DSBConnectionError as e:
    print(f"Connection failed: {e}")
```

## Timeout Errors

```python
from dsb_sdk.exceptions import DSBTimeoutError

try:
    # Long-running operation
    result = client.sandbox.exec(
        sandbox.id,
        ["sleep", "300"],
        timeout=10,
    )
except DSBTimeoutError as e:
    print(f"Operation timed out: {e}")
```

## Best Practices

### 1. Always Handle Exceptions

```python
try:
    result = client.sandbox.create(image="python:3.12")
except DSBError as e:
    # Log and handle
    logger.error(f"Sandbox creation failed: {e}")
```

### 2. Set Appropriate Timeouts

```python
# Quick operations - short timeout
client = DSBClient(timeout=10.0)

# Long operations - longer timeout
result = client.sandbox.exec(
    sandbox.id,
    ["python", "long_script.py"],
    timeout=300,
)
```

### 3. Use Retry for Transient Errors

```python
@RetryStrategies.long_running
def create_sandbox(image):
    return client.sandbox.create(image=image)
```

### 4. Use Circuit Breakers for External Services

```python
@CircuitBreakers.web
def scrape_website(url):
    return client.web.scrape(url)
```

### 5. Log Errors with Context

```python
import logging

logger = logging.getLogger(__name__)

try:
    sandbox = client.sandbox.create(image=image)
except DSBError as e:
    logger.error(
        f"Sandbox creation failed: {e}",
        extra={"image": image, "error": str(e)}
    )
```

### 6. Fail Gracefully

```python
def create_sandbox_safe(image):
    try:
        return client.sandbox.create(image=image)
    except DSBAPIError as e:
        if e.status_code == 404:
            return None  # Image not found
        raise
```
