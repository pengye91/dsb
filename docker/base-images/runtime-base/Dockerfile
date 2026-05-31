# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# DSB Runtime Base Image
#
# Purpose: Common runtime dependencies for DSB services
# Base: ${DOCKER_REGISTRY}/debian:bookworm (Debian 12, stable)
# Size: ~100 MB
#
# This image provides a minimal Debian runtime with common dependencies
# and the DSB user account for running DSB services.
#
#
# Configurable via ARG:
# - Docker Registry: docker.io
# - Debian: deb.debian.org
#
# Build:
#   docker build -t dsb/runtime-base:latest .
#   docker build --build-arg DOCKER_REGISTRY=custom.registry.com --build-arg DEBIAN_MIRROR=custom.debian.mirror -t dsb/runtime-base:latest .
#
# Usage in application Dockerfiles:
#   FROM dsb/runtime-base:latest

ARG DOCKER_REGISTRY=docker.io
FROM ${DOCKER_REGISTRY}/debian:bookworm

ARG DEBIAN_MIRROR=deb.debian.org

# Replace Debian sources if mirror is provided and not default
RUN if [ "${DEBIAN_MIRROR}" != "deb.debian.org" ]; then \
        sed -i "s/deb.debian.org/${DEBIAN_MIRROR}/g" /etc/apt/sources.list.d/debian.sources; \
    fi


# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    adduser \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

# Create non-root user (UID 1001 to avoid conflicts)
# This user is used by all DSB services for security
RUN adduser --uid 1001 --gecos "" --disabled-password dsb

# Set up labels
LABEL com.dsb.base-image="runtime-base"
LABEL com.dsb.base-image.version="1.0.0"
LABEL description="DSB runtime base image with common dependencies"

# Set up environment
ENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Default command (keep container alive)
CMD ["bash"]
