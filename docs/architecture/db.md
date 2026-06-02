# Database Module

The Database module provides PostgreSQL persistence for DSB using deadpool-postgres for connection pooling and tokio-postgres for async database operations.

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Schema Design](#schema-design)
4. [Connection Pool](#connection-pool)
5. [Migrations](#migrations)
6. [State Store](#state-store)
7. [Activity Persistence](#activity-persistence)
8. [SSH Session Persistence](#ssh-session-persistence)
9. [File Structure](#file-structure)
10. [Usage Examples](#usage-examples)

---

## Overview

The Database module provides:

- **Persistent Storage**: PostgreSQL-based state persistence
- **Connection Pooling**: Efficient connection management with deadpool
- **Schema Migrations**: Automatic table creation and updates
- **Activity Tracking**: Persistent activity history
- **SSH Session Management**: Session persistence and auditing
- **Multi-Instance Support**: Shared state across multiple DSB instances

---

## Architecture

### System Architecture

```mermaid
flowchart TB
    subgraph DSB Application
        API[API Handlers]
        Services[Core Services]
    end

    subgraph Connection Pool
        Pool[deadpool::Pool]
        Manager[Pool Manager]
    end

    subgraph PostgreSQL
        Client[PostgreSQL Client]
        Database[(dsb Database)]
        Tables[(Tables)]
    end

    API --> Services
    Services --> Pool
    Pool --> Manager
    Manager --> Client
    Client --> Database
    Database --> Tables

    style DSB Application fill:#e8f5e9
    style Connection Pool fill:#e1f5fe
    style PostgreSQL fill:#fff4e1
```

### Data Flow

```mermaid
sequenceDiagram
    participant Handler as API Handler
    participant Store as PostgresStateStore
    participant Pool as Connection Pool
    participant DB as PostgreSQL

    Handler->>Store: get_sandbox(id)
    Store->>Pool: get_connection()
    Pool-->>Store: Available connection

    Store->>DB: SELECT * FROM sandboxes WHERE id = $1
    DB-->>Store: Sandbox record

    Store->>Pool: return_connection()
    Store-->>Handler: Sandbox

    Note over Pool: Connections are reused
    Note over Pool: No new connection needed
```

---

## Schema Design

### Entity Relationship Diagram

```mermaid
erDiagram
    sandboxes ||--o{ sandbox_activities : has
    sandboxes ||--o{ ssh_sessions : has

    sandboxes {
        uuid id PK
        uuid container_id UK
        string name
        string image
        string state
        jsonb config
        string error_message
        timestamp created_at
        timestamp updated_at
        timestamp last_activity_at
        int inactivity_timeout_minutes
    }

    sandbox_activities {
        uuid id PK
        uuid sandbox_id FK
        string activity_type
        string client_ip
        string client_user
        jsonb details
        timestamp created_at
    }

    ssh_sessions {
        uuid id PK
        uuid sandbox_id FK
        string state
        string client_ip
        string client_user
        string auth_method
        timestamp connected_at
        timestamp last_activity_at
        timestamp terminated_at
        string termination_reason
        bigint bytes_sent
        bigint bytes_received
    }
```

### Table Schemas

```mermaid
erDiagram
    sandboxes {
        uuid id PK "Primary key"
        uuid container_id UK "Docker container ID"
        string name "Sandbox name"
        string image "Docker image"
        string state "Current state"
        jsonb config "Full configuration"
        string error_message "Error details if failed"
        timestamp created_at "Creation time"
        timestamp updated_at "Last update"
        timestamp last_activity_at "Activity tracking"
        int inactivity_timeout_minutes "Auto-cleanup timeout"
    }

    sandbox_activities {
        uuid id PK "Primary key"
        uuid sandbox_id FK "References sandboxes.id"
        string activity_type "Type of activity"
        string client_ip "Client IP address"
        string client_user "SSH user if applicable"
        jsonb details "Activity details"
        timestamp created_at "When activity occurred"
    }

    ssh_sessions {
        uuid id PK "Primary key"
        uuid sandbox_id FK "References sandboxes.id"
        string state "Session state"
        string client_ip "Client IP"
        string client_user "SSH username"
        string auth_method "Authentication method"
        timestamp connected_at "Connection time"
        timestamp last_activity_at "Last activity"
        timestamp terminated_at "When terminated"
        string termination_reason "Why terminated"
        bigint bytes_sent "Bytes sent"
        bigint bytes_received "Bytes received"
    }
```

### Indexes

```mermaid
flowchart LR
    subgraph Indexes
        I1[CREATE INDEX idx_sandboxes_state<br/>ON sandboxes(state)]
        I2[CREATE INDEX idx_sandboxes_created<br/>ON sandboxes(created_at DESC)]
        I3[CREATE INDEX idx_activities_sandbox<br/>ON sandbox_activities(sandbox_id)]
        I4[CREATE INDEX idx_activities_created<br/>ON sandbox_activities(created_at DESC)]
        I5[CREATE INDEX idx_ssh_sessions_sandbox<br/>ON ssh_sessions(sandbox_id)]
        I6[CREATE INDEX idx_ssh_sessions_state<br/>ON ssh_sessions(state)]
    end
```

---

## Connection Pool

### Pool Architecture

```mermaid
flowchart TB
    subgraph Pool Manager
        A[Create Pool]
        B[Configure Size]
        C[Manage Connections]
        D[Handle Failures]
    end

    subgraph Connection States
        S1[Available]
        S2[In Use]
        S3[Recycled]
        S4[Failed]
    end

    A --> B
    B --> C
    C --> D
    D --> S1
    S1 --> S2
    S2 --> S3
    S3 --> S1
    S2 --> S4

    style Pool Manager fill:#e8f5e9
    style Connection States fill:#fff4e1
```

### Pool Configuration Flow

```mermaid
flowchart TD
    A[Load Config] --> B{URL provided?}
    B -->|Yes| C[Use URL directly]
    B -->|No| D{Individual params provided?}

    D -->|Yes| E[Build URL from components]
    D -->|No| F[Use defaults]

    C --> G[Create DeadpoolConfig]
    E --> G
    F --> G

    G --> H[Set pool size]
    H --> I[Create Pool]
    I --> J[Verify connection]

    style G fill:#e1f5fe
    style I fill:#c8e6c9
```

---

## Migrations

### Migration Flow

```mermaid
flowchart TD
    A[run_migrations()] --> B[Connect to database]
    B --> C[Check current version]
    C --> D{Schema exists?}

    D -->|No| E[Create sandboxes table]
    D -->|Yes| F[Check for missing tables]

    E --> G[Create sandbox_activities table]
    G --> H[Create ssh_sessions table]
    H --> I[Create indexes]
    I --> J[Mark migrations complete]

    F --> G
```

### Migration SQL

```sql
-- sandboxes table
CREATE TABLE IF NOT EXISTS sandboxes (
    id UUID PRIMARY KEY,
    container_id UUID UNIQUE,
    name TEXT,
    image TEXT NOT NULL,
    state TEXT NOT NULL,
    config JSONB NOT NULL DEFAULT '{}',
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_activity_at TIMESTAMP WITH TIME ZONE,
    inactivity_timeout_minutes INTEGER
);

-- sandbox_activities table
CREATE TABLE IF NOT EXISTS sandbox_activities (
    id UUID PRIMARY KEY,
    sandbox_id UUID REFERENCES sandboxes(id) ON DELETE CASCADE,
    activity_type TEXT NOT NULL,
    client_ip TEXT,
    client_user TEXT,
    details JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- ssh_sessions table
CREATE TABLE IF NOT EXISTS ssh_sessions (
    id UUID PRIMARY KEY,
    sandbox_id UUID REFERENCES sandboxes(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    client_ip TEXT,
    client_user TEXT,
    auth_method TEXT,
    connected_at TIMESTAMP WITH TIME ZONE,
    last_activity_at TIMESTAMP WITH TIME ZONE,
    terminated_at TIMESTAMP WITH TIME ZONE,
    termination_reason TEXT,
    bytes_sent BIGINT DEFAULT 0,
    bytes_received BIGINT DEFAULT 0
);
```

---

## State Store

### StateStore Trait Implementation

```mermaid
flowchart LR
    subgraph StateStoreTrait
        T1[get(id)]
        T2[list()]
        T3[save(sandbox)]
        T4[delete(id)]
        T5[update_state()]
    end

    subgraph PostgresStateStore
        P1[SELECT query]
        P2[SELECT all query]
        P3[INSERT/UPDATE query]
        P4[DELETE query]
        P5[UPDATE state query]
    end

    T1 --> P1
    T2 --> P2
    T3 --> P3
    T4 --> P4
    T5 --> P5
```

### Query Patterns

```mermaid
sequenceDiagram
    participant S as StateStore
    participant P as Pool
    participant C as Client
    participant DB as Database

    S->>P: get_connection()
    P-->>S: Connection

    S->>C: Prepare statement
    C->>DB: PREPARE query

    S->>C: Execute with params
    C->>DB: EXECUTE $1, $2, ...

    DB-->>C: Result rows
    C-->>S: Parsed Sandbox
    S->>P: return_connection()
```

---

## Activity Persistence

### Activity Tracking Flow

```mermaid
flowchart LR
    subgraph Source
        E[Exec Command]
        S[SSH Session]
        A[API Call]
    end

    subgraph Activity Service
        R[Record Activity]
        Q[Query Activities]
    end

    subgraph Database
        T[sandbox_activities table]
    end

    E --> R
    S --> R
    A --> R
    R --> T
    T --> Q
```

---

## SSH Session Persistence

### SSH Session Lifecycle in DB

```mermaid
stateDiagram-v2
    [*] --> Creating: Insert session

    Creating --> Active: Connected
    Creating --> Terminated: Connection failed

    Active --> Active: Heartbeat
    Active --> Disconnected: Client disconnect
    Active --> Terminated: Admin terminate

    Disconnected --> Active: Reconnect
    Disconnected --> Terminated: Timeout

    Terminated --> [*]: Update terminated_at

    note right of Creating
        INSERT INTO ssh_sessions
        VALUES (id, sandbox_id, 'connecting', ...)
    end note

    note right of Active
        UPDATE ssh_sessions
        SET state = 'active',
        last_activity_at = NOW()
        WHERE id = ?
    end note
```

---

## File Structure

```
src/db/
├── mod.rs                    # Module exports
├── pool.rs                   # Connection pool
│   ├── create_pool()         # From connection string
│   ├── create_pool_from_config()  # From app config
│   └── Pool                  # deadpool::Pool wrapper
├── migration.rs              # Schema migrations
│   ├── ensure_database_exists()   # Create DB if needed
│   ├── run_migrations()      # Apply schema changes
│   └── migration_sql         # SQL statements
├── store/                    # PostgresStateStore implementation
│   ├── mod.rs
│   ├── PostgresStateStore    # Main store implementation
│   ├── StoreError            # Error types
│   ├── create_sandbox()      # INSERT operation
│   ├── get_sandbox()         # SELECT by ID
│   ├── list_sandboxes()      # SELECT all
│   ├── update_sandbox()      # UPDATE operation
│   ├── delete_sandbox()      # DELETE operation
│   └── update_state()        # State transition
├── activities.rs             # Activity persistence
│   ├── ActivityStore         # Activity data access
│   ├── create_activity()     # Record activity
│   ├── list_activities()     # Query activities
│   └── cleanup_activities()  # Remove old records
├── api_key_store.rs          # API key table access
├── session_token_store.rs    # Session token table access
├── ssh_sessions.rs           # SSH session persistence
│   ├── SshSessionStore       # Session data access
│   ├── create_session()      # Insert session
│   ├── update_session()      # Update session state
│   ├── terminate_session()   # Mark terminated
│   ├── heartbeat()           # Update activity
│   └── get_statistics()      # Aggregate queries
└── test_db.rs                # Test-only helpers
```

> **Schema note:** the database holds **7 tables** (`sandboxes`, `sandbox_activities`, `ssh_sessions`, `api_keys`, `session_tokens`, `vnc_tokens`, plus internal migration tracking). The `api_key_store` and `session_token_store` modules are where auth-related tables are accessed.

---

## Usage Examples

### Creating a Pool

```rust
use dsb::db::pool::create_pool;

let pool = create_pool("postgresql://postgres:pass@localhost/dsb").await?;
```

### Running Migrations

```rust
use dsb::db::migration::run_migrations;

run_migrations(&pool).await?;
```

### Using the State Store

```rust
use dsb::db::PostgresStateStore;

let store = PostgresStateStore::new(pool).await?;

// Create sandbox
store.create_sandbox(sandbox).await?;

// Get sandbox
let sandbox = store.get_sandbox(&id).await?;

// List all sandboxes
let sandboxes = store.list_sandboxes().await?;

// Update state
store.update_state(&id, SandboxState::Running).await?;

// Delete sandbox
store.delete_sandbox(&id).await?;
```

---

## Configuration

### Configuration Options

```mermaid
flowchart LR
    subgraph Config Sources
        C1[dsb.yaml]
        C2[Environment variables]
        C3[Command line]
    end

    subgraph Config Options
        O1[database.url]
        O2[database.host]
        O3[database.port]
        O4[database.name]
        O5[database.user]
        O6[database.password]
        O7[database.pool_max_size]
    end

    C1 --> O1
    C2 --> O2
    C2 --> O3
    C2 --> O4
    C2 --> O5
    C2 --> O6
    C3 --> O7
```

### Environment Variable Mapping

| Setting | Env Variable | Default |
|---------|--------------|---------|
| URL | `DSB_DATABASE__URL` | `null` |
| Host | `DSB_DATABASE__HOST` | `localhost` |
| Port | `DSB_DATABASE__PORT` | `5432` |
| Name | `DSB_DATABASE__NAME` | `dsb` |
| User | `DSB_DATABASE__USER` | `postgres` |
| Password | `DSB_DATABASE__PASSWORD` | `null` |
| Pool Size | `DSB_DATABASE__POOL_MAX_SIZE` | `10` |

---

## See Also

- [Core Module](./core.md) - State management
- [API Module](./api.md) - API handlers
- [Configuration](./configuration.md) - Configuration
