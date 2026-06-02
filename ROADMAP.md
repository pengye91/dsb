# DSB Roadmap

This document tracks the high-level direction of DSB. It is **not** a commitment to specific dates — items move between buckets as priorities shift and contributors join.

> **Want to influence the roadmap?** Open a [Discussion](https://github.com/pengye91/dsb/discussions) or vote on existing feature-request issues.

---

## ✅ Shipped (v0.1.0)

- Core REST API with Axum + tokio
- Docker backend (Bollard client) and Kubernetes backend (Pod exec)
- PostgreSQL persistence with auto-migration
- WebSocket terminal (xterm.js) and VNC proxy
- React/TypeScript dashboard
- Model Context Protocol server with 15 tools
- SSH gateway with persistent Ed25519 host keys
- Python SDK (sync + async) with circuit breaker and retry
- Web-based CLI
- Unified RFC 9457 error system (35 codes)
- Two-tier config (`dsb.yaml` + `.env`)
- Helm chart for Kubernetes deployment
- Tiered API key system (admin / database / legacy)

## 🚧 In Progress

- **Image registry UI polish** — feature detection, labels, pull progress
- **Multi-replica DSB server** with sticky routing for WebSocket sessions
- **Static server extraction** from main binary to standalone workspace member
- **dsb-mcp-server** per-session sandbox pooling (one sandbox per MCP session)
- **Egress proxy** for corporate Kubernetes clusters (HTTP_PROXY / NO_PROXY injection)
- **Frontend migration to Chakra v3** (from v2)

## 📋 Next Up (next 1–2 minor versions)

### v0.2 — Production hardening

- API key scopes enforcement (`scopes: ["sandbox:read", "sandbox:write"]`)
- Rate limiting per API key
- Webhook notifications on sandbox lifecycle events
- Audit log export (CSV / JSON)
- Prometheus metrics endpoint
- OpenTelemetry tracing export

### v0.3 — Ecosystem

- **Go SDK** (mirroring Python SDK surface)
- **TypeScript SDK** (for browser and Node.js)
- Helm chart promotion to OCI registry
- `dsb` Homebrew formula
- `pip install dsb-sdk` pre-built wheels for ARM64 + x86_64

### v0.4 — Performance

- Container start latency reduction (pre-warmed pool)
- Incremental file transfer (rsync-style)
- Snapshot/restore for fast sandbox cloning
- Read replica support for PostgreSQL

## 🔭 Long-Term Explorations

- **Multi-tenant isolation** — per-tenant network namespaces, resource quotas
- **GPU support** — pass-through for ML workloads
- **Sandbox templates** — pre-configured images with metadata
- **Time-travel debugging** — record and replay container state
- **WebAssembly sandboxes** — non-Docker alternative for untrusted code

## ❌ Out of Scope (for now)

- Building a competing orchestrator (Kubernetes / Nomad exist)
- Hosting a managed DSB cloud service
- Supporting Windows-native containers (use WSL2)
- Replacing standard CI/CD — DSB is for ephemeral execution, not pipelines

---

## How to contribute to the roadmap

1. **Open a Discussion** in the `Ideas` category
2. **Upvote** existing feature-request issues you care about
3. **Pick up a `good first issue`** to get familiar with the codebase
4. **Propose a major change** via a GitHub Issue with the `proposal` label before opening a large PR

Maintainers review the roadmap monthly. Last reviewed: 2026-06-02.
