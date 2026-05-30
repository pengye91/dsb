# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# DSB Node Base Image
#
# Purpose: Pre-install npm packages for dashboard and sandbox tools
# Base: ${DOCKER_REGISTRY}/node:20.14.0-alpine
# - npm: https://registry.npmjs.org
#
# Build:
#   docker build -t dsb/node-base:latest .
#   docker build --build-arg DOCKER_REGISTRY=custom.registry.com --build-arg NPM_MIRROR=https://custom.npm.org -t dsb/node-base:latest .
#
# Usage in application Dockerfiles:
#   FROM dsb/node-base:latest

ARG DOCKER_REGISTRY=docker.io
FROM ${DOCKER_REGISTRY}/node:20.14.0-alpine

ARG NPM_MIRROR=https://registry.npmjs.org

# Configure NPM mirror if provided
RUN if [ "${NPM_MIRROR}" != "https://registry.npmjs.org" ]; then \
        npm config set registry ${NPM_MIRROR}; \
    fi


# Install global packages
RUN npm install -g typescript ts-node typescript-language-server

# Create a temporary directory for dependency caching
WORKDIR /tmp/node-cache

# Copy dependency files
COPY dashboard/package.json dashboard/package-lock.json ./

# Pre-cache all npm dependencies
# Note: Use npm install instead of npm ci to support --legacy-peer-deps
RUN npm install --legacy-peer-deps || echo "Warning: Some npm packages may have failed"

# Clean up temporary files
RUN rm -rf package.json node_modules

# Set up labels
LABEL com.dsb.base-image="node-base"
LABEL com.dsb.base-image.version="1.0.0"
LABEL description="DSB Node base image with pre-cached npm dependencies"

# Default command (keep container alive)
CMD ["sh"]
