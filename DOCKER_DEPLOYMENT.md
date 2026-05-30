# DSB Docker Deployment Guide

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Prerequisites](#prerequisites)
4. [Quick Start](#quick-start)
5. [Configuration](#configuration)
6. [Service Management](#service-management)
7. [Development Workflows](#development-workflows)
8. [Testing VNC Functionality](#testing-vnc-functionality)
9. [Troubleshooting](#troubleshooting)
10. [Migration from Host-Based](#migration-from-host-based)
11. [Production Considerations](#production-considerations)

## Overview

DSB (Development Sandbox) can now run entirely in Docker containers. This deployment method solves the VNC proxy issue on macOS Docker Desktop by running the DSB server on the same Docker network as sandbox containers, enabling direct container-to-container communication.

### Key Benefits

- ✅ **VNC Proxy Works**: Direct network communication between DSB server and sandboxes
- ✅ **No Port Exhaustion**: Sandboxes don't need host port mappings
- ✅ **Single Command Deployment**: `docker compose up -d`
- ✅ **Isolated Environment**: All services in containers
- ✅ **Easy Management**: Start, stop, and monitor with docker-compose
- ✅ **Development Friendly**: Can still run locally if needed

## Architecture

### Container Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Docker Host                              │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │         dsb_dsb-network (172.28.0.0/16)             │   │
│  │                                                      │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────┐ │   │
│  │  │  dsb-server  │  │  sandbox-1   │  │sandbox-2 │ │   │
│  │  │  :8080       │  │  :5901 (VNC) │  │ :5901    │ │   │
│  │  └──────┬───────┘  └──────┬───────┘  └────┬─────┘ │   │
│  │         │                  │                │       │   │
│  │         └──────────────────┴────────────────┘       │   │
│  │                  (Docker DNS + Direct TCP)          │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  Exposed Ports:                                             │
│  - 3001 → Dashboard (nginx)                                 │
│  - 8080 → DSB Server API                                    │
│  - 2222 → SSH Gateway                                       │
└───────────────────────────────────────────────────────────────┘
```

### Services

| Service | Container Name | Port | Description |
|---------|----------------|------|-------------|
| DSB Server | `dsb-server` | 8080 | Main API server, manages sandboxes |
| Dashboard | `dsb-dashboard` | 3001 | React web UI (nginx) |
| PostgreSQL | `dsb-postgres` | 5432 | Database (internal only) |
| Redis | `dsb-redis` | 6379 | Cache (internal only) |
| SSH Gateway | `dsb-ssh-gateway` | 2222 | SSH access to sandboxes |

### Data Persistence

- **postgres-data**: PostgreSQL database volume
- **dsb-static-files**: Static file storage volume
- **Docker socket**: Mounted from host for container management

## Prerequisites

### Required Software

1. **Docker Desktop** (macOS/Windows) or **Docker Engine** (Linux)
   - Version 20.10 or later recommended
   - Verify: `docker --version`

2. **Docker Compose** (V2)
   - Comes with Docker Desktop
   - Verify: `docker compose version`

3. **Git** (for cloning repository)
   - Verify: `git --version`

### System Requirements

- **CPU**: 4+ cores recommended
- **Memory**: 8GB+ RAM recommended
- **Disk**: 20GB+ free space
- **Network**: Ports 3001, 8080, 2222 must be available

## Quick Start

### 1. Clone Repository

```bash
git clone <repository-url>
cd dsb
```

### 2. Configure Environment

```bash
# Copy Docker environment file
cp .env.docker .env.local

# Edit if needed (optional)
nano .env.local
```

### 3. Start Services

```bash
# Start all services
docker compose up -d

# Check status
docker compose ps
```

### 4. Verify Deployment

```bash
# Check DSB server health
curl http://localhost:8080/health

# Expected output:
# {"status":"healthy"}

# View logs
docker compose logs -f dsb-server
```

### 5. Access Dashboard

Open browser to: **<http://localhost:3001>**

Default admin API key: `dsb-admin-key` (set in `.env.docker`)

## Configuration Architecture

### Two-Tier Configuration System

DSB separates infrastructure configuration from application configuration:

#### Tier 1: Docker Compose Configuration (`.env`)

- **Purpose**: Configure Docker Compose infrastructure
- **Loaded by**: `docker compose` command (automatic)
- **Contains**: Ports, image versions, container names, build mirrors
- **Template**: `.env.example`
- **Overrides**: `.env.local` (optional, for local development)

#### Tier 2: Application Configuration

DSB configuration system supports **two formats**:

**Option A: .env format** (`.env.docker`)

- **File**: `.env.docker` (mounted at `/config/.env:ro`)
- **Format**: Flat `DSB_SECTION__KEY=value` syntax
- **Template**: `.env.docker.example`

**Option B: YAML format** (`dsb.yaml`) ⭐ **Recommended**

- **File**: `dsb.yaml` (mounted at `/config/dsb.yaml:ro`)
- **Format**: Nested YAML structure (more readable)
- **Template**: `dsb.yaml.example`

**Choose one format** - YAML is recommended for production deployments due to its structured format and better readability.

### Configuration Loading Priority

1. Default values (from Rust struct defaults)
2. `.env` file (if found at `/config/.env`)
3. YAML file (`dsb.yaml` or `dsb.yml`, if found)
4. Environment variables (DSB_*)
5. CLI arguments (highest priority)

Note: You can use both `.env.docker` and `dsb.yaml` together - they will be merged.

### Quick Start

#### Using YAML format (recommended)

```bash
# 1. Copy templates
cp .env.example .env
cp dsb.yaml.example dsb.yaml

# 2. Customize values
nano .env      # Edit ports, mirrors
nano dsb.yaml  # Edit database, API keys

# 3. Start services
docker compose up -d
```

#### Using .env format

```bash
# 1. Copy templates
cp .env.example .env
cp .env.docker.example .env.docker

# 2. Customize values
nano .env          # Edit ports, mirrors
nano .env.docker   # Edit database, API keys

# 3. Start services
docker compose up -d
```

### Configuration File Examples

#### `.env` (Docker Compose infrastructure)

```bash
# Ports
DSB_SERVER_PORT=8080
DSB_DASHBOARD_PORT=3001

# Versions
POSTGRES_VERSION=18-bookworm
REDIS_VERSION=7-alpine

# Mirrors (China)
USE_CHINA_MIRRORS=true
DOCKER_REGISTRY=docker.io
CARGO_REGISTRY=https://mirrors.aliyun.com/crates.io-index/
```

#### `dsb.yaml` (Application configuration)

```yaml
server:
  bind_address: "0.0.0.0:8080"
  require_auth: true
  admin_api_key: "dsb-admin-key"

database:
  host: "postgres"
  port: 5432
  name: "dsb"
  user: "postgres"
  password: "postgres"

docker:
  host: "unix:///var/run/docker.sock"
  network: "dsb_dsb-network"

logging:
  level: "info"
  format: "pretty"
  ansi: true
```

### Docker Compose Overrides

Create `docker-compose.override.yml` for local customization:

```yaml
services:
  dsb-server:
    environment:
      - RUST_LOG=debug
    volumes:
      - ./src:/app/src:ro  # Live code reload

  postgres:
    ports:
      - "5433:5432"  # Expose to host
```

## Service Management

### Start Services

```bash
# Start all services
docker compose up -d

# Start specific services
docker compose up postgres redis -d

# Start with rebuild
docker compose up -d --build
```

### Stop Services

```bash
# Stop all services (preserves data)
docker compose stop

# Stop and remove containers (preserves volumes)
docker compose down

# Stop and remove everything (including volumes!)
docker compose down -v
```

### View Logs

```bash
# All services
docker compose logs -f

# Specific service
docker compose logs -f dsb-server

# Last 100 lines
docker compose logs --tail=100 dsb-server
```

### Restart Services

```bash
# Restart specific service
docker compose restart dsb-server

# Force recreate
docker compose up -d --force-recreate dsb-server
```

### Update Services

```bash
# Pull latest images
docker compose pull

# Rebuild with local changes
docker compose build dsb-server
docker compose up -d dsb-server

# Full rebuild
docker compose build --no-cache
docker compose up -d
```

## Development Workflows

### Option 1: Fully Containerized (Recommended)

```bash
# Edit code locally
vim src/docker/manager.rs

# Rebuild and restart
docker compose build dsb-server
docker compose up -d dsb-server

# View logs
docker compose logs -f dsb-server
```

### Option 2: Hybrid (DB in Docker, App Local)

```bash
# Start only database
docker compose up postgres redis -d

# Run DSB locally
RUST_LOG=DEBUG cargo run --bin dsb -- server --env-file .env
```

### Option 3: Full Local Development

```bash
# Stop all containers
docker compose down

# Run locally as before
RUST_LOG=DEBUG cargo run --bin dsb -- server --env-file .env
```

### Live Code Development

Create `docker-compose.dev.yml`:

```yaml
services:
  dsb-server:
    volumes:
      - ./src:/build/src:ro
      - ./Cargo.toml:/build/Cargo.toml:ro
      - ./Cargo.lock:/build/Cargo.lock:ro
    command: ["cargo", "run", "--release", "--bin", "dsb", "server", "--env-file", "/config/.env"]
```

Run with: `docker compose -f docker-compose.dev.yml up dsb-server`

## Testing VNC Functionality

### 1. Create a Sandbox

**Via Dashboard**:

1. Open <http://localhost:3001>
2. Click "New Sandbox"
3. Select image: `dsb/sandbox:latest`
4. Click "Create"

**Via API**:

```bash
curl -X POST http://localhost:8080/sandboxes \
  -H "X-API-Key: dsb-admin-key" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-vnc-sandbox",
    "image": "dsb/sandbox:latest",
    "port_mappings": [
      {"container_port": 5901, "host_port": 5901, "protocol": "tcp"}
    ]
  }'
```

### 2. Start the Sandbox

**Via Dashboard**: Click "Start" button

**Via API**:

```bash
curl -X POST http://localhost:8080/sandboxes/{sandbox_id}/start \
  -H "X-API-Key: dsb-admin-key"
```

### 3. Access VNC

**Via Dashboard**:

1. Navigate to sandbox details page
2. Click "VNC" tab
3. Connection should establish automatically

**Verify Connection**:

```bash
# Check container is on DSB network
docker network inspect dsb_dsb-network | grep sandbox

# From DSB container, test TCP connection to VNC
docker exec dsb-server nc -zv <container_name> 5901

# Should show:
# Connection to <container_name> 5901 port [tcp/*] succeeded!
```

### Expected Logs

```bash
docker compose logs -f dsb-server | grep -i vnc

# Should see:
# [VNC] WebSocket VNC connection requested for sandbox: {id}
# Container {id} connected to network: dsb_dsb-network
# Connecting to VNC server at {container_id}:5901
# WebSocket VNC connection established for sandbox: {id}
```

## Troubleshooting

### Container Won't Start

**Problem**: Container exits immediately

```bash
# Check logs
docker compose logs dsb-server

# Check if port is already in use
lsof -i :8080

# Solution: Kill conflicting process or change port
```

### Database Connection Failed

**Problem**: DSB server can't connect to PostgreSQL

```bash
# Check PostgreSQL is running
docker compose ps postgres

# Check logs
docker compose logs postgres

# Test connection from DSB container
docker exec dsb-server nc -zv postgres 5432

# Solution: Ensure both containers are on same network
docker network inspect dsb_dsb-network
```

### VNC Connection Fails

**Problem**: VNC shows "Connection closed (code: 1005)"

**Diagnosis**:

```bash
# 1. Check sandbox is running
docker compose ps | grep sandbox

# 2. Check sandbox is on DSB network
docker inspect <sandbox_id> | grep -A 10 "Networks"

# 3. Check VNC server is running in container
docker exec <sandbox_id> ps aux | grep Xvnc

# 4. Test network connectivity
docker exec dsb-server ping <sandbox_id>

# 5. Test TCP connection to VNC
docker exec dsb-server nc -zv <sandbox_id> 5901
```

**Common Solutions**:

1. **Sandbox not on network**:

   ```bash
   # Manually connect to network
   docker network connect dsb_dsb-network <sandbox_id>
   ```

2. **VNC server not running in container**:

   ```bash
   # Check container logs
   docker logs <sandbox_id>

   # VNC should be started by supervisord
   docker exec <sandbox_id> supervisorctl status
   ```

3. **Network mismatch**:

   ```bash
   # Verify network name matches config
   grep NETWORK .env.docker
   docker network ls | grep dsb
   ```

### Out of Memory

**Problem**: Container killed due to OOM

```bash
# Check container memory usage
docker stats

# Increase Docker Desktop memory limit (macOS/Windows)
# Settings → Resources → Memory → 8GB+

# Or limit specific services in docker-compose.yml:
services:
  dsb-server:
    deploy:
      resources:
        limits:
          memory: 2G
```

### Permission Denied on Docker Socket

**Problem**: DSB server can't access Docker socket

```bash
# Check socket permissions
ls -l /var/run/docker.sock

# Should show root:root with rw-rw-rw-

# In container, check socket exists
docker exec dsb-server ls -l /var/run/docker.sock

# If missing, check docker-compose.yml volume mount
# - /var/run/docker.sock:/var/run/docker.sock:ro
```

### Dashboard Can't Reach API

**Problem**: Dashboard shows connection errors

```bash
# Check both services are running
docker compose ps dsb-server dashboard

# Test API from dashboard container
docker exec dsb-dashboard curl http://dsb-server:8080/health

# Check nginx proxy configuration
docker exec dsb-dashboard cat /etc/nginx/nginx.conf

# Solution: Ensure both on same network
docker network inspect dsb_dsb-network | grep dsb-dashboard
```

### Build Errors

**Problem**: Docker build fails

```bash
# Clear build cache
docker builder prune -a

# Rebuild without cache
docker compose build --no-cache dsb-server

# Check disk space
df -h

# If out of space:
docker system prune -a --volumes
```

## Migration from Host-Based

### Before Migration

1. **Backup Current Data**:

   ```bash
   # Export database
   pg_dump postgresql://postgres:postgres@localhost:5433/dsb > dsb-backup.sql

   # Backup static files
   tar -czf static-files-backup.tar.gz /var/lib/dsb/static-files
   ```

2. **Stop Host Services**:

   ```bash
   # Stop any running DSB processes
   pkill -f dsb

   # Or if using systemd:
   # systemctl stop dsb
   ```

### Migration Steps

#### 1. Create Docker Environment

```bash
# Copy Docker environment template
cp .env.docker .env.local

# Edit to match your setup
vim .env.local
```

#### 2. Migrate Database

```bash
# Start PostgreSQL only
docker compose up postgres -d

# Wait for it to be ready
docker compose logs -f postgres
# Wait for: "database system is ready to accept connections"

# Import backup
docker exec -i dsb-postgres psql -U postgres dsb < dsb-backup.sql

# Verify data
docker exec -it dsb-postgres psql -U postgres dsb
\dt
SELECT COUNT(*) FROM sandboxes;
\q
```

#### 3. Migrate Static Files

```bash
# Start DSB server (creates volume)
docker compose up dsb-server -d

# Copy files to volume
docker cp /path/to/static-files-backup/. dsb-server:/var/lib/dsb/static-files/

# Fix permissions
docker exec dsb-server chown -R dsb:dsb /var/lib/dsb/static-files
```

#### 4. Verify Migration

```bash
# Start all services
docker compose up -d

# Check health
curl http://localhost:8080/health

# List sandboxes
curl -H "X-API-Key: dsb-admin-key" http://localhost:8080/sandboxes

# Test VNC access (create new sandbox and test)
```

#### 5. Update Client Configuration

Update any scripts/tools to use new endpoints:

```bash
# Old (host-based)
export DSB_API_URL="http://localhost:8080"

# New (Docker - same URL!)
export DSB_API_URL="http://localhost:8080"

# Dashboard URL changed
export DSB_DASHBOARD_URL="http://localhost:3001"
```

### Rollback Plan

If you need to rollback:

```bash
# Stop Docker services
docker compose down

# Export data from Docker
docker exec dsb-postgres pg_dump -U postgres dsb > dsb-docker-backup.sql
docker cp dsb-server:/var/lib/dsb/static-files ./static-files-docker-backup

# Start host services again
RUST_LOG=INFO cargo run --bin dsb -- server --env-file .env

# Import data if needed
psql postgresql://postgres:postgres@localhost:5433/dsb < dsb-docker-backup.sql
```

## Production Considerations

### Security

#### Docker Socket Mount

⚠️ **Warning**: Mounting `/var/run/docker.sock` grants root-equivalent access to the host.

**Mitigation**:

- Use read-only mount (`:ro`)
- Run DSB container as non-root user (UID 1001)
- Implement proper API key authentication
- Restrict network access with firewalls

**Production Alternatives**:

- Rootless Docker
- Separate Docker-in-Docker VM
- Docker context isolation

#### API Keys

```bash
# Generate strong random keys
openssl rand -hex 32

# Set in .env.docker
DSB_SERVER__ADMIN_API_KEY=<generated-key>
DSB_SERVER__API_KEY=<generated-key>
```

#### Network Isolation

```yaml
# In docker-compose.prod.yml
services:
  dsb-server:
    networks:
      - dsb-internal
    # Don't expose ports directly
    # Use reverse proxy (Traefik/Nginx)

  traefik:
    # Handles SSL and routing
    ports:
      - "443:443"
```

### Resource Limits

```yaml
# Add to docker-compose.yml
services:
  dsb-server:
    deploy:
      resources:
        limits:
          cpus: '2'
          memory: 2G
        reservations:
          cpus: '0.5'
          memory: 512M

  postgres:
    deploy:
      resources:
        limits:
          memory: 1G
```

### Monitoring

#### Health Checks

```bash
# All services
docker compose ps

# DSB server health endpoint
curl http://localhost:8080/health

# Container health
docker inspect dsb-server | grep -A 10 Health
```

#### Metrics Collection

```bash
# Container stats
docker stats

# Resource usage
docker top dsb-server

# Logs aggregation
docker compose logs --since 1h dsb-server > dsb-server.log
```

### Backup Strategy

#### Database Backups

```bash
# Automated backup script
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="/backups/postgres"

mkdir -p $BACKUP_DIR

docker exec dsb-postgres pg_dump -U postgres dsb | gzip > \
  $BACKUP_DIR/dsb_$DATE.sql.gz

# Keep last 7 days
find $BACKUP_DIR -name "dsb_*.sql.gz" -mtime +7 -delete
```

#### Volume Backups

```bash
# Backup static files volume
docker run --rm -v dsb_dsb-static-files:/data -v \
  $(pwd):/backup ubuntu tar czf /backup/static-files-backup.tar.gz /data

# Backup database volume
docker run --rm -v dsb_postgres-data:/data -v \
  $(pwd):/backup ubuntu tar czf /backup/postgres-backup.tar.gz /data
```

### Scaling

#### Horizontal Scaling

DSB server is stateless (except for database connections). Multiple instances can run behind a load balancer:

```yaml
# docker-compose.scale.yml
services:
  dsb-server:
    deploy:
      replicas: 3

  # Add load balancer
  nginx:
    image: nginx:alpine
    ports:
      - "8080:80"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
```

#### Database Scaling

For production, consider managed database services:

- AWS RDS
- Google Cloud SQL
- Azure Database for PostgreSQL

### SSL/TLS

#### Using Traefik

```yaml
# docker-compose.yml
services:
  traefik:
    image: traefik:v2.10
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - ./traefik.yml:/etc/traefik/traefik.yml

  dsb-server:
    labels:
      - "traefik.enable=true"
      - "traefik.http.routers.dsb.rule=Host(`dsb.example.com`)"
      - "traefik.http.routers.dsb.tls=true"
      - "traefik.http.routers.dsb.tls.certresolver=letsencrypt"
```

## Advanced Usage

### Custom Sandbox Networks

If you want sandboxes on isolated networks:

```yaml
# .env.docker
DSB_DOCKER__NETWORK=dsb_dsb-network
DSB_DOCKER__SANDBOX_NETWORK_PREFIX=sandbox-

# In src/docker/manager.rs, create per-sandbox networks
let sandbox_network = format!("{}{}", prefix, sandbox_id);
docker.create_network(&create_network_config(&sandbox_network)).await?;
```

### Multi-Host Deployment

For multi-host scenarios, use Docker Swarm:

```bash
# Initialize swarm
docker swarm init

# Deploy stack
docker stack deploy -c docker-compose.yml dsb

# Scale services
docker service scale dsb_dsb-server=3
```

### Custom Images

```yaml
# docker-compose.yml
services:
  dsb-server:
    build:
      context: .
      dockerfile: Dockerfile
      args:
        - RUST_VERSION=1.75
        - UBUNTU_VERSION=24.04
```

### CI/CD Integration

```yaml
# .github/workflows/docker.yml
name: Docker Build

on:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Build images
        run: |
          docker compose build

      - name: Run tests
        run: |
          docker compose up -d
          docker compose exec dsb-server cargo test

      - name: Push to registry
        run: |
          echo ${{ secrets.DOCKER_PASSWORD }} | docker login -u ${{ secrets.DOCKER_USERNAME }} --password-stdin
          docker compose push
```

## Support

### Getting Help

- **Documentation**: Check this guide first
- **Issues**: GitHub Issues
- **Logs**: Always include logs when reporting issues
- **Environment**: Specify Docker version, OS, and configuration

### Useful Commands

```bash
# Clean everything
docker compose down -v
docker system prune -a

# Debug container startup
docker run -it --rm dsb-server bash

# Check network connectivity
docker exec dsb-server ping postgres
docker exec dsb-server nc -zv postgres 5432

# Inspect container
docker inspect dsb-server | less

# Follow logs with timestamps
docker compose logs -f --timestamps dsb-server
```

### Performance Tuning

```bash
# Check container resource usage
docker stats --no-stream

# Limit container resources
docker update dsb-server --memory="2g" --cpus="2"

# Clean up unused resources
docker system prune -f
```

---

**Last Updated**: 2025-01-15
**Version**: 1.0.0
**Maintainer**: DSB Team
