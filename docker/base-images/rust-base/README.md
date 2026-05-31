# DSB Rust Base Image

Pre-installed Rust workspace dependencies and build tools for faster Docker builds.

## Overview

This base image pre-downloads and caches all Cargo workspace dependencies used by DSB's Rust components (main server, SSH gateway, MCP server, static server).

## Contents

### Build Tools
- **sccache** - Compiler cache for faster rebuilds
- **cargo-llvm-cov** - Code coverage tool for testing

### Pre-cached Dependencies
All workspace dependencies from `Cargo.toml`, including:
- bollard (Docker)
- axum, tower (Web framework)
- tokio (Async runtime)
- serde (Serialization)
- clap (CLI)
- And 30+ other dependencies

## Image Details

- **Base:** `rust:latest`
- **Size:** ~800 MB
- **Tag:** `dsb/rust-base:latest` (international), `dsb/rust-base:china` (China mirrors)

## Building

### International Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/rust-base:latest .
```

### China Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=true \
  --build-arg CARGO_REGISTRY=https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/ \
  -t dsb/rust-base:china .
```

## Usage

Use in application Dockerfiles:

```dockerfile
# Build stage
FROM dsb/rust-base:latest AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY ssh-gateway ./ssh-gateway

# Build (dependencies already cached)
RUN cargo build --release --bin dsb
```

## When to Rebuild

Rebuild this image when:
- `Cargo.lock` changes (new dependency versions)
- New workspace dependencies added
- Security vulnerabilities in Rust dependencies
- Monthly maintenance updates

## Benefits

1. **Faster builds:** 60-75% reduction in build time for Rust components
2. **Shared cache:** Dependencies cached in Docker registry
3. **Consistent environment:** Same base across team and CI/CD

## Trade-offs

- **Storage:** ~800 MB additional image size
- **Maintenance:** Need to rebuild when dependencies update
- **Complexity:** Additional layer in Docker build process

## Technical Details

### Dependency Caching Strategy

The image uses a dummy Cargo workspace to pre-fetch all dependencies without building:

1. Creates a temporary workspace with all dependencies
2. Runs `cargo fetch` to download and cache dependencies
3. Cleans up temporary files
4. Leaves cached dependencies in `/usr/local/cargo`

### Mirror Support

Supports both international and China mirrors:

- **International:** Uses `https://index.crates.io/` (default)
- **China:** Uses Tsinghua TUNA mirror `https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/`

## See Also

- [Main README](../README.md)
- [Cargo.toml](../../../../Cargo.toml)
