# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# DSB Full Sandbox Dockerfile (China Mirrors)
#
# This image uses pre-built dsb/sandbox-base for faster builds:
# - Pre-installed Python packages (crawl4ai stack, data science, Playwright Python)
# - Pre-installed Node packages (typescript for development tools)
# - Pre-installed OS packages (chromium, fonts, build tools)
# - Pre-configured DSB user with sudo access
#
# Browser automation:
# - agent-browser CLI (Node.js) - AI-native browser automation
# - Playwright Python - fallback for legacy browser_tools.py
# - Both connect to Chromium via CDP on port 9222
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


# Switch to root for package installation
USER root

# Install additional packages for VNC/desktop environment (not in base)
RUN sed -i "s|deb.debian.org|${DEBIAN_MIRROR}|g" /etc/apt/sources.list.d/debian.sources && \
    apt-get update && apt-get install -y \
    iputils-ping \
    bind9-dnsutils \
    # X11 libraries required for computer use plugin
    libx11-6 \
    libxrandr2 \
    libxext6 \
    libxrender1 \
    libxfixes3 \
    libxss1 \
    libxtst6 \
    libxi6 \
    # VNC and desktop environment for computer use
    xvfb \
    x11vnc \
    novnc \
    xfce4 \
    xfce4-terminal \
    dbus-x11 \
    wmctrl \
    xdotool \
    # Additional CJK fonts
    fonts-arphic-uming \
    fonts-arphic-ukai \
    && rm -rf /var/lib/apt/lists/*

ENV LANG=en_US.UTF-8 \
    LC_ALL=en_US.UTF-8

# Chromium wrapper (corporate HTTP proxy → explicit --proxy-server for CDP navigation)
COPY chromium-launch.sh /opt/tools/chromium-launch.sh
RUN chmod 0755 /opt/tools/chromium-launch.sh && chown dsb:dsb /opt/tools/chromium-launch.sh

# Copy supervisord configuration for chromium CDP and VNC
COPY supervisord.conf /etc/supervisor/conf.d/supervisord.conf

# Pre-configure XFce panel to bottom of screen (avoids panel covering Chromium tabs at top)
# Panel position: p=6=top, p=10=bottom. Create the xfce4-panel.xml with bottom position.
RUN mkdir -p /home/dsb/.config/xfce4/xfconf/xfce-perchannel-xml && \
    echo '<?xml version="1.1" encoding="UTF-8"?><channel name="xfce4-panel" version="1.0"><property name="configver" type="int" value="2"/><property name="panels" type="array"><value type="int" value="1"/><value type="int" value="2"/><property name="dark-mode" type="bool" value="true"/><property name="panel-1" type="empty"><property name="position" type="string" value="p=10;x=0;y=0"/><property name="length" type="uint" value="100"/><property name="position-locked" type="bool" value="true"/><property name="icon-size" type="uint" value="16"/><property name="size" type="uint" value="26"/><property name="plugin-ids" type="array"><value type="int" value="1"/><value type="int" value="2"/><value type="int" value="3"/><value type="int" value="4"/><value type="int" value="5"/><value type="int" value="6"/><value type="int" value="7"/><value type="int" value="8"/><value type="int" value="9"/><value type="int" value="10"/></property></property><property name="panel-2" type="empty"><property name="autohide-behavior" type="uint" value="1"/><property name="position" type="string" value="p=10;x=0;y=0"/><property name="length" type="uint" value="1"/><property name="position-locked" type="bool" value="true"/><property name="size" type="uint" value="48"/><property name="plugin-ids" type="array"><value type="int" value="11"/><value type="int" value="12"/><value type="int" value="13"/><value type="int" value="14"/><value type="int" value="15"/><value type="int" value="16"/><value type="int" value="17"/><value type="int" value="18"/></property></property></property><property name="plugins" type="empty"><property name="plugin-1" type="string" value="applicationsmenu"/><property name="plugin-2" type="string" value="tasklist"><property name="grouping" type="uint" value="1"/></property><property name="plugin-3" type="string" value="separator"><property name="expand" type="bool" value="true"/><property name="style" type="uint" value="0"/></property><property name="plugin-4" type="string" value="pager"/><property name="plugin-5" type="string" value="separator"><property name="style" type="uint" value="0"/></property><property name="plugin-6" type="string" value="systray"><property name="square-icons" type="bool" value="true"/></property><property name="plugin-7" type="string" value="separator"><property name="style" type="uint" value="0"/></property><property name="plugin-8" type="string" value="clock"/><property name="plugin-9" type="string" value="separator"><property name="style" type="uint" value="0"/></property><property name="plugin-10" type="string" value="actions"/><property name="plugin-11" type="string" value="showdesktop"/><property name="plugin-12" type="string" value="separator"/><property name="plugin-13" type="string" value="launcher"><property name="items" type="array"><value type="string" value="17752271641.desktop"/></property></property><property name="plugin-14" type="string" value="launcher"><property name="items" type="array"><value type="string" value="17752271642.desktop"/></property></property><property name="plugin-15" type="string" value="launcher"><property name="items" type="array"><value type="string" value="17752271643.desktop"/></property></property><property name="plugin-16" type="string" value="launcher"><property name="items" type="array"><value type="string" value="17752271644.desktop"/></property></property><property name="plugin-17" type="string" value="separator"/><property name="plugin-18" type="string" value="directorymenu"><property name="base-directory" type="string" value="/home/dsb"/></property></property></property></channel>' > /home/dsb/.config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml && \
    chown -R dsb:dsb /home/dsb/.config

# Copy Python scripts (as root - /opt/tools already owned by dsb from base image)
# NEW: agent_browser_tools.py - AI-native browser automation via agent-browser CLI
COPY agent_browser_tools.py /opt/tools/
# LEGACY: browser_tools.py and web_tools.py - kept for backward compatibility during migration
COPY browser_tools.py /opt/tools/
COPY web_tools.py /opt/tools/
COPY error_codes.py /opt/tools/
COPY error_handler.py /opt/tools/
COPY tool_proxy.py /opt/tools/
COPY meta/table_meta.xml /opt/tools/meta/

# Install agent-browser globally for AI-native browser automation
# agent-browser provides accessibility snapshots with refs (@e1, @e2) for deterministic element selection
# Must run as root for global npm install (symlinks to /usr/bin)
USER root
RUN npm config set registry ${NPM_MIRROR} && \
    npm install -g agent-browser && \
    agent-browser --version

# Switch to dsb user for pip installations
USER dsb

# Install additional Python dependencies for web scraping
# These are used by agent_browser_tools.py for advanced scraping features
RUN uv pip install --system --no-cache-dir -i ${PYPI_MIRROR} \
    trafilatura>=1.6.0 \
    rank-bm25>=0.2.2 \
    html2text>=2020.1.16

# Switch to dsb user for all subsequent operations
USER dsb
WORKDIR /public

# Create .zshrc to suppress zsh-newuser-install prompt
RUN touch ~/.zshrc

ENV DEBIAN_FRONTEND=noninteractive
ENV DISPLAY=:1
ENV VNC_PORT=5901
ENV NO_VNC_PORT=6080
ENV VNC_RESOLUTION=2560x1440

# crawl4ai configuration
# Playwright will connect to system chromium via CDP
ENV CRAWL4AI_BROWSER_TYPE=playwright
ENV CRAWL4AI_HEADLESS=true

# Tool proxy port (internal, not exposed to host)
EXPOSE 8080

# DSB feature metadata
LABEL com.dsb.features='{"version":"1.0","features":{"vnc":{"description":"VNC server with web client","ports":[{"host":5901,"container":5901,"protocol":"tcp","description":"x11vnc"},{"host":6080,"container":6080,"protocol":"tcp","description":"noVNC web client"}],"env":{"DISPLAY":":1","VNC_PORT":"5901","NO_VNC_PORT":"6080","VNC_RESOLUTION":"2560x1440"},"enabled_by_default":true},"browser":{"description":"Chromium with CDP for automation","ports":[{"host":9222,"container":9222,"protocol":"tcp","description":"Chrome DevTools Protocol"}],"env":{"CHROMIUM_PORT":"9222"},"enabled_by_default":true},"desktop":{"description":"XFCE4 desktop environment","enabled_by_default":true}},"default_command":["sudo","-E","/usr/bin/supervisord","-c","/etc/supervisor/conf.d/supervisord.conf"]}'

# Entrypoint
# We use supervisord to manage Xvfb, VNC, and the Chromium instance
# Use sudo -E to preserve environment variables
CMD ["sudo", "-E", "/usr/bin/supervisord", "-c", "/etc/supervisor/conf.d/supervisord.conf"]
# Cache bust Thu Feb  5 08:50:40 CST 2026
