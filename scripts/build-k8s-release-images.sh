#!/usr/bin/env bash
# Build release artifacts for Kubernetes: cargo release + Docker images tagged for deploy.
#
# Uses a temporary DOCKER_CONFIG without credsStore so anonymous docker.io pulls (BuildKit
# dockerfile frontend, public bases) work in non-interactive shells. For private registry
# pushes, run `docker login` in your normal shell or unset DOCKER_CONFIG before push.
#
# Usage:
#   ./scripts/build-k8s-release-images.sh
#   TAG=my-tag ./scripts/build-k8s-release-images.sh
#   SANDBOX_BASE_TAG=latest   # default; must match your dsb/sandbox-base:… image
#
# Requires: .env in repo root (see `make config-default`), Docker, base images dsb/rust-base:latest
# and dsb/runtime-base:latest (`make rust-base runtime-base` or full base-images-build).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TAG="${TAG:-dev}"
# Base image tag for sandbox Dockerfile (must exist locally or in registry); not the same as release TAG.
SANDBOX_BASE_TAG="${SANDBOX_BASE_TAG:-latest}"

_DSB_DOCKER_CFG="$(mktemp -d)"
printf '%s\n' '{"auths":{}}' >"$_DSB_DOCKER_CFG/config.json"
export DOCKER_CONFIG="$_DSB_DOCKER_CFG"
cleanup_docker_cfg() {
  rm -rf "${_DSB_DOCKER_CFG:-}"
  unset DOCKER_CONFIG
}
trap cleanup_docker_cfg EXIT INT TERM

if [[ ! -f .env ]]; then
  echo "Error: missing .env — run: make config-default   (or config-china)" >&2
  exit 1
fi

echo "==> Release cargo builds"
cargo build --release -p dsb --features kubernetes
cargo build --release -p dsb-mcp-server

echo "==> Docker: dsb/server:${TAG} (--features kubernetes)"
docker build -f docker/Dockerfile \
  --build-arg BASE_IMAGE_TAG=latest \
  --build-arg FEATURES=kubernetes \
  -t "dsb/server:${TAG}" .

echo "==> Docker: dsb/mcp-server:${TAG}"
docker build -f docker/Dockerfile.mcp \
  --build-arg BASE_IMAGE_TAG=latest \
  -t "dsb/mcp-server:${TAG}" .

echo "==> Docker: dsb/sandbox:${TAG} (FROM dsb/sandbox-base:${SANDBOX_BASE_TAG})"
# Do not use `make sandbox TAG=…`: Makefile sets BASE_IMAGE_TAG=\$(TAG), which breaks release tags.
cd docker/images/sandbox
# shellcheck disable=SC2046
docker build \
  --build-arg "BASE_IMAGE_TAG=${SANDBOX_BASE_TAG}" \
  $(grep -E "^(DOCKER_REGISTRY|DEBIAN_MIRROR|PYPI_MIRROR)" ../../../.env 2>/dev/null | sed 's/^/--build-arg /') \
  -t "dsb/sandbox:${TAG}" .
cd "${ROOT}"

echo ""
echo "==> Done. Re-tag and push for your registry, for example:"
echo "    docker tag dsb/server:${TAG} <registry>/dsb/server:${TAG}"
echo "    docker tag dsb/mcp-server:${TAG} <registry>/dsb/mcp-server:${TAG}"
echo "    docker tag dsb/sandbox:${TAG} <registry>/dsb/sandbox:${TAG}"
echo "Then helm upgrade with the new server (and sandbox default) image tags."
