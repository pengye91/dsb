# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# DSB Sandbox Base Image
#
# Purpose: Shared dependencies for sandbox images
# Base: ${DOCKER_REGISTRY}/python:3.12.11-slim
# Size: ~1 GB
#
# This image pre-installs all Python, Node, and OS dependencies used by
# both the full sandbox and slim sandbox images.
#
# Configurable via ARG:
# - Docker Registry: docker.io
# - Debian: deb.debian.org
# - PyPI: https://pypi.org/simple
# - npm: https://registry.npmjs.org
#
# Build:
#   docker build -t dsb/sandbox-base:latest .
#   docker build --build-arg DOCKER_REGISTRY=custom.registry.com --build-arg PYPI_MIRROR=https://custom.pypi.org/simple --build-arg NPM_MIRROR=https://custom.npm.org -t dsb/sandbox-base:latest .
#
# Usage in application Dockerfiles:
#   FROM dsb/sandbox-base:latest

ARG DOCKER_REGISTRY=docker.io
FROM ${DOCKER_REGISTRY}/python:3.12.11-slim

ARG PYPI_MIRROR=https://pypi.org/simple
ENV PIP_INDEX_URL=${PYPI_MIRROR}

ARG DEBIAN_MIRROR=deb.debian.org
ARG NPM_MIRROR=https://registry.npmjs.org

# Replace Debian sources if mirror is provided and not default
RUN if [ "${DEBIAN_MIRROR}" != "deb.debian.org" ]; then \
        sed -i "s/deb.debian.org/${DEBIAN_MIRROR}/g" /etc/apt/sources.list.d/debian.sources; \
    fi


# Update package list and install required dependencies
RUN apt-get update && apt-get install -y \
    # Basic tools
    curl \
    sudo \
    bash \
    python3-pip \
    python3-venv \
    ripgrep \
    # Build tools for crawl4ai
    build-essential \
    cmake \
    pkg-config \
    # Browser and fonts
    chromium \
    fonts-liberation \
    fonts-noto-color-emoji \
    fonts-wqy-zenhei \
    fonts-wqy-microhei \
    fonts-noto-cjk \
    # Process management
    supervisor \
    && rm -rf /var/lib/apt/lists/*

# ============================================================================
# CREATE DSB USER EARLY - All subsequent operations run as dsb
# ============================================================================

# SAFETY: dsb user gets passwordless sudo ONLY for specific package-manager commands.
# This is a sandbox image; the dsb user needs to install packages without interactivity.
RUN useradd -m dsb && echo "dsb ALL=(root) NOPASSWD:SETENV: /usr/bin/apt-get, /usr/bin/npm, /usr/local/bin/pip, /usr/local/bin/uv, /usr/bin/supervisord" > /etc/sudoers.d/91-dsb

# Create directories and set ownership BEFORE switching to dsb
# dsb needs write access to /usr/local/*, /usr/lib/node_modules, and /usr/bin for system package installations
RUN mkdir -p /opt/tools /usr/local/lib /usr/local/bin /usr/local/share /usr/local/include /usr/lib/node_modules && \
    chown -R dsb:dsb /opt/tools /usr/local /usr/lib/node_modules

# Switch to dsb user for ALL package installations
USER dsb
WORKDIR /home/dsb

# Set up environment for dsb user
ENV PATH="/home/dsb/.local/bin:$PATH"
ENV UV_HTTP_TIMEOUT=600
ENV LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8 \
    PYTHONUNBUFFERED=1 \
    HOME=/home/dsb

RUN python3 -m pip install --user --no-cache-dir -i ${PYPI_MIRROR} uv

RUN uv pip install --system --no-cache-dir -i ${PYPI_MIRROR} \
    numpy pandas scikit-learn scipy seaborn matplotlib \
    requests pillow python-lsp-server

# Create temporary directory and copy requirements
WORKDIR /tmp/python-cache

# Copy dependency file
COPY docker/base-images/sandbox-base/requirements.txt ./

RUN uv pip install --system --no-cache-dir -i ${PYPI_MIRROR} -r requirements.txt

# Clean up (as dsb user)
RUN rm -rf requirements.txt

# Install Node.js 24.x from NodeSource with GPG verification (avoid curl | bash supply-chain risk)
# Need root for this, then switch back to dsb
USER root
RUN apt-get update && \
    apt-get install -y gnupg && \
    curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key | gpg --dearmor -o /usr/share/keyrings/nodesource.gpg && \
    echo "deb [signed-by=/usr/share/keyrings/nodesource.gpg] https://deb.nodesource.com/node_24.x nodistro main" > /etc/apt/sources.list.d/nodesource.list && \
    apt-get update && \
    apt-get install -y nodejs && \
    node --version && npm --version && \
    rm -rf /var/lib/apt/lists/*

# Configure npm registry and install global packages (as root — global install needs /usr/bin write)
# Note: turndown and puppeteer removed - now using Playwright Python for browser automation
RUN npm config set registry ${NPM_MIRROR}
RUN npm install -g ts-node typescript typescript-language-server
USER dsb

# Create dsb-owned directories
# Need root to create /public, then chown to dsb
USER root
RUN mkdir -p /public && chown dsb:dsb /public
USER dsb

WORKDIR /public

# Set up labels
LABEL com.dsb.base-image="sandbox-base"
LABEL com.dsb.base-image.version="1.0.0"
LABEL description="DSB sandbox base image with Python, Node, and OS dependencies"

# Default command (keep container alive)
CMD ["bash"]
