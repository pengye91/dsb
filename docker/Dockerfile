# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# Built-in BuildKit Dockerfile frontend only (no `# syntax=docker/dockerfile:1`): avoids pulling
# docker.io/docker/dockerfile:1, which breaks non-interactive builds when the OS keychain is locked.
# DSB Main Application Dockerfile (China Mirrors)
#
# Uses cargo-chef for Docker layer caching:
#   - planner stage generates recipe.json from Cargo manifests
#   - builder stage cooks dependencies (cached layer) before source copy
#   - source code changes only invalidate the final build layer, not deps
#
# China mirrors (configurable via ARG, passed to base images):
# - Cargo: sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/
# - Debian: mirrors.tuna.tsinghua.edu.cn

ARG DOCKER_REGISTRY=docker.io
ARG BASE_IMAGE_TAG=latest

# ==========================================
# Stage 1: Planner — generate dependency recipe
# ==========================================
FROM dsb/rust-base:${BASE_IMAGE_TAG} AS planner
WORKDIR /build

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ==========================================
# Stage 2: Builder (Rust)
# ==========================================
FROM dsb/rust-base:${BASE_IMAGE_TAG} AS builder

ARG CARGO_REGISTRY=https://index.crates.io

ARG DOCKER_REGISTRY
ARG CARGO_REGISTRY
ARG DEBIAN_MIRROR
ARG PYPI_MIRROR
ARG NPM_MIRROR
ARG FEATURES=

WORKDIR /build

# Copy recipe and cook dependencies — this layer is cached unless
# Cargo.toml / Cargo.lock / workspace members change.
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json ${FEATURES:+--features ${FEATURES}}

# Copy actual source code — changes here only invalidate the next layer
COPY . .

# Build the binary with BuildKit cache mount for incremental compilation.
# FEATURES: space-separated list of Cargo features to enable (e.g., "kubernetes" for K8s backend)
ARG CACHE_BUST=1
RUN --mount=type=cache,target=/build/target \
    echo "Cache bust: $CACHE_BUST" && \
    cargo build --release --bin dsb ${FEATURES:+--features ${FEATURES}} && \
    cp /build/target/release/dsb /usr/local/bin/dsb

# Stage 3: Runtime
FROM dsb/runtime-base:${BASE_IMAGE_TAG}

# Copy binary from builder
COPY --from=builder /usr/local/bin/dsb /usr/local/bin/dsb

# Copy dashboard SPA
RUN mkdir -p /home/dsb/dashboard && chown dsb:dsb /home/dsb/dashboard
COPY --chown=dsb:dsb dashboard/dist /home/dsb/dashboard/dist

# Create directories for static files
RUN mkdir -p /var/lib/dsb/static-files && \
    chown -R dsb:dsb /var/lib/dsb

USER dsb
WORKDIR /home/dsb

EXPOSE 8080

# Health check
HEALTHCHECK --interval=5s --timeout=1s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

CMD ["dsb", "server", "--config-file", "/home/dsb/dsb.yaml"]
