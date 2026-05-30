# Core Module

The Core module is the heart of the DSB (Distributed Sandboxes) system, providing all the business logic for managing sandbox lifecycles, state management, activity tracking, and feature profiles.

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Sandbox Lifecycle](#sandbox-lifecycle)
4. [Key Components](#key-components)
5. [State Management](#state-management)
6. [Activity Tracking](#activity-tracking)
7. [Feature Profiles](#feature-profiles)
8. [SSH Session Management](#ssh-session-management)
9. [Static Files](#static-files)
10. [File Structure](#file-structure)
11. [Relationships](#relationships)

---

## Overview

The Core module handles:

- **Sandbox Lifecycle**: Create, start, stop, delete, and cleanup operations
- **State Management**: Track sandbox states and transitions
- **Activity Tracking**: Monitor sandbox activity for cleanup and auditing
- **Feature Profiles**: Auto-configure sandboxes based on Docker image metadata
- **SSH Sessions**: Manage SSH session lifecycle
- **Static Files**: Serve static files from sandboxes

---

## Architecture

### System Architecture

```mermaid
flowchart TB
    subgraph API Layer
        Handlers[API Handlers]
    end

    subgraph Core Layer
        SandboxService[SandboxService]
        ActivityService[ActivityService]
        FeatureEngine[Feature Engine]
        SSHService[SSH Session Service]
        StaticFileService[Static File Service]
    end

    subgraph Storage Layer
        StateStore[StateStore Trait]
        ActivityStore[Activity Store]
    end

    subgraph Docker Layer
        DockerManager[Docker Manager]
    end

    Handlers --> SandboxService
    Handlers --> SSHService
    Handlers --> StaticFileService

    SandboxService --> DockerManager
    SandboxService --> StateStore
    SandboxService --> ActivityService
    SandboxService --> FeatureEngine

    SSHService --> StateStore
    SSHService --> ActivityStore

    StaticFileService --> StateStore

    ActivityService --> ActivityStore

    style API Layer fill:#e1f5fe
    style Core Layer fill:#e8f5e9
    style Storage Layer fill:#fff4e1
    style Docker Layer fill:#fce4ec
```

### Module Dependencies

```mermaid
flowchart LR
    subgraph core
        A[types.rs] --> B[sandbox.rs]
        B --> C[state.rs]
        B --> D[activities.rs]
        B --> E[features.rs]
        B --> F[ssh_service.rs]
        B --> G[static_files.rs]
    end

    style A fill:#bbdefb
    style B fill:#c8e6c9
    style C fill:#fff9c4
    style D fill:#ffcdd2
    style E fill:#e1bee7
    style F fill:#b2dfdb
    style G fill:#ffccbc
```

---

## Sandbox Lifecycle

### State Machine

```mermaid
stateDiagram-v2
    [*] --> Creating: create_sandbox()

    Creating --> Created: Container created successfully
    Creating --> Error: Container creation failed

    Created --> Starting: start_sandbox()
    Created --> Destroying: delete_sandbox()

    Starting --> Running: Container started successfully
    Starting --> Error: Container start failed

    Running --> Stopped: stop_sandbox()
    Running --> Destroying: delete_sandbox()
    Running --> Error: Runtime error

    Stopped --> Starting: start_sandbox()
    Stopped --> Destroying: delete_sandbox()

    Error --> Destroying: delete_sandbox()
    Error --> Creating: Retry after cleanup

    Destroying --> [*]: Cleanup complete

    note right of Creating
        Pulling image...
        Creating container...
    end note

    note right of Running
        Ready for exec
        Active monitoring
    end note
```

### Lifecycle Flow

```mermaid
sequenceDiagram
    participant CLI as CLI/API
    participant Service as SandboxService
    participant Docker as DockerManager
    participant State as StateStore

    CLI->>Service: create_sandbox(config)
    Service->>State: Store with state=Creating
    Service->>Docker: Pull image (if needed)
    Docker-->>Service: Image pulled

    Service->>Docker: create_container()
    Docker-->>Service: Container created
    Service->>State: Update state=Created

    Service->>Docker: start_container()
    Docker-->>Service: Container started
    Service->>State: Update state=Running

    Service-->>CLI: Sandbox ready

    Note over CLI, Service: Sandbox is active

    CLI->>Service: exec_sandbox(id, command)
    Service->>Docker: exec_command()
    Docker-->>Service: Command output
    Service-->>CLI: Output returned

    CLI->>Service: stop_sandbox(id)
    Service->>Docker: stop_container()
    Docker-->>Service: Container stopped
    Service->>State: Update state=Stopped

    CLI->>Service: delete_sandbox(id)
    Service->>Docker: delete_container()
    Service->>State: Remove sandbox
```

---

## Key Components

### SandboxService

The main orchestrator for sandbox operations.

```mermaid
classDiagram
    class SandboxService {
        +docker: DockerManager
        +state: Arc~StateStoreTrait~
        +activity_service: Option~ActivityService~
        +default_inactivity_timeout: u64
        +cleanup_dry_run: bool

        +create_sandbox(config) Sandbox
        +get_sandbox(id) Option~Sandbox~
        +list_sandboxes() Vec~Sandbox~
        +stop_sandbox(id) Result
        +delete_sandbox(id) Result
        +exec_sandbox(id, command) Result~ExecOutput~
        +cleanup_inactive() Result~u32~
    }

    class DockerManager {
        +create_container(config) Container
        +start_container(id) Result
        +stop_container(id) Result
        +delete_container(id) Result
        +exec_command(id, cmd) ExecOutput
    }

    class StateStoreTrait {
        <<interface>>
        +get(id) Option~Sandbox~
        +list() Vec~Sandbox~
        +save(sandbox) Result
        +delete(id) Result
    }

    SandboxService --> DockerManager
    SandboxService --> StateStoreTrait
```

### Key Types

```mermaid
erDiagram
    Sandbox ||--o{ Activity : tracks
    Sandbox ||--o{ SSHSession : has
    Sandbox {
        uuid id PK
        string image
        string name
        SandboxState state
        string container_id
        string error_message
        datetime created_at
        datetime last_activity_at
    }

    SandboxState ||--|| Creating : initial
    SandboxState ||--|| Running : active
    SandboxState ||--|| Stopped : inactive
    SandboxState ||--|| Error : failed
```

---

## State Management

### StateStore Abstraction

```mermaid
flowchart TB
    subgraph Implementations
        InMemory[InMemoryStateStore]
        Postgres[PostgresStateStore]
    end

    subgraph Consumers
        SandboxService
        SSHService
    end

    subgraph Trait
        StateStoreTrait
    end

    Consumers --> StateStoreTrait
    StateStoreTrait <|-- InMemory
    StateStoreTrait <|-- Postgres

    style StateStoreTrait fill:#bbdefb
    style InMemory fill:#c8e6c9
    style Postgres fill:#c8e6c9
```

### State Transitions

```mermaid
stateDiagram-v2
    direction TB

    [*] --> Creating

    Creating : Entry: Initialize sandbox
    Creating : Exit: Container created

    Created : Entry: Store container ID
    Created : Exit: Start container

    Starting : Entry: Notify watchers
    Starting : Exit: Container running

    Running : Entry: Enable exec
    Running : Exit: Stop requested

    Stopped : Entry: Mark inactive
    Stopped : Exit: Restart or delete

    Error : Entry: Store error
    Error : Exit: Cleanup requested

    Destroying : Entry: Release resources
    Destroying : Exit: Removed from store
```

---

## Activity Tracking

### Activity System Architecture

```mermaid
flowchart LR
    subgraph Sources
        Exec[Command Execution]
        SSH[SSH Sessions]
        Stats[Stats Request]
    end

    subgraph ActivityService
        Record[Record Activity]
        Monitor[Monitor for Cleanup]
    end

    subgraph Storage
        Activities[(Activities Table)]
    end

    Sources --> Record
    Record --> Activities
    Monitor --> Activities

    style ActivityService fill:#e8f5e9
```

### Activity Types

```mermaid
erDiagram
    Activity {
        uuid id PK
        uuid sandbox_id FK
        ActivityType activity_type
        string client_ip
        string client_user
        datetime created_at
        json details
    }

    ActivityType ||--|| Exec : command
    ActivityType ||--|| SshSession : session
    ActivityType ||--|| StatsRequest : monitoring
    ActivityType ||--|| ApiCall : management
```

---

## Feature Profiles

### Feature Detection Flow

```mermaid
flowchart TD
    A[Create Sandbox] --> B[Detect Image Labels]
    B --> C{Features Found?}
    C -->|Yes| D[Parse com.dsb.features JSON]
    C -->|No| G[Use Manual Config]

    D --> E[Apply Default Features]
    E --> F[Merge with User Config]
    G --> F

    F --> H[Create Sandbox with Config]

    style B fill:#fff4e1
    style D fill:#e8f5e9
    style H fill:#c8e6c9
```

### Feature Schema

```mermaid
erDiagram
    FeatureProfile {
        string version
        map features
        list default_command
    }

    Feature {
        string description
        list ports
        map env
        volumes[]
        static_server
        bool enabled_by_default
    }

    FeatureProfile ||--o{ Feature : contains
```

**📚 Comprehensive Guide:** See [Feature Profile System](feature_profiles.md) for complete documentation including:
- Full schema reference with all field types
- Volume types (bind, named, dynamic_bind)
- Static file serving integration
- Complete examples (VNC, web apps, databases)
- Best practices and troubleshooting

---

## SSH Session Management

### SSH Session Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Connecting: Create Session

    Connecting --> Active: Connection Established
    Connecting --> Error: Connection Failed

    Active --> Active: Heartbeat
    Active --> Disconnected: Client Disconnect
    Active --> Terminated: Admin Terminate

    Disconnected --> Active: Reconnect
    Disconnected --> Terminated: Timeout

    Error --> [*]: Cleanup

    Terminated --> [*]: Record End Time
```

### SSH Session Flow

```mermaid
sequenceDiagram
    participant Client as SSH Client
    participant Gateway as SSH Gateway
    participant API as DSB API
    participant DB as PostgreSQL
    participant Container as Sandbox

    Client->>Gateway: SSH Connection
    Gateway->>API: Authorize sandbox_id
    API->>DB: Get sandbox state
    DB-->>API: State=Running
    API-->>Gateway: Authorized
    Gateway->>Container: Docker exec with PTY

    Container-->>Gateway: PTY Output
    Gateway-->>Client: SSH Data

    loop Heartbeat
        Gateway->>API: POST /ssh-sessions/{id}/heartbeat
        API->>DB: Update last_activity
    end

    Client->>Gateway: Disconnect
    Gateway->>API: Update session state
```

---

## Static Files

### Static File Architecture

```mermaid
flowchart TB
    subgraph Sandbox Container
        Public["/public directory"]
    end

    subgraph Host System
        StaticFiles["/var/lib/dsb/static-files"]
        StaticService["StaticFileService"]
    end

    subgraph API
        StaticHandlers[Static Handlers]
    end

    subgraph External
        Client[HTTP Client]
    end

    Public -->|bind mount| StaticFiles
    StaticFiles --> StaticService
    StaticService --> StaticHandlers
    StaticHandlers --> Client

    style Public fill:#e8f5e9
    style StaticFiles fill:#fff4e1
    style StaticService fill:#c8e6c9
    style API fill:#e1f5fe
```

---

## File Structure

```
src/core/
├── mod.rs                    # Module exports and documentation
├── types.rs                  # Core data types (57KB)
│   ├── SandboxState         # Lifecycle states
│   ├── SandboxConfig        # Creation configuration
│   ├── ResourceLimits       # CPU, memory limits
│   ├── PortMapping          # Port bindings
│   ├── Sandbox              # Complete sandbox domain object
│   └── PullPolicy           # Image pull strategies
├── sandbox.rs               # SandboxService (102KB)
│   ├── create_sandbox()     # Create with feature profiles
│   ├── start_sandbox()      # Start container
│   ├── stop_sandbox()       # Stop container
│   ├── delete_sandbox()     # Cleanup resources
│   ├── exec_sandbox()       # Execute commands
│   └── cleanup_inactive()   # Auto-cleanup task
├── state.rs                 # In-memory state store (16KB)
├── store_trait.rs           # StateStore trait abstraction (2.7KB)
├── activities.rs            # ActivityService (13KB)
├── features.rs              # Feature profile system (26KB)
├── ssh_service.rs           # SSH session management (28KB)
└── static_files.rs          # Static file serving (12KB)
```

---

## Relationships

### Module Interaction Map

```mermaid
flowchart TB
    subgraph api[API Module]
        H[Handlers]
    end

    subgraph core[Core Module]
        SS[SandboxService]
        AS[ActivityService]
        FS[FeatureService]
        SSH[SSHService]
        SF[StaticFileService]
    end

    subgraph docker[Docker Module]
        DM[DockerManager]
    end

    subgraph db[Database Module]
        PS[PostgresStore]
        ActDB[Activity Store]
    end

    H --> SS
    H --> SSH
    H --> SF

    SS --> DM
    SS --> PS
    SS --> AS
    SS --> FS

    SSH --> PS
    SSH --> ActDB

    SF --> PS

    AS --> ActDB

    style api fill:#e1f5fe
    style core fill:#e8f5e9
    style docker fill:#fce4ec
    style db fill:#fff4e1
```

---

## Usage Examples

### Creating a Sandbox

```rust
use dsb::core::{SandboxService, SandboxConfig, SandboxState};
use dsb::docker::DockerManager;
use std::sync::Arc;

let docker = DockerManager::new()?;
let state = Arc::new(StateStore::new());
let service = SandboxService::new(docker, state);

let config = SandboxConfig {
    image: "nginx:alpine".to_string(),
    name: Some("webapp".to_string()),
    ..Default::default()
};

let sandbox = service.create_sandbox(config).await?;
assert_eq!(sandbox.state, SandboxState::Running);
```

### Executing Commands

```rust
let output = service
    .exec_sandbox(
        &sandbox.id,
        vec!["echo".to_string(), "Hello World".to_string()],
    )
    .await?;

println!("Output: {}", output.stdout);
```

---

## See Also

- [Docker Module](../docker/README.md) - Container management
- [API Module](../api/README.md) - REST API handlers
- [Database Module](../db/README.md) - PostgreSQL persistence
- [Static File Serving](../static_serving/STATIC_SERVING.md) - Static file feature
