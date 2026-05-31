#!/bin/bash
# Build and push DSB images to registry
# Usage: ./scripts/build-and-push.sh <tag>

set -e

TAG=${1:-dev-v0.5}
REGISTRY=${REGISTRY:-docker.io/}

echo "=================================================="
echo "Building DSB images with tag: $TAG"
echo "Registry: $REGISTRY"
echo "=================================================="

# Build base images
echo "Building base images..."
make base-images-build TAG=$TAG DOCKER_REGISTRY=$REGISTRY

# Build service images
echo "Building service images..."
TAG=$TAG DOCKER_REGISTRY=$REGISTRY docker compose -f docker/docker-compose.yml build

# Build sandbox images
echo "Building sandbox images..."
make sandbox TAG=$TAG DOCKER_REGISTRY=$REGISTRY
make sandbox-slim TAG=$TAG DOCKER_REGISTRY=$REGISTRY

echo ""
echo "=================================================="
echo "Pushing images to $REGISTRY..."
echo "=================================================="

# Function to push image
push_image() {
    local image=$1
    local tag=$2
    echo "Pushing $image:$tag"
    docker push $image:$tag
}

# Push all DSB images
push_image "${REGISTRY}dsb/rust-base" "$TAG"
push_image "${REGISTRY}dsb/python-base" "$TAG"
push_image "${REGISTRY}dsb/node-base" "$TAG"
push_image "${REGISTRY}dsb/runtime-base" "$TAG"
push_image "${REGISTRY}dsb/sandbox-base" "$TAG"
push_image "${REGISTRY}dsb/server" "$TAG"
push_image "${REGISTRY}dsb/mcp-server" "$TAG"
push_image "${REGISTRY}dsb/dashboard" "$TAG"
push_image "${REGISTRY}dsb/ssh-gateway" "$TAG"
push_image "${REGISTRY}dsb/sandbox" "$TAG"
push_image "${REGISTRY}dsb/sandbox-slim" "$TAG"

echo ""
echo "=================================================="
echo "All images built and pushed successfully!"
echo "=================================================="
echo ""
echo "To use these images, update your environment:"
echo "  export TAG=$TAG"
echo "  export DOCKER_REGISTRY=$REGISTRY"
echo ""
echo "Or update .env:"
echo "  DOCKER_REGISTRY=$REGISTRY"
