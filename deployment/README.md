# DSB Deployment

Production deployment package for DSB (Docker Sandbox). Deploy DSB without building from source using pre-built Docker images.

## Quick Start

```bash
# 1. Configure
cp .env.example .env
cp dsb.yaml.example dsb.yaml

# 2. Set API keys (required!)
# Edit dsb.yaml and set:
#   - server.api_key
#   - server.admin_api_key
#   - database.password

# 3. Start
./start.sh
```

## Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+
- 4GB RAM minimum (8GB recommended)
- 10GB free disk space

## Configuration

### Step 1: Copy Example Files

```bash
# International (default)
make setup

# China region (optimized mirrors)
make setup-china
```

### Step 2: Edit dsb.yaml (REQUIRED)

Set these required values in `dsb.yaml`:

```yaml
server:
  api_key: "your-api-key-here"           # Generate: openssl rand -hex 32
  admin_api_key: "your-admin-key-here"   # Generate: openssl rand -hex 32

database:
  password: "your-db-password"           # Generate: openssl rand -hex 16
```

**Never commit dsb.yaml to git** - it contains secrets!

### Step 3: Customize Ports (Optional)

Edit `.env` to change host ports if you have conflicts:

```bash
DSB_SERVER_HOST_PORT=8080      # Change if port 8080 is taken
DSB_DASHBOARD_HOST_PORT=3001   # Change if port 3001 is taken
```

### Step 4: Configure Base Path (Optional — for reverse proxy deployments)

When deploying behind a reverse proxy that uses a URL prefix (e.g., enterprise nginx maps `/sandboxes/*` → this host), set `DSB_BASE_PATH` in `.env` to match that prefix:

```bash
DSB_BASE_PATH=/              # Standalone (default) — no prefix
DSB_BASE_PATH=/dsb           # e.g., enterprise nginx maps /dsb/*
DSB_BASE_PATH=/tools/dsb     # any URL path prefix works
```

This is a **runtime setting** — change it and restart, no rebuild needed. The external proxy must strip the prefix before forwarding to the dashboard.

```
Browser → reverse proxy (/sandboxes/*) → strips prefix → dashboard:3001 (/)
```

## Management Commands

```bash
# Start services
make start
# or
./start.sh

# View logs
make logs
# or
./logs.sh
# or specific service: ./logs.sh dsb-server

# Stop services
make stop
# or
./stop.sh

# Restart services
make restart

# Update images
make pull

# Check status
make status
```

## Services

| Service | Image | Port | Description |
|---------|-------|------|-------------|
| dashboard | dsb/dashboard:latest | 3001 | Web UI + reverse proxy (single entry point) |
| dsb-server | dsb/server:latest | - | Main API server (internal only) |
| dsb-mcp-server | dsb/mcp-server:latest | - | MCP protocol interface (internal only) |
| ssh-gateway | dsb/ssh-gateway:latest | 2223 | SSH proxy |
| postgres | postgres:18-alpine | - | Database |
| searxng | searxng/searxng:latest | 8888 | Search engine |

### Traffic Flow

The dashboard container runs nginx that serves the SPA and proxies all backend traffic:

```
Browser / Enterprise Nginx
    │
    ▼ port 3001 (or 80 on EC2)
dashboard nginx
    ├── /           → static SPA files
    ├── /api/*      → dsb-server:8080
    ├── /vnc/*      → dsb-server:8080 (WebSocket)
    ├── /terminal/* → dsb-server:8080 (WebSocket)
    ├── /static/*   → dsb-server:8080
    └── /mcp        → dsb-mcp-server:3000 (SSE)
```

## Data Persistence

Data is stored in Docker named volumes:

| Volume | Purpose |
|--------|---------|
| dsb-postgres-data | Database files |
| dsb-static-files | User uploads |
| dsb-searxng-data | Search index |

**Backup:**
```bash
# Backup database
docker exec dsb-postgres pg_dump -U postgres dsb > backup.sql

# Backup static files
docker run --rm -v dsb-static-files:/data -v $(pwd):/backup alpine tar czf /backup/static-files.tar.gz -C /data .
```

## Security Considerations

### Docker Socket Access
DSB server mounts `/var/run/docker.sock` to create sandbox containers. This grants significant host access. **Run DSB on a dedicated host/VM**, not shared with other services.

### API Keys
Generate strong API keys:
```bash
openssl rand -hex 32
```

### Network Security
- Internal services (postgres, dsb-server, dsb-mcp-server) are NOT exposed to the host
- Only dashboard (3001), ssh-gateway (2223), searxng (8888) are exposed
- All HTTP traffic (API, MCP, WebSocket) flows through the dashboard's nginx
- For SSL/TLS, place a reverse proxy (enterprise nginx, traefik) in front of the dashboard port

## China Region

For users in China, use the China-optimized configuration:

```bash
make setup-china
```

This uses domestic Docker mirrors for faster image pulls.

## Troubleshooting

### Services won't start
```bash
# Check configuration
make config

# Check logs
./logs.sh

# Check specific service
./logs.sh dsb-server
```

### Port conflicts
Edit `.env` and change the `*_HOST_PORT` values, then restart.

### Database connection errors
Ensure `database.password` is set in `dsb.yaml` and matches any existing database.

## Sandbox Images

The default sandbox image is `dsb/sandbox:latest` (full image: sandbox daemon, browser tooling for MCP/web E2E). For a smaller image use `dsb/sandbox-minimal:latest` or `dsb/sandbox-slim:latest`. To use custom sandbox images:

1. Build your sandbox: `make sandbox` (in project source)
2. Push to registry or load on host
3. Update `sandbox.default_image` in `dsb.yaml`

## Updating

```bash
# Pull latest images
make pull

# Restart with new images
make restart
```

## Uninstall

```bash
# Stop and remove containers
make stop

# Remove volumes (WARNING: deletes all data!)
docker volume rm dsb-postgres-data dsb-static-files dsb-searxng-data
```

## Support

- Documentation: https://github.com/dsb/docs
- Issues: https://github.com/dsb/issues
