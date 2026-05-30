# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# DSB Rust Base Image
#
# Purpose: Pre-install Rust workspace dependencies and build tools
# Base: ${DOCKER_REGISTRY}/rust:1.88
# Size: ~800 MB
#
# This image pre-downloads and caches all Cargo workspace dependencies
# to significantly speed up Docker builds for Rust components.
#
# Configurable via ARG:
# - Docker Registry: docker.io
# - Cargo: https://index.crates.io
# - Debian: deb.debian.org
#
# Build:
#   docker build -t dsb/rust-base:latest .
#   docker build --build-arg DOCKER_REGISTRY=custom.registry.com -t dsb/rust-base:latest .
#
# Usage in application Dockerfiles:
#   FROM dsb/rust-base:latest

ARG DOCKER_REGISTRY=docker.io
FROM ${DOCKER_REGISTRY}/rust:1.88

ARG DEBIAN_MIRROR=deb.debian.org
ARG CARGO_REGISTRY=https://index.crates.io
# Rustup uses two URLs: compiler/toolchain tarballs under RUSTUP_DIST_SERVER/dist/..., and
# rustup self-update metadata under RUSTUP_UPDATE_ROOT. Do not set RUSTUP_DIST_SERVER to
# .../rustup (that breaks channel resolution: "no release found for 'stable'").
ARG RUSTUP_DIST_SERVER=https://static.rust-lang.org
ARG RUSTUP_UPDATE_ROOT=https://static.rust-lang.org/rustup

# Replace Debian sources if mirror is provided and not default
RUN if [ "${DEBIAN_MIRROR}" != "deb.debian.org" ]; then \
        sed -i "s/deb.debian.org/${DEBIAN_MIRROR}/g" /etc/apt/sources.list.d/debian.sources; \
    fi

# Configure Cargo registry if provided
RUN if [ "${CARGO_REGISTRY}" != "https://index.crates.io" ]; then \
        mkdir -p ~/.cargo && \
        echo "[source.crates-io]" > ~/.cargo/config && \
        echo "replace-with = 'mirror'" >> ~/.cargo/config && \
        echo "[source.mirror]" >> ~/.cargo/config && \
        echo "registry = '${CARGO_REGISTRY}'" >> ~/.cargo/config; \
    fi

# Configure Rustup mirrors (override in docker-compose for regional mirrors; see .env.example)
ENV RUSTUP_DIST_SERVER=${RUSTUP_DIST_SERVER}
ENV RUSTUP_UPDATE_ROOT=${RUSTUP_UPDATE_ROOT}


# Install cargo-llvm-cov for code coverage (Skipped due to network restrictions)
# RUN rustup self update && \
#     rustup component add llvm-tools-preview && \
#     cargo install cargo-llvm-cov

# Install cargo-chef for optimized Docker layer caching
# (see: https://github.com/LukeMathWalker/cargo-chef)
RUN cargo install --locked cargo-chef

# Set up sccache environment (Skipped)
# ENV RUSTC_WRAPPER=sccache

# Create a temporary directory for dependency caching
WORKDIR /tmp/build-cache

# Copy dependency file
COPY Cargo.toml .

# Remove workspace members to avoid missing directory errors
# We only need to cache the main package dependencies
RUN sed -i '/^members = /,/\]/c\
members = ["."]\
' Cargo.toml

# Create a dummy workspace to trick Cargo into downloading and caching all dependencies
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs

# Fetch all dependencies
RUN cargo fetch

# Clean up temporary workspace but KEEP Cargo.toml and Cargo.lock for reuse
RUN rm -rf src

# Set up labels
LABEL com.dsb.base-image="rust-base"
LABEL com.dsb.base-image.version="1.0.0"
LABEL description="DSB Rust base image with pre-cached Cargo dependencies"

# Default command (keep container alive)
CMD ["bash"]
