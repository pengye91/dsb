# DSB Code → License Mapping

This document provides the complete mapping between source paths in
this repository and their applicable license. The high-level rules are
in [LICENSE](LICENSE); this file is the detailed reference.

---

## Apache License 2.0 (default)

**Unless otherwise specified below, all files in this repository are
licensed under the Apache License, Version 2.0.**

See [LICENSE-APACHE-2.0](LICENSE-APACHE-2.0) for the full license text.

### Core server (Rust) — `src/`

All subdirectories of `src/` are Apache 2.0 **except** `src/k8s/`:

| Path | Notes |
|------|-------|
| `src/api/` | HTTP API layer (Axum routes, handlers, middleware, errors) |
| `src/auth/` | Auth tokens (VNC) |
| `src/bin/` | Auxiliary binaries |
| `src/cli/` | `dsb` CLI commands and display |
| `src/config/` | Configuration loading and validation |
| `src/core/` | Business logic (sandbox, SSH, activities, features) |
| `src/db/` | PostgreSQL persistence |
| `src/docker/` | **Docker** backend implementation |
| `src/lib.rs` | Library root |
| `src/logging/` | Tracing initialization |
| `src/main.rs` | Binary entry point |
| `src/session_token.rs` | Session token types |
| `src/static/` | Static assets |
| `src/tasks/` | Background task management |
| `src/testing/` | Test utilities and fixtures |
| `src/utils/` | MIME detection and shared helpers |
| `src/vnc_proxy.rs` | WebSocket VNC proxy |
| `src/web_terminal.rs` | WebSocket terminal (xterm.js) |

### Workspace members

| Path | Notes |
|------|-------|
| `dashboard/` | React/TypeScript web UI |
| `dsb-agent-tester/` | E2E MCP test harness |
| `dsb-mcp-server/` | Model Context Protocol server |
| `ssh-gateway/` | SSH-to-container gateway |
| `static-server/` | Standalone static file server |

### Container images & Docker Compose

| Path | Notes |
|------|-------|
| `docker/` | Dockerfiles, base images, docker-compose files |
| `Dockerfile` (root) | Multi-stage DSB server build |
| `docker-compose*.yml` (root) | Compose service definitions |
| `dsb.yaml.example` | Application config template |
| `dsb.test.yaml.example` | Test config template |
| `.env.example` | Infrastructure env template |
| `Makefile` | Build & test targets |

### Build, tooling, CI

| Path | Notes |
|------|-------|
| `Cargo.toml`, `Cargo.lock` | Rust workspace manifests |
| `pyproject.toml`, `package.json` | Python and dashboard manifests |
| `rust-toolchain.toml` | Pinned Rust version |
| `.github/` | Issue templates, PR template, CI workflows |
| `scripts/` | Build, verify, and helper scripts |

### Tests

| Path | Notes |
|------|-------|
| `tests/` | Integration tests (with `tests/common/` fixtures) |

### Documentation

All `*.md` files at the project root, in `docs/`, and in component
directories are Apache 2.0:

- `README.md`, `ARCHITECTURE.md`, `ROADMAP.md`, `CHANGELOG.md`
- `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`, `TESTING.md`
- `CLAUDE.md` (project-internal AI assistant instructions)
- `docs/architecture/`, `docs/config/`
- `dashboard/README.md`, `deployment/README.md`, etc.

---

## AGPL-3.0-or-later (Kubernetes backend only)

The following paths are licensed under the **GNU Affero General Public
License v3.0 or later**. See [LICENSE-AGPL-3.0](LICENSE-AGPL-3.0) for
the full license text.

| Path | Notes |
|------|-------|
| `src/k8s/` | **Kubernetes** backend implementation (Pod exec, watcher, CRDs) |
| `deployment/helm/` | Helm chart for Kubernetes deployment |

All files under `src/k8s/` carry the SPDX header
`// SPDX-License-Identifier: AGPL-3.0-or-later`. All files under
`deployment/helm/` carry the YAML equivalent
`# SPDX-License-Identifier: AGPL-3.0-or-later`.

**What this means in practice:**

- You **can** use the Kubernetes backend internally at your company,
  including for production workloads, free of charge.
- You **can** modify it and self-host the modified version for your
  own use.
- You **cannot** offer the Kubernetes backend (modified or unmodified)
  as a hosted or managed service to third parties without releasing
  the complete corresponding source under AGPL 3.0.
- You **can** distribute the Kubernetes backend as part of a larger
  product that you sell (e.g. a hardware appliance), as long as you
  comply with AGPL 3.0 § 6 (conveying non-source alongside).

If you need a commercial license that allows offering the Kubernetes
backend as a hosted service, contact the copyright holder.

---

## MIT (Python SDK)

| Path | License |
|------|---------|
| `sdks/python/` | MIT — see [sdks/python/LICENSE](sdks/python/LICENSE) |

The Python SDK is the most permissive part of the project and is
licensed under MIT (more permissive than Apache 2.0 in some respects —
no patent grant, but also no patent retaliation clause). It can be
combined with code under any of the licenses above.

---

## Quick reference

| Question | Answer |
|----------|--------|
| Can I use the Docker Compose stack commercially? | **Yes** — Apache 2.0 |
| Can I modify the Docker Compose stack and sell it? | **Yes** — Apache 2.0, with attribution |
| Can I self-host the Kubernetes backend internally? | **Yes** — AGPL 3.0 allows this |
| Can I offer the Kubernetes backend as a SaaS? | **No** — AGPL 3.0 requires source release |
| Can I use the Python SDK in a closed-source product? | **Yes** — MIT |
| Can I mix Docker Compose code with K8S code in one product? | **Yes** — but you must comply with both licenses for the respective files |

---

For questions about licensing that aren't answered here, please open a
GitHub Discussion or contact the copyright holder.
