# Test Common Utilities

Shared infrastructure for DSB integration tests.

## Modules

### `db_test_setup.rs` - Test Database Setup

Provides `TestDatabase` for running PostgreSQL in Docker containers during tests.

```rust
use tests::common::db_test_setup::TestDatabase;

#[tokio::test]
async fn my_test() {
    let db = TestDatabase::new().await.unwrap();

    // Use db.pool for queries
    let client = db.pool.get().await.unwrap();

    // Cleanup test data
    db.cleanup_data().await.unwrap();
}
```

**Key Features:**
- Automatically starts PostgreSQL in Docker
- Runs database migrations
- Provides connection pooling
- `cleanup_data()` method with CASCADE support
- Gracefully handles missing tables

### `resource_registry.rs` - Centralized Resource Tracking

Provides `ResourceRegistry` for tracking and cleaning up test resources.

```rust
use tests::common::resource_registry::{ResourceRegistry, ResourceType};

#[tokio::test]
async fn my_test() {
    let registry = ResourceRegistry::new("my_test");

    // Register a container for cleanup
    registry.register(
        "container-123",
        ResourceType::Container,
        "test container".to_string(),
        || Box::pin(async {
            delete_container("container-123").await.map_err(|e| e.to_string())
        })
    );

    // ... test code ...

    // Cleanup all resources
    let result = registry.cleanup_all(30).await;
    assert!(result.is_success());
}
```

**Key Features:**
- Track containers, sandboxes, SSH sessions, database records
- Timeout support for stuck resources
- Detailed error reporting (failed/timeout counts)
- Drop trait for emergency cleanup
- Order-preserving cleanup

### `test_panic_hook.rs` - Panic Cleanup Handler

Provides panic hook that ensures cleanup runs even when tests panic.

```rust
use tests::common::test_panic_hook::{install_test_panic_hook, register_panic_cleanup};

#[tokio::test]
async fn my_test() {
    install_test_panic_hook();

    // Register cleanup on panic
    register_panic_cleanup(|| {
        println!("Cleaning up after panic!");
    });

    // Test code that might panic
    panic!("Oh no!");
    // Cleanup will still run!
}
```

**Key Features:**
- Runs cleanup functions before standard panic handlers
- `register_panic_cleanup()` for sync cleanup
- `register_async_panic_cleanup()` for async cleanup
- `register_registry_cleanup()` for ResourceRegistry cleanup

### `docker_test_setup.rs` - Docker Test Utilities

Utilities for working with Docker in tests.

### `test_config.rs` - Test Configuration

Configuration loading from environment variables.

### `test_setup.rs` - Shared Test Setup

Shared `#[ctor]`-based setup for test binaries that need the env mutex, sandbox image defaults, or other one-time initialization.

### `testcontainers_postgres.rs` - Testcontainers Postgres

PostgreSQL fixture specifically for tests that need an isolated database with migrations applied.

### `server_fixture.rs` - Server Test Fixture

Spin up the DSB server in-process for integration tests. Used by API E2E tests that need a real Axum router without docker-compose.

## Usage Guidelines

### 1. Always Use ResourceRegistry

Every test that creates resources (containers, sandboxes, SSH sessions) MUST use `ResourceRegistry`.

```rust
let registry = ResourceRegistry::new(test_name);
// Register all resources
let result = registry.cleanup_all(30).await;
assert!(result.is_success());
```

### 2. Install Panic Hook for Safety

For tests that might panic, install the panic hook to ensure cleanup runs.

```rust
install_test_panic_hook();
register_registry_cleanup(registry_arc, 30);
```

### 3. Use Appropriate Timeouts

Set reasonable timeouts based on your cleanup operations:

- Fast operations: 5-10 seconds
- Normal operations: 30 seconds
- Slow operations: 60+ seconds

### 4. Handle Cleanup Failures

Check cleanup results and handle failures appropriately:

```rust
let result = registry.cleanup_all(30).await;
if !result.is_success() {
    eprintln!("Cleanup failures: {:?}", result.failed);
    eprintln!("Cleanup timeouts: {:?}", result.timed_out);
}
assert!(result.is_success(), "Cleanup failed");
```

## Testing

Run verification tests to ensure cleanup infrastructure works:

```bash
cargo test -p dsb --test test_cleanup_verification
```

This runs meta-tests that verify:
- ResourceRegistry cleanup
- Panic hook cleanup
- Database cleanup
- Timeout handling
- Partial failure handling
