# Docker Quick Reference

## Essential Commands

### Start & Stop

```bash
# Start all services
docker compose up -d

# Stop all services
docker compose stop

# Stop and remove containers
docker compose down

# Force stop and remove everything
docker compose down -v
```

### Logs & Status

```bash
# View all logs
docker compose logs -f

# View specific service logs
docker compose logs -f dsb-server

# Check service status
docker compose ps

# Resource usage
docker stats
```

### Rebuild & Restart

```bash
# Rebuild and restart
docker compose up -d --build

# Force rebuild without cache
docker compose build --no-cache
docker compose up -d

# Restart specific service
docker compose restart dsb-server
```

### Database

```bash
# Access PostgreSQL
docker exec -it dsb-postgres psql -U postgres dsb

# Backup database
docker exec dsb-postgres pg_dump -U postgres dsb > backup.sql

# Restore database
docker exec -i dsb-postgres psql -U postgres dsb < backup.sql
```

### Troubleshooting

```bash
# Container shell access
docker exec -it dsb-server bash

# Check network connectivity
docker exec dsb-server ping postgres
docker exec dsb-server nc -zv postgres 5432

# Inspect container
docker inspect dsb-server | less

# Check container processes
docker top dsb-server
```

## URLs

| Service | URL | Credentials |
|---------|-----|-------------|
| Dashboard | <http://localhost:3001> | Configured in `dsb.yaml` |
| API | <http://localhost:8080> | Configured in `dsb.yaml` |
| Health Check | <http://localhost:8080/health> | None |
| SSH Gateway | ssh://localhost:2223 | Your SSH keys |

**Note**: Default ports can be customized via `.env` (see Configuration section).

## File Locations

| File | Purpose |
|------|---------|
| `.env.example` | Example docker-compose configuration |
| `.env.local` | Your custom configuration overrides (optional) |
| `docker/docker-compose.yml` | Service orchestration (uses env vars) |
| `Dockerfile` | DSB server container build |
| `dashboard/Dockerfile` | Dashboard container build |
| `DOCKER_DEPLOYMENT.md` | Full documentation |

## Configuration

### Customizing Ports and Settings

To customize ports, passwords, and other settings:

```bash
# Copy the example configuration
cp .env.example .env

# Edit .env with your values
nano .env

# Start services with your custom configuration
docker compose -f docker/docker-compose.yml up -d
```

Common customizations in `.env`:

```bash
# Change ports if defaults conflict
DSB_SERVER_PORT=9000
DSB_DASHBOARD_PORT=3001
DSB_SSH_GATEWAY_PORT=2223

# Change PostgreSQL password (recommended for production)
POSTGRES_PASSWORD=your_secure_password

# Change network subnet if there's a conflict
DSB_NETWORK_SUBNET=172.29.0.0/16
```

## Common Workflows

### Create Sandbox via API

```bash
curl -X POST http://localhost:8080/sandboxes \
  -H "X-API-Key: your-api-key" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-sandbox",
    "image": "dsb/sandbox:latest"
  }'
```

### List Sandboxes

```bash
curl -H "X-API-Key: your-api-key" \
  http://localhost:8080/sandboxes
```

### Start Sandbox

```bash
curl -X POST \
  -H "X-API-Key: your-api-key" \
  http://localhost:8080/sandboxes/{id}/start
```

### Access VNC

1. Open dashboard: <http://localhost:3001>
2. Navigate to sandbox
3. Click "VNC" tab

## Port Reference

| Port | Service | Internal | External | Configurable Via |
|------|---------|----------|----------|------------------|
| 3001 | Dashboard | 80 | 3001 | DSB_DASHBOARD_PORT |
| 8080 | DSB Server | 8080 | 8080 | DSB_SERVER_PORT |
| 2223 | SSH Gateway | 2222 | 2223 | DSB_SSH_GATEWAY_PORT |
| 5432 | PostgreSQL | 5432 | (internal) | - |
| 6379 | Redis | 6379 | (internal) | - |

**Note**: External ports can be changed via `.env` to avoid conflicts.

## Network

- **Network Name**: `dsb_dsb-network`
- **Subnet**: `172.28.0.0/16`
- **Driver**: bridge

All sandbox containers are automatically attached to this network for VNC proxy functionality.

## Environment Variables

Key variables in `dsb.yaml` and `.env`:

```yaml
# Database
database:
  host: postgres
  port: 5432
  name: dsb
  user: postgres
  password: your_secure_password

# Docker
docker:
  network: dsb_dsb-network

# Authentication
server:
  require_auth: true
  admin_api_key: your_admin_key
```

## Development

### Local Development (Host-Based)

```bash
# Stop Docker services
docker compose down

# Run locally
RUST_LOG=DEBUG cargo run --bin dsb -- server --env-file .env
```

### Hybrid (DB in Docker, App Local)

```bash
# Start only database
docker compose up postgres redis -d

# Run app locally
RUST_LOG=DEBUG cargo run --bin dsb -- server --env-file .env
```

## Cleanup

```bash
# Remove all containers and volumes
docker compose down -v

# Remove unused images
docker image prune -a

# Remove unused build cache
docker builder prune -a

# Full cleanup (⚠️ destructive)
docker system prune -a --volumes
```

## Tips

1. **First Startup**: Database migrations run automatically
2. **Logs**: Always check logs first when troubleshooting
3. **Network**: All containers must be on `dsb_dsb-network` for VNC to work
4. **Resources**: Increase Docker memory to 8GB+ if OOM errors occur
5. **Updates**: Run `docker compose pull` to get latest image updates

## Help

- Full Documentation: `DOCKER_DEPLOYMENT.md`
- Main README: `README.md`
- Issues: GitHub Issues
