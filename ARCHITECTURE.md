# DSB Architecture Documentation

> **DSB** (Distributed Sandboxes) is a fast, minimal Docker sandbox manager for ephemeral container environments. It provides a REST API, WebSocket terminal/VNC access, an MCP server for AI agent integration, a CLI, and a web dashboard.

> **For per-module reference** (handlers, schemas, file structure, Mermaid diagrams), see [`docs/architecture/`](docs/architecture/README.md). This file is the high-level system view.

---

## Table of Contents

- [System Overview](#system-overview)
- [Technology Stack](#technology-stack)
- [Repository Structure](#repository-structure)
- [Component Architecture](#component-architecture)
  - [DSB Server (Core)](#dsb-server-core)
  - [API Layer](#api-layer)
  - [Docker Integration Layer](#docker-integration-layer)
  - [Database Layer](#database-layer)
  - [Configuration System](#configuration-system)
  - [CLI](#cli)
  - [SSH Gateway](#ssh-gateway)
  - [MCP Server](#mcp-server)
  - [Dashboard (Frontend)](#dashboard-frontend)
  - [Python SDK](#python-sdk)
- [Authentication & Authorization](#authentication--authorization)
- [Data Model](#data-model)
- [Background Tasks](#background-tasks)
- [Feature Profile System](#feature-profile-system)
- [Error Handling](#error-handling)
- [Deployment](#deployment)
- [Request Flow](#request-flow)

---

## System Overview

```
                                ┌─────────────────────┐
                                │   LLM AI Agents     │
                                │ (Claude, GPT, etc.) │
                                └────────┬────────────┘
                                         │ MCP Protocol (SSE/HTTP)
                                         ▼
┌──────────┐    SSH     ┌───────────────────────────────┐
│  Users   │───────────►│     SSH Gateway (port 2223)   │
└──────────┘            └───────────────┬───────────────┘
                                        │ REST API (auth + session tracking)
┌──────────┐    HTTP    ┌───────────────┴───────────────┐    Docker API    ┌────────────────────┐
│   CLI    │───────────►│                               │───────────────►│                    │
└──────────┘            │      DSB Server (port 8080)   │                │  Docker Sandboxes  │
                        │                               │◄───────────────│  (ephemeral)       │
┌──────────┐   HTTP/WS  │  ┌─────────┐  ┌───────────┐  │                └────────────────────┘
│Dashboard │───────────►│  │  API    │  │  Docker   │  │
│ (port    │            │  │ Router  │  │ Manager   │  │
│  3001)   │            │  └────┬────┘  └─────┬─────┘  │
└──────────┘            │       │             │         │
                        │  ┌────▼────┐  ┌─────▼─────┐  │
┌──────────┐   HTTP    │  │ Core    │  │  Bollard   │  │
│ Python   │───────────►│  │ Services│  │  Client    │  │
│   SDK    │            │  └────┬────┘  └───────────┘  │
└──────────┘            │       │                       │
                        │  ┌────▼────┐                  │
                        │  │PostgreSQL│                  │
                        │  │  (state) │                  │
                        │  └─────────┘                  │
                        └───────────────────────────────┘
```

---

## Technology Stack

| Layer | Technology |
|---|---|
| **Core Server** | Rust (edition 2021), Tokio async runtime |
| **Web Framework** | Axum 0.8 (with tower middleware) |
| **Docker Client** | Bollard 0.19 (Docker Engine API) |
| **Database** | PostgreSQL 18, tokio-postgres, deadpool-postgres |
| **CLI** | Clap 4.5 (derive), indicatif (progress bars) |
| **MCP Server** | Rust, rmcp SDK 0.12, Axum (Streamable HTTP) |
| **SSH Gateway** | Rust, russh 0.56 |
| **Frontend** | React 19, TypeScript, Vite 6, Chakra UI v2 |
| **Python SDK** | Python 3.10+, httpx, Pydantic v2 |
| **Containerization** | Docker Compose, multi-stage builds |
| **Testing** | Playwright (E2E), cargo test, pytest |

---

## Repository Structure

```
dsb/
├── src/                    # Core Rust crate (DSB server)
│   ├── api/                # HTTP API layer (Axum handlers, middleware, auth)
│   ├── auth/tokens/        # VNC token authentication
│   ├── bin/                # Additional binaries
│   ├── cli/                # CLI commands and display
│   ├── config/             # Configuration loading and validation
│   ├── core/               # Business logic (sandbox, SSH, activities, features)
│   ├── db/                 # PostgreSQL persistence (7 tables)
│   ├── docker/             # Docker integration (container lifecycle)
│   ├── k8s/                # Kubernetes backend (Pod exec, watcher)
│   ├── logging/            # Tracing initialization
│   ├── static/             # Static assets (dashboard SPA)
│   ├── tasks/              # Background task management
│   ├── testing/            # Test utilities (MockDocker, fixtures)
│   ├── utils/              # MIME detection and shared helpers
│   ├── vnc_proxy.rs        # WebSocket VNC proxy
│   ├── session_token.rs    # Session token types
│   └── web_terminal.rs     # WebSocket terminal (xterm.js)
│
├── ssh-gateway/            # SSH-to-container gateway service
├── dsb-mcp-server/         # MCP server for AI agent integration
├── dsb-agent-tester/       # E2E MCP server test harness
├── static-server/          # Static file server (workspace member; extraction planned)
├── dashboard/              # React web UI
├── sdks/python/            # Python SDK (sync + async clients)
├── deployment/             # Production deployment (docker-compose, helm)
├── docker/                 # Dockerfiles, compose files, base images
├── docs/                   # Documentation
└── tests/                  # Integration tests
```

---

## Component Architecture

### DSB Server (Core)

The main `dsb` crate is the heart of the system. It is both a library (`src/lib.rs`) and a binary (`src/main.rs`).

**Entry point flow:**
1. `main.rs` loads configuration via `config::load()`
2. Initializes structured logging via `logging::init_logging()`
3. Dispatches to CLI handler via `cli::run_cli()`

**If the `server` command is used**, `api::server::start_server()` bootstraps the full application:

```
start_server(config)
    │
    ├─► DockerManager::new()              # Connect to Docker daemon
    │
    ├─► State Store initialization        # PostgreSQL or in-memory
    │   ├─ PostgreSQL: run migrations, create pool
    │   └─ In-memory: Arc<RwLock<HashMap>>
    │
    ├─► Service creation
    │   ├─ SandboxService                 # Core orchestrator
    │   ├─ SshSessionService              # SSH session lifecycle
    │   ├─ ActivityService (optional)     # Audit logging (PostgreSQL only)
    │   ├─ StaticFileService              # File serving
    │   └─ VncTokenService                # VNC auth tokens
    │
    ├─► Background tasks
    │   ├─ Auto-cleanup (inactivity timeout)
    │   ├─ State monitor (DB/Docker sync)
    │   ├─ Orphan cleanup
    │   ├─ Expired sandbox deletion
    │   ├─ SSH session cleanup
    │   └─ Session token cleanup
    │
    └─► Axum Router with middleware
        ├─ Request ID generation
        ├─ API key authentication
        ├─ Request logging
        ├─ CORS
        └─ Error handling (HTML/JSON)
```

**Key modules:**

| Module | Responsibility |
|---|---|
| `core/sandbox.rs` | SandboxService -- the main orchestrator (~5000 lines). Manages the full sandbox lifecycle, state transitions, file transfers, activity tracking |
| `core/manager.rs` | `SandboxManager` trait -- abstracts the container backend (Docker, potentially Podman/K8s) |
| `core/store_trait.rs` | `StateStoreTrait` -- abstracts state persistence (in-memory vs PostgreSQL) |
| `core/features.rs` | Feature profile system -- auto-configuration from Docker image labels |
| `core/activities.rs` | Activity tracking service for audit/debugging |
| `core/ssh_service.rs` | SSH session lifecycle management |
| `core/static_files.rs` | Per-sandbox static file publishing |

---

### API Layer

Built on Axum with a layered middleware architecture. All routes are defined in `api/server/mod.rs` for production and `api/mod.rs` for testing.

**Middleware stack (inner to outer):**
1. Request ID generation (`X-Request-ID` header)
2. API key authentication (multi-source: admin key, database keys, config key)
3. Structured request logging (method, path, status, duration, client IP)
4. CORS
5. Error handler (HTML for dashboard routes, JSON for API routes)

**Route groups:**

| Prefix | Purpose | Auth |
|---|---|---|
| `GET /health` | Liveness probe | None |
| `/sandboxes` | Sandbox CRUD, exec, upload, download, stats | API key |
| `/sandboxes/create-stream` | SSE sandbox creation with progress | API key |
| `/ssh-sessions` | SSH session management | API key |
| `/ssh/authorize/{id}` | Internal SSH gateway authorization | SSH gateway key |
| `/terminal`, `/terminal/{id}` | WebSocket terminal (xterm.js) | Terminal key |
| `/vnc/{id}` | WebSocket VNC proxy | VNC key |
| `/static/{sandbox_id}/*` | Per-sandbox static file serving | Configurable |
| `/images` | Docker image management | API key |
| `/activities` | Activity audit logs | API key |
| `/session-tokens` | Short-lived service tokens | API key |
| `/admin/api-keys` | API key management | Admin key only |
| `/config` | Frontend configuration | API key |
| `/dashboard/*` | SPA static files | None |

**SSE endpoints** provide streaming for:
- Sandbox creation progress (`pulling` -> `creating` -> `starting` -> `ready`)
- Container stats streaming
- Image pull progress

---

### Docker Integration Layer

Manages all container operations via the Bollard Docker client library.

**Architecture:**
```
SandboxService
    │ uses SandboxManager trait
    ▼
DockerManager (implements SandboxManager + DockerTrait)
    │ wraps bollard::Docker client
    │
    ├─► Container lifecycle: create, start, stop, remove
    ├─► Image management: pull, list, inspect, delete
    ├─► Command execution: exec, exec_with_stdin
    ├─► HTTP proxy: exec_http() -> container:8080 (tool_proxy)
    ├─► File transfer: tar archive upload/download
    ├─► Monitoring: stats streaming, health checks
    └─► PTY: DockerExecProxy for SSH/terminal
```

**Key design decisions:**
- `DockerManager` connects via UNIX socket (Linux), TCP, or Docker Desktop socket (macOS)
- Container creation handles port bindings, volume mounts (bind + named), resource limits, SELinux labels, and feature detection
- IP address caching for tool_proxy HTTP calls (avoids repeated Docker inspect)
- Retry logic with exponential backoff for container removal

---

### Database Layer

PostgreSQL-backed persistence with 7 tables. Uses `deadpool-postgres` for connection pooling (max 20 connections).

**Tables:**

```
┌─────────────────┐     ┌──────────────────────┐
│    sandboxes     │     │   sandbox_activities  │
│─────────────────│     │──────────────────────│
│ id (UUID PK)    │◄────│ sandbox_id (UUID)     │
│ image           │     │ activity_type         │
│ name            │     │ timestamp             │
│ state           │     │ details (JSONB)       │
│ container_id    │     └──────────────────────┘
│ environment     │
│ port_mappings   │     ┌──────────────────────┐
│ resource_limits │     │     ssh_sessions      │
│ volumes         │     │──────────────────────│
│ features        │     │ id (UUID PK)         │
│ pull_policy     │     │ sandbox_id (FK)       │
│ deleted_at      │     │ client_ip             │
│ api_key_id (FK) │     │ state                 │
│ activity fields │     │ bytes_sent/received   │
└────┬────────────┘     └──────────────────────┘
     │
     │  FK
     ▼
┌─────────────────┐     ┌──────────────────────┐
│    api_keys      │     │    session_tokens     │
│─────────────────│     │──────────────────────│
│ id (UUID PK)    │     │ token (TEXT PK)       │
│ key_hash (bcrypt)│    │ sandbox_id            │
│ key_prefix      │     │ service               │
│ name            │     │ expires_at            │
│ scopes (JSONB)  │     └──────────────────────┘
│ is_active       │
│ last_used_at    │     ┌──────────────────────┐
└─────────────────┘     │      vnc_tokens       │
                        │──────────────────────│
                        │ id (UUID PK)         │
                        │ token_hash (SHA-256)  │
                        │ sandbox_id           │
                        │ expires_at           │
                        └──────────────────────┘
```

**Pluggable storage:** The system supports both PostgreSQL (production, persistent) and in-memory (development/testing) via the `StateStoreTrait`. The storage backend is auto-detected based on whether database credentials are configured.

**Soft delete:** Sandboxes support soft deletion (`deleted_at`, `deleted_by`) with configurable retention before permanent deletion.

---

### Configuration System

Multi-source, hierarchical configuration with clear priority ordering.

**Priority (highest to lowest):**
1. CLI arguments (`--port`, `--host`, etc.)
2. Environment variables (`DSB_` prefix, `__` for nesting, e.g., `DSB_SERVER__PORT`)
3. `.env` file
4. YAML config (`dsb.yaml`)
5. Hardcoded defaults

**Configuration sections:**

| Section | Key Settings |
|---|---|
| `server` | Port, host, API keys, auth flags, token TTLs |
| `database` | PostgreSQL connection (URL or components) |
| `docker` | Registry, socket path, default image, network, proxy env |
| `sandbox` | Inactivity timeout, cleanup settings, VNC resolution, tool timeouts, resource limits |
| `ssh` | Port, API URL, host key path, cleanup timeouts |
| `logging` | Level, format (pretty/json), file path, rotation |
| `static_server` | Base path, cache control, file size limits, ZIP download |

**Loading pipeline:**
```
load()
  ├─► Find .env and dsb.yaml (search up to 3 parent dirs)
  ├─► Read proxy env vars (HTTP_PROXY, etc.)
  ├─► Merge from config crate (env + YAML)
  ├─► Apply CLI overrides
  ├─► Validate all sections
  └─► Return Config struct
```

---

### CLI

A comprehensive command-line interface built with Clap that wraps the DSB REST API.

**Key commands:**
- `dsb server` -- Start the DSB server
- `dsb create` -- Create a sandbox (with SSE progress streaming)
- `dsb list` -- List sandboxes (table or JSON output)
- `dsb info` -- Get sandbox details
- `dsb exec` -- Execute a command in a sandbox
- `dsb ssh` -- Interactive SSH terminal to a sandbox
- `dsb stop` / `dsb delete` / `dsb restore` -- Lifecycle management
- `dsb upload` / `dsb download` -- File transfer
- `dsb tools` -- Execute tools inside sandboxes
- `dsb web` -- Web search (via SearXNG) and scraping
- `dsb images` -- Image management (list, pull, delete)
- `dsb api-key` -- API key management (create, list, rotate, delete)
- `dsb activities` -- Activity audit logs
- `dsb stats` -- Container resource stats (with streaming)
- `dsb health` / `dsb config` -- Server health and config

The CLI resolves credentials from CLI flags > config file > environment variables.

---

### SSH Gateway

A standalone Rust service providing SSH terminal access to running sandboxes.

**Architecture:**
```
User: ssh -p 2223 <sandbox-uuid>@localhost
         │
         ▼
   SSH Gateway (russh)
         │
         ├─► Authenticate via public key or password
         │   (password = API key)
         │
         ├─► Authorize sandbox access
         │   GET /ssh/authorize/{sandbox_id}
         │   (calls DSB API to verify sandbox exists & is running)
         │
         ├─► Create SSH session record
         │   POST /ssh-sessions
         │
         └─► Create Docker exec PTY
             (connects directly to Docker daemon)
             Bidirectional pipe: SSH <-> Docker exec
```

**Key features:**
- Session lifecycle tracking (connecting -> active -> disconnected/terminated)
- Heartbeat with byte counters for monitoring
- Automatic cleanup of stale/stuck/orphaned sessions
- Runs on host port 2223 (container port 2222)

---

### MCP Server

Exposes DSB sandbox capabilities as MCP tools for AI agent integration using the Streamable HTTP transport.

**Service endpoints (8 paths):**

| Path | Tools | Purpose |
|---|---|---|
| `/mcp/dsb/sandbox` | 8 | Sandbox CRUD, exec, file ops |
| `/mcp/dsb/browser` | 14 | Browser automation (navigate, click, fill, screenshot, etc.) |
| `/mcp/dsb/exec` | 2 | Python and Bash execution |
| `/mcp/dsb/web` | 2 | Web search and scraping |
| `/mcp/dsb/terminal` | 3 | Terminal access |
| `/mcp/system` | 1 | System information |
| `/mcp/value_retrieval` | 2 | Milvus-based knowledge retrieval |

**Session management:** Uses a `DashMap`-backed `SessionManager` that maps MCP session IDs to sandbox IDs. Each AI agent conversation gets a persistent sandbox that is reused across tool calls within the session.

**All sandbox operations are proxied** through the DSB REST API via `DSBClient` (HTTP client wrapper). The MCP server does not access Docker directly.

---

### Dashboard (Frontend)

A React SPA providing a web UI for sandbox management.

**Key pages:**
- **Dashboard** -- Overview with stats and recent activity
- **Sandboxes** -- List, create, and manage sandboxes
- **Sandbox Details** -- Individual sandbox with terminal, stats, file browser
- **Images** -- Docker image management
- **Activities** -- Audit log viewer
- **API Keys** -- Key management (admin only)
- **Settings** -- Configuration viewer
- **VNC Viewer** -- Standalone VNC access to graphical sandboxes

**Key integrations:**
- xterm.js for WebSocket terminal access
- react-vnc for browser-based VNC
- SSE for streaming creation progress and stats
- Recharts for resource monitoring charts

**Production setup:** The dashboard container runs nginx that serves the built SPA and reverse-proxies all backend traffic:
```
Browser --> nginx (port 3001)
    ├── /              → static SPA files
    ├── /api/*         → dsb-server:8080
    ├── /vnc/*         → dsb-server:8080 (WebSocket)
    ├── /terminal/*    → dsb-server:8080 (WebSocket)
    ├── /static/*      → dsb-server:8080
    └── /mcp           → dsb-mcp-server:3000 (SSE)
```

---

### Python SDK

Dual-mode (sync + async) Python client for the DSB REST API.

**Package:** `dsb-sdk` (Pydantic v2, httpx)

**API modules (11):**
- `sandbox` -- Sandbox CRUD, exec, stats, streaming
- `ssh` -- SSH session management
- `terminal` -- WebSocket terminal
- `images` -- Image management
- `activities` -- Activity audit
- `admin` -- API key management
- `config` -- Configuration retrieval
- `health` -- Health checks
- `static_files` -- Static file operations
- `web` -- Web search and scraping

**Features:**
- Retry logic with tenacity
- Circuit breaker with pybreaker
- SSE streaming support
- WebSocket terminal connections
- Prometheus metrics
- Structured logging with structlog

---

## Authentication & Authorization

DSB uses a tiered API key system with ownership-based multi-tenancy.

**Key types:**

| Type | Source | Scope |
|---|---|---|
| `Privileged` | Admin API key (config) or legacy config key | Full access to all sandboxes |
| `Database` | API keys stored in PostgreSQL (bcrypt hashed) | Scoped to owned sandboxes only |

**Authentication flow:**
1. Extract API key from `X-API-Key` header or `api_key` query parameter
2. Check admin key (exact match)
3. Check database keys (prefix lookup + bcrypt verify)
4. Check legacy config key (exact match)
5. Return 401 if no valid key found

**Authorization model:**
- Privileged keys can access all sandboxes
- Database keys can only access sandboxes they created (`api_key_id` FK)
- Some endpoints are admin-only (API key management, global activities)

**VNC/terminal auth:** Uses separate keys or short-lived session tokens (`POST /session-tokens`) that are scoped to a specific sandbox and service.

---

## Data Model

### Sandbox Lifecycle

```
Creating → Created → Starting → Running
                                    │
                          ┌─────────┼──────────┐
                          ▼         ▼          ▼
                        Stopped    Error    (auto-cleanup)
                          │
                          ▼
                  (manual cleanup)
                          │
              ┌───────────┼───────────┐
              ▼                       ▼
         Restored               Soft Deleted
         (back to Stopped)     (state=Destroying)
                                      │
                                      ▼ (retention period)
                               Permanently Deleted
                               (state=Destroyed)
```

### Key Types

**SandboxConfig** -- Creation parameters: image, name, environment variables, port mappings, resource limits, volume mounts, command, pull policy, feature selection, VNC resolution.

**Sandbox** -- Complete instance: UUID, config, state, container ID, timestamps, error message, activity tracking, soft-delete fields, owner (api_key_id).

**PortMapping** -- Host port, container port, protocol (TCP/UDP).

**ResourceLimits** -- memory_mb, cpu_quota, cpu_period, cpu_shares, pids_limit, ulimits.

**VolumeMount** -- Bind mounts (host_path + container_path) or named volumes (name + container_path), with read-only option.

---

## Background Tasks

DSB runs several periodic background tasks for maintenance:

| Task | Interval | Purpose |
|---|---|---|
| Auto-cleanup | 60s | Stops sandboxes inactive beyond configured timeout |
| State monitor | Configurable | Detects state mismatches between DB and Docker |
| Orphan cleanup | 5 min | Removes containers inactive >30 minutes without DB record |
| Destroyed cleanup | 5 min | Removes orphaned containers for destroyed sandboxes |
| Expired deletion | Hourly | Permanently deletes soft-deleted sandboxes past retention |
| SSH cleanup | 30s | Terminates stale/stuck/orphaned SSH sessions |
| Session token cleanup | 5 min | Deletes expired session tokens |
| Startup recovery | Once | Recovers sandboxes stuck in `Creating` state |

---

## Feature Profile System

Docker images can declare capabilities via the `com.dsb.features` label (JSON), and DSB automatically applies configuration when features are enabled.

**How it works:**
1. Image includes `com.dsb.features` label with feature definitions
2. Each feature specifies: ports, environment variables, volume mounts, static server config
3. User requests features via `enable_features` or `enable_all_features` in sandbox config
4. `build_feature_profile()` merges image metadata with user selection
5. `apply_feature_profile()` adds ports, volumes, env vars to the sandbox config
6. User-specified values always take precedence over feature defaults

**Example feature label:**
```json
{
  "version": "1.0",
  "features": {
    "vnc": {
      "description": "VNC desktop access",
      "enabled_by_default": false,
      "ports": [{"container": 5901, "description": "VNC server"}],
      "env": {"DISPLAY": ":1"}
    },
    "browser": {
      "description": "Headless browser",
      "enabled_by_default": true,
      "ports": [{"container": 9222, "description": "Chrome DevTools"}]
    }
  }
}
```

---

## Error Handling

DSB uses RFC 9457 (Problem Details) compliant error responses with 35 synchronized error codes across Rust, Python SDK, and Sandbox.

**Error response format:**
```json
{
  "type": "https://dsb.dev/errors/sandbox-not-found",
  "title": "Sandbox Not Found",
  "status": 404,
  "detail": "Sandbox abc-123 does not exist",
  "error_code": "SANDBOX_NOT_FOUND",
  "retryable": false,
  "request_id": "uuid",
  "timestamp": "2026-01-01T00:00:00Z"
}
```

**Error categories:**

| Category | Examples |
|---|---|
| Sandbox | NOT_FOUND, INVALID_STATE, CREATION_FAILED |
| Tool | NOT_FOUND, EXECUTION_FAILED, TIMEOUT |
| Docker | IMAGE_PULL_FAILED, CONTAINER_CREATE_FAILED |
| SSH/Terminal | SESSION_NOT_FOUND, AUTHENTICATION_FAILED |
| Validation | INVALID_PORT, MISSING_FIELD, INVALID_IMAGE_NAME |
| Auth | MISSING, INVALID_API_KEY, INSUFFICIENT_PERMISSIONS |
| Database | CONNECTION_FAILED, QUERY_FAILED |
| Infrastructure | SERVICE_UNAVAILABLE, RATE_LIMIT_EXCEEDED |

Each error code maps to an HTTP status and retryability hint. Verification: see the `scripts/mcp_tool_verification.py` helper and the Rust `src/api/errors.rs` constants.

---

## Deployment

### Docker Compose (Production)

The `deployment/` directory provides a complete production setup with 6 services:

```
┌─────────────────────────────────────────────────────┐
│  Docker Compose Network (dsb-network)               │
│                                                      │
│  ┌──────────────┐     ┌────────────────────────┐    │
│  │  dashboard   │────►│  dsb-server            │    │
│  │  (nginx)     │     │  (port 8080 internal)  │    │
│  │  port 3001   │     └───────────┬────────────┘    │
│  └──────┬───────┘                 │                 │
│         │                         │ Docker API      │
│         │    ┌────────────────────┼──────────┐      │
│         │    │              ┌────▼────┐      │      │
│         │    │              │ Sandbox │      │      │
│         │    │              │Container│      │      │
│         │    │              └─────────┘      │      │
│         │    │                               │      │
│         ├───►│  dsb-mcp-server (port 3000)   │      │
│         │    └───────────────────────────────┘      │
│         │                                            │
│         ├───►  ssh-gateway (port 2222, host 2223)   │
│         │                                            │
│         │    ┌─────────────────┐                     │
│         └───►│  postgres       │                     │
│              │  (port 5432)    │                     │
│              └─────────────────┘                     │
│                                                      │
│              ┌─────────────────┐                     │
│              │  searxng        │                     │
│              │  (port 8888)    │                     │
│              └─────────────────┘                     │
└─────────────────────────────────────────────────────┘
```

**Data persistence:** Host bind mounts under `DSB_VOLUME_ROOT` for PostgreSQL data, static files, and SearXNG data.

### Build System

The `Makefile` provides all build and operations commands:

| Command | Purpose |
|---|---|
| `make base-images-build` | Build Docker base images |
| `make dc-build` | Build all project images |
| `make dc-up` | Start all services |
| `make dc-down` | Stop all services |
| `make test` | Run the test suite |
| `make clean-docker` | Clear Docker build cache (and prune unused images) |

---

## Request Flow

### Example: Create Sandbox

```
Client                API Layer              SandboxService         DockerManager         Database
  │                     │                        │                      │                    │
  │ POST /sandboxes     │                        │                      │                    │
  │────────────────────►│                        │                      │                    │
  │                     │  auth middleware        │                      │                    │
  │                     │  (validate API key)     │                      │                    │
  │                     │                        │                      │                    │
  │                     │  create_sandbox()       │                      │                    │
  │                     │───────────────────────►│                      │                    │
  │                     │                        │  generate UUID       │                    │
  │                     │                        │                      │                    │
  │                     │                        │  build feature profile                  │
  │                     │                        │  (from image labels)  │                    │
  │                     │                        │                      │                    │
  │                     │                        │  create_sandbox()    │                    │
  │                     │                        │─────────────────────►│                    │
  │                     │                        │      container_id    │                    │
  │                     │                        │◄─────────────────────│                    │
  │                     │                        │                      │                    │
  │                     │                        │  state = Creating    │                    │
  │                     │                        │─────────────────────────────────────────►│
  │                     │                        │                      │                    │
  │                     │                        │  start_container()   │                    │
  │                     │                        │─────────────────────►│                    │
  │                     │                        │                      │                    │
  │                     │                        │  health check (tool_proxy)               │
  │                     │                        │                      │                    │
  │                     │                        │  state = Running     │                    │
  │                     │                        │─────────────────────────────────────────►│
  │                     │                        │                      │                    │
  │   201 Created       │                        │                      │                    │
  │◄────────────────────│                        │                      │                    │
  │  SandboxResponse    │                        │                      │                    │
```
