# DSB Runtime Base Image

Minimal Ubuntu runtime with common dependencies and DSB user.

## Overview

This base image provides a minimal Ubuntu 24.04 runtime environment with common runtime dependencies and the DSB user account pre-configured.

## Contents

### System Packages
- **ca-certificates** - SSL/TLS certificates
- **curl** - HTTP client
- **adduser** - User management utilities

### User Account
- **dsb** - Non-root user (UID 1001)
  - Disabled password (no login)
  - No shell access initially
  - Home directory: `/home/dsb`

## Image Details

- **Base:** `ubuntu:24.04`
- **Size:** ~100 MB
- **Tag:** `dsb/runtime-base:latest`

## Building

```bash
docker build -t dsb/runtime-base:latest .
```

## Usage

Use in application Dockerfiles for runtime stages:

```dockerfile
# Build stage uses rust-base
FROM dsb/rust-base:latest AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --bin dsb

# Runtime stage uses runtime-base
FROM dsb/runtime-base:latest
COPY --from=builder /build/target/release/dsb /usr/local/bin/dsb
USER dsb
CMD ["dsb", "server"]
```

## Why Separate Runtime Base?

### Security Benefits
- Minimal attack surface (only essential packages)
- Non-root user by default
- No build tools or compilers

### Size Benefits
- Smaller final images (~100 MB vs ~800 MB for build images)
- Faster deployment and pull times

### Consistency
- Same user account across all DSB services
- Consistent UID/GID for volume mounting
- Standardized runtime environment

## When to Rebuild

Rebuild this image when:
- Ubuntu security updates available
- New runtime dependencies needed
- User account configuration changes

## Benefits

1. **Smaller images:** Runtime-only images are ~700 MB smaller than build images
2. **Better security:** No build tools or compilers in production images
3. **Consistency:** Same user and environment across all services
4. **Faster deployment:** Smaller images pull faster

## Technical Details

### UID 1001

The DSB user uses UID 1001 to:
- Avoid conflicts with system users (0-999)
- Follow container best practices
- Ensure consistent file permissions
- Work well with volume mounts

### Multi-Stage Builds

This image is designed for use in multi-stage builds:

1. **Build stage:** Use `dsb/rust-base` to compile binaries
2. **Runtime stage:** Use `dsb/runtime-base` to run the application

This pattern:
- Reduces final image size
- Improves security
- Separates build and runtime concerns

## Used By

- `Dockerfile` - Main DSB server (runtime stage)
- `ssh-gateway/Dockerfile` - SSH gateway (runtime stage)

## See Also

- [Main README](../README.md)
- [rust-base README](../rust-base/README.md)
