# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# DSB Python Base Image
#
# Purpose: Pre-install Python SDK dependencies and tools
# Base: ${DOCKER_REGISTRY}/python:3.12.11-slim
# Size: ~500 MB
#
# This image pre-downloads and caches DSB SDK Python dependencies
# and common Python development tools.
#
# Configurable via ARG:
# - Docker Registry: docker.io
# - PyPI: https://pypi.org/simple
# - Debian: deb.debian.org
#
# Build:
#   docker build -t dsb/python-base:latest .
#   docker build --build-arg DOCKER_REGISTRY=custom.registry.com --build-arg PYPI_MIRROR=https://custom.pypi.org/simple -t dsb/python-base:latest .
#
# Usage in application Dockerfiles:
#   FROM dsb/python-base:latest

ARG DOCKER_REGISTRY=docker.io
FROM ${DOCKER_REGISTRY}/python:3.12.11-slim

ARG PYPI_MIRROR=https://pypi.org/simple
ENV PIP_INDEX_URL=${PYPI_MIRROR}

ARG DEBIAN_MIRROR=deb.debian.org

# Replace Debian sources if mirror is provided and not default
RUN if [ "${DEBIAN_MIRROR}" != "deb.debian.org" ]; then \
        sed -i "s/deb.debian.org/${DEBIAN_MIRROR}/g" /etc/apt/sources.list.d/debian.sources; \
    fi


# Install system dependencies
RUN apt-get update && apt-get install -y \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install uv for fast Python package management
RUN pip install --no-cache-dir --break-system-packages -i ${PYPI_MIRROR} uv

# Set Python version for uv
ENV UV_PYTHON=3.12

# Install pipx
RUN pip install --no-cache-dir --break-system-packages -i ${PYPI_MIRROR} pipx && \
    pipx ensurepath

# Create a temporary directory for dependency caching
WORKDIR /tmp/python-cache

# Copy dependency file
COPY sdks/python/pyproject.toml sdks/python/README.md sdks/python/LICENSE ./

# Pre-cache all dependencies using uv
# Note: uv requires --system flag when not in a virtual environment
RUN uv pip install --system -e .[dev] --index-url ${PYPI_MIRROR}

# Clean up temporary files
RUN rm -rf pyproject.toml

# Set up labels
LABEL com.dsb.base-image="python-base"
LABEL com.dsb.base-image.version="1.0.0"
LABEL description="DSB Python base image with pre-cached SDK dependencies"

# Default command (keep container alive)
CMD ["python"]
