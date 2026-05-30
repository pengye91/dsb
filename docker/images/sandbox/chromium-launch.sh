#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
# Chromium does not consistently honor HTTP_PROXY for navigation when managed by
# supervisord/crawl4ai+CDP. Pass explicit proxy flags when corporate egress requires it.
set -euo pipefail

proxy_url="${HTTPS_PROXY:-${https_proxy:-${HTTP_PROXY:-${http_proxy:-}}}}"
bypass="${NO_PROXY:-${no_proxy:-}}"

args=(
  /usr/bin/chromium
  --no-sandbox
  --disable-setuid-sandbox
  --disable-dev-shm-usage
  --remote-debugging-port=9222
  --remote-debugging-address=0.0.0.0
  --user-data-dir=/tmp/.chromium-cdp
  --display=:1
  --start-maximized
  --disable-software-rasterizer
  --enable-unsafe-swiftshader
)

if [[ -n "${proxy_url}" ]]; then
  args+=(--proxy-server="${proxy_url}")
  if [[ -n "${bypass}" ]]; then
    args+=(--proxy-bypass-list="${bypass}")
  else
    # Safe defaults so local CDP / loopback never go through the corporate proxy
    args+=(--proxy-bypass-list="localhost,127.0.0.1,::1")
  fi
fi

exec "${args[@]}"
