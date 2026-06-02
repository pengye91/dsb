# DSB Base Images

Optimized Docker base images that pre-install and cache dependencies for faster builds.

## Overview

These base images significantly reduce Docker build times by pre-downloading and caching:
- Cargo workspace dependencies (Rust)
- Python packages (PyPI)
- npm packages (Node.js)
- OS-level dependencies

**Expected build time improvements:**
- Cold cache: 30-50% faster
- Warm cache: 60-75% faster

## Base Images

### 1. `dsb/rust-base:latest`
**Purpose:** Pre-install Rust workspace dependencies and build tools

**Base Image:** `rust:latest`

**Contents:**
- sccache (compiler cache)
- cargo-llvm-cov (code coverage)
- All workspace Cargo dependencies

**Size:** ~800 MB

**Used by:**
- `Dockerfile` (main DSB server)
- `ssh-gateway/Dockerfile`
- `Dockerfile.test`

### 2. `dsb/python-base:latest`
**Purpose:** Pre-install Python SDK dependencies and tools

**Base Image:** `python:3.12-slim`

**Contents:**
- uv (fast Python package installer)
- pipx
- DSB SDK dependencies (from `sdks/python/pyproject.toml`)

**Size:** ~500 MB

**Used by:**
- `Dockerfile.test`

### 3. `dsb/node-base:latest`
**Purpose:** Pre-install npm packages

**Base Image:** `node:20-alpine`

**Contents:**
- Global packages: typescript, ts-node, typescript-language-server
- Dashboard dependencies (from `dashboard/package.json`)

**Size:** ~400 MB

**Used by:**
- `dashboard/Dockerfile`

### 4. `dsb/runtime-base:latest`
**Purpose:** Common runtime dependencies for Ubuntu-based images

**Base Image:** `ubuntu:24.04`

**Contents:**
- ca-certificates, curl, adduser
- Non-root user (dsb, UID 1001)

**Size:** ~100 MB

**Used by:**
- `Dockerfile` (main DSB server runtime)
- `ssh-gateway/Dockerfile` runtime

### 5. `dsb/sandbox-base:latest`
**Purpose:** Shared dependencies for sandbox images

**Base Image:** `python:3.12.11-slim`

**Contents:**
- OS packages: build-essential, cmake, pkg-config, chromium, fonts
- Python: All packages from `images/sandbox/requirements.txt`
- Node: Global packages + browser tools (playwright, turndown)

**Size:** ~1 GB

**Used by:**
- `images/sandbox/Dockerfile`
- `images/sandbox-slim/Dockerfile`

## Mirror Support

All base images support both international and China mirrors:

### International (Default)
```bash
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/rust-base:latest .
```

### China Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=true -t dsb/rust-base:china .
```

### Tagging Strategy
- `latest` - International mirrors
- `china` - China mirrors
- `v1.2.3` - Semantic versioning for reproducibility

## Building Base Images

### Build All Base Images
```bash
# International mirrors
make base-images-build

# China mirrors
make base-images-build-china
```

### Build Individual Base Images
```bash
# Rust base
cd docker/base-images/rust-base
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/rust-base:latest .
docker build --build-arg USE_CHINA_MIRRORS=true -t dsb/rust-base:china .

# Python base
cd docker/base-images/python-base
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/python-base:latest .
docker build --build-arg USE_CHINA_MIRRORS=true -t dsb/python-base:china .

# Node base
cd docker/base-images/node-base
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/node-base:latest .
docker build --build-arg USE_CHINA_MIRRORS=true -t dsb/node-base:china .

# Runtime base
cd docker/base-images/runtime-base
docker build -t dsb/runtime-base:latest .

# Sandbox base
cd docker/base-images/sandbox-base
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/sandbox-base:latest .
docker build --build-arg USE_CHINA_MIRRORS=true -t dsb/sandbox-base:china .
```

## When to Rebuild Base Images

Rebuild base images when:

1. **Dependency Updates:**
   - `Cargo.lock` changes → Rebuild `rust-base`
   - `package-lock.json` changes → Rebuild `node-base`
   - `pyproject.toml` changes → Rebuild `python-base`
   - `requirements.txt` changes → Rebuild `sandbox-base`

2. **Security Vulnerabilities:**
   - Run `make check-base-updates` to check for security issues

3. **Monthly Maintenance:**
   - Rebuild and push new versions monthly

## Update Workflow

### 1. Check for Updates
```bash
make check-base-updates
```

### 2. Build New Base Images
```bash
make base-images-build-china
```

### 3. Test New Base Images
```bash
# Pull latest changes
git pull origin main

# Rebuild application images with new base
make dc-build

# Run tests
make test
```

### 4. Push to Registry
```bash
make base-images-push
```

### 5. Update Application Dockerfiles
If you're creating a new version (e.g., v1.3.0), update the FROM line in application Dockerfiles:
```dockerfile
FROM dsb/rust-base:v1.3.0
```

## Architecture

```
docker/base-images/
├── README.md
├── rust-base/
│   ├── Dockerfile
│   └── README.md
├── python-base/
│   ├── Dockerfile
│   └── README.md
├── node-base/
│   ├── Dockerfile
│   └── README.md
├── runtime-base/
│   ├── Dockerfile
│   └── README.md
└── sandbox-base/
    ├── Dockerfile
    └── README.md
```

## Benefits

1. **Faster Builds:** 30-75% reduction in build time
2. **Consistent Environments:** Same base across team and CI/CD
3. **Shared Layer Caching:** Layers cached in Docker registry
4. **Reduced Bandwidth:** Only download changed dependencies
5. **Better Developer Experience:** Less waiting, more coding

## Trade-offs

### Maintenance Overhead
- Initial setup: 1-2 weeks
- Ongoing: 1-2 hours/month for updates

### Storage Requirements
- Additional registry storage: ~2-3 GB total
- Local disk: ~2-3 GB when pulling base images

### Complexity
- More complex build process
- Need to understand when to rebuild
- Potential confusion if base images get out of sync

## Troubleshooting

### Base Image Out of Sync
If application Dockerfiles reference a base image version that doesn't exist:
```bash
# Pull the latest base images
docker pull dsb/rust-base:latest
docker pull dsb/python-base:latest
docker pull dsb/node-base:latest
docker pull dsb/runtime-base:latest
docker pull dsb/sandbox-base:latest
```

### Build Failures After Dependency Updates
If builds fail after updating dependencies:
```bash
# Rebuild the affected base image
cd docker/base-images/rust-base
docker build --build-arg USE_CHINA_MIRRORS=true -t dsb/rust-base:china .
```

### Mirror Issues
If China mirrors fail:
```bash
# Fall back to international mirrors
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/rust-base:latest .
```

## Migration Guide

### For Existing Developers
```bash
# 1. Pull pre-built base images
docker pull dsb/rust-base:china
docker pull dsb/python-base:china
docker pull dsb/node-base:china
docker pull dsb/runtime-base:china
docker pull dsb/sandbox-base:china

# 2. Pull latest changes
git pull origin main

# 3. Rebuild application images
make dc-build
```

### For New Developers
No changes needed - base images will be pre-built and available in the registry.

### Rollback
If issues occur:
```bash
# Revert application Dockerfile changes
git checkout HEAD -- Dockerfile docker/*/Dockerfile images/*/Dockerfile

# Rebuild with original base images
make dc-build
```

## Maintenance Schedule

### Daily (Automated)
- Check for security updates

### Weekly (Manual)
- Review dependency updates

### Monthly (Manual)
- Build and push new base image versions
- Update application Dockerfiles if needed
- Document changes in CHANGELOG

## References

- [Main README](../../../README.md)
- [Deployment Guide](../../../deployment/README.md)
