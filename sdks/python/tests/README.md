# Python SDK Test Infrastructure

## Overview

The Python SDK tests use pytest fixtures with automatic resource cleanup, ensuring no test resources leak even when tests fail or raise exceptions.

## Architecture

```python
# conftest.py - Automatic cleanup fixture
@pytest.fixture(autouse=True, scope="function")
def auto_cleanup_test_sandboxes():
    """Automatically clean up test sandboxes after each test."""
    yield
    # Cleanup runs even if test fails
    cleanup_test_sandboxes(prefix="test-")
```

## Key Features

### ✅ Automatic Cleanup
- `autouse=True` - runs automatically for every test
- `scope="function"` - cleans up after each test function
- Cleanup runs even if test fails or raises exception
- No manual cleanup needed in tests

### ✅ Unique Naming
Tests use UUID-based names to avoid conflicts:
```python
sandbox_name = f"test-{test_name}-{uuid.uuid4()}"
```

### ✅ Server Lifecycle
Server runs in context manager for automatic startup/shutdown:
```python
@pytest.fixture(scope="session")
def dsb_server():
    with DsbServerManager() as server:
        yield server
    # Automatic cleanup
```

## Test Pattern

### Creating Tests

```python
def test_sandbox_creation(dsb_server):
    """Test sandbox creation with automatic cleanup."""
    # No need to manually cleanup - fixture handles it
    sandbox = dsb_server.create_sandbox(...)
    assert sandbox.id is not None
    # Cleanup happens automatically when test exits
```

### Cleanup Behavior

```python
def test_with_failure(dsb_server):
    """Cleanup runs even when test fails."""
    sandbox = dsb_server.create_sandbox(name="test-fail-123")
    assert False, "Test fails!"
    # Sandbox still gets cleaned up by fixture
```

## Benefits

1. **No Resource Leaks** - Fixtures guarantee cleanup
2. **Simple Tests** - No manual cleanup code needed
3. **Reliable** - Works even with test failures
4. **Isolated** - Each test gets fresh environment

## Best Practices

1. **Always use "test-" prefix** for sandbox names
2. **Let the fixture handle cleanup** - don't manually delete
3. **Use unique names** - include UUID or test function name
4. **Test failure scenarios** - fixture still cleans up

## Migration Guide for Rust Tests

When writing Rust tests, follow this pattern:

```rust
// ❌ OLD: Manual cleanup (doesn't run on panic)
#[tokio::test]
async fn test_something() {
    let sandbox = create_sandbox().await;
    // Test code...
    delete_sandbox(sandbox).await; // Won't run if test panics!
}

// ✅ NEW: ResourceRegistry with panic hook
#[tokio::test]
async fn test_something() {
    let mut registry = ResourceRegistry::new();
    setup_panic_hook_with_verification(registry.clone()).await;

    let sandbox_id = create_test_sandbox_auto_cleanup(
        &mut registry,
        &client,
        "test_something"
    ).await.expect("Failed to create sandbox");

    // Test code...
    // Automatic cleanup via Drop or explicit call
    registry.cleanup_all().await.expect("Cleanup failed");
    registry.verify_cleanup().await.expect("Resources leaked");
}
```

## Comparison: Python vs Rust

| Feature | Python (pytest) | Rust (ResourceRegistry) |
|---------|-----------------|-------------------------|
| Automatic cleanup | ✅ autouse fixture | ✅ Drop trait |
| Panic safety | ✅ yield + cleanup | ✅ panic hook |
| Unique naming | ✅ UUID in names | ✅ UUID in names |
| Verification | ⚠️ Manual check | ✅ verify_cleanup() |
| Resource tracking | ⚠️ Post-cleanup scan | ✅ Real-time tracking |

## Related Files

- `sdks/python/tests/conftest.py` - Pytest fixtures
- `sdks/python/tests/dsb_server_manager.py` - Server lifecycle
- `tests/common/resource_registry.rs` - Rust equivalent
- `tests/common/test_panic_hook.rs` - Rust panic handling
