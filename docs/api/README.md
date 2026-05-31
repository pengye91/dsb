# API Module

The API module provides a RESTful HTTP API for managing DSB sandboxes using the Axum web framework. It handles all HTTP requests, authentication, and response formatting.

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Router Structure](#router-structure)
4. [Authentication](#authentication)
5. [Request Handlers](#request-handlers)
6. [Server Initialization](#server-initialization)
7. [Error Handling](#error-handling)
8. [Testing Strategy](#testing-strategy)
9. [File Structure](#file-structure)
10. [Usage Examples](#usage-examples)

---

## Overview

The API module provides:

- **RESTful Endpoints**: CRUD operations for sandboxes, activities, SSH sessions, and static files
- **Authentication**: API key validation via `X-API-Key` header
- **Server-Sent Events (SSE)**: Progress streaming for sandbox creation
- **WebSocket Support**: Terminal access to sandboxes
- **Error Handling**: Consistent error responses with helpful hints

---

## Architecture

### System Architecture

```mermaid
flowchart TB
    subgraph Clients
        CLI[CLI Client]
        Web[Web Browser]
        SDK[Python SDK]
    end

    subgraph API Layer
        Router[Axum Router]
        Auth[Auth Middleware]
        Rate[Rate Limiting]
    end

    subgraph Handlers
        SandboxH[Sandbox Handlers]
        ActivityH[Activity Handlers]
        SSHH[SSH Handlers]
        StaticH[Static Files Handlers]
        TerminalH[Web Terminal]
    end

    subgraph Services
        SandboxS[SandboxService]
        ActivityS[ActivityService]
        SSHS[SSH Session Service]
        StaticS[Static File Service]
    end

    CLI --> Router
    Web --> Router
    SDK --> Router

    Router --> Auth
    Auth --> Rate
    Rate --> SandboxH
    Rate --> ActivityH
    Rate --> SSHH
    Rate --> StaticH
    Rate --> TerminalH

    SandboxH --> SandboxS
    ActivityH --> ActivityS
    SSHH --> SSHS
    StaticH --> StaticS

    style API Layer fill:#e1f5fe
    style Handlers fill:#e8f5e9
    style Services fill:#fff4e1
```

### Request Flow

```mermaid
sequenceDiagram
    participant Client as HTTP Client
    participant Router as Axum Router
    participant Auth as Auth Middleware
    participant Handler as Request Handler
    participant Service as Service Layer
    participant Docker as Docker Manager

    Client->>Router: GET /sandboxes/{id}
    Router->>Auth: Extract headers
    Auth->>Auth: Validate X-API-Key
    Auth-->>Router: Authorized or 401

    alt Unauthorized
        Router-->>Client: 401 Unauthorized
    else Authorized
        Router->>Handler: Dispatch to handler
        Handler->>Service: Call service method
        Service->>Docker: Docker operations
        Docker-->>Service: Result
        Service-->>Handler: Sandbox data
        Handler-->>Client: JSON Response
    end
```

---

## Router Structure

### Route Hierarchy

```mermaid
flowchart TB
    subgraph Main Router
        Health["/health"] --> SandboxRoutes
        SandboxRoutes["/sandboxes"] --> ActivityRoutes
        ActivityRoutes["/activities"] --> SSHRoutes
        SSHRoutes["/ssh-sessions"] --> StaticRoutes
        StaticRoutes["/static"] --> TerminalRoutes
        TerminalRoutes["/terminal"]
    end

    subgraph Sandbox Routes
        S1["GET /sandboxes"]
        S2["POST /sandboxes"]
        S3["POST /sandboxes/create-stream"]
        S4["GET /sandboxes/{id}"]
        S5["DELETE /sandboxes/{id}"]
        S6["POST /sandboxes/{id}/stop"]
        S7["POST /sandboxes/{id}/exec"]
        S8["POST /sandboxes/{id}/upload"]
        S9["GET /sandboxes/{id}/download"]
        S10["GET /sandboxes/{id}/stats"]
        S11["GET /sandboxes/{id}/stats-stream"]
        S12["POST /sandboxes/{id}/cleanup"]
    end

    subgraph Activity Routes
        A1["GET /activities"]
        A2["GET /activities/{id}"]
        A3["GET /sandboxes/{id}/activities"]
        A4["POST /activities/cleanup-all"]
    end

    subgraph SSH Routes
        SSH1["POST /ssh-sessions"]
        SSH2["GET /ssh-sessions"]
        SSH3["GET /ssh-sessions/{id}"]
        SSH4["POST /ssh-sessions/{id}/terminate"]
        SSH5["POST /ssh-sessions/{id}/heartbeat"]
        SSH6["GET /ssh-sessions/statistics"]
        SSH7["GET /ssh/authorize/{sandbox_id}"]
    end

    subgraph Static Routes
        ST1["GET /static/{id}/{path}"]
        ST2["GET /static/files/{id}"]
        ST3["DELETE /static/file/{id}/{path}"]
        ST4["DELETE /static/sandbox/{id}"]
    end

    subgraph Terminal Routes
        T1["GET /terminal"]
        T2["GET /terminal/{sandbox_id}"]
    end

    SandboxRoutes --> S1
    SandboxRoutes --> S2
    SandboxRoutes --> S3
    SandboxRoutes --> S4
    SandboxRoutes --> S5
    SandboxRoutes --> S6
    SandboxRoutes --> S7
    SandboxRoutes --> S8
    SandboxRoutes --> S9
    SandboxRoutes --> S10

    ActivityRoutes --> A1
    ActivityRoutes --> A2
    ActivityRoutes --> A3
    ActivityRoutes --> A4

    SSHRoutes --> SSH1
    SSHRoutes --> SSH2
    SSHRoutes --> SSH3
    SSHRoutes --> SSH4
    SSHRoutes --> SSH5
    SSHRoutes --> SSH6
    SSHRoutes --> SSH7

    StaticRoutes --> ST1
    StaticRoutes --> ST2
    StaticRoutes --> ST3
    StaticRoutes --> ST4

    TerminalRoutes --> T1
    TerminalRoutes --> T2
```

### Router Configuration

```mermaid
flowchart LR
    subgraph Router Creation
        R1[Router::new] --> R2[Add routes]
        R2 --> R3[Merge sub-routers]
        R3 --> R4[Add state]
        R4 --> R5[Ready]
    end

    subgraph State Types
        ST1[Arc~SandboxService~]
        ST2[Arc~ActivityService~]
        ST3[Arc~SSH Session Service~]
        ST4[Arc~Static File Service~]
        ST5[Option~String~ API Key]
    end

    R5 --> ST1
    R5 --> ST2
    R5 --> ST3
    R5 --> ST4
    R5 --> ST5
```

---

## Authentication

### API Key Authentication Flow

```mermaid
flowchart TD
    A[Request arrives] --> B{Path == /health?}
    B -->|Yes| C[Skip auth]
    B -->|No| D{API key configured?}

    D -->|No| E[Allow request]
    D -->|Yes| F{X-API-Key header present?}

    F -->|No| G[401 Unauthorized]
    F -->|Yes| H{Key matches?}

    H -->|Yes| E
    H -->|No| G
```

### Auth Middleware Code Flow

```mermaid
sequenceDiagram
    participant Client as HTTP Client
    participant Middleware as Auth Middleware
    participant Next as Handler

    Client->>Middleware: GET /sandboxes/{id}<br/>Headers: {X-API-Key: "secret"}

    Note over Middleware: Extract api_key from state

    Middleware->>Middleware: Get expected key from config

    alt Keys match
        Middleware->>Next: Continue request
        Next-->>Client: 200 OK with data
    else Keys don't match
        Middleware-->>Client: 401 Unauthorized
    end
```

---

## Request Handlers

### Handler Categories

```mermaid
flowchart TB
    subgraph Sandbox Handlers
        H1[create_sandbox]
        H2[create_sandbox_stream]
        H3[get_sandbox]
        H4[list_sandboxes]
        H5[stop_sandbox]
        H6[delete_sandbox]
        H7[exec_sandbox]
        H8[upload_file]
        H9[download_file]
        H10[get_sandbox_stats]
        H11[stream_sandbox_stats]
        H12[cleanup_sandbox]
    end

    subgraph Activity Handlers
        A1[list_activities]
        A2[get_activity]
        A3[list_sandbox_activities]
        A4[cleanup_inactive_sandboxes]
    end

    subgraph SSH Handlers
        S1[create_ssh_session]
        S2[list_ssh_sessions]
        S3[get_ssh_session]
        S4[terminate_ssh_session]
        S5[update_session_activity]
        S6[get_ssh_session_statistics]
        S7[authorize_ssh_access]
    end

    subgraph Static Handlers
        F1[serve_static_file]
        F2[list_static_files]
        F3[delete_static_file]
        F4[delete_sandbox_static_files]
    end

    subgraph Terminal Handlers
        T1[terminal_page]
        T2[terminal_websocket]
    end

    style Sandbox Handlers fill:#e8f5e9
    style Activity Handlers fill:#e1f5fe
    style SSH Handlers fill:#fff4e1
    style Static Handlers fill:#fce4ec
    style Terminal Handlers fill:#c8e6c9
```

### Create Sandbox Handler Flow

```mermaid
sequenceDiagram
    participant Client as HTTP Client
    participant Handler as create_sandbox
    participant Config as SandboxConfig
    participant Service as SandboxService
    participant State as StateStore

    Client->>Handler: POST /sandboxes<br/>{"image": "nginx:latest"}

    Handler->>Config: Build config from request

    Config->>Service: create_sandbox(config)

    Service->>Service: Apply feature profiles
    Service->>Service: Create container
    Service->>Service: Start container

    Service-->>Handler: Sandbox

    Handler-->>Client: 201 Created<br/>{"id": "...", "state": "running"}
```

### SSE Progress Streaming

```mermaid
sequenceDiagram
    participant Client as SSE Client
    participant Handler as create_sandbox_stream
    participant Service as SandboxService
    participant Events as Progress Events

    Client->>Handler: POST /sandboxes/create-stream<br/>Accept: text/event-stream

    Handler->>Service: create_sandbox_stream()

    Note over Service: Generate events
    Service->>Events: {type: "pulling", image: "..."}
    Events-->>Client: data: {"type":"pulling"...}

    Service->>Events: {type: "creating", image: "..."}
    Events-->>Client: data: {"type":"creating"...}

    Service->>Events: {type: "ready", sandbox_id: "..."}
    Events-->>Client: data: {"type":"ready"...}

    Service-->>Handler: Stream complete
    Handler-->>Client: Connection closed
```

---

## Server Initialization

### Startup Sequence

```mermaid
flowchart TD
    A[start_server(config)] --> B[Create Docker Manager]
    B --> C{Check DB config}
    C -->|PostgreSQL| D[Create PostgresStateStore]
    C -->|No DB| E[Create InMemory StateStore]

    D --> F[Create ActivityService]
    E --> F

    F --> G[Create SandboxService]
    G --> H[Create SSH Session Service]
    H --> I[Create Static File Service]

    I --> J[Create API Key from config]
    J --> K[Build Router]

    K --> L[Add health route]
    L --> M[Add sandbox routes]
    M --> N[Add activity routes]
    N --> O[Add SSH routes]
    O --> P[Add static routes]
    P --> Q[Add terminal routes]
    Q --> R[Merge all routes]
    R --> S[Bind to socket]
    S --> T[Start listening]

    style A fill:#e1f5fe
    style T fill:#c8e6c9
```

---

## Error Handling

### Error Response Format

```mermaid
flowchart LR
    subgraph Error Types
        E1[400 Bad Request]
        E2[401 Unauthorized]
        E3[404 Not Found]
        E4[500 Internal Error]
    end

    subgraph Response Format
        R1[JSON Body]
        R2["{error: string, hint?: string}"]
    end

    E1 --> R1
    E2 --> R1
    E3 --> R1
    E4 --> R1
```

### Error Mapping

```mermaid
flowchart TD
    A[Service Error] --> B{Error message contains?}
    B -->|"no such image"| C[404 Not Found]
    B -->|"already exists"| D[409 Conflict]
    B -->|"permission denied"| E[403 Forbidden]
    B -->|"timeout"| F[408 Request Timeout]
    B -->|other| G[500 Internal Error]

    C --> H[Add helpful hint]
    D --> H
    E --> H
    F --> H
    G --> H
```

---

## Testing Strategy

### Test Pyramid

```mermaid
flowchart BT
    subgraph Unit Tests [tests/]
        U1[Handler tests]
        U2[Auth middleware tests]
        U3[Request/Response tests]
    end

    subgraph Integration Tests [tests/]
        I1[API server e2e tests]
        I2[CLI-HTTP integration]
        I3[Docker integration]
    end

    subgraph Manual Testing
        M1[curl commands]
        M2[API docs testing]
    end

    U1 --> I1
    U2 --> I1
    U3 --> I2
    I1 --> M1
    I2 --> M1
    I3 --> M1

    style Unit Tests fill:#e8f5e9
    style Integration Tests fill:#e1f5fe
    style Manual Testing fill:#fff4e1
```

---

## File Structure

```
src/api/
├── mod.rs                    # Module exports, build_test_router
├── server/
│   └── mod.rs                # start_server function (10KB)
│       ├── start_server()    # Server initialization
│       ├── build_router()    # Route configuration
│       └── services          # Service creation
├── handlers/
│   ├── mod.rs                # Handler exports
│   ├── sandbox.rs            # Sandbox CRUD (40KB)
│   │   ├── create_sandbox()
│   │   ├── create_sandbox_stream()
│   │   ├── get_sandbox()
│   │   ├── list_sandboxes()
│   │   ├── stop_sandbox()
│   │   ├── delete_sandbox()
│   │   ├── exec_sandbox()
│   │   ├── upload_file()
│   │   ├── download_file()
│   │   ├── get_sandbox_stats()
│   │   └── stream_sandbox_stats()
│   ├── activities.rs         # Activity endpoints (13KB)
│   │   ├── list_activities()
│   │   ├── get_activity()
│   │   ├── list_sandbox_activities()
│   │   └── cleanup_inactive_sandboxes()
│   ├── ssh.rs                # SSH session handlers (26KB)
│   │   ├── create_ssh_session()
│   │   ├── list_ssh_sessions()
│   │   ├── get_ssh_session()
│   │   ├── terminate_ssh_session()
│   │   └── update_session_activity()
│   ├── static_files.rs       # Static file handlers (22KB)
│   │   ├── serve_static_file()
│   │   ├── list_static_files()
│   │   ├── delete_static_file()
│   │   └── delete_sandbox_static_files()
│   ├── health.rs             # Health check (2.4KB)
│   │   └── health_check()
│   └── execution_tests.rs    # CLI execution tests (15KB)
└── auth.rs                   # Authentication middleware (10KB)
    ├── api_key_auth()        # Auth middleware
    └── is_api_key_valid()    # Key validation helper
```

---

## Usage Examples

### Making API Requests

```bash
# Create a sandbox
curl -X POST http://localhost:8080/sandboxes \
  -H "Content-Type: application/json" \
  -d '{"image": "nginx:alpine"}'

# List sandboxes
curl http://localhost:8080/sandboxes \
  -H "X-API-Key: your-secret-key"

# Execute command
curl -X POST http://localhost:8080/sandboxes/{id}/exec \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-secret-key" \
  -d '{"command": ["ls", "-la"]}'

# Upload a file to sandbox
curl -X POST http://localhost:8080/sandboxes/{id}/upload \
  -F "path=/app/config.json" \
  -F "file=@local-config.json"

# Download a file from sandbox
curl -O -J "http://localhost:8080/sandboxes/{id}/download?path=/app/config.json"

# Or download to specific filename
curl -o local-config.json "http://localhost:8080/sandboxes/{id}/download?path=/app/config.json"

# View file inline (in browser)
curl -O -J "http://localhost:8080/sandboxes/{id}/download?path=/app/page.html&disposition=inline"

# Stream statistics
curl -N http://localhost:8080/sandboxes/{id}/stats-stream \
  -H "X-API-Key: your-secret-key"
```

### SSE Progress Streaming

```javascript
const eventSource = new EventSource(
  'http://localhost:8080/sandboxes/create-stream',
  {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ image: 'nginx:alpine' })
  }
);

eventSource.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log('Progress:', data);
};
```

---

## See Also

- [Core Module](../core/README.md) - Sandbox service
- [Docker Module](../docker/README.md) - Container management
- [CLI Module](../cli/README.md) - Command-line interface
- [Static File Serving](../../src/core/static_files.rs) - Static file endpoints
