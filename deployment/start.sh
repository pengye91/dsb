#!/bin/bash
# DSB Start Script
# Validates configuration and starts services

set -e

echo "🚀 DSB Deployment Starter"
echo "========================="

# Check configuration files exist
if [ ! -f .env ]; then
    echo "❌ Missing .env file"
    echo "   Run: cp .env.example .env"
    echo "   Then edit .env to set your ports"
    exit 1
fi

if [ ! -f dsb.yaml ]; then
    echo "❌ Missing dsb.yaml file"
    echo "   Run: cp dsb.yaml.example dsb.yaml"
    echo "   Then edit dsb.yaml to set API keys and database password"
    exit 1
fi

# Check for required values in dsb.yaml
echo "🔍 Checking dsb.yaml configuration..."
if grep -E -q "api_key: (null|\"\")" dsb.yaml; then
    echo "❌ server.api_key is not set in dsb.yaml"
    echo "   Generate with: openssl rand -hex 32"
    exit 1
fi

if grep -E -q "admin_api_key: (null|\"\")" dsb.yaml; then
    echo "❌ server.admin_api_key is not set in dsb.yaml"
    echo "   Generate with: openssl rand -hex 32"
    exit 1
fi

if grep -E -q "password: (null|\"\")" dsb.yaml; then
    echo "❌ database.password is not set in dsb.yaml"
    echo "   Generate with: openssl rand -hex 16"
    exit 1
fi

echo "✅ Configuration looks good"

# Pull latest images
echo ""
echo "📥 Pulling latest images..."
# docker compose pull

# Start services
echo ""
echo "🚀 Starting services..."
docker compose up -d

# Wait for health checks
echo ""
echo "⏳ Waiting for services to be ready..."
sleep 10

# Show status
echo ""
echo "📊 Service Status:"
docker compose ps

# Get ports from .env
API_PORT=$(grep DSB_SERVER_HOST_PORT .env | cut -d= -f2 || echo 8080)
DASHBOARD_PORT=$(grep DSB_DASHBOARD_HOST_PORT .env | cut -d= -f2 || echo 3001)

echo ""
echo "✅ DSB is starting up!"
echo ""
echo "   Dashboard: http://localhost:${DASHBOARD_PORT}"
echo "   API:       http://localhost:${API_PORT}"
echo ""
echo "   View logs:  ./logs.sh"
echo "   Stop:       ./stop.sh"
