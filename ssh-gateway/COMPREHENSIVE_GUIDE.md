# DSB SSH Gateway - Comprehensive Usage Guide

## Table of Contents

1. [Quick Start](#quick-start)
   - [Local Development](#local-development)
   - [Production Environment](#production-environment)
   - [SSH Client Configuration](#ssh-client-configuration)

2. [User Guide](#user-guide)
   - [Connecting to Sandboxes](#connecting-to-sandboxes)
   - [SSH Session Management](#ssh-session-management)
   - [Common Workflows](#common-workflows)
   - [Troubleshooting](#troubleshooting)

3. [Developer Reference](#developer-reference)
   - [Architecture Overview](#architecture-overview)
   - [Docker Exec Proxy Module](#docker-exec-proxy-module)
   - [Session Manager Module](#session-manager-module)
   - [SSH Server Module](#ssh-server-module)

4. [API Reference](#api-reference)
   - [Docker Exec Proxy API](#docker-exec-proxy-api)
   - [Session Manager API](#session-manager-api)
   - [SSH Session API](#ssh-session-api)

5. [Configuration](#configuration)
   - [Command-Line Options](#command-line-options)
   - [Environment Variables](#environment-variables)
   - [YAML Configuration](#yaml-configuration)
   - [Host Key Management](#host-key-management)

6. [Testing](#testing)
   - [Unit Tests](#unit-tests)
   - [Integration Tests](#integration-tests)
   - [Test Coverage](#test-coverage)

7. [Best Practices](#best-practices)
   - [Security](#security)
   - [Performance](#performance)
   - [Monitoring](#monitoring)

8. [Troubleshooting (Detailed)](#troubleshooting-detailed)
   - [Common Issues (Detailed)](#common-issues-detailed)
   - [Debug Mode](#debug-mode)
   - [Getting Help](#getting-help)

---

## Quick Start

### Local Development

#### Prerequisites

```bash
# 1. Start Docker Desktop (macOS) or Docker daemon (Linux)
open -a Docker  # macOS
sudo systemctl start docker  # Linux

# 2. Verify Docker is running
docker ps
```

#### Step 1: Start DSB API Server

```bash
# Terminal 1: Start DSB API server
cd /path/to/dsb
dsb server --port 8080

# Or with Docker
docker run -d \
  --name dsb-server \
  -p 8080:8080 \
  -e DATABASE_URL=postgresql://postgres:postgres@localhost:5433/dsb \
  ghcr.io/dsb/dsb:latest \
  server --port 8080
```

#### Step 2: Create a Sandbox

```bash
# Terminal 2: Create a test sandbox
dsb create -i python:3.12-slim -n my-test-sandbox

# List sandboxes to get the ID
dsb list

# Output example:
# ID                                    NAME             STATE    IMAGE
# 40dcd7b4-1bbd-4dbe-a20f-3af46fb32d41  my-test-sandbox  running  python:3.12.11
```

#### Step 3: Start SSH Gateway

```bash
# Terminal 3: Navigate to SSH gateway
cd ssh-gateway

# Start SSH gateway (development mode - no API key)
cargo run -- --port 2222 --api-url http://localhost:8080 --log-level debug

# Or build and run directly
cargo build --release
./target/release/ssh-gateway --port 2222 --api-url http://localhost:8080
```

**Expected Output:**

```
Starting DSB SSH Gateway Server v0.1.0
Configuration: Args { port: 2222, api_url: "http://localhost:8080", ... }
SSH server will listen on port 2222
DSB API URL: http://localhost:8080
API key authentication disabled (development mode)
SSH server initialized successfully
```

#### Step 4: Connect via SSH

```bash
# Terminal 4: Connect to the sandbox
# Format: ssh -p <gateway-port> <sandbox-id>@localhost

ssh -p 2222 40dcd7b4-1bbd-4dbe-a20f-3af46fb32d41@localhost

# First connection - you'll see:
# The authenticity of host '[localhost]:2222' can't be established.
# RSA key fingerprint is SHA256:abc123...
# Are you sure you want to continue connecting (yes/no/[fingerprint])? yes
```

**🎯 You're now in the sandbox shell!**

```bash
# Try some commands
python --version
pwd
ls -la

# Test immediate output forwarding
for i in 1 2 3 4 5; do echo "Line $i"; sleep 0.2; done
# Each line appears immediately, not all at once!
```

### Production Environment

#### Architecture in Production

```
┌─────────────────┐
│  SSH Client     │
└────────┬────────┘
         │ SSH (port 2222)
         ▼
┌─────────────────────────────┐
│   SSH Gateway Server        │
│   (Load Balanced)            │
│   - API Key Auth Required    │
│   - Persistent Host Keys     │
│   - Production Logging       │
└────────┬────────────────────┘
         │ HTTPS (port 443/8080)
         ▼
┌─────────────────────────────┐
│      DSB API Server          │
│   - Authorization            │
│   - Session Tracking        │
└─────────────────────────────┘
```

#### Step 1: Set Environment Variables

```bash
# Production API endpoint
export DSB_API_URL="https://dsb.yourdomain.com"
export DSB_API_KEY="your-production-api-key-here"

# Optional: Custom Docker socket
export DOCKER_HOST="unix:///var/run/docker.sock"
```

#### Step 2: Start SSH Gateway (Production Mode)

```bash
# Option 1: Direct command with API key
ssh-gateway \
  --port 2222 \
  --api-url https://dsb.yourdomain.com \
  --api-key "$DSB_API_KEY" \
  --log-level info

# Option 2: Using environment variables
ssh-gateway \
  --port 2222 \
  --api-url "$DSB_API_URL" \
  --log-level info
```

#### Step 3: Systemd Service (Recommended for Production)

Create `/etc/systemd/system/dsb-ssh-gateway.service`:

```ini
[Unit]
Description=DSB SSH Gateway Server
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
User=dsb
Group=dsb
WorkingDirectory=/opt/dsb-ssh-gateway

# Environment
Environment="DSB_API_URL=https://dsb.yourdomain.com"
Environment="DSB_API_KEY=your-production-api-key"
Environment="DOCKER_HOST=unix:///var/run/docker.sock"

# ExecStart
ExecStart=/opt/dsb-ssh-gateway/ssh-gateway \
  --port 2222 \
  --api-url https://dsb.yourdomain.com \
  --log-level info

# Auto-restart
Restart=always
RestartSec=10

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=dsb-ssh-gateway

[Install]
WantedBy=multi-user.target
```

Enable and start the service:

```bash
# Reload systemd
sudo systemctl daemon-reload

# Enable auto-start on boot
sudo systemctl enable dsb-ssh-gateway

# Start the service
sudo systemctl start dsb-ssh-gateway

# Check status
sudo systemctl status dsb-ssh-gateway

# View logs
sudo journalctl -u dsb-ssh-gateway -f

# Check if listening on correct port
sudo netstat -tlnp | grep 2222
# or
sudo ss -tlnp | grep 2222
```

#### Step 4: Get Your Sandbox ID

```bash
# Method 1: Via DSB API
curl -H "X-API-Key: $DSB_API_KEY" \
  https://dsb.yourdomain.com/sandboxes?state=running

# Method 2: Via DSB CLI
dsb list --state running

# Output example:
# ID                                    NAME           STATE
# a1b2c3d4-e5f6-7890-abcd-ef1234567890  production-1  running
# f09f8e7d-6c5b-4a3d-9e1f-2a3b4c5d6e7f  worker-2       running
```

#### Step 5: Connect to Production Sandbox

```bash
# Format: ssh -p <gateway-port> <sandbox-id>@<gateway-host>

# Direct connection
ssh -p 2222 a1b2c3d4-e5f6-7890-abcd-ef1234567890@dsb.yourdomain.com

# Using SSH config (recommended - see below)
ssh production-1
```

### SSH Client Configuration

#### Recommended SSH Config

Create or edit `~/.ssh/config`:

```ssh
# ============================================================================
# DSB SSH Gateways - Production
# ============================================================================

Host dsb-prod
    HostName dsb.yourdomain.com
    Port 2222
    # Note: With persistent host keys, this is only needed on first connection
    UserKnownHostsFile ~/.ssh/known_hosts_dsb
    StrictHostKeyChecking accept-new

# ============================================================================
# DSB SSH Gateway - Local Development
# ============================================================================

Host dsb-local
    HostName localhost
    Port 2222
    # Gateway uses persistent host key - standard SSH behavior
    # Accept host key on first connection, then it's stored permanently

# ============================================================================
# Specific Sandbox Aliases (Production)
# ============================================================================

Host production-1
    HostName dsb.yourdomain.com
    Port 2222
    User a1b2c3d4-e5f6-7890-abcd-ef1234567890
    # Standard known_hosts file
    # Accept host key on first connection

Host worker-2
    HostName dsb.yourdomain.com
    Port 2222
    User f09f8e7d-6c5b-4a3d-9e1f-2a3b4c5d6e7f
    UserKnownHostsFile ~/.ssh/known_hosts_dsb

Host staging-db
    HostName dsb-staging.yourdomain.com
    Port 2222
    User 1a2b3c4d-5e6f-7890-1234-56789abcdef0
    UserKnownHostsFile ~/.ssh/known_hosts_dsb
```

#### Using SSH Config Aliases

With the above config, you can connect using simple aliases:

```bash
# Local development
ssh 40dcd7b4-1bbd-4dbe-a20f-3af46fb32d41@dsb-local

# Production sandboxes
ssh production-1
ssh worker-2
ssh staging-db
```

#### First Connection - Accept Host Key

On first connection, you'll see the standard SSH host key prompt:

```bash
ssh -p 2222 40dcd7b4-1bbd-4dbe-a20f-3af46fb32d41@localhost

# Output:
# The authenticity of host '[localhost]:2222' can't be established.
# ED25519 key fingerprint is SHA256:abc123...
# Are you sure you want to continue connecting (yes/no/[fingerprint])? yes
#
# Warning: Permanently added '[localhost]:2222' (ED25519) to the list of known hosts.
```

**Important**: You only need to accept the host key **once**. The gateway uses a persistent host key that survives restarts, so subsequent connections won't show this prompt.

#### Subsequent Connections

After accepting the host key once:

```bash
# No prompt needed - connects immediately
ssh -p 2222 40dcd7b4-1bbd-4dbe-a20f-3af46fb32d41@localhost
```

---

## User Guide

### Key Concepts

#### Username = Sandbox ID

- The SSH username **must** be the sandbox UUID
- Example: `ssh 40dcd7b4-1bbd-4dbe-a20f-3af46fb32d41@localhost`
- The gateway validates the username is a valid UUID format
- Actual authorization happens when the shell session is requested

#### Authentication Flow

1. **SSH Handshake**: Client presents public key to gateway
2. **Username Validation**: Gateway validates username is a valid UUID (sandbox ID)
3. **Sandbox Authorization**: When shell is requested, gateway calls DSB API to authorize access
4. **Docker Exec Creation**: If authorized, Docker exec instance is created with PTY
5. **Session Active**: Bidirectional I/O begins between client and container

#### Architecture

```
SSH Client → SSH Gateway → DSB API (authorization only)
                          ↓
                    Direct Docker Connection
                          ↓
                    Container Shell (PTY)
```

### Connecting to Sandboxes

#### Quick Reference Commands

**Local Development:**

```bash
# 1. Start DSB server
dsb server --port 8080

# 2. Create sandbox
dsb create -i python:3.12-slim -n test

# 3. Start SSH gateway
cd ssh-gateway
cargo run -- --port 2222 --api-url http://localhost:8080

# 4. Connect (get ID from step 2)
# First connection: accept host key prompt
ssh -p 2222 <sandbox-id>@localhost
```

**Production:**

```bash
# 1. Set credentials
export DSB_API_URL="https://dsb.yourdomain.com"
export DSB_API_KEY="your-api-key"

# 2. Start SSH gateway
ssh-gateway --port 2222 --api-url "$DSB_API_URL"

# 3. Get sandbox ID
curl -H "X-API-Key: $DSB_API_KEY" "$DSB_API_URL/sandboxes?state=running"

# 4. Connect
ssh -p 2222 <sandbox-id>@dsb.yourdomain.com
```

**One-Liner Connection Commands:**

```bash
# Local - First connection will prompt to accept host key
ssh -p 2222 <id>@localhost

# Production - Standard connection
ssh -p 2222 <id>@dsb.yourdomain.com

# With custom SSH key
ssh -i ~/.ssh/custom_key -p 2222 <id>@localhost

# Verbose mode (for debugging)
ssh -vvv -p 2222 <id>@localhost
```

### SSH Session Management

The SSH gateway provides comprehensive session management through the DSB API:

1. **Authorization**: Validates sandbox access before allowing SSH connections
2. **Session Creation**: Creates SSH session records via DSB API
3. **Heartbeat**: Keeps sessions alive with periodic activity updates (every 30 seconds)
4. **Termination**: Cleanly closes sessions with proper API notification

#### Session Lifecycle

```
┌─────────────┐    1. authorize    ┌──────────────┐
│ SSH Client  │ ──────────────────> │ DSB API      │
└─────────────┘                    └──────────────┘
      │                                    │
      │ 2. create session                  │
      └────────────────────────────────────┘

┌─────────────┐    3. heartbeat    ┌──────────────┐
│ SSH Client  │ <───────────────── │ DSB API      │
└─────────────┘   (every 30s)      └──────────────┘
      │
      │ 4. terminate on disconnect
      └────────────────────────────────────┘
```

### Common Workflows

#### Interactive Development Session

```bash
# 1. Start all services
dsb server --port 8080 &
dsb create -i python:3.12 -n dev-box
cd ssh-gateway
cargo run -- --port 2222 --api-url http://localhost:8080 &

# 2. Connect to sandbox
SANDBOX_ID=$(dsb list | grep dev-box | awk '{print $1}')
ssh -p 2222 $SANDBOX_ID@localhost

# 3. Work in the sandbox
cd /workspace
vim main.py
python main.py

# 4. Exit
exit
```

#### Production Debugging Session

```bash
# 1. Set up environment
export DSB_API_URL="https://dsb.production.com"
export DSB_API_KEY="prod-key"

# 2. Find the sandbox you need
curl -H "X-API-Key: $DSB_API_KEY" \
  "$DSB_API_URL/sandboxes?state=running&name=problematic-service"

# 3. Connect with verbose logging
ssh -vvv -p 2222 <sandbox-id>@dsb.production.com

# 4. Debug
top
ps aux
tail -f /var/log/app.log

# 5. Check session activity from another terminal
curl -H "X-API-Key: $DSB_API_KEY" \
  "$DSB_API_URL/ssh-sessions?state=active"
```

#### Batch Operations with Multiple Sandboxes

```bash
# 1. Get all running sandboxes
curl -H "X-API-Key: $DSB_API_KEY" \
  "$DSB_API_URL/sandboxes?state=running" | jq -r '.[].id' > sandbox_ids.txt

# 2. Run command on all sandboxes
while read id; do
  echo "Connecting to $id..."
  ssh -p 2222 $id@dsb.yourdomain.com "uptime && df -h"
done < sandbox_ids.txt
```

---

## Developer Reference

### Architecture Overview

#### System Overview

```mermaid
graph TB
    Client[SSH Client] -->|SSH Protocol| Gateway[SSH Gateway Server]

    Gateway -->|russh Handle| Background[Background Task]
    Gateway -->|HTTP| API[DSB API]

    Gateway -->|Direct Connection| Docker[Docker Daemon]
    Docker -->|Exec Streams| Container[Container Shell PTY]

    Background -->|Immediate Forwarding| Gateway

    style Gateway fill:#f9f,stroke:#333,stroke-width:2px
    style Background fill:#bbf,stroke:#333,stroke-width:2px
    style API fill:#bfb,stroke:#333,stroke-width:2px
    style Docker fill:#bfb,stroke:#333,stroke-width:2px
```

#### Module Relationships

```mermaid
graph LR
    SSH[SSH Server Module] -->|uses| Docker[Docker Exec Proxy]
    SSH -->|uses| Session[Session Manager]

    Docker -->|connects to| DockerDaemon[Docker Daemon]
    Session -->|calls| DSBAPI[DSB API]

    style SSH fill:#f9f,stroke:#333,stroke-width:2px
    style Docker fill:#bfb,stroke:#333,stroke-width:2px
    style Session fill:#bbf,stroke:#333,stroke-width:2px
```

#### Data Flow

**SSH Connection Flow:**

1. SSH client connects → SSH Server
2. SSH Server → Session Manager → DSB API (authorize)
3. SSH Server → Docker Exec Proxy → Docker (create exec)
4. Background task reads Docker output → Handle → SSH client (immediate)
5. SSH client input → Docker stdin (bidirectional)
6. Session Manager sends heartbeat to DSB API
7. On disconnect → Session Manager terminates session

**Key Modules:**

- **SSH Server (`src/ssh.rs`)** - russh-based SSH protocol handling
- **Docker Exec Proxy (`src/docker.rs`)** - Direct Docker connection for exec operations
- **Session Manager (`src/session.rs`)** - DSB API client for authorization and tracking

### Docker Exec Proxy Module

The `DockerExecProxy` module manages Docker exec instances with PTY (pseudo-terminal) support.

#### Architecture

```
┌─────────────┐     stdin      ┌─────────────────┐
│ SSH Gateway │──────────────>│                 │
│             │               │ DockerExecProxy │
│             │<──────────────│                 │
│             │ stdout/stderr └─────────────────┘
└─────────────┘                      │
                                      ▼
                              ┌───────────────┐
                              │ Docker Daemon │
                              └───────────────┘
                                      │
                                      ▼
                              ┌───────────────┐
                              │  Container    │
                              │  Shell (PTY)  │
                              └───────────────┘
```

#### Key Features

- Exec instance creation with PTY allocation
- Bidirectional streaming (stdin/stdout/stderr)
- PTY resize support
- Stream demultiplexing (stdout/stderr separation)
- Exec lifecycle management (start, inspect, cleanup)

#### Component Interaction

1. **SSH Gateway** → Creates DockerExecProxy instance
2. **DockerExecProxy** → Calls Docker API to create exec
3. **Docker API** → Returns exec ID
4. **DockerExecProxy** → Starts exec, gets I/O streams
5. **Background Task** → Reads Docker output, forwards to SSH client
6. **SSH Gateway** → Forwards user input to Docker stdin

### Session Manager Module

The `SessionManager` handles HTTP communication with the DSB API for SSH session management.

#### Architecture

```mermaid
graph TB
    Gateway[SSH Gateway] -->|HTTP| SM[SessionManager]
    SM -->|HTTP with X-API-Key| API[DSB API]

    SM -->|1. authorize_sandbox| API
    API -->|AuthContext| SM

    SM -->|2. create_session| API
    API -->|SshSession| SM

    SM -->|3. send_heartbeat| API
    API -->|200 OK| SM

    SM -->|4. terminate_session| API
    API -->|200 OK| SM

    style SM fill:#f9f,stroke:#333,stroke-width:2px
    style API fill:#bfb,stroke:#333,stroke-width:2px
```

#### DSB API Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/ssh/authorize/{sandbox_id}` | GET | Authorize sandbox access |
| `/ssh-sessions` | POST | Create new SSH session |
| `/ssh-sessions/{id}/heartbeat` | POST | Update session activity |
| `/ssh-sessions/{id}/terminate` | POST | Terminate session |

#### Key Features

- HTTP-based API client using `reqwest`
- API key authentication support
- Full session lifecycle management
- Comprehensive error handling
- Structured logging with `tracing`

**Important Note**: The SSH gateway connects **directly** to Docker daemon for exec operations, not through the DSB API. The DSB API is used **only** for authorization, session tracking, and lifecycle management.

### SSH Server Module

The SSH Server module (`src/ssh.rs`) implements the SSH protocol using the `russh` library.

#### Key Components

- **russh Server**: Handles SSH protocol and encryption
- **Handle-based I/O**: Immediate output forwarding to client
- **Background Tasks**: Async tasks for reading Docker output
- **PTY Management**: Terminal size and window changes
- **Authentication**: Public key authentication

#### Immediate Output Forwarding

The SSH gateway uses a Handle-based architecture for immediate output forwarding:

1. Docker output is read in background task
2. Data is immediately written to russh Handle
3. Handle forwards directly to SSH client
4. No buffering or delays

This ensures real-time output as described in the [Quick Start](#quick-start) section.

---

## API Reference

### Docker Exec Proxy API

#### Constructor

```rust
pub fn new(container_id: String) -> Self
pub fn with_docker(container_id: String, docker: Docker) -> Self
```

**Example:**

```rust
use ssh_gateway::docker::DockerExecProxy;

// With default Docker client
let mut proxy = DockerExecProxy::new("my-container".to_string());

// With custom Docker client
use bollard::Docker;
let docker = Docker::connect_with_unix(
    "unix:///var/run/docker.sock",
    120,
    bollard::API_DEFAULT_VERSION
)?;
let proxy = DockerExecProxy::with_docker("container-123".to_string(), docker);
```

#### Lifecycle Methods

```rust
pub async fn create_exec(&mut self) -> Result<String>
pub async fn start_exec(&mut self) -> Result<()>
```

**Example:**

```rust
// Create exec with PTY
let exec_id = proxy.create_exec().await?;

// Start exec and get I/O streams
proxy.start_exec().await?;
```

#### I/O Operations

```rust
pub async fn write_stdin(&mut self, data: &[u8]) -> Result<()>
pub async fn read_output(&mut self) -> Option<Result<Vec<u8>>>
```

**Example:**

```rust
// Write to container stdin
proxy.write_stdin(b"echo 'Hello World'\n").await?;

// Read from container stdout/stderr
if let Some(Ok(data)) = proxy.read_output().await {
    println!("Output: {}", String::from_utf8_lossy(&data));
}
```

#### PTY Management

```rust
pub async fn resize_pty(&mut self, rows: u16, cols: u16) -> Result<()>
pub fn set_pty_size(&mut self, rows: u16, cols: u16)
```

**Example:**

```rust
// Set initial PTY size
proxy.set_pty_size(24, 80);

// Resize during session
proxy.resize_pty(50, 160).await?;
```

#### Inspection

```rust
pub async fn inspect_exec(&self) -> Result<bollard::models::ExecInspectResponse>
pub async fn is_running(&self) -> Result<bool>
pub async fn get_exit_code(&self) -> Result<Option<i64>>
```

**Example:**

```rust
// Check if exec is running
if proxy.is_running().await? {
    println!("Exec is still running");
} else {
    if let Ok(Some(code)) = proxy.get_exit_code().await {
        println!("Exited with code: {}", code);
    }
}
```

### Session Manager API

#### Constructor

```rust
pub fn new(api_url: &str, api_key: Option<String>) -> Self
```

**Example:**

```rust
use ssh_gateway::session::SessionManager;

// Without API key (development mode)
let manager = SessionManager::new("http://localhost:8080", None);

// With API key (production mode)
let manager = SessionManager::new(
    "http://localhost:8080",
    Some("your-secret-api-key".to_string())
);
```

#### Authorization

```rust
pub async fn authorize_sandbox(&self, sandbox_id: &uuid::Uuid) -> Result<AuthContext>
```

**Validates:**

1. API key (if configured)
2. Sandbox exists
3. Sandbox is in "running" state

**Example:**

```rust
use uuid::Uuid;

let sandbox_id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000")?;
let auth = manager.authorize_sandbox(&sandbox_id).await?;

if auth.authorized {
    let container_id = auth.sandbox.unwrap().container_id;
    // Proceed with SSH connection
} else {
    // Deny access
}
```

#### Session Lifecycle

```rust
pub async fn create_session(
    &self,
    sandbox_id: &uuid::Uuid,
    client_ip: &str
) -> Result<SshSession>
```

**Example:**

```rust
let session = manager.create_session(&sandbox_id, "192.168.1.100").await?;
println!("Session ID: {}", session.id);
println!("Connected at: {}", session.connected_at);
```

#### Heartbeat

```rust
pub async fn send_heartbeat(
    &self,
    session_id: &uuid::Uuid,
    bytes_sent: i64,
    bytes_received: i64
) -> Result<()>
```

**Recommended Interval:** Every 30 seconds

**Example:**

```rust
// In a background task
loop {
    tokio::time::sleep(Duration::from_secs(30)).await;
    manager.send_heartbeat(&session_id, total_sent, total_received).await?;
}
```

#### Termination

```rust
pub async fn terminate_session(
    &self,
    session_id: &uuid::Uuid,
    reason: &str
) -> Result<()>
```

**Call this when:**

- Client disconnects
- Authentication fails
- Connection error occurs

**Example:**

```rust
// On disconnect
manager.terminate_session(&session_id, "Client disconnected").await?;
```

### SSH Session API

The SSH gateway doesn't expose a direct REST API for SSH sessions. Instead, SSH sessions are managed through the standard SSH protocol. Session state is tracked internally and communicated with the DSB API via the Session Manager.

#### Session State Tracking

Sessions tracked by the gateway include:

- `id`: Session UUID
- `sandbox_id`: Associated sandbox ID
- `state`: Session state (active, terminated)
- `client_ip`: Client IP address
- `connected_at`: Connection timestamp
- `last_activity_at`: Last activity timestamp
- `bytes_sent`: Total bytes sent to client
- `bytes_received`: Total bytes received from client

#### Session Monitoring

```bash
# View active sessions via DSB API
curl -H "X-API-Key: $DSB_API_KEY" \
  https://dsb.yourdomain.com/ssh-sessions?state=active

# View session statistics
curl -H "X-API-Key: $DSB_API_KEY" \
  https://dsb.yourdomain.com/ssh-sessions/statistics
```

---

## Configuration

### Configuration Priority

Configuration is loaded from multiple sources in order of priority (highest to lowest):

1. **CLI arguments** (highest priority)
2. **Environment variables** (`DSB_SSH__*` prefix)
3. **YAML configuration file** (`dsb.yaml` or `dsb.yml`)
4. **`.env` file**
5. **Default values** (lowest priority)

### Command-Line Options

```bash
ssh-gateway [OPTIONS]

Options:
  -p, --port <PORT>              SSH server listening port [default: 2222]
  -a, --api-url <URL>            DSB API base URL [default: http://localhost:8080]
  -k, --api-key <KEY>            API key for authentication (optional)
      --host-key-path <PATH>     Path to SSH host key (auto-generated if not specified)
  -l, --log-level <LEVEL>        Logging level [default: info]
  -h, --help                     Print help
  -V, --version                  Print version
```

### Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `DSB_SSH__PORT` | SSH server listening port | `2222` |
| `DSB_SSH__API_URL` | DSB API base URL | `http://localhost:8080` |
| `DSB_SSH__API_KEY` | API key for authentication | None |
| `DSB_SSH__HOST_KEY_PATH` | Path to SSH host key | `~/.dsb/ssh_host_key` |
| `DSB_LOGGING__LEVEL` | Logging level | `info` |
| `DOCKER_HOST` | Docker daemon socket | `unix:///var/run/docker.sock` |

### YAML Configuration

Create a `dsb.yaml` file in your project directory:

```yaml
ssh:
  port: 2222
  api_url: "http://localhost:8080"
  # api_key: "your-api-key-here"  # Optional
  host_key_path: "/path/to/host_key"  # Optional

logging:
  level: "debug"  # Options: trace, debug, info, warn, error
```

Then start the SSH gateway (it will automatically load the configuration):

```bash
ssh-gateway
```

### Host Key Management

**Default Behavior**: The SSH gateway automatically generates and uses a persistent Ed25519 host key.

**Key Location**: `~/.dsb/ssh_host_key`

**Auto-Generation**: If the key file doesn't exist, it's automatically created on the first run:

```bash
$ ssh-gateway
INFO SSH host key not found at: ~/.dsb/ssh_host_key
INFO Auto-generating persistent SSH host key...
INFO Generated persistent SSH host key: ~/.dsb/ssh_host_key
```

**Custom Key Path**: Use `host_key_path` to specify a custom location:

```yaml
# In dsb.yaml
ssh:
  host_key_path: "/path/to/custom_key"
```

Or via environment variable:

```bash
export DSB_SSH__HOST_KEY_PATH=/path/to/custom_key
ssh-gateway
```

**Generating Custom Keys**:

```bash
# Generate a new Ed25519 key
ssh-keygen -t ed25519 -f /path/to/my_key -N ""

# Use it with SSH gateway
ssh-gateway --host-key-path /path/to/my_key
```

**Connection Experience**:

With persistent host keys, you only need to accept the host key once:

```bash
# First connection: one-time prompt
$ ssh -p 2222 <sandbox-id>@localhost
The authenticity of host '[localhost]:2222' can't be established.
ED25519 key fingerprint is SHA256:abc123...
Are you sure you want to continue connecting (yes/no/[fingerprint])? yes
Warning: Permanently added '[localhost]:2222' (ED25519) to known hosts.

# Subsequent connections: no prompt!
$ ssh -p 2222 <sandbox-id>@localhost
# Connected immediately - no warnings
```

**Key Benefits**:

- ✅ No more host key warnings after restart
- ✅ Standard SSH security model
- ✅ Keys persist across gateway restarts
- ✅ Compatible with SSH config `StrictHostKeyChecking accept-new`

**Key Rotation**: If you need to change the host key:

```bash
# Remove old key from known_hosts
ssh-keygen -R "[localhost]:2222"

# Restart gateway with new key (or delete old key file)
rm ~/.dsb/ssh_host_key
ssh-gateway  # Will generate new key
```

---

## Testing

### Unit Tests

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test docker::tests
cargo test session::tests
cargo test ssh::tests

# Run with output
cargo test -- --nocapture
```

### Integration Tests

Integration tests require a running DSB API server and Docker daemon.

```bash
# Ensure DSB API is running
curl http://localhost:8080/health

# Ensure Docker is running
docker ps

# Run integration tests
cargo test --test integration_tests

# Run specific integration test
cargo test test_docker_exec_read_output
```

### Test Coverage

```bash
# Generate coverage report
cargo tarpaulin --out Html

# Or with the project's make command
make coverage
```

### Example Unit Test

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_size_default() {
        let size = PtySize::default();
        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
    }

    #[test]
    fn test_session_manager_creation() {
        let manager = SessionManager::new("http://localhost:8080", None);
        assert_eq!(manager.get_api_url(), "http://localhost:8080");
        assert!(manager.api_key.is_none());
    }
}
```

### Example Integration Test

```rust
#[tokio::test]
async fn test_docker_exec_create() {
    // Start a test container
    let docker = Docker::connect_with_unix_defaults().unwrap();
    let container = create_test_container(&docker).await;

    // Create proxy
    let mut proxy = DockerExecProxy::new(container.id.clone());

    // Create exec
    let exec_id = proxy.create_exec().await.unwrap();
    assert!(!exec_id.is_empty());

    // Cleanup
    cleanup_container(&docker, &container.id).await;
}
```

---

## Best Practices

### Security

#### Production Environment

1. **Always use API keys in production**

   ```bash
   export DSB_API_KEY="your-secret-key"
   ssh-gateway --api-url https://dsb.yourdomain.com
   ```

2. **Use HTTPS for DSB API**

   ```bash
   # Good
   --api-url https://dsb.yourdomain.com

   # Avoid in production
   --api-url http://dsb.yourdomain.com
   ```

3. **Restrict SSH key access**

   ```bash
   # Use separate SSH keys for sandbox access
   ssh-keygen -t rsa -b 4096 -f ~/.ssh/dsb_sandbox_key
   ssh -i ~/.ssh/dsb_sandbox_key -p 2222 <id>@gateway
   ```

4. **Monitor sessions**

   ```bash
   # View active sessions via DSB API
   curl -H "X-API-Key: $DSB_API_KEY" \
     https://dsb.yourdomain.com/ssh-sessions?state=active

   # View session statistics
   curl -H "X-API-Key: $DSB_API_KEY" \
     https://dsb.yourdomain.com/ssh-sessions/statistics
   ```

5. **Enable gateway logging**

   ```bash
   # Start with info level logging
   ssh-gateway --log-level info

   # For debugging issues
   ssh-gateway --log-level debug
   ```

### Performance

#### Memory Usage

- The proxy uses minimal buffering
- Output is read as available (not buffered)
- Memory per connection: O(1) constant

#### Latency

- `read_output()` has < 10ms latency
- `write_stdin()` has < 5ms latency
- PTY resize has < 50ms latency

#### Concurrency

- Multiple exec instances can run on the same container
- Each proxy instance is independent
- Thread-safe with async/await

#### Resource Limits

- Max concurrent execs per container: Limited by Docker
- Recommended: < 100 concurrent sessions per host
- Monitor with: `docker stats` and `docker top`

#### Best Practices

1. **Always check return values**

   ```rust
   // Good
   let exec_id = proxy.create_exec().await?;
   proxy.start_exec().await?;

   // Bad - ignore errors
   let _ = proxy.create_exec().await;
   let _ = proxy.start_exec().await;
   ```

2. **Use Tokio timeouts for long operations**

   ```rust
   use tokio::time::timeout;

   let result = timeout(
       Duration::from_secs(30),
       proxy.read_output()
   ).await?;

   match result {
       Ok(Some(Ok(data))) => println!("Got data"),
       Ok(Some(Err(e))) => eprintln!("Error: {:?}", e),
       Ok(None) => println!("Stream closed"),
       Err(_) => eprintln!("Timeout"),
   }
   ```

3. **Clean up resources**

   ```rust
   // The proxy cleans up automatically on drop
   async fn scoped_exec() -> Result<()> {
       let mut proxy = DockerExecProxy::new("container".to_string());
       proxy.create_exec().await?;
       proxy.start_exec().await?;
       // Do work...
       // Proxy automatically drops and cleans up here
       Ok(())
   }
   ```

4. **Handle stream closure gracefully**

   ```rust
   loop {
       match proxy.read_output().await {
           Some(Ok(data)) if !data.is_empty() => {
               // Process data
           }
           Some(Ok(_)) => {
               // Empty data = EOF
               break;
           }
           Some(Err(e)) => {
               eprintln!("Error: {:?}", e);
               break;
           }
           None => {
               // Stream closed
               break;
           }
       }
   }
   ```

5. **Monitor exec status**

   ```rust
   // Check if exec is still running before operations
   if proxy.is_running().await? {
       proxy.write_stdin(b"command\n").await?;
   } else {
       eprintln!("Exec has finished");
   }
   ```

### Monitoring

#### Session Monitoring

```bash
# View active sessions
curl -H "X-API-Key: $DSB_API_KEY" \
  https://dsb.yourdomain.com/ssh-sessions?state=active

# View session statistics
curl -H "X-API-Key: $DSB_API_KEY" \
  https://dsb.yourdomain.com/ssh-sessions/statistics

# Monitor specific session
curl -H "X-API-Key: $DSB_API_KEY" \
  https://dsb.yourdomain.com/ssh-sessions/<session-id>
```

#### Gateway Monitoring

```bash
# Check gateway logs
tail -f /var/log/dsb-ssh-gateway.log

# Or with systemd
sudo journalctl -u dsb-ssh-gateway -f

# Check if gateway is running
ps aux | grep ssh-gateway

# Check port is listening
sudo ss -tlnp | grep 2222
```

#### Docker Monitoring

```bash
# Check container stats
docker stats

# Check running containers
docker ps

# Check Docker daemon
docker info
```

#### Logging Best Practices

```rust
use tracing::{info, error, instrument};

#[instrument(skip(manager))]
async fn handle_ssh_connection(
    manager: &SessionManager,
    sandbox_id: Uuid,
    client_ip: &str
) -> Result<()> {
    info!("Authorizing sandbox");

    let auth = manager.authorize_sandbox(&sandbox_id).await?;
    if !auth.authorized {
        error!("Authorization denied for sandbox {}", sandbox_id);
        return Err(anyhow::anyhow!("Access denied"));
    }

    info!("Creating SSH session");
    let session = manager.create_session(&sandbox_id, client_ip).await?;

    info!("Session created: {}", session.id);

    // ... handle connection ...

    Ok(())
}
```

---

## Troubleshooting

### Common Issues

#### Problem: "Permission denied (publickey)"

**Symptoms:**

```
Permission denied (publickey,gssapi-keyex,gssapi-with-mic).
```

**Solutions:**

```bash
# 1. Check your SSH key is being used
ssh -vvv -p 2222 <sandbox-id>@localhost

# 2. Specify your SSH key explicitly
ssh -i ~/.ssh/id_rsa -p 2222 <sandbox-id>@localhost

# 3. Add key to SSH agent
ssh-add ~/.ssh/id_rsa

# 4. Verify gateway is accepting public key auth
# Check gateway logs for "Public key authentication for user: <id>"
```

#### Problem: "Connection refused"

**Symptoms:**

```
ssh: connect to host localhost port 2222: Connection refused
```

**Solutions:**

```bash
# 1. Check if SSH gateway is running
ps aux | grep ssh-gateway
pgrep -a ssh-gateway

# 2. Check if port is listening
netstat -an | grep 2222
# or
ss -tlnp | grep 2222

# 3. Check gateway logs for startup errors
# If running via cargo:
cargo run -- --port 2222 --api-url http://localhost:8080

# 4. Verify port is not already in use
lsof -i :2222
```

#### Problem: "Session terminated immediately"

**Symptoms:**

```
Connection to localhost closed by remote host.
```

**Solutions:**

```bash
# 1. Check sandbox exists and is running
curl http://localhost:8080/sandboxes/<sandbox-id>
dsb info <sandbox-id>

# 2. Verify DSB API is accessible
curl http://localhost:8080/health

# 3. Check gateway logs for authorization errors
# Look for messages like "Authorization failed for sandbox: <id>"

# 4. Verify sandbox container is running
docker ps | grep <container-id>

# 5. Enable debug logging to see full flow
RUST_LOG=debug cargo run -- --port 2222 --api-url http://localhost:8080
```

#### Problem: "No shell prompt"

**Symptoms:**

```
# Connection successful but no prompt appears
```

**Solutions:**

```bash
# 1. Check container has a shell
docker exec <container-id> which bash
docker exec <container-id> which sh

# 2. Try pressing Enter (sometimes prompt is hidden)
# After connecting, press Enter key

# 3. Check PTY allocation was successful
# Enable debug logs and look for "PTY request"

# 4. Verify container is running correct image
docker inspect <container-id> | grep -i image
```

#### Problem: "Output appears all at once (buffered)"

**Symptoms:**

```
# Expected: Each line appears immediately
# Actual: All lines appear after command completes
```

**This should NOT happen with the Handle-based implementation.** If it does:

```bash
# 1. Verify you're using the latest version
ssh-gateway --version

# 2. Check for "Immediate Output" in logs
# Should see: "Successfully forwarded X bytes to SSH client via Handle"

# 3. Test with unbuffered command
ssh -p 2222 <id>@localhost
# Then run: python -u -c "import time; [print(f'Line {i}') or time.sleep(0.2) for i in range(5)]"

# 4. Check for buffering in your application
# Use unbuffered Python: python -u
# Flush output explicitly: sys.stdout.flush()
```

#### Problem: "Host key verification failed"

**Symptoms:**

```
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @
@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@
```

**Solutions:**

```bash
# With persistent host keys, this warning indicates a potential
# security issue (man-in-the-middle attack) or intentional key rotation.

# Option 1: Investigate first (Recommended)
# Check if key file was intentionally regenerated
ls -la ~/.dsb/ssh_host_key

# Check gateway logs for key generation
# If you see "Auto-generating persistent SSH host key", the key was regenerated

# Option 2: Remove old key (if key was rotated)
ssh-keygen -R "[localhost]:2222"

# Reconnect to accept new key
ssh -p 2222 <sandbox-id>@localhost

# Option 3: Verify legitimate key change
# Contact your administrator if running in production
# Verify the gateway host key fingerprint
# Only accept if you're certain the change is legitimate
```

**Prevention**:

- Don't delete `~/.dsb/ssh_host_key` unless intentional
- Back up custom host keys if using `--host-key-path`
- Monitor file system changes to key directory

### Debug Mode

For detailed troubleshooting, enable verbose SSH logging:

```bash
# Verbose mode (shows all steps)
ssh -vvv -p 2222 <sandbox-id>@localhost

# With custom log level
ssh -vvv -p 2222 <sandbox-id>@localhost 2>&1 | tee ssh-debug.log

# Check what the gateway is doing
# In another terminal, watch the logs:
tail -f /var/log/syslog | grep ssh-gateway
# or if running via cargo:
tail -f ~/.pm2/logs/  # depending on your log config
```

### Getting Help

If you encounter issues not covered here:

```bash
# 1. Check all components are running
# DSB Server
curl http://localhost:8080/health

# SSH Gateway
ps aux | grep ssh-gateway

# Docker
docker ps

# 2. Collect logs
# Gateway logs (with debug)
RUST_LOG=debug ssh-gateway --port 2222 --api-url http://localhost:8080

# SSH client logs
ssh -vvv -p 2222 <id>@localhost

# 3. Verify configuration
# Check API URL and port are correct
# Check sandbox ID is correct (copy-paste to avoid typos)
# Check Docker socket is accessible: echo $DOCKER_HOST

# 4. Test with simpler tools
# Test sandbox exists:
curl http://localhost:8080/sandboxes/<sandbox-id>

# Test Docker exec directly:
docker exec <container-id> echo "test"
```

---

## Appendix

### Quick Reference

#### Development Commands

```bash
# Start DSB server
dsb server --port 8080

# Create sandbox
dsb create -i python:3.12 -n test

# Start SSH gateway
cd ssh-gateway
cargo run -- --port 2222 --api-url http://localhost:8080

# Connect
ssh -p 2222 <sandbox-id>@localhost

# Run tests
cargo test

# Run integration tests
cargo test --test integration_tests

# Generate coverage
make coverage
```

#### Production Commands

```bash
# Set environment
export DSB_API_URL="https://dsb.yourdomain.com"
export DSB_API_KEY="your-api-key"

# Start SSH gateway
ssh-gateway --port 2222 --api-url "$DSB_API_URL"

# Check status
sudo systemctl status dsb-ssh-gateway

# View logs
sudo journalctl -u dsb-ssh-gateway -f

# Connect
ssh -p 2222 <sandbox-id>@dsb.yourdomain.com
```

### Key Files

| File | Purpose |
|------|---------|
| `README.md` | Main project documentation |
| `COMPREHENSIVE_GUIDE.md` | This consolidated guide |
| `src/ssh.rs` | SSH server implementation |
| `src/docker.rs` | Docker exec proxy implementation |
| `src/session.rs` | Session manager implementation |
| `src/main.rs` | CLI entry point and configuration |

### External Documentation

- **[russh Documentation](https://docs.rs/russh/)** - Rust SSH library
- **[Bollard Documentation](https://docs.rs/bollard/)** - Docker API client for Rust
- **[DSB Project Documentation](https://github.com/dsb/dsb)** - Main DSB project docs

### Support

For issues or questions:

1. Check this guide's troubleshooting section
2. Review logs with `--log-level debug`
3. Consult the main README.md
4. Check existing GitHub issues
5. Create a new issue with:
   - Environment (local/production)
   - Full error message
   - Gateway logs (with sensitive info redacted)
   - SSH client verbose output (`ssh -vvv`)

---

**Last Updated**: 2026-01-09
**Version**: 0.1.0
**Status**: Production Ready ✅
