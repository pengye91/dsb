# DSB (Distributed Sandboxes)

DSB is a secure, isolated sandbox manager built on Docker. It provides a REST API,
WebSocket terminal/VNC access, an MCP server for AI agent integration, a CLI tool,
a web dashboard, and a Python SDK.

**License:** Dual-licensed under Apache 2.0 and AGPL 3.0. The Python SDK is MIT.

---

## Ecosystem Overview

| Component | Language | Purpose |
|-----------|----------|---------|
| **DSB Server** | Rust | Core REST API, sandbox lifecycle, static files |
| **Dashboard** | React/TypeScript | Web UI for sandbox management |
| **MCP Server** | Rust | AI agent integration via Model Context Protocol |
| **SSH Gateway** | Rust | SSH proxy into sandbox containers |
| **Static Server** | Rust | Standalone static file hosting |
| **Python SDK** | Python | Programmatic client library |
| **CLI** | Rust | Command-line tool (`dsb create`, `dsb exec`, etc.) |

---

## Quick Start

### Prerequisites

- **Rust** 1.88+ ([rustup](https://rustup.rs/))
- **Docker** (for sandbox backend)
- **PostgreSQL** 16+ (optional, for persistence)

### Build from Source

```bash
# Clone the repository
git clone https://github.com/pengye91/dsb.git && cd dsb

# Build the dev binary (faster compilation)
cargo build --profile dev-build --bin dsb

# Build release binary
cargo build --release --bin dsb

# Build with Kubernetes backend support
cargo build --release --bin dsb --features kubernetes
```

### Run Tests

```bash
# Run unit tests (no external dependencies)
cargo test --lib -p dsb

# Run doc-tests
cargo test --doc -p dsb

# Run all checks (clippy, tests, docs)
cargo clippy --workspace -- -D warnings
cargo test -p dsb
cargo doc --workspace
```

### Docker Build

DSB uses [`cargo-chef`](https://github.com/LukeMathWalker/cargo-chef) for optimized
Docker layer caching. Dependencies are compiled in a separate layer, so source code
changes do not invalidate the dependency cache.

```bash
# Build all services
docker compose -f docker/docker-compose.yml build

# Build just the server
docker build -f docker/Dockerfile -t dsb/server:latest .

# Build with China mirrors
make config-china
docker compose -f docker/docker-compose.yml build
```

### Configuration

DSB uses a two-tier configuration system:

**Infrastructure** (Docker Compose):
- **`.env`** — ports, registry mirrors, versions
- `make config-default` — public mirrors
- `make config-china` — China-accessible mirrors

**Application** (DSB Server):
- **`dsb.yaml`** — logic, limits, database connection
- Template: `dsb.yaml.example`

See `docs/config/README.md` for full details.

---

## Creating Your First Sandbox

> **Security Note:** In production, configure `DSB_API_KEY` in `.env` or `dsb.yaml`
> before exposing the API. Without it, the server allows unauthenticated access.

```bash
# Create a sandbox with real-time progress
dsb create -i python:3.12 -n myapp

# Create with port mappings
dsb create -i nginx:alpine -p 8080:80 -n webserver

# Execute a command inside a running sandbox
dsb exec -n myapp -- python -c "print('hello')"

# List all sandboxes
dsb list

# Stop and remove a sandbox
dsb stop -n myapp
dsb delete -n myapp
```

---

## Documentation

- **[Architecture](docs/ARCHITECTURE.md)** — System design, component breakdown, request flows
- **[API Documentation](docs/api/)** — REST API reference
- **[Configuration Guide](docs/config/)** — Environment variables, `dsb.yaml` reference
- **[Deployment](deployment/)** — Helm charts, Docker Compose, K8s manifests

---

## License

The core project is dual-licensed under **Apache 2.0** and **AGPL 3.0**.
The Python SDK is licensed under **MIT**.
