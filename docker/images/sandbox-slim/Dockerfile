# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# DSB Slim Sandbox Dockerfile (China Mirrors)
#
# This image uses pre-built dsb/sandbox-base for faster builds:
# - Pre-installed Python packages (crawl4ai stack, data science, Playwright Python)
# - Pre-installed Node packages (typescript for development tools)
# - Pre-installed OS packages (chromium, fonts, build tools)
# - Pre-configured DSB user with sudo access
#
# Browser automation uses Playwright Python (not Node.js/Puppeteer)
# - Direct Python function calls (no subprocess overhead)
# - Connects to Chromium via CDP on port 9222
#
# China mirrors (configurable via ARG, passed to base images):
# - Debian: mirrors.tuna.tsinghua.edu.cn
# - PyPI: https://pypi.tuna.tsinghua.edu.cn/simple
# - npm: https://registry.npmmirror.com

ARG BASE_IMAGE_TAG=latest
FROM dsb/sandbox-base:${BASE_IMAGE_TAG}

ARG DOCKER_REGISTRY=docker.io
ARG PYPI_MIRROR=https://pypi.org/simple
ARG DEBIAN_MIRROR=deb.debian.org

ENV PIP_INDEX_URL=${PYPI_MIRROR}
ARG DOCKER_REGISTRY
ARG DEBIAN_MIRROR
ARG PYPI_MIRROR
ARG NPM_MIRROR

# Copy supervisord configuration for chromium CDP
COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf

# Copy Python scripts (as root - /opt/tools already owned by dsb from base image)
# Note: browser_tools.js removed - now using browser_tools.py with Playwright Python
COPY agent_browser_tools.py /opt/tools/
COPY browser_tools.py /opt/tools/
COPY web_tools.py /opt/tools/
COPY error_codes.py /opt/tools/
COPY error_handler.py /opt/tools/
COPY tool_proxy.py /opt/tools/
COPY meta/table_meta.xml /opt/tools/meta/

# Switch to dsb user for npm and pip installations
USER dsb

# Install agent-browser globally for AI-native browser automation
# agent-browser provides accessibility snapshots with refs (@e1, @e2) for deterministic element selection
RUN npm config set registry ${NPM_MIRROR} && \
    npm install -g agent-browser && \
    agent-browser --version || echo "agent-browser installed"

# Install additional Python dependencies for web scraping (trafilatura, rank_bm25, duckduckgo-search)
# These are used by agent_browser_tools.py for advanced scraping features
RUN uv pip install --system --no-cache-dir -i ${PYPI_MIRROR} \
    trafilatura>=1.6.0 \
    rank-bm25>=0.2.2 \
    duckduckgo-search>=4.0.0 \
    html2text>=2020.1.16

WORKDIR /public

# crawl4ai configuration
# Playwright will connect to system chromium via CDP
ENV CRAWL4AI_BROWSER_TYPE=playwright
ENV CRAWL4AI_HEADLESS=true

# DSB feature metadata (web tools enabled)
LABEL com.dsb.features='{"version":"1.0","features":{"browser":{"description":"Chromium with CDP for web scraping and automation","ports":[{"host":9222,"container":9222,"protocol":"tcp","description":"Chrome DevTools Protocol"}],"env":{"CHROMIUM_PORT":"9222"},"enabled_by_default":true}},"default_command":["sudo","-E","/usr/bin/supervisord","-c","/etc/supervisor/conf.d/supervisord.conf"]}'

# Use supervisord to manage chromium for web tools
# Use sudo -E to preserve environment variables
CMD ["sudo", "-E", "/usr/bin/supervisord", "-c", "/etc/supervisor/conf.d/supervisord.conf"]
