#!/bin/bash
# DSB Logs Script
# View service logs

if [ -z "$1" ]; then
    # Show all logs
    docker compose logs --tail=100 -f
else
    # Show specific service logs
    docker compose logs --tail=100 -f "$1"
fi
