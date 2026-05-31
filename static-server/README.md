# DSB Static File Server

⚠️ **STATUS: Architecture Backbone Only**

This is a placeholder workspace member for future extraction of static file serving from the main DSB server.

## Purpose

Enable standalone deployment of static file serving with:

- Shared storage with main DSB server
- Optional authentication via DSB API
- Independent scaling capabilities
- Clear extraction path from embedded server

## Current State

**What Exists:**

- ✅ Workspace member structure
- ✅ Cargo.toml with dependencies
- ✅ Main binary skeleton with argument parsing
- ✅ Library exports structure
- ✅ Placeholder modules

**What's NOT Implemented:**

- ❌ HTTP handlers (currently in main DSB: `src/api/handlers/static_files.rs`)
- ❌ File serving logic
- ❌ Authentication integration
- ❌ Configuration implementation beyond skeleton

## Architecture Design

```
┌─────────────────────┐
│ Main DSB API :8080  │
│ (publishes files)   │
└──────────┬──────────┘
           │
           ▼
    ┌─────────────────┐
    │ Shared Storage  │
    │ /var/lib/dsb/   │
    │ static-files/   │
    │   {sandbox_id}/ │
    └────────┬────────┘
             │
             ▼
┌─────────────────────┐
│ Static Server :8081 │
│ (serves files)      │
│ ⚠️ Not implemented │
└─────────────────────┘
```

## Future Extraction Plan

When ready to extract:

1. **Phase 1**: Copy handlers from main DSB → static-server/src/handlers.rs
2. **Phase 2**: Implement authentication (optional DSB API validation)
3. **Phase 3**: Set up Axum router with routes
4. **Phase 4**: Deploy and test standalone server
5. **Phase 5**: Remove static file routes from main DSB API
6. **Phase 6**: Update documentation and deployment guides

## Usage (Future)

```bash
# Not yet implemented - this is the planned interface
static-server --port 8081
static-server --dsb-api-url http://localhost:8080 --api-key secret
```

## Development Status

- **Phase 1**: ✅ Architecture backbone (CURRENT)
- **Phase 2**: ❌ Core implementation (FUTURE)
- **Phase 3**: ❌ Authentication integration (FUTURE)
- **Phase 4**: ❌ Extraction from main DSB (FUTURE)

## See Also

- Main implementation: `src/api/handlers/static_files.rs`
- Configuration: `src/config/types.rs::StaticServerConfig`
- SSH Gateway pattern: `ssh-gateway/` (similar extraction)
