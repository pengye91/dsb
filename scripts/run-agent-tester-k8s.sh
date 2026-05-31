#!/usr/bin/env bash
# Run dsb-agent-tester against a DSB API in kind (kubernetes sandbox backend).
# Prerequisites: `make test-k8s-e2e` (or equivalent Helm install), Docker (for SearXNG),
# kubectl context pointing at the kind cluster.
#
# Environment (optional):
#   K8S_E2E_NAMESPACE       - DSB namespace (default: dsb-system)
#   K8S_E2E_CLUSTER_NAME    - kind cluster name (default: dsb-e2e)
#   K8S_AGENT_SANDBOX_IMAGE - Image for tests (default: dsb/sandbox:latest)
#   DSB_K8S_API_PORT        - Local port for kubectl port-forward (default: 18080)
#   DSB_MCP_PORT            - Local MCP listen port (default: 3223)
#   SEARXNG_HOST_PORT       - Host port for SearXNG (default: 8888)
#   TAG                     - Passed to docker compose for searxng (default: latest)
#   DSB_MCP_SERVER_BIN      - Pre-built dsb-mcp-server binary (skips `cargo build` if set)
#   DSB_MCP_DSB__TIMEOUT_SECS - HTTP timeout for MCP -> DSB API (default: 600; k8s create_sandbox can be slow)
#   DSB_AGENT_TEST_THREADS   - Limit parallel tests (default: 2) so kind is not flooded with concurrent sandbox creates

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Resolve cargo target dir (honors CARGO_TARGET_DIR / IDE sandboxes — not always $ROOT/target)
cargo_target_dir() {
  cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c "import sys, json; print(json.load(sys.stdin)['target_directory'])"
}

K8S_E2E_NAMESPACE="${K8S_E2E_NAMESPACE:-dsb-system}"
K8S_E2E_CLUSTER_NAME="${K8S_E2E_CLUSTER_NAME:-dsb-e2e}"
K8S_AGENT_SANDBOX_IMAGE="${K8S_AGENT_SANDBOX_IMAGE:-dsb/sandbox:latest}"
DSB_K8S_API_PORT="${DSB_K8S_API_PORT:-18080}"
DSB_MCP_PORT="${DSB_MCP_PORT:-3223}"
SEARXNG_HOST_PORT="${SEARXNG_HOST_PORT:-8888}"
TAG="${TAG:-latest}"

MCP_URL="http://127.0.0.1:${DSB_MCP_PORT}/mcp"
API_URL="http://127.0.0.1:${DSB_K8S_API_PORT}"

PF_PID=""
MCP_PID=""

cleanup() {
  if [[ -n "${MCP_PID}" ]]; then
    kill "${MCP_PID}" 2>/dev/null || true
    wait "${MCP_PID}" 2>/dev/null || true
  fi
  if [[ -n "${PF_PID}" ]]; then
    kill "${PF_PID}" 2>/dev/null || true
    wait "${PF_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

echo "==> Resolving DSB Service in namespace ${K8S_E2E_NAMESPACE}"
SVC="$(kubectl get svc -n "${K8S_E2E_NAMESPACE}" -l app.kubernetes.io/name=dsb -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)"
if [[ -z "${SVC}" ]]; then
  echo "Error: no Service with label app.kubernetes.io/name=dsb in ${K8S_E2E_NAMESPACE}" >&2
  echo "Install the chart first (e.g. make test-k8s-e2e)." >&2
  exit 1
fi
echo "    Using Service/${SVC}"

echo "==> Reading API key from *api-keys secret"
SECRET_NAME="$(kubectl get secrets -n "${K8S_E2E_NAMESPACE}" --no-headers 2>/dev/null | awk '/api-keys/ {print $1}' | head -1 || true)"
if [[ -z "${SECRET_NAME}" ]]; then
  echo "Error: no *api-keys secret in ${K8S_E2E_NAMESPACE}" >&2
  exit 1
fi
DSB_API_KEY="$(kubectl get secret "${SECRET_NAME}" -n "${K8S_E2E_NAMESPACE}" -o jsonpath='{.data.api-key}' | base64 -d)"
export DSB_API_KEY

# Default 30s reqwest timeout is too low for create_sandbox on kind (image pull + pod schedule + ready).
export DSB_MCP_DSB__TIMEOUT_SECS="${DSB_MCP_DSB__TIMEOUT_SECS:-600}"

echo "==> Loading sandbox test images into kind cluster ${K8S_E2E_CLUSTER_NAME} (best effort)"
for img in "${K8S_AGENT_SANDBOX_IMAGE}" ubuntu:22.04 python:3.12; do
  if docker image inspect "${img}" >/dev/null 2>&1; then
    kind load docker-image "${img}" --name "${K8S_E2E_CLUSTER_NAME}" 2>/dev/null && echo "    loaded ${img}" || echo "    warning: kind load failed for ${img}"
  else
    echo "    pulling ${img}..."
    if docker pull "${img}" >/dev/null 2>&1; then
      kind load docker-image "${img}" --name "${K8S_E2E_CLUSTER_NAME}" 2>/dev/null && echo "    loaded ${img}" || echo "    warning: kind load failed for ${img}"
    else
      echo "    warning: skip ${img} (not local and pull failed — build/tag locally or use a public test image)"
    fi
  fi
done

echo "==> Starting SearXNG (docker compose searxng) on host port ${SEARXNG_HOST_PORT}"
export DSB_SEARXNG_PORT="${SEARXNG_HOST_PORT}"
( cd "${ROOT}/docker" && TAG="${TAG}" docker compose up -d searxng )
for _ in $(seq 1 60); do
  if curl -sf "http://127.0.0.1:${SEARXNG_HOST_PORT}/" >/dev/null 2>&1; then
    echo "    SearXNG is up"
    break
  fi
  sleep 1
done

if [[ -n "${DSB_MCP_SERVER_BIN:-}" ]]; then
  MCP_BIN="${DSB_MCP_SERVER_BIN}"
  echo "==> Using dsb-mcp-server: ${MCP_BIN}"
  [[ -x "${MCP_BIN}" ]] || { echo "Error: not executable: ${MCP_BIN}" >&2; exit 1; }
else
  echo "==> Building dsb-mcp-server"
  cargo build -p dsb-mcp-server --release
  MCP_BIN="$(cargo_target_dir)/release/dsb-mcp-server"
  [[ -x "${MCP_BIN}" ]] || {
    echo "Error: expected dsb-mcp-server at ${MCP_BIN} (check CARGO_TARGET_DIR)" >&2
    exit 1
  }
  echo "    Binary: ${MCP_BIN}"
fi

echo "==> kubectl port-forward: ${K8S_E2E_NAMESPACE}/svc/${SVC} ${DSB_K8S_API_PORT}:8080"
kubectl port-forward -n "${K8S_E2E_NAMESPACE}" "svc/${SVC}" "${DSB_K8S_API_PORT}:8080" &
PF_PID=$!

echo "==> Waiting for DSB /health"
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

SEARXNG_SEARCH_URL="http://127.0.0.1:${SEARXNG_HOST_PORT}/search"
export DSB_SEARXNG_API_URL="${SEARXNG_SEARCH_URL}"

echo "==> Starting dsb-mcp-server on port ${DSB_MCP_PORT}"
"${MCP_BIN}" \
  --port "${DSB_MCP_PORT}" \
  --dsb-api-url "${API_URL}" \
  --searxng-api-url "${SEARXNG_SEARCH_URL}" &
MCP_PID=$!

MCP_OK=0
for _ in $(seq 1 60); do
  if curl -sf -X POST "http://127.0.0.1:${DSB_MCP_PORT}/mcp" \
    -H "Content-Type: application/json" \
    -H "Accept: application/json, text/event-stream" \
    -H "x-api-key: ${DSB_API_KEY}" \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}' \
    >/dev/null 2>&1; then
    echo "    MCP responding"
    MCP_OK=1
    break
  fi
  if ! kill -0 "${MCP_PID}" 2>/dev/null; then
    echo "Error: dsb-mcp-server exited unexpectedly" >&2
    exit 1
  fi
  sleep 1
done
if [[ "${MCP_OK}" -ne 1 ]]; then
  echo "Error: MCP did not respond on port ${DSB_MCP_PORT}" >&2
  exit 1
fi

export DSB_MCP_URL="${MCP_URL}"
export DSB_TEST_SANDBOX_IMAGE="${K8S_AGENT_SANDBOX_IMAGE}"

DSB_AGENT_TEST_THREADS="${DSB_AGENT_TEST_THREADS:-2}"
echo "==> Running dsb-agent-tester (skipping test_dsb_stack_lifecycle)"
echo "    DSB_MCP_URL=${DSB_MCP_URL}"
echo "    DSB_TEST_SANDBOX_IMAGE=${DSB_TEST_SANDBOX_IMAGE}"
echo "    DSB_MCP_DSB__TIMEOUT_SECS=${DSB_MCP_DSB__TIMEOUT_SECS}"
echo "    test threads=${DSB_AGENT_TEST_THREADS} (lower if sandboxes time out on kind)"
cargo test -p dsb-agent-tester -- --test-threads="${DSB_AGENT_TEST_THREADS}" --nocapture --skip test_dsb_stack_lifecycle
