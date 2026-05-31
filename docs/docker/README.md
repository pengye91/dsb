# Docker Module

The Docker module provides a high-level interface for managing Docker containers using the Bollard library. It handles all container lifecycle operations, PTY-enabled command execution, and feature detection from image labels.

## Table of Contents

1. [Overview](#overview)
2. [Architecture](#architecture)
3. [Key Components](#key-components)
4. [Container Lifecycle](#container-lifecycle)
5. [Command Execution](#command-execution)
6. [Feature Detection](#feature-detection)
7. [Resource Management](#resource-management)
8. [Volume Mounts](#volume-mounts)
9. [File Structure](#file-structure)
10. [Usage Examples](#usage-examples)

---

## Overview

The Docker module provides:

- **Container Management**: Create, start, stop, delete containers
- **Command Execution**: PTY-enabled exec with proper shell wrapping
- **Feature Detection**: Extract feature profiles from image labels
- **Resource Limits**: CPU, memory, and process constraints
- **Volume Management**: Bind mounts and volume configuration
- **Image Pulling**: Flexible pull policies (always, missing, never)

---

## Architecture

### System Architecture

```mermaid
flowchart TB
    subgraph Core Layer
        SM[SandboxService]
    end

    subgraph Docker Module
        DM[DockerManager]
        DP[DockerExecProxy]
        FD[FeatureDetector]
    end

    subgraph Bollard Library
        Client[Arc<Docker>]
    end

    subgraph Docker Daemon
        API[Docker API]
        Runtime[Container Runtime]
    end

    SM --> DM
    DM --> Client
    DM --> DP
    DM --> FD
    Client --> API
    API --> Runtime

    style Core Layer fill:#e8f5e9
    style Docker Module fill:#e1f5fe
    style Bollard Library fill:#fff4e1
    style Docker Daemon fill:#fce4ec
```

### Connection Architecture

```mermaid
flowchart LR
    subgraph DSB Process
        DockerManager
        BollardClient[Arc<Docker>]
    end

    subgraph Connection Types
        Unix["/var/run/docker.sock"]
        TCP["tcp://host:2375"]
        NamedPipe["//./pipe/docker_engine"]
    end

    BollardClient -->|platform default| Unix
    BollardClient -->|custom| TCP
    BollardClient -->|Windows| NamedPipe

    style Unix fill:#c8e6c9
    style TCP fill:#fff9c4
    style NamedPipe fill:#ffcdd2
```

---

## Key Components

### DockerManager

The main interface for Docker operations.

```mermaid
classDiagram
    class DockerManager {
        +docker: Arc~Docker~
        +config: Arc~Config~

        +new() Result~DockerManager~
        +new_with_config(config) Result~DockerManager~
        +create_container(config) Result~String~
        +start_container(id) Result
        +stop_container(id) Result
        +remove_container(id, volumes) Result
        +exec_container(id, command) Result~String~
        +get_container_stats(id) Result~ContainerStats~
        +pull_image(image) Result
        +inspect_container(id) Result~ContainerInspect~
    }

    class Docker {
        <<external>>
        +create_container() String
        +start_container()
        +stop_container()
        +exec()
    }

    DockerManager --> Docker
```

### DockerExecProxy

Handles PTY-enabled command execution.

```mermaid
flowchart TB
    subgraph Exec Flow
        C[Create Exec] --> S[Start Exec]
        S --> Stream[Get I/O Stream]
        Stream --> Resize[Resize PTY]
        Resize --> Close[Close Exec]
    end

    subgraph ExecConfig
        container_id: String
        command: Vec~String~
        working_dir: Option~String~
        env: Option~Vec~String~~
        user: Option~String~
        attach_stdout: bool
        attach_stderr: bool
        tty: bool
    end

    style ExecConfig fill:#e8f5e9
```

### FeatureDetector

Extracts feature profiles from Docker image labels.

```mermaid
flowchart TD
    A[Create Sandbox] --> B[FeatureDetector.detect_from_image]
    B --> C[docker.inspect_image]
    C --> D{Image has com.dsb.features label?}

    D -->|No| E[Return empty FeatureProfile]
    D -->|Yes| F[Parse JSON from label]

    F --> G{Valid JSON?}
    G -->|No| H[Log warning, return empty]
    G -->|Yes| I[Extract features]

    I --> J[Apply user selection]
    J --> K[Return FeatureProfile]

    style B fill:#e1f5fe
    style K fill:#c8e6c9
```

---

## Container Lifecycle

### Lifecycle State Machine

```mermaid
stateDiagram-v2
    [*] --> ImagePull: create_container()

    ImagePull --> Creating: Pull complete
    ImagePull --> Error: Pull failed

    Creating --> Created: Container created
    Creating --> Error: Creation failed

    Created --> Starting: start_container()
    Created --> Removing: remove_container()

    Starting --> Running: Container started
    Starting --> Error: Start failed

    Running --> Stopping: stop_container()
    Running --> Removing: remove_container()
    Running --> Error: Runtime error

    Stopping --> Stopped: Container stopped
    Stopping --> Error: Stop failed

    Stopped --> Starting: start_container()
    Stopped --> Removing: remove_container()

    Removing --> [*]: Cleanup complete

    Error --> Removing: delete_container()
```

### Container Creation Flow

```mermaid
sequenceDiagram
    participant Service as SandboxService
    participant Docker as DockerManager
    participant Bollard as Bollard
    participant Daemon as Docker Daemon

    Service->>Docker: create_container(config)
    Docker->>Bollard: Pull image if needed
    Bollard->>Daemon: Pull image
    Daemon-->>Bollard: Image pulled
    Bollard-->>Docker: Image ready

    Docker->>Bollard: create_container(HostConfig)
    Bollard->>Daemon: Create container
    Daemon-->>Bollard: Container ID
    Bollard-->>Docker: Container ID

    Docker-->>Service: Container ID
```

---

## Command Execution

### Exec Flow with PTY

```mermaid
sequenceDiagram
    participant CLI as CLI/API
    participant Service as SandboxService
    participant Proxy as DockerExecProxy
    participant Docker as Bollard
    participant Container as Container

    CLI->>Service: exec_sandbox(id, command)
    Service->>Proxy: exec_container(id, command)

    Note over Proxy: Wraps command with sh -c

    Proxy->>Docker: create_exec(ExecConfig)
    Docker->>Container: Create exec with TTY

    Docker-->>Proxy: Exec ID

    Proxy->>Docker: start_exec(exec_id)
    Docker->>Container: Start exec

    Docker-->>Proxy: I/O Stream

    loop Command execution
        Proxy->>Docker: Send input
        Docker->>Container: PTY input
        Container-->>Docker: PTY output
        Docker-->>Proxy: Output stream
        Proxy-->>Service: Output data
    end

    Service-->>CLI: Command output
```

### PTY Window Resizing

```mermaid
flowchart LR
    Client -->|WebSocket Data| Server
    Server -->|Resize Request| Proxy
    Proxy -->|resize_exec| Docker
    Docker -->|API Call| Container
    Container -->|Update PTY| Shell

    style Proxy fill:#c8e6c9
    style Docker fill:#fff4e1
```

---

## Feature Detection

### Feature Detection Architecture

```mermaid
flowchart TB
    subgraph Image
        Labels[Image Labels]
        DSBFeatures["com.dsb.features<br/>JSON"]
    end

    subgraph Detector
        Parse[Parse JSON]
        Validate[Validate Schema]
        Select[Feature Selection]
    end

    subgraph Output
        Profile[FeatureProfile]
    end

    Labels --> DSBFeatures
    DSBFeatures --> Parse
    Parse --> Validate
    Validate --> Select
    Select --> Profile

    style Image fill:#e8f5e9
    style Detector fill:#e1f5fe
    style Output fill:#c8e6c9
```

### Feature Label Schema

```json
{
  "version": "1.0",
  "features": {
    "vnc": {
      "description": "VNC server with web client",
      "ports": [
        {"host": 5901, "container": 5901, "protocol": "tcp"}
      ],
      "env": {"DISPLAY": ":1"},
      "enabled_by_default": true
    }
  },
  "default_command": ["sudo", "/usr/bin/supervisord"]
}
```

---

## Resource Management

### Resource Limits Configuration

```mermaid
erDiagram
    ResourceLimits {
        int memory_mb
        int cpu_quota
        int cpu_period
        int cpu_shares
        int pids_limit
        list ulimits
    }

    HostConfig {
        long memory
        int cpu_quota
        int cpu_period
        int cpu_shares
        int pids_limit
        list ulimits
    }

    ResourceLimits --> HostConfig: Maps to
```

### Memory Limit Mapping

```mermaid
flowchart TD
    A[ResourceLimits.memory_mb] --> B{Set?}
    B -->|Yes| C[Convert to bytes]
    B -->|No| D[No memory limit]

    C --> E[HostConfig.memory]

    style A fill:#e1f5fe
    style C fill:#c8e6c9
    style E fill:#fff4e1
```

### CPU Limit Mapping

```mermaid
flowchart LR
    subgraph Input
        M1[cpu_quota: 50000]
        M2[cpu_period: 100000]
        M3[cpu_shares: 1024]
    end

    subgraph Docker
        D1[cpu_quota]
        D2[cpu_period]
        D3[cpu_shares]
    end

    M1 --> D1
    M2 --> D2
    M3 --> D3

    style M1 fill:#e8f5e9
    style M2 fill:#e8f5e9
    style M3 fill:#e8f5e9
```

---

## Volume Mounts

### Volume Mount Flow

```mermaid
flowchart TB
    A[SandboxConfig.volumes] --> B[Parse Volume Spec]
    B --> C[Create HostConfig Binds]

    subgraph Bind Mounts
        D[Host Path]
        E[Container Path]
        F[Read/Write Mode]
    end

    subgraph Named Volumes
        G[Volume Name]
        H[Container Path]
    end

    C --> D
    C --> G

    D --> E
    E --> F

    style Bind Mounts fill:#e8f5e9
    style Named Volumes fill:#fff4e1
```

### Mount Format

```text
# Bind mount
host_path:container_path:rw

# Named volume
volume_name:container_path:rw
```

---

## File Structure

```
src/docker/
├── mod.rs                    # Module exports
├── manager.rs                # DockerManager (70KB)
│   ├── new()                 # Create manager
│   ├── new_with_config()     # Create with custom config
│   ├── create_container()    # Create with full config
│   ├── start_container()     # Start container
│   ├── stop_container()      # Stop with timeout
│   ├── remove_container()    # Delete with volumes
│   ├── exec_container()      # Execute command
│   ├── get_container_stats() # Get resource stats
│   ├── pull_image()          # Pull image manually
│   └── inspect_container()   # Get container details
├── exec_proxy.rs             # DockerExecProxy (23KB)
│   ├── ExecConfig            # Exec configuration
│   ├── create_exec_pty()     # Create with PTY
│   ├── start_exec()          # Start and get stream
│   ├── resize_exec()         # Resize PTY window
│   └── exec_stream()         # Stream I/O
├── docker_trait.rs           # DockerTrait (5.2KB)
│   └── DockerTrait           # Trait for mocking
└── features.rs               # Feature detection (13KB)
    ├── FeatureDetector       # Inspect image labels
    ├── detect_from_image()   # Get feature profile
    └── determine_enabled_features()
```

---

## Usage Examples

### Creating a Container

```rust
use dsb::docker::DockerManager;
use dsb::core::types::{SandboxConfig, PortMapping, PortProtocol};

let docker = DockerManager::new()?;

let config = SandboxConfig {
    image: "nginx:alpine".to_string(),
    name: Some("web".to_string()),
    port_mappings: vec![
        PortMapping {
            host_port: 8080,
            container_port: 80,
            protocol: PortProtocol::Tcp,
        }
    ],
    ..Default::default()
};

let container_id = docker.create_container(&config).await?;
docker.start_container(&container_id).await?;
```

### Executing Commands

```rust
let output = docker.exec_container(
    &container_id,
    vec!["ls".to_string(), "-la".to_string()]
).await?;

println!("Output: {}", output);
```

### Pulling Images with Policy

```rust
use dsb::core::types::PullPolicy;

match PullPolicy::Always {
    PullPolicy::Always => docker.pull_image(&image).await?,
    PullPolicy::Missing => {
        if !docker.image_exists(&image).await? {
            docker.pull_image(&image).await?;
        }
    }
    PullPolicy::Never => { /* Skip pull */ }
}
```

---

## See Also

- [Core Module](../core/README.md) - Sandbox orchestration
- [API Module](../api/README.md) - REST API handlers
- [Config Module](../config/README.md) - Configuration management
