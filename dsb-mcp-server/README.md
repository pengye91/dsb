# DSB MCP Server

[![Rust](https://img.shields.io/badge/rust-1.75+-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Model Context Protocol (MCP) server for [DSB (Distributed Sandboxes)](https://github.com/pengye91/dsb).

Exposes DSB capabilities as MCP tools for LLMs, including:

- 🐳 **Sandbox Management**: Create, list, and delete Docker sandboxes
- 💻 **Code Execution**: Execute Python and Bash commands
- 🌐 **Web Scraping**: Scrape web pages with JavaScript rendering
- 🤖 **Browser Automation**: Automate browser interactions
- 🔌 **Streamable HTTP Transport**: Modern, efficient MCP transport

## Features

- ✅ **MCP Protocol Version 2024-11-05** (Latest spec)
- ✅ **rmcp SDK v0.12.0** (Official Rust MCP SDK)
- ✅ **Streamable HTTP Transport** (Single `/mcp` endpoint, bidirectional)
- ✅ **15 Production-Ready Tools** with declarative macro-based registration
- ✅ **API Key Authentication** support for DSB server
- ✅ **Type-Safe Argument Handling** with automatic JSON schema generation
- ✅ **Async Rust Implementation** with tokio

## Architecture

```
┌─────────────┐
│  MCP Client │ (Claude, custom clients)
└──────┬──────┘
       │ Streamable HTTP (POST/GET)
       ▼
┌─────────────────────────────────────────┐
│  dsb-mcp-server (Port 3000)            │
│  - Single /mcp endpoint                 │
│  - Session management                   │
│  - 15 MCP tools                         │
└───────────┬─────────────────────────────┘
            │ HTTP REST + X-API-Key header
            ▼
┌─────────────────────────────────────────┐
│  DSB Server (Port 8080)                │
│  - Sandbox management                   │
│  - Command execution                    │
└───────────┬─────────────────────────────┘
            │ Docker API
            ▼
┌─────────────────────────────────────────┐
│  Docker Sandboxes                      │
│  - Full: GUI + browser automation       │
│  - Slim: Headless scraping              │
└─────────────────────────────────────────┘
```

## Quick Start

### Prerequisites

- Rust 1.75+
- DSB Server running (see main DSB project)
- Docker (for managing sandboxes)

### Build

```bash
cd /path/to/dsb
cargo build --release --package dsb-mcp-server --bin dsb-mcp-server
```

### Run

```bash
# Basic usage (connects to DSB server on localhost:8080)
./target/release/dsb-mcp-server

# With authentication
export DSB_API_KEY=your-api-key
./target/release/dsb-mcp-server

# Custom configuration
./target/release/dsb-mcp-server \
  --port 3000 \
  --dsb-api-url http://dsb-server:8080 \
  --log-level debug
```

### Docker Compose

The server is designed to run via Docker Compose:

```bash
# From project root
docker compose -f docker/docker-compose.test.yml up -d dsb-mcp-server-test
```

## MCP Tools (15 Total)

### Sandbox Management (4 tools)

| Tool | Description |
|------|-------------|
| `create_sandbox` | Create sandbox with full configuration |
| `create_sandbox_simple` | Quick sandbox creation |
| `list_sandboxes` | List all sandboxes |
| `delete_sandbox` | Delete sandbox by ID |

### Code Execution (2 tools)

| Tool | Description |
|------|-------------|
| `execute_code` | Execute Python code in sandbox |
| `execute_bash` | Execute bash commands in sandbox |

### Web Scraping (7 tools)

| Tool | Description |
|------|-------------|
| `scrape_web` | Scrape web pages with JS rendering |
| `extract_css` | Extract data using CSS selectors |
| `extract_table` | Extract HTML tables |
| `screenshot_web` | Capture page screenshots |
| `search_web` | Search web via the configured SearXNG instance |
| `extract_links` | Extract links from pages |
| `crawl_web` | Crawl multiple URLs |

### Browser (1 tool)

| Tool | Description |
|------|-------------|
| `automate_browser` | Interactive browser automation |

### System (1 tool)

| Tool | Description |
|------|-------------|
| `health_check` | Liveness probe for the MCP server and its dependencies |

## Authentication

The MCP server supports API key authentication with the DSB server:

```bash
# Via CLI flag
./target/release/dsb-mcp-server --api-key your-secret-key

# Via environment variable
export DSB_API_KEY=your-secret-key
./target/release/dsb-mcp-server
```

The API key is sent as `X-API-Key` header in all requests to the DSB server.

## Configuration

### Command-Line Arguments

| Argument | Description | Default |
|----------|-------------|---------|
| `--port` | Port to listen on | 3000 |
| `--dsb-api-url` | DSB API base URL | http://localhost:8080 |
| `--searxng-api-url` | SearXNG search API URL | http://localhost:8888/search |
| `--api-key` | API key for DSB authentication | (from env) |
| `--log-level` | Log level (trace/debug/info/warn/error) | info |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `DSB_API_KEY` | API key for DSB authentication |
| `DSB_SEARXNG_API_URL` | SearXNG search API URL for `search_web` |
| `RUST_LOG` | Log level (e.g., `dsb_mcp_server=debug`) |

`search_web` now queries the configured SearXNG HTTP API directly. Older callers may still send `sandbox_id`, but it is deprecated and no longer required for search execution.

## MCP Protocol

### Transport: Streamable HTTP

The server uses **Streamable HTTP** transport (rmcp v0.12), which provides:

- Single `/mcp` endpoint for all operations
- POST for client → server requests
- GET with streaming for server → client events
- Built-in session management

### Connection Example

```bash
# Initialize connection
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "Accept: application/json, text/event-stream" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2024-11-05",
      "capabilities": {},
      "clientInfo": {"name": "test-client", "version": "1.0"}
    }
  }'

# List tools
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "Mcp-Session-Id: <session-id>" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/list"
  }'

# Call a tool
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "Mcp-Session-Id: <session-id>" \
  -d '{
    "jsonrpc": "2.0",
    "id": 3,
    "method": "tools/call",
    "params": {
      "name": "create_sandbox",
      "arguments": {
        "image": "python:3.12",
        "name": "my-sandbox"
      }
    }
  }'
```

## Tool Examples

### Create Sandbox

```json
{
  "name": "create_sandbox",
  "arguments": {
    "image": "python:3.12",
    "name": "data-processing",
    "environment": {"API_KEY": "secret"},
    "resource_limits": {
      "memory_mb": 1024,
      "cpu_shares": 512
    }
  }
}
```

Response:
```
Created sandbox: 550e8400-e29b-41d4-a716-446655440000 (image: python:3.12, state: running)
```

### Execute Python Code

```json
{
  "name": "execute_code",
  "arguments": {
    "sandbox_id": "550e8400-e29b-41d4-a716-446655440000",
    "code": "print('Hello from Python!')"
  }
}
```

### Scrape Web Page

```json
{
  "name": "scrape_web",
  "arguments": {
    "sandbox_id": "550e8400-e29b-41d4-a716-446655440000",
    "url": "https://example.com",
    "format": "markdown"
  }
}
```

## Testing

```bash
# Run all tests
cargo test -p dsb-mcp-server

# Run with output
cargo test -p dsb-mcp-server -- --nocapture

# Run integration tests (requires DSB server)
cargo test -p dsb-mcp-server -- --ignored
```

## Docker Integration

The MCP server is designed to run in Docker alongside the DSB server:

```yaml
# docker/docker-compose.test.yml
services:
  dsb-mcp-server-test:
    build:
      context: ..
      dockerfile: docker/Dockerfile.mcp
    environment:
      - DSB_API_KEY=test-admin-key-for-testing-only
    ports:
      - "13223:3000"
    command:
      - "dsb-mcp-server"
      - "--port"
      - "3000"
      - "--dsb-api-url"
      - "http://dsb-server-test:8080"
    healthcheck:
      test: ["CMD", "curl", "-sf", "-X", "POST", ...]
```

See the main DSB project `docker/` directory for complete Compose files.

## Project Structure

```
dsb-mcp-server/
├── src/
│   ├── main.rs           # CLI entry point
│   ├── lib.rs            # Library exports
│   ├── server.rs         # MCP server (Streamable HTTP)
│   ├── dsb_client.rs     # DSB API client with auth
│   ├── dsb_service.rs    # MCP service with 15 tools
│   ├── config.rs         # Server configuration
│   ├── tools/            # Tool definitions
│   │   ├── mod.rs
│   │   ├── sandbox.rs
│   │   ├── exec.rs
│   │   ├── browser.rs
│   │   └── handlers.rs
│   ├── prompts/          # MCP prompts
│   └── resources/        # MCP resources
├── tests/                # Integration tests
├── Cargo.toml
└── README.md
```

## Dependencies

Key dependencies:

- **rmcp 0.12** - Official MCP Rust SDK
- **tokio** - Async runtime
- **axum** - HTTP server framework
- **reqwest** - HTTP client for DSB API
- **schemars** - JSON schema generation
- **serde** - Serialization
- **uuid** - UUID handling

## License

MIT License - see LICENSE file for details.

## Links

- [MCP Specification](https://modelcontextprotocol.io/specification/2024-11-05)
- [rmcp SDK](https://github.com/modelcontextprotocol/rust-sdk)
- [DSB Project](https://github.com/pengye91/dsb)
