# Configuration Guide

This guide covers how to configure the DSB SDK using environment variables, config files, and programmatic settings.

## Environment Variables

Set environment variables to configure the SDK:

```bash
export DSB_API_URL="http://localhost:8080"
export DSB_TIMEOUT="60.0"
export DSB_API_KEY="your-api-key"
export DSB_VERIFY_SSL="true"
```

Then load the configuration:

```python
from dsb_sdk import DSBConfig

config = DSBConfig.load()
print(f"API URL: {config.api_url}")
print(f"Timeout: {config.timeout}")
```

## Configuration Files

### YAML Config File

Create a `dsb.yaml` file:

```yaml
dsb:
  api_url: "http://localhost:8080"
  timeout: 60.0
  verify_ssl: true
  api_key: "your-api-key"
```

Load it:

```python
config = DSBConfig.load("/path/to/dsb.yaml")
```

### .env File

Create a `.env` file:

```bash
DSB_API_URL=http://localhost:8080
DSB_TIMEOUT=60.0
DSB_API_KEY=your-api-key
```

Load it:

```python
config = DSBConfig.load("/path/to/.env")
```

## Programmatic Configuration

Create a config programmatically:

```python
from dsb_sdk import DSBConfig

config = DSBConfig(
    api_url="http://localhost:8080",
    timeout=60.0,
    verify_ssl=True,
    api_key="your-api-key",
)

client = DSBClient(
    api_url=config.api_url,
    timeout=config.timeout,
    api_key=config.api_key,
)
```

## Configuration Priority

The SDK loads configuration in this priority order (highest to lowest):

1. Programmatic arguments
2. Environment variables
3. Config file values
4. Default values

## Test Configuration

For tests, use `load_for_tests()`:

```python
from dsb_sdk import DSBConfig

config = DSBConfig.load_for_tests()
```

You can also use a `.env.test` file or `dsb.test.yaml`.

## Environment Variable Reference

| Variable | Description | Default |
|----------|-------------|---------|
| `DSB_API_URL` | API server URL | `http://localhost:8080` |
| `DSB_TIMEOUT` | Request timeout (seconds) | `30.0` |
| `DSB_API_KEY` | API key for authentication | None |
| `DSB_VERIFY_SSL` | Verify SSL certificates | `true` |

## Test Environment Variables

| Variable | Description |
|----------|-------------|
| `TEST_DSB_API_URL` | Test API server URL |
| `TEST_DSB_TIMEOUT` | Test request timeout |
| `TEST_DSB_API_KEY` | Test API key |
| `TEST_DSB_VERIFY_SSL` | Test SSL verification |
