#!/bin/bash
# DSB Stop Script
# Gracefully stops all services

echo "🛑 Stopping DSB services..."
docker compose down

echo "✅ Services stopped"
