# API Authentication Documentation

Complete guide to DSB API authentication system.

## Table of Contents

- [Overview](#overview)
- [Authentication Flow](#authentication-flow)
- [Endpoints](#endpoints)
- [Request Format](#request-format)
- [Response Format](#response-format)
- [Error Responses](#error-responses)
- [Security Considerations](#security-considerations)

## Overview

DSB uses API key-based authentication with support for multiple key sources:

1. **Admin API Key** - For administrative operations
2. **Database API Keys** - Dynamically managed keys with rich metadata
3. **Legacy Config Key** - Single configuration file key (backward compatibility)

### Key Features

- ✅ **Optional Authentication** - Can be disabled for development
- ✅ **Multiple Keys** - Support for unlimited concurrent API keys
- ✅ **Rich Metadata** - Keys include name, description, expiration, and usage tracking
- ✅ **Admin APIs** - Full CRUD operations for key management
- ✅ **Secure Storage** - Bcrypt hashing with cost factor 12
- ✅ **Health Endpoint** - Always accessible without authentication

## Authentication Flow

```mermaid
flowchart TD
    A[Client Request] --> B{Has X-API-Key?}
    B -->|No| C[Return 401 Unauthorized]
    B -->|Yes| D{require_auth?}
    D -->|false| E[Allow Request]
    D -->|true| F{/health endpoint?}
    F -->|Yes| E
    F -->|No| G{Admin API Key?}
    G -->|Yes| H{Valid?}
    G -->|No| I{Database Key?}
    H -->|Yes| E
    H -->|No| I
    I -->|Yes| J{Valid & Active?}
    I -->|No| K{Legacy Config Key?}
    J -->|Yes| L[Update last_used_at]
    J -->|No| C
    L --> E
    K -->|Yes| M{Valid?}
    K -->|No| C
    M -->|Yes| E
    M -->|No| C

    style E fill:#e8f5e9
    style C fill:#ffebee
```

### Authentication Sources (Checked in Order)

1. **Admin API Key** (`server.admin_api_key`)
   - Used for `/admin/*` endpoints
   - Can also access regular endpoints

2. **Database Keys** (PostgreSQL)
   - Created via admin API
   - Include metadata (name, description, expiration)
   - Include `last_used_at` tracking

3. **Legacy Config Key** (`server.api_key`)
   - Single key from configuration
   - Backward compatibility with existing deployments

### Special Cases

#### Health Endpoint

The `/health` endpoint is **always** accessible without authentication:

```bash
# Works without API key, even when require_auth=true
curl http://localhost:8080/health
```

#### Authentication Disabled

When `require_auth=false` (default), **all requests** are allowed:

```bash
export DSB_SERVER__REQUIRE_AUTH=false
curl http://localhost:8080/sandboxes  # Works without API key
```

## Endpoints

### Public Endpoints

| Method | Endpoint | Auth Required | Description |
|--------|----------|---------------|-------------|
| GET | `/health` | ❌ No | Health check |

### Protected Endpoints (Require API Key)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/sandboxes` | List all sandboxes |
| POST | `/sandboxes` | Create new sandbox |
| GET | `/sandboxes/{id}` | Get sandbox details |
| DELETE | `/sandboxes/{id}` | Delete sandbox |
| POST | `/sandboxes/{id}/stop` | Stop sandbox |
| POST | `/sandboxes/{id}/exec` | Execute command |
| GET | `/sandboxes/{id}/stats` | Get statistics |
| GET | `/activities` | List activities |
| GET | `/ssh-sessions` | List SSH sessions |
| POST | `/ssh-sessions` | Create SSH session |
| GET | `/images` | List images |
| POST | `/images/pull` | Pull image |

### Admin Endpoints (Require Admin API Key)

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/admin/api-keys` | Create new API key |
| GET | `/admin/api-keys` | List all API keys |
| GET | `/admin/api-keys/{id}` | Get specific API key |
| DELETE | `/admin/api-keys/{id}` | Delete API key |
| POST | `/admin/api-keys/{id}/rotate` | Rotate API key |

## Request Format

### Authentication Header

All authenticated requests must include the `X-API-Key` header:

```bash
curl http://localhost:8080/sandboxes \
  -H "X-API-Key: dsb_pk_7xK9Mn2PqR4tY6VwZ8aBcDeFgHiJkLmNoPqRsTuVw"
```

### Case Insensitivity

The `X-API-Key` header is case-insensitive (HTTP standard):

```bash
# All of these work
curl -H "x-api-key: KEY" ...
curl -H "X-API-KEY: KEY" ...
curl -H "X-Api-Key: KEY" ...
```

## Response Format

### Success Responses

#### Create API Key

**Request:**

```bash
POST /admin/api-keys
X-API-Key: dsb_admin_...
Content-Type: application/json

{
  "name": "CLI Key",
  "description": "API key for CLI access",
  "scopes": ["sandbox:read", "sandbox:write"],
  "expires_in_days": 365
}
```

**Response:** `201 Created`

```json
{
  "api_key": "dsb_pk_7xK9Mn2PqR4tY6VwZ8aBcDeFgHiJkLmNoPqRsTuVw",
  "key": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "key_prefix": "dsb_pk_7x",
    "name": "CLI Key",
    "description": "API key for CLI access",
    "scopes": ["sandbox:read", "sandbox:write"],
    "is_active": true,
    "created_at": "2026-01-14T11:22:53Z",
    "expires_at": "2027-01-14T11:22:53Z",
    "last_used_at": null,
    "created_by": null
  }
}
```

**Important:** Save the `api_key` value immediately - you won't be able to see it again!

#### List API Keys

**Request:**

```bash
GET /admin/api-keys
X-API-Key: dsb_admin_...
```

**Response:** `200 OK`

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "key_prefix": "dsb_pk_7x",
    "name": "CLI Key",
    "description": "API key for CLI access",
    "scopes": ["sandbox:read", "sandbox:write"],
    "is_active": true,
    "created_at": "2026-01-14T11:22:53Z",
    "expires_at": "2027-01-14T11:22:53Z",
    "last_used_at": "2026-01-14T12:30:00Z",
    "created_by": "admin@example.com"
  }
]
```

**Note:** The `key_hash` field is never exposed in API responses.

#### Rotate API Key

**Request:**

```bash
POST /admin/api-keys/550e8400-e29b-41d4-a716-446655440000/rotate
X-API-Key: dsb_admin_...
```

**Response:** `200 OK`

```json
{
  "api_key": "dsb_pk_Q2wE4rT6yU8iO0pAsDfGhJkLmNoPqRsTuVwXyZ",
  "key": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "key_prefix": "dsb_pk_Q2",
    "name": "CLI Key",
    "description": "API key for CLI access",
    "scopes": ["sandbox:read", "sandbox:write"],
    "is_active": true,
    "created_at": "2026-01-14T11:22:53Z",
    "expires_at": "2027-01-14T11:22:53Z",
    "last_used_at": null,
    "created_by": "admin@example.com"
  }
}
```

**Note:** The old key becomes invalid immediately after rotation.

## Error Responses

### 401 Unauthorized

Returned when authentication is required but not provided or invalid.

```json
{
  "error": "Unauthorized",
  "message": "Valid API key required"
}
```

**Causes:**
- Missing `X-API-Key` header
- Invalid API key
- Expired API key
- Inactive API key

### 404 Not Found

Returned when requesting a non-existent API key:

```json
{
  "error": "Not Found",
  "message": "API key not found"
}
```

## Security Considerations

### Key Storage

- **Never log API keys** - Server logs never contain full API keys or hashes
- **Bcrypt hashing** - Keys are hashed with cost factor 12 before storage
- **No plaintext storage** - Only the hash is stored in the database

### Key Format

```
dsb_pk_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

- **Prefix**: `dsb_pk_` identifies it as a DSB public key
- **Secret**: 32 alphanumeric characters (randomly generated)
- **Entropy**: ~190 bits of entropy

### Key Lifecycle

1. **Creation** - Key is generated and returned **once** in the response
2. **Usage** - Key is validated against hash, `last_used_at` is updated
3. **Expiration** - Expired keys are automatically rejected
4. **Rotation** - Old key invalidated immediately, new key generated
5. **Deletion** - Key is permanently removed from database

### Best Practices

#### For Users

1. **Use Environment Variables** - Store keys in `.env` files, not code
2. **Rotate Regularly** - Use the rotate endpoint periodically
3. **Set Expiration** - Use `expires_in_days` to limit key lifetime
4. **Monitor Usage** - Check `last_used_at` to identify unused keys
5. **Separate Environments** - Use different keys for dev/staging/production

#### For Developers

1. **Never Hardcode Keys** - Always use environment variables
2. **Use HTTPS** - Prevent key interception in production
3. **Implement Key Rotation** - Build rotation into your deployment process
4. **Audit Access** - Regularly review `last_used_at` timestamps
5. **Limit Key Scope** - Only grant necessary permissions (when scopes are enforced)

#### For Administrators

1. **Protect Admin Key** - The admin key can create/delete other keys
2. **Use Strong Admin Keys** - Generate with high entropy
3. **Regular Audits** - Review all API keys monthly
4. **Revoke Unused Keys** - Delete keys with stale `last_used_at`
5. **Enable Authentication** - Use `require_auth=true` in production

### Configuration Examples

#### Development (Auth Disabled)

```yaml
server:
  require_auth: false  # Allow all requests
```

```bash
# No API key needed
curl http://localhost:8080/sandboxes
```

#### Production (Auth Enabled)

```yaml
server:
  require_auth: true
  admin_api_key: "${DSB_SERVER__ADMIN_API_KEY}"  # From environment

database:
  url: "postgresql://user:pass@localhost:5432/dsb"
```

```bash
# Set admin key
export DSB_SERVER__ADMIN_API_KEY="dsb_admin_$(openssl rand -base64 32)"

# Start server
dsb server

# Create API keys for clients
ADMIN_KEY="dsb_admin_..."
curl -X POST http://localhost:8080/admin/api-keys \
  -H "X-API-Key: $ADMIN_KEY" \
  -d '{"name": "CLI Key"}'
```

## Migration Guide

### From No Authentication to Multi-Key

```bash
# 1. Set admin key
export DSB_SERVER__REQUIRE_AUTH=true
export DSB_SERVER__ADMIN_API_KEY="dsb_admin_new_key"

# 2. Restart server
dsb server

# 3. Create keys for each client
ADMIN_KEY="dsb_admin_new_key"

# CLI
curl -X POST http://localhost:8080/admin/api-keys \
  -H "X-API-Key: $ADMIN_KEY" \
  -d '{"name": "CLI", "scopes": ["sandbox:all"]}'

# Dashboard
curl -X POST http://localhost:8080/admin/api-keys \
  -H "X-API-Key: $ADMIN_KEY" \
  -d '{"name": "Dashboard", "scopes": ["sandbox:read"]}'

# Python SDK
curl -X POST http://localhost:8080/admin/api-keys \
  -H "X-API-Key: $ADMIN_KEY" \
  -d '{"name": "Python SDK", "scopes": ["sandbox:all"]}'
```

### From Single Key to Multi-Key

```bash
# 1. Add admin key (keep existing key temporarily)
export DSB_SERVER__API_KEY="existing_key"
export DSB_SERVER__REQUIRE_AUTH=true
export DSB_SERVER__ADMIN_API_KEY="dsb_admin_new"

# 2. Restart server (existing clients still work)
dsb server

# 3. Create new database keys
# 4. Update clients to use new keys
# 5. Remove legacy key
export DSB_SERVER__API_KEY=""
```

## API Reference

### Create API Key

`POST /admin/api-keys`

Creates a new API key and returns it (shown only once).

**Request Body:**

```json
{
  "name": "string (required)",
  "description": "string (optional)",
  "scopes": ["string"] (optional),
  "expires_in_days": "number (optional)",
  "created_by": "string (optional)"
}
```

**Response:** `201 Created`

Returns the complete API key (shown only once).

### List API Keys

`GET /admin/api-keys`

Lists all API keys with metadata (excluding actual keys).

**Response:** `200 OK`

Array of API key objects.

### Get API Key

`GET /admin/api-keys/{id}`

Gets a specific API key by ID.

**Response:** `200 OK` or `404 Not Found`

### Delete API Key

`DELETE /admin/api-keys/{id}`

Permanently deletes an API key.

**Response:** `204 No Content` or `404 Not Found`

### Rotate API Key

`POST /admin/api-keys/{id}/rotate`

Generates a new API key for the same metadata. The old key is invalidated immediately.

**Response:** `200 OK`

Returns the new API key (shown only once).

## Testing

### Testing Authentication

```bash
# Test without auth (should work when require_auth=false)
curl http://localhost:8080/health
curl http://localhost:8080/sandboxes

# Test with auth (when require_auth=true)
curl http://localhost:8080/sandboxes \
  -H "X-API-Key: dsb_pk_your-key-here"

# Test health endpoint (always works)
curl http://localhost:8080/health
```

### Testing Admin API

```bash
export ADMIN_KEY="dsb_admin_your-admin-key"

# Create key
curl -X POST http://localhost:8080/admin/api-keys \
  -H "X-API-Key: $ADMIN_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "Test Key"}'

# List keys
curl http://localhost:8080/admin/api-keys \
  -H "X-API-Key: $ADMIN_KEY"

# Rotate key
curl -X POST http://localhost:8080/admin/api-keys/{id}/rotate \
  -H "X-API-Key: $ADMIN_KEY"

# Delete key
curl -X DELETE http://localhost:8080/admin/api-keys/{id} \
  -H "X-API-Key: $ADMIN_KEY"
```

## Support

For issues or questions about authentication:

- Check the [troubleshooting section](../README.md#troubleshooting) in the main README
- Review [integration tests](../../tests/api_key_integration_tests.rs) for examples
- Open an issue on GitHub

## See Also

- [Main README](../../README.md) - Complete DSB documentation
- [Static File Serving](../static_serving/STATIC_SERVING.md) - Static file API documentation
- [API Examples](../../examples/) - Code examples
