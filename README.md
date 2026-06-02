# DSB (Distributed Sandboxes)

[![CI](https://img.shields.io/github/actions/workflow/status/pengye91/dsb/ci.yml?branch=main&label=CI&style=flat-square)](https://github.com/pengye91/dsb/actions/workflows/ci.yml)
[![License: Apache-2.0 OR AGPL-3.0](https://img.shields.io/badge/license-Apache--2.0%20OR%20AGPL--3.0-blue.svg)](./LICENSE)
[![Python SDK: MIT](https://img.shields.io/badge/Python%20SDK-MIT-yellow.svg)](./sdks/python/LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)
[![Discord](https://img.shields.io/badge/Discord-Join-7289da?logo=discord&style=flat-square)](https://discord.gg/your-invite)
[![GitHub stars](https://img.shields.io/github/stars/pengye91/dsb?style=flat-square)](https://github.com/pengye91/dsb/stargazers)
[![GitHub issues](https://img.shields.io/github/issues/pengye91/dsb?style=flat-square)](https://github.com/pengye91/dsb/issues)

> A secure, isolated sandbox manager for ephemeral container environments — REST API, WebSocket terminal/VNC, MCP server for AI agents, CLI, web dashboard, and Python SDK.

**License:** Dual-licensed under **Apache 2.0** and **AGPL 3.0** for the core. The Python SDK is **MIT**.

---

## Why DSB?

DSB gives you everything you need to run short-lived, isolated workloads:

- **🚀 One command, full stack** — `docker compose up` brings up the API server, dashboard, MCP server, SSH gateway, PostgreSQL, and SearXNG together.
- **🤖 AI-agent native** — First-class [Model Context Protocol](https://modelcontextprotocol.io) integration so Claude, GPT, and other LLM agents can spin up sandboxes, run code, browse the web, and manage files through 15 MCP tools.
- **🐳 Docker or Kubernetes** — Same `dsb` binary, swap backends with a config flag. Default is Docker; pass `--features kubernetes` for in-cluster pod execution.
- **🔌 WebSocket terminal & VNC** — Browser-based terminal (xterm.js) and VNC viewer built in; no separate noVNC deployment.
- **📦 Polyglot SDKs** — Rust crate, Python SDK (sync + async with circuit breaker / retry), or just `curl` against the REST API.
- **🔒 Production-grade auth** — Tiered API keys (admin + database-backed), bcrypt hashing, short-lived session tokens, ownership-based multi-tenancy.

---

## Ecosystem Overview

| Component | Language | Purpose |
|-----------|----------|---------|
| **DSB Server** | Rust | Core REST API, sandbox lifecycle, background tasks |
| **Dashboard** | React/TypeScript | Web UI for sandbox management |
| **MCP Server** | Rust | AI agent integration via Model Context Protocol |
| **SSH Gateway** | Rust | SSH proxy into sandbox containers |
| **Static Server** | Rust | Standalone static file hosting (workspace member) |
| **Python SDK** | Python | `dsb-sdk` — sync + async client library |
| **CLI** | Rust | `dsb create`, `dsb exec`, `dsb list`, `dsb ssh`, ... |

---

## Quick Start

### Prerequisites

- **Rust** 1.88+ ([rustup](https://rustup.rs/))
- **Docker** (for sandbox backend)
- **PostgreSQL** 16+ (18 recommended, optional for persistence)

### Option A: Pre-built Docker images (fastest)

```bash
git clone https://github.com/pengye91/dsb.git && cd dsb
cp .env.example .env
cp dsb.yaml.example dsb.yaml
# Edit dsb.yaml and set at least: server.api_key, server.admin_api_key, database.password
make dc-up
```

Dashboard: <http://localhost:3001> · API: <http://localhost:8080/health> · SSH: `localhost:2223`

For China-region users, run `make config-china` first to use domestic Docker mirrors.

### Option B: Build from source

```bash
git clone https://github.com/pengye91/dsb.git && cd dsb

# Build the dev binary (faster compilation)
cargo build --profile dev-build --bin dsb

# Build the release binary
cargo build --release --bin dsb

# Build with Kubernetes backend support
cargo build --release --bin dsb --features kubernetes
```

The `cargo-chef`-based Dockerfile gives optimized layer caching; dependencies are compiled in a separate layer so source changes don't invalidate the cache.

### Option C: Python SDK only

```bash
pip install dsb-sdk
```

```python
from dsb_sdk import DSBClient

with DSBClient(api_url="http://localhost:8080") as client:
    sandbox = client.sandbox.create(image="python:3.12", name="myapp")
    print(sandbox.id)
    client.sandbox.delete(sandbox.id)
```

---

## Creating Your First Sandbox

> **Security Note:** In production, configure `DSB_API_KEY` in `.env` or `dsb.yaml` before exposing the API. Without it, the server allows unauthenticated access.

```bash
# Create a sandbox with real-time progress
dsb create -i python:3.12 -n myapp

# Create with port mappings
dsb create -i nginx:alpine -p 8080:80 -n webserver

# Execute a command inside a running sandbox
dsb exec -n myapp -- python -c "print('hello')"

# SSH in for an interactive shell
dsb ssh -n myapp

# List all sandboxes
dsb list

# Stream resource stats
dsb stats -n myapp --stream

# Stop and remove a sandbox
dsb stop -n myapp
dsb delete -n myapp
```

---

## Configuration

DSB uses a two-tier configuration system. Never hardcode values — always go through `dsb::config::load()` (production) or `dsb::config::load_for_tests()` (tests).

| Tier | File | Purpose |
|------|------|---------|
| Infrastructure | `.env` | Ports, registry mirrors, image versions |
| Application | `dsb.yaml` | Logic, limits, database connection, API keys |

See [`docs/config/README.md`](docs/config/README.md) for the full reference and [`.env.example`](.env.example) / [`dsb.yaml.example`](dsb.yaml.example) for templates.

---

## Documentation

| Doc | What it covers |
|-----|----------------|
| [Architecture](ARCHITECTURE.md) | High-level system design, component breakdown, request flows, deployment topology |
| [Architecture Details](docs/architecture/) | Per-module reference (API, CLI, core, db, docker, web_terminal, utils, configuration, authentication) |
| [Configuration](docs/config/README.md) | Two-tier config, environment variables, `dsb.yaml` reference |
| [Testing](TESTING.md) | Test pyramid, fixtures, Docker-Compose testing, CI |
| [Roadmap](ROADMAP.md) | What's done, in progress, and planned |
| [Deployment](deployment/) | Helm chart, Docker Compose, K8s manifests |
| [Changelog](CHANGELOG.md) | Release notes |
| [Security](SECURITY.md) | Reporting vulnerabilities, security best practices |
| [Contributing](CONTRIBUTING.md) | How to contribute, dev setup, PR process |
| [Code of Conduct](CODE_OF_CONDUCT.md) | Community standards |

---

## Running Tests

```bash
# Unit tests (no external dependencies)
cargo test --lib -p dsb

# Workspace-wide clippy, tests, docs
cargo clippy --workspace -- -D warnings
cargo test -p dsb
cargo doc --workspace

# Full integration suite via Docker (uses testcontainers + compose)
make test
```

See [TESTING.md](TESTING.md) for the full test pyramid and fixture guide.

---

## Project Status

DSB is in **active development** (v0.1.0). The core API, dashboard, MCP server, SSH gateway, and Python SDK are production-ready for evaluation workloads. See [ROADMAP.md](ROADMAP.md) for what's next.

---

## Contributing

We welcome contributions of all sizes — bug reports, docs, code, examples. See [CONTRIBUTING.md](CONTRIBUTING.md) for the workflow, dev setup, and PR conventions. New contributors should look for issues labeled [`good first issue`](https://github.com/pengye91/dsb/issues?q=is%3Aopen+is%3Aissue+label%3A%22good+first+issue%22) or [`help wanted`](https://github.com/pengye91/dsb/issues?q=is%3Aopen+is%3Aissue+label%3A%22help+wanted%22).

By participating, you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## Security

Found a security issue? Please follow the [coordinated disclosure process in SECURITY.md](SECURITY.md) — **do not** open a public GitHub issue. Email `security@dsb.dev`.

---

## License

- **Core (Rust, Dashboard, MCP, SSH Gateway, Static Server):** Dual-licensed under [Apache 2.0](LICENSE-APACHE-2.0) or [AGPL 3.0](LICENSE-AGPL-3.0). You may choose either license.
- **Python SDK (`sdks/python/`):** [MIT](sdks/python/LICENSE).

If you distribute a modified version of the AGPL components, you must release your modifications under a compatible license. For proprietary/SaaS use, the Apache 2.0 path is appropriate.

---

## Acknowledgments

- Built on [Axum](https://github.com/tokio-rs/axum), [Bollard](https://github.com/fussybeaver/bollard), [rmcp](https://github.com/modelcontextprotocol/rust-sdk), [russh](https://github.com/warp-tech/russh), and many other open-source Rust crates.
- The Python SDK uses [httpx](https://www.python-httpx.org/), [Pydantic v2](https://docs.pydantic.dev/), [tenacity](https://github.com/jd/tenacity), and [pybreaker](https://github.com/danielfm/pybreaker).
- The dashboard uses [React 19](https://react.dev/), [Vite 6](https://vitejs.dev/), and [Chakra UI v2](https://chakra-ui.com/).
