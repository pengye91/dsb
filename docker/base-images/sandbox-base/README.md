# DSB Sandbox Base Image

Shared dependencies for full and slim sandbox images.

## Overview

This base image pre-installs all Python, Node.js, and OS-level dependencies used by both the full sandbox (with VNC/desktop) and slim sandbox (browser tools only) images.

## Contents

### OS Packages
- **Build tools:** build-essential, cmake, pkg-config
- **Browser:** chromium (system-wide)
- **Fonts:** Liberation, Noto, WQY (CJK support)
- **Node.js:** nodejs, npm
- **Python:** python3-pip, python3-venv
- **Utilities:** curl, sudo, bash, ripgrep
- **Process manager:** supervisor

### Python Packages

**Data Science:**
- numpy, pandas, scipy, scikit-learn
- matplotlib, seaborn

**Web Scraping (crawl4ai stack):**
- crawl4ai, beautifulsoup4, lxml
- playwright, httpx[http2]
- nltk, rich, aiofiles, aiohttp

**Development:**
- python-lsp-server

### Node.js Packages

**Global:**
- typescript, ts-node, typescript-language-server

**Browser Tools:**
- playwright (browser automation)
- turndown (HTML to Markdown)

## Image Details

- **Base:** `python:3.12.11-slim`
- **Size:** ~1 GB
- **Tag:** `dsb/sandbox-base:latest` (international), `dsb/sandbox-base:china` (China mirrors)

## Building

### International Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=false -t dsb/sandbox-base:latest .
```

### China Mirrors
```bash
docker build --build-arg USE_CHINA_MIRRORS=true \
  --build-arg DEBIAN_MIRROR=mirrors.tuna.tsinghua.edu.cn \
  --build-arg PYPI_MIRROR=https://pypi.tuna.tsinghua.edu.cn/simple \
  --build-arg NPM_MIRROR=https://registry.npmmirror.com \
  -t dsb/sandbox-base:china .
```

## Usage

Use in sandbox Dockerfiles:

```dockerfile
FROM dsb/sandbox-base:latest

# Copy scripts
COPY supervisord.conf /etc/supervisor/conf.d/
COPY browser_tools.js /opt/browser_tools/

# Create DSB user (already created in base, just ensure permissions)
USER dsb
WORKDIR /home/dsb

CMD ["sudo", "/usr/bin/supervisord"]
```

## When to Rebuild

Rebuild this image when:
- `images/sandbox/requirements.txt` changes
- New Python or Node dependencies added
- Chromium or OS packages need updates
- Security vulnerabilities
- Monthly maintenance updates

## Benefits

1. **Faster builds:** 60-70% reduction in sandbox image build time
2. **Shared layers:** Both sandbox images use same base layers
3. **Consistent environment:** Same tools across all sandboxes
4. **Pre-configured:** User accounts, sudo, directories ready

## Architecture

### Directory Structure

The image creates these directories for sandbox use:
- `/public` - Public workspace volume mount
- `/opt/browser_tools` - Browser automation scripts
- `/usr/local/lib` - Computer use plugin location
- `/home/dsb` - DSB user home directory

### User Account

**dsb** user:
- UID: Automatically assigned (usually 1000)
- Groups: sudo (with NOPASSWD)
- Home: `/home/dsb`
- Shell: `/bin/bash`

### Browser Configuration

System Chromium is used as single source of truth:
- Located at `/usr/bin/chromium`
- Playwright connects via Chrome DevTools Protocol (CDP)
- No separate browser installation needed

## Used By

- `images/sandbox/Dockerfile` - Full sandbox with VNC/desktop
- `images/sandbox-slim/Dockerfile` - Slim sandbox with browser tools only

## Technical Details

### Why Separate Base Image?

Both sandbox images share ~90% of dependencies:
- Same Python packages
- Same Node packages
- Same OS packages
- Same user setup

Separating base image:
- Avoids duplicating 1 GB layers
- Faster builds (cache shared layers)
- Easier maintenance (single update point)

### Mirror Support

All mirrors configured for China and international:
- **APT:** Debian mirrors (Tsinghua for China)
- **PyPI:** Official or Tsinghua
- **npm:** Official or npmmirror (Taobao)

### Dependency Caching

Uses BuildKit cache mounts for:
- `/root/.cache/pip` - Python packages
- `/root/.npm` - npm packages

## See Also

- [Main README](../README.md)
- [Sandbox README](../../../../images/sandbox/README.md)
- [requirements.txt](../../../../images/sandbox/requirements.txt)
