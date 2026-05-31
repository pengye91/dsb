# DSB Agent Tester

E2E test agent for DSB that validates all MCP tools through a real MCP client connection.

## Overview

This crate provides:
- `MonorailAgent` - Direct MCP client using rmcp 0.12 for accessing all 15 DSB MCP tools
- Integration tests for all MCP tools via docker-compose
- Scenario-based E2E tests for complete workflows

## Prerequisites

- Docker and docker-compose
- Rust 1.75+
- DSB stack running (dsb-server + dsb-mcp-server)

## Architecture

```
dsb-agent-tester
    |
    └─► rmcp client ──► dsb-mcp-server:3000 ──► dsb-server:8080
```

The agent uses `rmcp` 0.12 directly (same version as dsb-mcp-server) for compatibility.

## MCP Tools (8 total)

| Category | Tools |
|----------|-------|
| Sandbox | `create_sandbox`, `list_sandboxes`, `delete_sandbox` |
| Execution | `execute_code`, `execute_bash` |
| Web | `scrape_web`, `search_web` |
| Browser | `automate_browser` |

## Usage

### Run all agent tests (requires docker-compose stack)

```bash
make test-agent
```

### Run with manual stack management

```bash
# Start the DSB stack
docker compose -f docker/docker-compose.test.yml up -d dsb-server-test dsb-mcp-server-test

# Run tests
export DSB_MCP_URL=http://localhost:13223/mip
cargo test -p dsb-agent-tester tests::monorail_tests tests::scenario_tests -- --nocapture
```

### Run specific test

```bash
cargo test -p dsb-agent-tester test_sandbox_lifecycle -- --nocapture
```

## Test Structure

- `tests::monorail_tests` - Core MCP tool tests (connection, lifecycle, error handling)
- `tests::scenario_tests` - E2E workflow tests (web scraping, parallel ops, code execution)

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DSB_MCP_URL` | MCP server URL | `http://localhost:3223/mcp` |
| `DSB_API_URL` | DSB API URL (fallback) | - |
| `DSB_API_KEY` | API key for authentication | - |
| `DSB_TEST_SANDBOX_IMAGE` | Image for browser/scenario tests | `dsb/sandbox:latest` (full stack; use `dsb/sandbox-minimal:latest` to save space) |
| `RUST_LOG` | Logging level | `info` |

## Docker Compose Integration

The `test-agent` target in the root Makefile:
1. Starts `dsb-server-test`, `dsb-mcp-server-test`, and supporting services
2. Waits for the MCP server to be healthy
3. Runs the agent tests with `DSB_MCP_URL` pointing to the MCP server
4. Tests run inside `dsb-test-runner` container with access to the docker network

## Notes

- Tests use unique sandbox names (with random suffix) to avoid container name conflicts
- Python code execution requires `python:*` images (e.g., `python:3.12`)
- Bash execution works with any Linux image (e.g., `ubuntu:22.04`)
