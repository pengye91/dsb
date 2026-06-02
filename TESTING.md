# DSB Testing Guide

This document provides comprehensive guidance on testing the DSB (Distributed Sandboxes) project.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Test Organization](#test-organization)
- [Running Tests](#running-tests)
- [Test Coverage](#test-coverage)
- [Writing Tests](#writing-tests)
- [Integration Tests](#integration-tests)
- [Test Fixtures](#test-fixtures)
- [Testing the Kubernetes Backend](#testing-the-kubernetes-backend)
- [Activity Tracking Tests](#activity-tracking-tests)
- [CI/CD Integration](#cicd-integration)
- [Best Practices](#best-practices)
- [Troubleshooting](#troubleshooting)

## Overview

DSB uses a multi-layered testing approach:

1. **Unit Tests** - Test individual functions and modules in isolation
2. **Integration Tests** - Test module interactions with real resources using testcontainers
3. **End-to-End Tests** - Test complete workflows with HTTP API and Docker

### Test Statistics

- **Total Tests:** 295+ (286 library + 9 E2E)
- **API Server Coverage:** 87 dedicated tests (78 unit + 9 E2E)
- **Test Execution Time:** ~10s (unit) + ~23s (E2E)

### Test Infrastructure

**Important:** DSB integration tests use **testcontainers** (not docker-compose) to spin up isolated PostgreSQL containers for each test. This ensures:

- ✅ Test isolation (each test gets a fresh database)
- ✅ Parallel test execution
- ✅ No manual setup required
- ✅ Works in CI environments

For testing with the full docker-compose stack, see [Testing with Docker Compose](#testing-with-docker-compose) below.

## Quick Start

### Unit Tests

```bash
# Run all unit tests
cargo test --lib

# Run with output
cargo test --lib -- --nocapture
```

### Integration Tests (with testcontainers)

```bash
# Integration tests use testcontainers PostgreSQL (automatic setup)
cargo test --test integration_test

# Database integration tests
cargo test --test db_integration_tests

# SSH session cleanup tests
cargo test --test test_ssh_session_cleanup
```

### E2E Tests

```bash
# API Server E2E tests (requires Docker daemon)
cargo test --test api_server_e2e

# With output
cargo test --test api_server_e2e -- --nocapture

# Keep containers for debugging
KEEP_TEST_CONTAINERS=true cargo test --test api_server_e2e
```

### All Tests

```bash
# Run all tests (unit + integration + E2E)
cargo test

# Run with Makefile (recommended)
make test
```

## Test Organization

### Directory Structure

```
dsb/
├── src/
│   ├── api/
│   │   ├── handlers/
│   │   │   ├── health/tests.rs          # Health handler tests (8 tests)
│   │   │   ├── activities/tests.rs      # Activities handler tests (18 tests)
│   │   │   └── ssh/tests.rs             # SSH handler tests (32 tests)
│   │   └── server/
│   │       └── tests.rs                 # API Server unit tests (78 tests)
│   ├── web_terminal/tests.rs             # Web terminal tests (28 tests)
│   ├── cli/
│   ├── core/
│   │   ├── activities/tests.rs
│   │   ├── sandbox/tests.rs
│   │   └── ssh/tests.rs
│   ├── docker/
│   │   └── manager/tests.rs
│   └── db/
│       └── */tests.rs
└── tests/
    ├── test_web_terminal.rs              # Web terminal integration tests
    └── api_server_e2e.rs                 # API E2E tests (9 tests)
```

### Test Module Pattern

Each module follows this pattern:

```rust
// In src/module.rs
pub mod module;

#[cfg(test)]
mod tests;

// In src/module/tests.rs
//! Unit tests for module

use super::*;

#[test]
fn test_example() {
    // Test code
}
```

## Running Tests

### Run All Tests

```bash
# Run all tests (library + integration)
cargo test

# Run only library unit tests
cargo test --lib

# Run only integration tests
cargo test --test
```

### Run Specific Test Suites

```bash
# API Server tests
cargo test api::server

# Web Terminal tests
cargo test web_terminal

# Core Sandbox tests
cargo test core::sandbox

# Docker Manager tests
cargo test docker::manager

# SSH Gateway tests (cd to ssh-gateway directory first)
cd ssh-gateway
cargo test --lib                          # Unit tests only
cargo test --test integration_tests      # Integration tests only
cargo test                               # All tests
```

### Run E2E Tests

```bash
# All E2E tests (requires Docker daemon)
cargo test --test api_server_e2e

# Specific E2E test
cargo test --test api_server_e2e test_create_sandbox

# Keep containers for debugging
KEEP_TEST_CONTAINERS=true cargo test --test api_server_e2e
```

### Run with Output

```bash
# Show test output
cargo test -- --nocapture

# Show test output for specific test
cargo test test_create_sandbox -- --nocapture

# Run single test with full output
cargo test test_create_sandbox -- --exact --nocapture
```

## Test Coverage

### Current Coverage

| Module | Unit Tests | Integration Tests | Status |
|--------|-----------|-------------------|--------|
| API Server | 78 | 9 | ✅ Complete |
| Health Handler | 8 | - | ✅ 100% |
| Activities Handler | 18 | - | ✅ Structs 100% |
| SSH Handler | 32 | - | ✅ Structs 100% |
| Web Terminal | 28 | 9 | ✅ Good |
| Docker Manager | 20+ | - | ✅ High |
| Core Sandbox | 10+ | - | 🔄 Medium |
| Core SSH | 5+ | - | 🔄 Low |
| Database modules | 10+ | - | 🔄 Low |
| SSH Gateway | 13 | 15 | ✅ Complete |

### Coverage Goals

- ✅ API Server: **75%** (achieved with 87 tests)
- 🔄 CLI Commands: **70%** (1,352 regions - pending)
- 🔄 Core Activities: **70%** (3.85% → pending)
- 🔄 Core SSH: **70%** (2.72% → pending)
- 🔄 Database modules: **60-70%** (pending)

## Writing Tests

### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_function() {
        // Arrange
        let input = "test";

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, "expected");
    }
}
```

### Async Test Example

```rust
#[tokio::test]
async fn test_async_function() {
    // Arrange
    let service = create_test_service().await;

    // Act
    let result = service.do_something().await;

    // Assert
    assert!(result.is_ok());
}
```

### Test Isolation with Mutex

```rust
use tokio::sync::Mutex;
use std::sync::OnceLock;

static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn env_mutex() -> &'static Mutex<()> {
    ENV_MUTEX.get_or_init(|| Mutex::new(()))
}

#[tokio::test]
async fn test_with_env_var() {
    let _guard = env_mutex().lock().await;
    std::env::set_var("TEST_VAR", "value");
    // Test code that uses environment variable
}
```

### Serialization Tests

```rust
#[test]
fn test_serialization() {
    let original = MyStruct { field: "value".to_string() };
    let json = serde_json::to_string(&original).unwrap();
    let deserialized: MyStruct = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.field, original.field);
}
```

## Integration Tests

### API Server E2E Tests

Location: `tests/api_server_e2e.rs`

**Features:**

- Real HTTP server on random port
- Actual Docker containers
- Automatic resource cleanup
- In-memory state store

**Example:**

```rust
#[tokio::test]
#[serial_test::serial]
async fn test_create_sandbox() {
    let mut server = setup_test_server().await;
    let client = TestClient::new(server.server_url.clone());

    let create_request = json!({
        "image": "python:3.12",
        "command": ["python", "-c", "print('hello')"]
    });

    let response = client.post_json("/sandboxes", &create_request).await;
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);

    let body: serde_json::Value = response.json().await.unwrap();
    if let Some(container_id) = body["container_id"].as_str() {
        server.cleanup_containers.push(container_id.to_string());
    }
}
```

### Resource Cleanup

```rust
struct TestServer {
    server_url: String,
    cleanup_containers: Vec<String>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if std::env::var("KEEP_TEST_CONTAINERS").is_ok() {
            return; // Keep containers for debugging
        }

        let containers = self.cleanup_containers.clone();
        tokio::spawn(async move {
            for container_id in containers {
                // Cleanup logic
            }
        });
    }
}
```

### SSH Gateway Tests

**Location:** `ssh-gateway/tests/integration_tests.rs`

**Prerequisites:**

- Docker daemon running and accessible
- Docker image: `python:3.12-slim` (or compatible)
- Docker socket: Default `unix:///var/run/docker.sock` or set `DOCKER_HOST`

**Running Tests:**

```bash
# From ssh-gateway directory
cd ssh-gateway

# Run all tests (unit + integration)
cargo test

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test integration_tests

# Run with custom Docker socket
export DOCKER_HOST=unix:///path/to/docker.sock
cargo test --test integration_tests

# Run with verbose output
cargo test -- --nocapture
```

**Test Coverage:**

- **Unit Tests** (13 tests): Configuration, connection state, Docker exec proxy basics
- **Integration Tests** (15 tests): Real container lifecycle, immediate output forwarding, bidirectional I/O, concurrent stress tests

**Key Test Scenarios:**

- ✅ Immediate Output Forwarding - Verifies output appears as generated (not buffered)
- ✅ Bidirectional Data Flow - Tests stdin → Docker → stdout roundtrip
- ✅ Concurrent Execs - Multiple exec instances on same container
- ✅ Background Cleanup - Proper task termination on exec completion
- ✅ Error Handling - Invalid containers, Docker errors, network issues
- ✅ Connection ID Uniqueness - Verifies each connection gets unique sequential ID
- ✅ Persistent Host Keys - Verifies host key persistence across server restarts
- ✅ Concurrent Stress Test - 100 concurrent connections without deadlock

**Example:**

```rust
#[tokio::test]
async fn test_immediate_output_forwarding() {
    let container_id = create_test_container("python:3.12.11").await;

    let mut exec_proxy = DockerExecProxy::new(container_id.clone());
    exec_proxy.create_exec().await.unwrap();
    exec_proxy.start_exec().await.unwrap();

    // Send command
    exec_proxy.write_stdin(b"echo test\n").await.unwrap();

    // Verify output appears immediately (not buffered)
    let start = std::time::Instant::now();
    let output = exec_proxy.read_output().await.unwrap().unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(100),
        "Output should appear immediately, but took {:?}", elapsed);
    assert!(output.contains(b"test"));
}
```

## Test Fixtures

DSB provides reusable test fixtures to simplify test setup and ensure consistency across the test suite.

### TestDatabase

**Location:** `tests/common/db_test_setup.rs`

For tests requiring PostgreSQL database:

- Uses testcontainers to run PostgreSQL in Docker
- Automatically runs database migrations
- Provides connection pooling
- Handles cleanup between tests

**Example:**

```rust
use tests::common::db_test_setup::TestDatabase;

#[tokio::test]
async fn test_with_database() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");

    // Use db.pool for queries
    let client = db.pool.get().await.unwrap();
    let rows = client.query("SELECT 1", &[]).await.unwrap();

    assert_eq!(rows.len(), 1);
}
```

**Used by:**

- Integration tests (`tests/integration_test.rs`)
- Database integration tests (`tests/db_integration_tests.rs`)
- SSH session cleanup tests (`tests/test_ssh_session_cleanup.rs`)

### TestDocker

**Location:** `tests/common/docker_test_setup.rs`

For tests requiring Docker but no database:

- Lightweight wrapper around system Docker daemon
- Provides default configuration (no .env.test needed)
- Manages Docker client lifecycle
- Simpler than TestDatabase for Docker-only tests

**Example:**

```rust
use tests::common::docker_test_setup::TestDocker;
use dsb::docker::DockerManager;

#[tokio::test]
async fn test_with_docker() {
    let test_docker = TestDocker::new().expect("Failed to create TestDocker");
    let docker_manager = DockerManager::new_with_config(&test_docker.config)
        .expect("Failed to create Docker manager");

    // Use docker_manager for container operations
    let containers = docker_manager.list_containers().await.unwrap();
    assert!(containers.is_ok());
}
```

**Used by:**

- SSH authorization tests (`tests/test_ssh_authorization.rs`)
- SSH gateway tests (`ssh-gateway/tests/integration_tests.rs`)

### Test Fixture Pattern

Both fixtures follow the same pattern:

```rust
pub struct TestFixture {
    // Resources (Docker client, database pool, etc.)
    pub resource: ResourceType,
}

impl TestFixture {
    /// Creates a new test instance with default configuration
    pub fn new() -> Result<Self, String> {
        // Setup logic using Config::default()
        // No .env.test file needed!
    }

    /// Optional: Helper methods for common operations
    pub async fn create_test_container(&self, name: &str) -> Result<String, String> {
        // Container creation logic
    }
}
```

### Meta-Tests

Tests that verify the test infrastructure itself:

- **TestDatabase fixture tests:** Verify PostgreSQL setup and cleanup
- **Helper function tests:** Verify database utility functions
- **Location:** `tests/common/db_test_setup.rs`, `tests/common/db_test_utils.rs`

These meta-tests run as part of the normal test suite (no `#[ignore]` attributes).

**Run meta-tests:**

```bash
# Run all meta-tests
cargo test --test test_database_setup

# Or use the Makefile target
make test-infra
```

**Why Meta-Tests Matter:**

Meta-tests ensure the test infrastructure is reliable. If a meta-test fails, it indicates a problem with the test setup itself, not the code being tested.



## Activity Tracking Tests

### Prerequisites

1. PostgreSQL database running
2. Database URL configured
3. Docker daemon running

### Setup

```bash
# Local database
export DATABASE_URL="postgresql://postgres:postgres@localhost:5433/dsb"

# Or with remote database
export DATABASE_URL="postgresql://postgres:postgres@host:5432/dsb"
```

### Activity Recording Test

```bash
# Create a sandbox
dsb create -i nginx:alpine -t 30

# Note the sandbox ID, e.g., abc-123-def

# Get sandbox info (records Info activity)
dsb info <sandbox-id>

# Get sandbox stats (records Stats activity)
dsb stats <sandbox-id>

# Execute command (records Exec activity)
dsb exec <sandbox-id> ls /

# Stop sandbox (records Stop activity)
dsb stop <sandbox-id>

# Delete sandbox (records Delete activity)
dsb delete <sandbox-id>
```

### CLI Commands Testing

```bash
# List all activities
dsb activities list

# List activities for a specific sandbox
dsb activities list --sandbox <sandbox-id>

# Show specific activity details
dsb activities show <activity-id>

# List sandboxes with activity information
dsb list --activity

# Cleanup inactive sandboxes (dry-run)
dsb activities cleanup-all --dry-run --timeout 30

# Cleanup inactive sandboxes (actual deletion)
dsb activities cleanup-all --timeout 30
```

### API Endpoints Testing

```bash
# List all activities
curl http://localhost:8080/activities

# List activities for specific sandbox
curl "http://localhost:8080/activities?sandbox_id=<sandbox-id>"

# Limit results
curl "http://localhost:8080/activities?limit=10"

# Filter by activity type
curl "http://localhost:8080/activities?activity_type=create"

# Get specific activity
curl http://localhost:8080/activities/<activity-id>

# List activities for a sandbox
curl http://localhost:8080/sandboxes/<sandbox-id>/activities

# Cleanup inactive sandboxes (dry-run)
curl -X POST "http://localhost:8080/activities/cleanup-all?dry_run=true&timeout=30"

# Cleanup inactive sandboxes (actual deletion)
curl -X POST "http://localhost:8080/activities/cleanup-all?timeout=30"
```

### Expected Behavior

All sandbox operations should be recorded:

- ✓ Create sandbox → Create activity
- ✓ Get sandbox info → Info activity
- ✓ Get sandbox stats → Stats activity
- ✓ Execute command → Exec activity
- ✓ Stop sandbox → Stop activity
- ✓ Delete sandbox → Delete activity

### Database Verification

```sql
-- Check activities table
SELECT * FROM sandbox_activities ORDER BY timestamp DESC LIMIT 10;

-- Count activities per sandbox
SELECT sandbox_id, COUNT(*) as activity_count
FROM sandbox_activities
GROUP BY sandbox_id
ORDER BY activity_count DESC;

-- Check activities for deleted sandboxes
SELECT * FROM sandbox_activities WHERE sandbox_is_deleted = true;

-- Find inactive sandboxes (no activity in last 30 minutes)
SELECT DISTINCT ON (s.sandbox_id) s.sandbox_id, s.timestamp
FROM sandbox_activities s
WHERE s.timestamp < NOW() - INTERVAL '30 minutes'
AND s.sandbox_is_deleted = false;
```

## Testing with Docker Compose

For testing against the full docker-compose stack (all services running):

### Prerequisites

```bash
# Start all services
docker compose up -d

# Verify services are running
docker compose ps
```

### Manual API Testing

```bash
# Create a sandbox via API
curl -X POST http://localhost:8080/sandboxes \
  -H "Content-Type: application/json" \
  -H "X-API-Key: test-admin-key" \
  -d '{
    "image": "dsb/sandbox:latest",
    "command": ["python3", "-c", "print(\"hello\")"],
    "timeout_minutes": 5
  }'

# List sandboxes
curl http://localhost:8080/sandboxes

# Get sandbox info
curl http://localhost:8080/sandboxes/<sandbox-id>

# List activities
curl http://localhost:8080/activities

# Stream sandbox logs
curl http://localhost:8080/sandboxes/<sandbox-id>/logs/stream
```

### Testing VNC Access

```bash
# VNC proxy requires the DSB server to be on the Docker network
# which docker-compose provides automatically

# Access VNC via noVNC in dashboard
open http://localhost:3001

# Or connect directly via WebSocket
wscat -c "ws://localhost:8080/vnc/<sandbox-id>?api_key=test-admin-key"
```

### Testing Web Terminal

```bash
# Web terminal WebSocket endpoint
wscat -c "ws://localhost:8080/terminal/<sandbox-id>?api_key=test-admin-key"

# Send commands
> ls /
> pwd
```

### Testing Dashboard

```bash
# Open dashboard in browser
open http://localhost:3001

# Dashboard provides:
# - Sandbox list and details
# - Create sandbox form
# - Resource stats (CPU, memory, network)
# - VNC desktop access
# - Web terminal access
```

### Cleanup

```bash
# Stop all services
docker compose down

# Remove volumes (deletes database data)
docker compose down -v
```

### When to Use Docker Compose for Testing

**Use docker-compose when:**

- Testing VNC proxy functionality (requires Docker network access)
- Testing WebSocket connections (terminal, VNC)
- Manual API testing with all dependencies
- Integration testing with real services
- Dashboard testing

**Use testcontainers when:**

- Running automated test suite
- Need isolated database per test
- Running tests in CI/CD
- Faster test execution (no service startup overhead)

## Testing the Kubernetes Backend

The Kubernetes backend is exercised end-to-end against a local
[kind](https://kind.sigs.k8s.io/) (Kubernetes-in-Docker) cluster.
The Helm chart at `deployment/helm/dsb/` is installed into the
cluster, the Sandbox CRD is registered, and the agent-tester
runs a real MCP conversation against a Kubernetes-backed DSB.

> **Why not in CI?** The full e2e cycle (kind cluster creation +
> NGINX ingress + Postgres + Helm install + image pre-pull) takes
> ~10–15 minutes and requires privileged Docker. It runs **on
> contributor machines and the release pipeline**, not on every PR.
> See [TESTING.md](TESTING.md) (this section) for the local
> workflow and [ROADMAP.md](ROADMAP.md) for the broader plan.

### Prerequisites

Install `kind` and `kubectl` (one-time per dev machine):

```bash
# macOS
brew install kind kubectl

# Linux
curl -Lo ./kind https://kind.sigs.k8s.io/dl/v0.24.0/kind-linux-amd64
chmod +x ./kind && sudo mv ./kind /usr/local/bin/
# (kubectl: https://kubernetes.io/docs/tasks/tools/)
```

Docker must be running (kind uses Docker as the node backend).

### Build the K8S-enabled binary

The DSB server image must be compiled with the `kubernetes` Cargo
feature. The default image (built with `make dc-build`) only has
the Docker backend.

```bash
# Set FEATURES when building
FEATURES=kubernetes make dc-build REGISTRY_PREFIX=docker.io/ TAG=latest
```

Or build a release binary locally for testing:

```bash
FEATURES=kubernetes cargo build --release --bin dsb
```

Without the `kubernetes` feature, the process exits with
"Kubernetes feature not enabled" when the backend is set to
`kubernetes`.

### Configure the K8S backend

`dsb.yaml.example` documents every K8S field. The minimum to
flip the backend is:

```yaml
sandbox:
  backend: "kubernetes"
  kubernetes:
    namespace: "dsb-sandboxes"
```

Or via env vars (equivalent, takes precedence over YAML):

```bash
export DSB_SANDBOX__BACKEND=kubernetes
export DSB_SANDBOX__KUBERNETES__NAMESPACE=dsb-sandboxes
```

The namespace must exist before the server starts; the K8S
service account must have create/get/delete on sandboxes,
pods, and services in that namespace. The Helm chart's
`templates/clusterrole.yaml` grants these.

### Run the e2e suite

```bash
# Full e2e: create kind cluster, install ingress + Postgres,
# install DSB Helm chart, run agent-tester
make test-k8s-e2e

# Run only the MCP-server-against-DSB-on-K8s scenario
make test-agent-k8s

# Tear down the kind cluster when done
make test-k8s-e2e-cleanup
make test-k8s-e2e-delete-cluster
```

The Makefile prints step-by-step progress (10 steps: cluster
→ ingress → postgres → RBAC → DSB image → Helm install →
image prepull → health check → run tests → summary).

### Troubleshooting

- **`kind create cluster` fails with "API server not ready"** —
  increase Docker resources (kind needs ~4GB RAM per node). On
  macOS, Docker Desktop → Settings → Resources.
- **`helm install` fails with "connection refused" on 8443** —
  NGINX ingress isn't fully up. Wait a minute and retry, or run
  `kubectl wait --for=condition=ready pod -n ingress-nginx -l
  app.kubernetes.io/component=controller --timeout=300s`.
- **`Kubernetes feature not enabled` on server boot** — the image
  was built without `FEATURES=kubernetes`. Rebuild with the right
  feature set (see above).
- **Sandbox pod stuck in `Pending`** — usually means the cluster
  has no nodes that match the pod's resource requests or
  nodeSelector. Run `kubectl describe pod -n dsb-sandboxes` for
  the scheduler's reason.

## Automated Docker-Compose Testing

For comprehensive integration testing in a production-like environment, DSB provides automated docker-compose testing that runs the test suite against a full docker-compose stack.

### Why Docker-Compose Testing?

**Advantages:**

- ✅ **Full Network Access**: DSB server runs on the same Docker network as sandbox containers
- ✅ **Platform Parity**: Tests run in Linux containers (same as production)
- ✅ **Real Services**: Tests against real PostgreSQL and Docker daemon
- ✅ **VNC/Terminal Testing**: Can properly test WebSocket-based features
- ✅ **Clean Environment**: Each test run gets a fresh environment

**Use Cases:**

- VNC proxy integration tests
- Web terminal integration tests
- Full-stack API testing
- Network feature testing
- Pre-production validation

### Test Configuration

The docker-compose test stack is defined in `docker-compose.test.yml`:

```yaml
services:
  dsb-server-test:    # DSB server (port 18080 to avoid conflicts)
  postgres-test:      # PostgreSQL for testing
  test-runner:        # Container that executes tests
```

Key differences from development stack:

- Uses port **18080** for DSB server (avoid conflicts with dev instance)
- Database name: **dsb_test** (isolated from dev database)
- Test runner container with Rust toolchain and test utilities
- Automatic health checks and service dependencies

### Running Docker-Compose Tests

#### Quick Start

```bash
# Run all docker-compose tests
make test-compose

# Run specific test categories
make test-compose-vnc              # VNC proxy tests
make test-compose-terminal         # Terminal tests
make test-compose-integration      # Integration tests
make test-compose-api              # API E2E tests
make test-compose-ssh              # SSH integration tests
```

#### Manual Execution

```bash
# 1. Start test stack
docker compose -f docker-compose.test.yml up -d

# 2. Wait for services to be healthy (automatic health checks)
docker compose -f docker-compose.test.yml ps

# 3. Run tests in test-runner container
docker compose -f docker-compose.test.yml exec test-runner \
  cargo test --test vnc_docker_compose_test -- --test-threads=1

# 4. Cleanup
docker compose -f docker-compose.test.yml down
```

### Test Files

Docker-compose specific tests are located in `tests/`:

- **`vnc_docker_compose_test.rs`** - VNC proxy integration tests
  - WebSocket connection establishment
  - API key authentication
  - Bidirectional data flow (WebSocket ↔ TCP ↔ VNC)
  - VNC server connectivity on Docker network

- **`terminal_docker_compose_test.rs`** - Terminal integration tests
  - WebSocket connection to terminal
  - Command execution and output
  - Terminal session lifecycle
  - Error handling

### Test Environment

**Environment Variables:**

```bash
# API endpoint (internal Docker network)
DSB_API_URL=http://dsb-server-test:8080

# API authentication
DSB_API_KEY=test-admin-key-for-testing-only

# Test mode flag
DOCKER_COMPOSE_TEST=true
```

**Test Utilities:**

The test runner container includes:

- Rust toolchain with test dependencies
- `curl` for HTTP testing
- `wscat` for WebSocket testing
- `netcat` for network testing
- Access to Docker socket via volume mount

### Writing Docker-Compose Tests

When writing tests for docker-compose environment:

1. **Use internal API URLs:**

   ```rust
   let api_url = std::env::var("DSB_API_URL")
       .unwrap_or_else(|_| "http://dsb-server-test:8080".to_string());
   ```

2. **Check for docker-compose mode:**

   ```rust
   let is_docker_compose = std::env::var("DOCKER_COMPOSE_TEST").is_ok();
   ```

3. **Handle WebSocket tests:**

   ```rust
   let ws_url = format!(
       "ws://{}/vnc/{}?api_key={}",
       api_url.replace("http://", "ws://"),
       sandbox_id,
       api_key()
   );
   ```

4. **Wait for service health:**

   ```rust
   // Wait for DSB server health endpoint
   wait_for_service(&format!("{}/health", api_url)).await?;
   ```

### Debugging Failed Tests

If docker-compose tests fail:

```bash
# 1. Check service status
docker compose -f docker-compose.test.yml ps

# 2. View logs
docker compose -f docker-compose.test.yml logs dsb-server-test

# 3. Keep stack running for inspection
docker compose -f docker-compose.test.yml up

# 4. Access test-runner container shell
docker compose -f docker-compose.test.yml exec test-runner bash

# 5. Run tests manually inside container
cd /app
cargo test --test vnc_docker_compose_test -- --test-threads=1 --nocapture
```

### CI/CD Integration

For CI/CD pipelines:

```yaml
# Example GitHub Actions workflow
- name: Run docker-compose tests
  run: |
    docker compose -f docker-compose.test.yml up -d
    docker compose -f docker-compose.test.yml exec test-runner \
      cargo test --workspace -- --test-threads=1
    docker compose -f docker-compose.test.yml down
```

### Performance Considerations

**Startup Time:** ~30-60 seconds for service startup and health checks
**Test Execution:** Similar to local tests (runs in Linux container)
**Cleanup:** ~5 seconds to stop and remove containers

**Tips:**

- Run docker-compose tests less frequently than unit/integration tests
- Use in pre-production validation or nightly builds
- Keep test stack running during development to avoid startup overhead

### Troubleshooting

**Port conflicts:**

```bash
# If port 18080 is in use, change it in docker-compose.test.yml:
# dsb-server-test:
#   ports:
#     - "18081:8080"  # Use different port
```

**Container name conflicts:**

```bash
# If container names conflict, clean up first:
docker compose -f docker-compose.test.yml down
docker rm -f dsb-server-test dsb-postgres-test dsb-test-runner
```

**Test timeout:**

```bash
# Increase wait time in Makefile if services take longer to start:
# @sleep 10  # Instead of @sleep 5
```

## Test Helpers

### Mock Docker Responses

For tests that don't need real Docker:

```rust
#[test]
fn test_with_mock_docker() {
    // Test logic that doesn't require actual Docker
    let config = Config::default();
    assert!(config.docker.registry == "docker.io");
}
```

### Test Fixtures

```rust
fn create_test_sandbox() -> Sandbox {
    Sandbox {
        id: Uuid::new_v4(),
        state: SandboxState::Running,
        container_id: Some("test-container".to_string()),
        ..Default::default()
    }
}
```

### Environment Variable Setup

```rust
#[tokio::test]
async fn test_with_api_key() {
    let _guard = env_mutex().lock().await;
    std::env::set_var("API_KEY", "test-key");

    let result = validate_api_key(&Some("test-key".to_string()));
    assert!(result.is_ok());
}
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest

    services:
      postgres:
        image: postgres:16
        env:
          POSTGRES_PASSWORD: postgres
          POSTGRES_DB: dsb_test
        options: >-
          --health-cmd pg_isready
          --health-interval 10s
          --health-timeout 5s
          --health-retries 5
        ports:
          - 5432:5432

    steps:
      - uses: actions/checkout@v3

      - uses: actions-rust-lang/setup-rust-toolchain@v1

      - name: Install Docker dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y docker-ce docker-ce-cli containerd.io

      - name: Run unit tests
        run: cargo test --lib

      - name: Run E2E tests
        run: cargo test --test api_server_e2e
        env:
          DOCKER_HOST: unix:///var/run/docker.sock
```

### Pre-commit Hooks

Create `.git/hooks/pre-commit`:

```bash
#!/bin/bash
echo "Running tests..."
cargo test --quiet --lib
if [ $? -ne 0 ]; then
    echo "❌ Tests failed"
    exit 1
fi
echo "✅ All tests passed"
```

## Best Practices

### 1. Test Isolation

Each test should be independent:

```rust
#[tokio::test]
async fn test_independent() {
    // Setup fresh state for each test
    let state = StateStore::new();
    // Don't rely on other tests
}
```

### 2. Descriptive Names

```rust
// Good
#[test]
fn test_create_sandbox_with_invalid_image_returns_error() {
    // Clear what is being tested
}

// Bad
#[test]
fn test_error() {
    // Vague - what error?
}
```

### 3. Arrange-Act-Assert Pattern

```rust
#[test]
fn test_sandbox_creation() {
    // Arrange
    let service = create_service();
    let request = CreateRequest { image: "test".to_string() };

    // Act
    let result = service.create_sandbox(request).await;

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap().state, SandboxState::Creating);
}
```

### 4. Test Edge Cases

```rust
#[test]
fn test_with_empty_string() {
    let result = parse_config("");
    assert!(result.is_err());
}

#[test]
fn test_with_unicode() {
    let input = "测试";
    let result = process(input);
    assert_eq!(result, "expected");
}

#[test]
fn test_boundary_values() {
    assert_eq!(process(0), "min");
    assert_eq!(process(u16::MAX), "max");
}
```

### 5. Use Serial Tests When Needed

```rust
#[tokio::test]
#[serial_test::serial]
async fn test_with_shared_resource() {
    // For tests that can't run in parallel
    let _guard = env_mutex().lock().await;
    // Test code
}
```

## Troubleshooting

### Test Fails with "Docker not running"

**Problem:** Tests require Docker daemon

**Solution:**

```bash
# Start Docker
sudo systemctl start docker  # Linux
open -a Docker              # macOS

# Or set DOCKER_HOST
export DOCKER_HOST=unix:///var/run/docker.sock
```

### Test Fails with "Address already in use"

**Problem:** Port conflict

**Solution:**

```bash
# Find what's using the port
lsof -i :8080

# Kill the process
kill -9 <PID>

# Or use random ports in tests (we do this automatically)
```

### Tests Are Slow

**Problem:** Too many Docker operations

**Solutions:**

1. Run unit tests only: `cargo test --lib`
2. Use `--test-threads=1` for sequential execution
3. Mock Docker responses where possible

### E2E Tests Leave Containers Running

**Problem:** Containers not cleaned up

**Solutions:**

```bash
# List all containers
docker ps -a

# Remove all test containers
docker rm -f $(docker ps -aq)

# Or use the KEEP_TEST_CONTAINERS env var for debugging
KEEP_TEST_CONTAINERS=true cargo test --test api_server_e2e
```

### Activities Not Being Recorded

**Problem:** Activity tracking not working

**Solutions:**

1. Check PostgreSQL connection
2. Verify `sandbox_activities` table exists
3. Check logs for errors: `RUST_LOG=DEBUG`

## Test Metrics

### Coverage Tracking

To track coverage improvements:

```bash
# Install llvm-tools
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov

# Generate coverage report
cargo llvm-cov --html

# View report
open target/llvm-cov/html/index.html
```

### Test Execution Analysis

```bash
# Time each test
cargo test -- --nocapture --test-threads=1

# Find slow tests
cargo test -- --nocapture --test-threads=1 2>&1 | grep "test result"
```

## Contributing Tests

When adding new features:

1. **Unit Tests First** - Test individual components
2. **Integration Tests** - Test module interactions
3. **E2E Tests** - Test complete workflows
4. **Update Documentation** - Document new tests
5. **Check Coverage** - Ensure >70% coverage goal

### Test Checklist

- [ ] Tests follow naming convention (`test_<function>_<scenario>`)
- [ ] Each test is independent
- [ ] Edge cases are covered
- [ ] Error conditions are tested
- [ ] Resources are cleaned up
- [ ] Test is documented (if complex)
- [ ] Coverage increased or maintained

## Quick Reference

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests in file
cargo test --lib api::server::tests

# Run with output
cargo test -- --nocapture

# Run E2E tests
cargo test --test api_server_e2e

# Debug test containers
KEEP_TEST_CONTAINERS=true cargo test --test api_server_e2e

# Generate coverage
cargo llvm-cov --html
```

## Database Schema

### sandbox_activities Table

```sql
CREATE TABLE sandbox_activities (
    id UUID PRIMARY KEY,
    sandbox_id UUID NOT NULL,
    activity_type TEXT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    details JSONB DEFAULT '{}'::jsonb,
    sandbox_is_deleted BOOLEAN DEFAULT FALSE
);

-- Indexes for performance
CREATE INDEX idx_sandbox_activities_sandbox_id ON sandbox_activities(sandbox_id);
CREATE INDEX idx_sandbox_activities_timestamp ON sandbox_activities(timestamp DESC);
CREATE INDEX idx_sandbox_activities_sandbox_timestamp ON sandbox_activities(sandbox_id, timestamp DESC);
CREATE INDEX idx_sandbox_activities_type ON sandbox_activities(activity_type);
CREATE INDEX idx_sandbox_activities_active ON sandbox_activities(sandbox_id, timestamp DESC)
WHERE sandbox_is_deleted = FALSE;
```

## API Response Examples

### List Activities Response

```json
[
  {
    "id": "abc-123-def",
    "sandbox_id": "sandbox-123",
    "activity_type": "create",
    "timestamp": "2025-12-29T13:52:29Z",
    "details": {
      "image": "nginx:alpine",
      "timeout_minutes": 30
    }
  }
]
```

### Cleanup Response

```json
{
  "message": "Cleanup complete: 2 sandboxes cleaned",
  "cleaned": 2,
  "dry_run": false
}
```

## Resources

- [Rust Testing Book](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Tokio Testing](https://tokio.rs/tokio/topics/testing)
- [Axum Testing](https://docs.rs/axum/latest/axum/index.html#testing)
- [Cargo Test Documentation](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
