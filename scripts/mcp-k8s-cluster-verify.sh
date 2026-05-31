#!/usr/bin/env bash
# End-to-end: kubectl port-forward → dsb-mcp-server → DSB API (Kubernetes sandboxes) → MCP web_fetch.
#
# Local image build (interactive Docker): ./scripts/build-k8s-release-images.sh
#
# Run this in YOUR terminal (not the Cursor agent shell) after `kubectl` works — VPN
# exec plugins often need an interactive session. Cursor's integrated Shell sometimes returns
# no output / no-op; use an external terminal or CI if verify must run unattended.
#
# Environment (optional):
#   DSB_K8S_NAMESPACE         - default: dsb
#   DSB_K8S_SVC               - Service name for DSB API, default: dsb
#   DSB_K8S_DEPLOY            - Deployment name for rollout wait; default dsb if unset (set to empty to skip)
#   DSB_K8S_API_URL           - if set (e.g. http://dsb.example.nip.io), skip port-forward; use Ingress/LB
#   DSB_K8S_API_PORT          - local port for port-forward, default: 18080 (ignored when DSB_K8S_API_URL set)
#   DSB_K8S_PF_WATCHDOG       - default 1: background restart if kubectl port-forward dies mid-run; 0 disables
#   DSB_MCP_PORT              - local MCP listen port (default: pick a free high port)
#   DSB_MCP_SANDBOX__DEFAULT_IMAGE - image for session sandboxes (must exist in cluster registry)
#                               default: ghcr.io/dsb/sandbox:k8s-v0.0.5
#   DSB_MCP_DSB__TIMEOUT_SECS - MCP → DSB HTTP timeout, default: 600
#   DSB_MCP_WEB__ALLOW_EXEC_FALLBACK - default 0/false; set 1 only if DSB exec works on your cluster
#   DSB_MCP_SERVER_BIN        - path to dsb-mcp-server binary (otherwise `cargo build -p dsb-mcp-server --release`)
#   WEB_FETCH_MAX_LENGTH      - passed to douban_mcp_html example, default: 120000 (faster than full page)
#   WEB_FETCH_URL             - default: https://example.com/ (small smoke page; set to Douban for full test)
#   SKIP_CARGO_RUN            - if set to 1, only start port-forward + MCP (manual testing)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo_target_dir() {
  cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "import sys, json; print(json.load(sys.stdin)['target_directory'])"
}

DSB_K8S_NAMESPACE="${DSB_K8S_NAMESPACE:-dsb}"
DSB_K8S_SVC="${DSB_K8S_SVC:-dsb}"
DSB_K8S_API_PORT="${DSB_K8S_API_PORT:-18080}"
if [[ -z "${DSB_MCP_PORT:-}" ]]; then
  DSB_MCP_PORT="$(python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()")"
fi
DSB_MCP_SANDBOX__DEFAULT_IMAGE="${DSB_MCP_SANDBOX__DEFAULT_IMAGE:-ghcr.io/dsb/sandbox:k8s-v0.0.5}"
DSB_MCP_DSB__TIMEOUT_SECS="${DSB_MCP_DSB__TIMEOUT_SECS:-600}"
WEB_FETCH_MAX_LENGTH="${WEB_FETCH_MAX_LENGTH:-120000}"
WEB_FETCH_URL="${WEB_FETCH_URL:-https://example.com/}"
SKIP_CARGO_RUN="${SKIP_CARGO_RUN:-0}"
DSB_K8S_DEPLOY="${DSB_K8S_DEPLOY-dsb}"
DSB_K8S_PF_WATCHDOG="${DSB_K8S_PF_WATCHDOG:-1}"

export DSB_MCP_SANDBOX__DEFAULT_IMAGE
export DSB_MCP_DSB__TIMEOUT_SECS

if [[ -n "${DSB_K8S_API_URL:-}" ]]; then
  API_URL="${DSB_K8S_API_URL%/}"
  USE_PORT_FORWARD=0
else
  API_URL="http://127.0.0.1:${DSB_K8S_API_PORT}"
  USE_PORT_FORWARD=1
fi
MCP_URL="http://127.0.0.1:${DSB_MCP_PORT}/mcp/dsb/web"

PF_PID=""
MCP_PID=""
PF_WATCH_PID=""
PF_PID_FILE=""

cleanup() {
  if [[ -n "${PF_WATCH_PID}" ]]; then
    kill "${PF_WATCH_PID}" 2>/dev/null || true
    wait "${PF_WATCH_PID}" 2>/dev/null || true
  fi
  if [[ -n "${MCP_PID}" ]]; then
    kill "${MCP_PID}" 2>/dev/null || true
    wait "${MCP_PID}" 2>/dev/null || true
  fi
  if [[ -n "${PF_PID_FILE}" && -f "${PF_PID_FILE}" ]]; then
    kill "$(cat "${PF_PID_FILE}")" 2>/dev/null || true
    rm -f "${PF_PID_FILE}"
  elif [[ -n "${PF_PID}" ]]; then
    kill "${PF_PID}" 2>/dev/null || true
    wait "${PF_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

echo "==> Namespace: ${DSB_K8S_NAMESPACE}, Service: ${DSB_K8S_SVC}"
kubectl get "svc/${DSB_K8S_SVC}" -n "${DSB_K8S_NAMESPACE}" >/dev/null

if [[ -n "${DSB_K8S_DEPLOY}" ]] && kubectl get "deploy/${DSB_K8S_DEPLOY}" -n "${DSB_K8S_NAMESPACE}" &>/dev/null; then
  echo "==> Waiting for stable Deployment/${DSB_K8S_DEPLOY}"
  kubectl rollout status "deploy/${DSB_K8S_DEPLOY}" -n "${DSB_K8S_NAMESPACE}" --timeout=120s
fi

echo "==> Reading API key from *api-keys secret"
SECRET_NAME="$(kubectl get secrets -n "${DSB_K8S_NAMESPACE}" --no-headers 2>/dev/null | awk '/api-keys/ {print $1}' | head -1 || true)"
if [[ -z "${SECRET_NAME}" ]]; then
  echo "Error: no *api-keys secret in ${DSB_K8S_NAMESPACE}" >&2
  exit 1
fi
DSB_API_KEY="$(kubectl get secret "${SECRET_NAME}" -n "${DSB_K8S_NAMESPACE}" -o jsonpath='{.data.api-key}' | base64 -d)"
export DSB_API_KEY
export DSB_MCP_DSB__API_KEY="${DSB_API_KEY}"

if [[ -n "${DSB_MCP_SERVER_BIN:-}" ]]; then
  MCP_BIN="${DSB_MCP_SERVER_BIN}"
  echo "==> Using dsb-mcp-server: ${MCP_BIN}"
  [[ -x "${MCP_BIN}" ]] || { echo "Error: not executable: ${MCP_BIN}" >&2; exit 1; }
else
  echo "==> Building dsb-mcp-server (release)"
  cargo build -p dsb-mcp-server --release
  MCP_BIN="$(cargo_target_dir)/release/dsb-mcp-server"
  [[ -x "${MCP_BIN}" ]] || {
    echo "Error: expected dsb-mcp-server at ${MCP_BIN}" >&2
    exit 1
  }
fi

if [[ "${USE_PORT_FORWARD}" -eq 1 ]]; then
  echo "==> kubectl port-forward: ${DSB_K8S_NAMESPACE}/svc/${DSB_K8S_SVC} ${DSB_K8S_API_PORT}:8080"
  kubectl port-forward -n "${DSB_K8S_NAMESPACE}" "svc/${DSB_K8S_SVC}" "${DSB_K8S_API_PORT}:8080" &
  PF_PID=$!

  if [[ "${DSB_K8S_PF_WATCHDOG}" != "0" ]]; then
    PF_PID_FILE="$(mktemp "${TMPDIR:-/tmp}/dsb-k8s-pf.XXXXXX")"
    echo "${PF_PID}" >"${PF_PID_FILE}"
    (
      set +e
      while sleep 4; do
        cur="$(cat "${PF_PID_FILE}" 2>/dev/null || true)"
        [[ -n "${cur}" ]] || exit 0
        if kill -0 "${cur}" 2>/dev/null; then
          continue
        fi
        echo "==> port-forward (pid ${cur}) exited; restarting kubectl port-forward..." >&2
        kubectl port-forward -n "${DSB_K8S_NAMESPACE}" "svc/${DSB_K8S_SVC}" "${DSB_K8S_API_PORT}:8080" \
          >/dev/null 2>&1 &
        newpf=$!
        echo "${newpf}" >"${PF_PID_FILE}"
        ok=0
        for _ in $(seq 1 90); do
          if curl -sf "${API_URL}/health" >/dev/null 2>&1; then
            echo "    DSB /health OK after port-forward restart" >&2
            ok=1
            break
          fi
          sleep 1
        done
        if [[ "${ok}" -ne 1 ]]; then
          echo "Error: DSB did not become healthy after port-forward restart" >&2
        fi
      done
    ) &
    PF_WATCH_PID=$!
  fi
else
  echo "==> Using DSB_K8S_API_URL (no port-forward): ${API_URL}"
fi

echo "==> Waiting for DSB /health on ${API_URL}"
for _ in $(seq 1 90); do
  if curl -sf "${API_URL}/health" >/dev/null 2>&1; then
    echo "    DSB API healthy"
    break
  fi
  sleep 1
done
if ! curl -sf "${API_URL}/health" >/dev/null 2>&1; then
  echo "Error: DSB did not become healthy on ${API_URL}" >&2
  exit 1
fi

echo "==> Starting dsb-mcp-server on ${DSB_MCP_PORT} (sandbox image: ${DSB_MCP_SANDBOX__DEFAULT_IMAGE})"
"${MCP_BIN}" \
  --port "${DSB_MCP_PORT}" \
  --dsb-api-url "${API_URL}" \
  --api-key "${DSB_API_KEY}" &
MCP_PID=$!

MCP_OK=0
for _ in $(seq 1 60); do
  if curl -sf -X POST "http://127.0.0.1:${DSB_MCP_PORT}/mcp/dsb/web" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "x-api-key: ${DSB_API_KEY}" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}' \
    >/dev/null 2>&1; then
    echo "    MCP /mcp/dsb/web responding on port ${DSB_MCP_PORT}"
    MCP_OK=1
    break
  fi
  if ! kill -0 "${MCP_PID}" 2>/dev/null; then
    echo "Error: dsb-mcp-server exited unexpectedly (check port ${DSB_MCP_PORT} not in use; set DSB_MCP_PORT explicitly)" >&2
    exit 1
  fi
  sleep 1
done
if [[ "${MCP_OK}" -ne 1 ]]; then
  echo "Error: MCP did not respond on port ${DSB_MCP_PORT}" >&2
  exit 1
fi

export DSB_MCP_URL="${MCP_URL}"
export DSB_MCP_SESSION="k8s-verify-$(date +%s)"
export WEB_FETCH_MAX_LENGTH
export WEB_FETCH_URL

echo "==> MCP → DSB → K8s sandbox → web_fetch"
echo "    DSB_MCP_URL=${DSB_MCP_URL}"
echo "    WEB_FETCH_URL=${WEB_FETCH_URL}"
echo "    WEB_FETCH_MAX_LENGTH=${WEB_FETCH_MAX_LENGTH}"

if [[ "${SKIP_CARGO_RUN}" == "1" ]]; then
  echo "==> SKIP_CARGO_RUN=1 — port-forward and MCP are running; press Ctrl+C when done."
  wait "${MCP_PID}" || true
  exit 0
fi

cargo run -p dsb-agent-tester --example douban_mcp_html

echo "==> OK — Douban flow: web_fetch (web MCP) → HTML → file_upload (/public) → GET static URL → saved verify HTML → macOS open"
echo "    Local merge: ./douban_top250_mcp.html — static snapshot: \$TMPDIR/dsb_douban_static_verify.html or VERIFY_HTML_PATH"
echo "    To skip auto-open: VERIFY_OPEN_BROWSER=0 $0"
echo "==> For a larger fetch, rerun with:"
echo "    WEB_FETCH_URL='https://movie.douban.com/top250' WEB_FETCH_MAX_LENGTH=800000 $0"
