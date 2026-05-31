# DSB Python Base Image

Pre-installed Python SDK dependencies and tools for faster Docker builds.

## Overview

This base image pre-downloads and caches all Python dependencies used by the DSB SDK and test infrastructure.

## Contents

### Python Tools
- **uv** - Fast Python package installer (10-100x faster than pip)
- **pipx** - Install and run Python applications in isolated environments

### Pre-cached Dependencies

All SDK dependencies from `sdks/python/pyproject.toml`, including:

**Core Dependencies:**
- httpx[http2] - HTTP client with HTTP/2 support
- pydantic - Data validation using Python type annotations
- sse-starlette - Server-Sent Events support
- websocket-client - WebSocket client
- anyio - Async compatibility layer
- tenacity - Retry library
- pybreaker - Circuit breaker pattern
- structlog - Structured logging
- prometheus-client - Prometheus metrics
- pyyaml - YAML parser

**Dev Dependencies:**
- pytest - Testing framework
- pytest-asyncio - Async test support
- pytest-xdist - Parallel testing
- pytest-cov - Coverage plugin
- pytest-mock - Mocking utilities
- ruff - Fast Python linter
- mypy - Static type checker
- bandit - Security linter
- pip-audit - Vulnerability scanner
- safety - Security dependency checker

## Image Details

- **Base:** `python:3.12-slim`
- **Size:** ~500 MB
- **Tag:** `dsb/python-base:latest` (international), `dsb/python-base:china` (China mirrors)

## Building

### International Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/python-base:latest .
```

### China Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=true \
  --build-arg PYPI_MIRROR=https://pypi.tuna.tsinghua.edu.cn/simple \
  -t dsb/python-base:china .
```

## Usage

Use in application Dockerfiles:

```dockerfile
FROM dsb/python-base:latest

WORKDIR /workspace
COPY sdks/python /tmp/sdks/python

# Install SDK (dependencies already cached)
RUN cd /tmp/sdks/python && \
    pip install -e .[dev] && \
    rm -rf /tmp/sdks/python
```

## When to Rebuild

Rebuild this image when:
- `sdks/python/pyproject.toml` changes
- New Python dependencies added
- Security vulnerabilities in Python packages
- Monthly maintenance updates

## Benefits

1. **Faster builds:** 60-70% reduction in Python dependency installation time
2. **Shared cache:** Dependencies cached in Docker registry
3. **Consistent environment:** Same Python packages across all environments

## Technical Details

### Why uv?

uv is a modern Python package installer written in Rust that's:
- 10-100x faster than pip
- Compatible with PyPI and all mirrors
- Drop-in replacement for pip in most cases

### Dependency Caching Strategy

The image creates a dummy project with all dependencies and pre-caches them:
1. Creates temporary `pyproject.toml` with all dependencies
2. Runs `uv pip install` to download and cache packages
3. Cleans up temporary files
4. Leaves cached packages in `/root/.cache/pip`

## See Also

- [Main README](../README.md)
- [Python SDK README](../../../../sdks/python/README.md)
- [pyproject.toml](../../../../sdks/python/pyproject.toml)
