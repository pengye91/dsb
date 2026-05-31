#!/bin/sh
set -e

BASE_PATH="${DSB_BASE_PATH:-/}"

# Normalize: ensure leading slash, no trailing slash
BASE_PATH=$(echo "$BASE_PATH" | sed 's|^/*|/|; s|/*$||')

if [ "$BASE_PATH" = "/" ] || [ -z "$BASE_PATH" ]; then
  # No base path — remove the placeholder entirely
  echo "[entrypoint] No base path configured (root deployment)"
  find /usr/share/nginx/html -type f \( -name "*.html" -o -name "*.js" -o -name "*.css" \) \
    -exec sed -i 's|/__DSB_BP__||g' {} +
else
  # Replace placeholder with the configured base path
  echo "[entrypoint] Setting base path to: ${BASE_PATH}"
  find /usr/share/nginx/html -type f \( -name "*.html" -o -name "*.js" -o -name "*.css" \) \
    -exec sed -i "s|/__DSB_BP__|${BASE_PATH}|g" {} +
fi

# Configure proxy upstreams
SEARXNG_URL="${DSB_SEARXNG_URL:-http://searxng:8080/search}"
echo "[entrypoint] Setting SearXNG URL to: ${SEARXNG_URL}"
sed -i "s|\${DSB_SEARXNG_URL}|${SEARXNG_URL}|g" /etc/nginx/nginx.conf

# Nginx starts automatically via the default ENTRYPOINT/CMD
