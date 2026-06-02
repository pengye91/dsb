# DSB Architecture Details

Per-module reference documentation. This folder is the detailed companion to [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md), which provides the high-level system overview.

## Table of Contents

- [API Module](./api.md) — HTTP API layer (Axum routes, handlers, middleware, errors)
- [Authentication](./authentication.md) — API key system, scopes, admin endpoints
- [CLI Module](./cli.md) — Command-line interface and subcommand reference
- [Core Module](./core.md) — Business logic (sandbox lifecycle, activities, feature profiles, static files)
- [Database Module](./db.md) — PostgreSQL persistence, schema, migrations
- [Docker Module](./docker.md) — Container lifecycle, PTY exec, feature detection
- [Web Terminal](./web_terminal.md) — WebSocket terminal protocol and message types
- [Utils Module](./utils.md) — MIME detection and shared helpers
- [Configuration](./configuration.md) — Two-tier config (`.env` + `dsb.yaml`)

## Reading Order

If you're new to the codebase:

1. Start with [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md) for the system overview
2. Read [API](./api.md) → [Core](./core.md) → [Docker](./docker.md) for the request flow
3. Skim [Database](./db.md) and [Configuration](./configuration.md) for persistence and config
4. Refer back to the others as needed

## Mermaid Diagrams

Every module file uses Mermaid diagrams for sequence/flow/state visualization. Render with the GitHub web view or a Mermaid-aware Markdown viewer.
