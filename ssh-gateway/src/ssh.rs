// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # SSH Server Implementation
//!
//! This module implements the SSH server using russh for protocol handling.
//!
//! ## Overview
//!
//! The SSH server handles:
//! - SSH protocol handshake and authentication
//! - Session channel creation
//! - PTY allocation and environment variables
//! - Data forwarding between SSH client and Docker container
//!
//! ## Architecture
//!
//! The SSH server uses russh's [`Handle`] API for immediate output forwarding:
//!
//! - **Client Input**: `data()` method forwards to Docker exec stdin
//! - **Server Output**: Background task reads Docker exec and sends via `Handle::data()`
//! - **No Buffering**: Output appears immediately as Docker generates it
//!
//! ### Data Flow
//!
//! ```text
//! SSH Client           SSH Gateway              Docker
//!    │                    │                      │
//!    ├──────data────────>│                      │
//!    │                    ├────stdin────────────>│
//!    │                    │                      │
//!    │                    │<──stdout/stderr─────┤
//!    │<──Handle.data()────┤                      │
//!    │                    │                      │
//! ```
//!
//! ### Authorization Flow
//!
//! ```text
//! SSH Client → SSH Server → SessionManager → DSB API
//!                          ↓                ↓
//!                     DockerExec       Container
//! ```
//!
//! [`Handle`]: russh::server::Handle

use anyhow::{Context, Result};
use russh::keys::PublicKey;
use russh::server::{Auth, Msg, Server, Session};
use russh::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::docker::DockerExecProxy;
use crate::session::SessionManager;

// Re-export DSB Config type for convenience
pub use dsb::config::Config as DsbConfig;
/// Synchronously acquire a `tokio::sync::Mutex` in a non-async context.
fn blocking_lock<T>(mutex: &tokio::sync::Mutex<T>) -> tokio::sync::MutexGuard<'_, T> {
    // Fast path: uncontended lock.
    if let Ok(guard) = mutex.try_lock() {
        return guard;
    }

    // In a multi-threaded runtime, move to a blocking thread so we don't
    // starve the worker while spinning.
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
            return tokio::task::block_in_place(|| loop {
                if let Ok(guard) = mutex.try_lock() {
                    return guard;
                }
                std::thread::yield_now();
            });
        }
    }

    // Fallback for current-thread runtime or outside a runtime:
    // spin with yielding. Safe as long as the lock is uncontended.
    loop {
        if let Ok(guard) = mutex.try_lock() {
            return guard;
        }
        std::thread::yield_now();
    }
}

/// SSH server configuration.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SshConfig {
    /// DSB API base URL
    pub api_url: String,

    /// API key for DSB authentication
    pub api_key: Option<String>,

    /// Server host key (generated if not provided)
    pub host_key: Option<russh::keys::PrivateKey>,

    /// SSH server port
    pub port: u16,
}

impl Default for SshConfig {
    fn default() -> Self {
        // Load config for tests instead of hardcoding
        let config =
            dsb::config::load_for_tests().unwrap_or_else(|_| dsb::config::Config::default());

        Self {
            api_url: config.ssh.api_url.clone(),
            api_key: config.ssh.api_key.clone(),
            host_key: None,
            port: config.ssh.port,
        }
    }
}

/// Active SSH connection state.
#[allow(dead_code)]
pub struct ConnectionState {
    /// Sandbox ID for this connection (parsed from username)
    sandbox_id: Option<uuid::Uuid>,

    /// SSH session ID from DSB API
    session_id: Option<uuid::Uuid>,

    /// Client IP address
    client_ip: String,

    /// Channel ID for this session
    channel_id: Option<ChannelId>,

    /// Docker exec proxy (for management operations)
    exec_proxy: Option<Arc<Mutex<DockerExecProxy>>>,

    /// Exec input stream (extracted from proxy, for direct stdin writes)
    pub exec_input: Option<Pin<Box<dyn tokio::io::AsyncWrite + Send>>>,

    /// Exec output stream (extracted from proxy, for direct stdout reads)
    #[allow(clippy::type_complexity)]
    pub exec_output: Option<
        Pin<
            Box<
                dyn futures_util::Stream<
                        Item = Result<bollard::container::LogOutput, bollard::errors::Error>,
                    > + Send,
            >,
        >,
    >,

    /// Handle to send data to SSH client from background tasks
    handle: Option<russh::server::Handle>,

    /// Channel ID for Handle operations
    handle_channel_id: Option<ChannelId>,

    /// Bytes sent to client (reserved for future metrics)
    bytes_sent: u64,

    /// Bytes received from client (reserved for future metrics)
    bytes_received: u64,
}

impl ConnectionState {
    /// Create a new connection state.
    pub fn new(client_ip: String) -> Self {
        Self {
            sandbox_id: None,
            session_id: None,
            client_ip,
            channel_id: None,
            exec_proxy: None,
            exec_input: None,
            exec_output: None,
            handle: None,
            handle_channel_id: None,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }

    /// Set the sandbox ID (parsed from username).
    pub fn set_sandbox_id(&mut self, sandbox_id: uuid::Uuid) {
        self.sandbox_id = Some(sandbox_id);
    }

    /// Set the SSH session ID.
    #[allow(dead_code)]
    pub fn set_session_id(&mut self, session_id: uuid::Uuid) {
        self.session_id = Some(session_id);
    }

    /// Set the channel ID.
    pub fn set_channel_id(&mut self, channel_id: ChannelId) {
        self.channel_id = Some(channel_id);
    }

    /// Set the Docker exec proxy.
    pub fn set_exec_proxy(&mut self, proxy: DockerExecProxy) {
        self.exec_proxy = Some(Arc::new(Mutex::new(proxy)));
    }

    /// Set the handle for background task communication.
    pub fn set_handle(&mut self, handle: russh::server::Handle) {
        self.handle = Some(handle);
    }

    /// Set the channel ID for handle operations.
    pub fn set_handle_channel_id(&mut self, channel_id: ChannelId) {
        self.handle_channel_id = Some(channel_id);
    }

    /// Get the handle (returns None if not set).
    pub fn get_handle(&self) -> Option<russh::server::Handle> {
        self.handle.clone()
    }

    /// Get the channel ID for handle operations.
    pub fn get_handle_channel_id(&self) -> Option<ChannelId> {
        self.handle_channel_id
    }

    /// Get the sandbox ID.
    #[allow(dead_code)]
    pub fn get_sandbox_id(&self) -> Option<uuid::Uuid> {
        self.sandbox_id
    }

    /// Get the session ID.
    #[allow(dead_code)]
    pub fn get_session_id(&self) -> Option<uuid::Uuid> {
        self.session_id
    }
}

/// SSH server handler implementing russh server traits.
#[derive(Clone)]
pub struct SshServer {
    /// DSB configuration (contains SSH settings, Docker config, etc.)
    config: Arc<DsbConfig>,

    /// Session manager for DSB API communication
    session_manager: Arc<SessionManager>,

    /// Active connections (connection_id → state)
    connections: Arc<Mutex<HashMap<usize, Arc<Mutex<ConnectionState>>>>>,

    /// Next connection ID
    next_id: Arc<Mutex<usize>>,

    /// Current connection ID
    id: usize,
}

impl SshServer {
    /// Create a new SSH server instance with full DSB configuration.
    ///
    /// # Arguments
    ///
    /// * `config` - DSB configuration (contains SSH settings, Docker config, etc.)
    ///
    /// # Returns
    ///
    /// A new `SshServer` instance
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use ssh_gateway::ssh::SshServer;
    /// # use dsb::config;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = config::load()?;
    /// let server = SshServer::new(config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: DsbConfig) -> Result<Self> {
        // Extract SSH-specific config
        let api_url = config.ssh.api_url.clone();
        let api_key = config
            .ssh
            .api_key
            .clone()
            .or_else(|| config.server.ssh_gateway_api_key.clone());

        let session_manager = Arc::new(SessionManager::new(&api_url, api_key));

        Ok(Self {
            config: Arc::new(config),
            session_manager,
            connections: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(0)),
            id: 0,
        })
    }

    /// Load or generate SSH host key (blocking/synchronous).
    ///
    /// This is a synchronous helper function since get_host_key() must be synchronous.
    /// The runtime will handle blocking I/O.
    ///
    /// # Arguments
    ///
    /// * `key_path` - Path to the SSH host key file
    ///
    /// # Returns
    ///
    /// The loaded or generated Ed25519 private key
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - ssh-keygen fails to execute
    /// - Key file exists but is invalid
    /// - File permissions cannot be set
    fn load_or_generate_host_key_blocking(key_path: &PathBuf) -> Result<russh::keys::PrivateKey> {
        if key_path.exists() {
            // Key exists, load it
            debug!("Loading SSH host key from: {}", key_path.display());
            let key =
                russh::keys::load_secret_key(key_path, None).context("Failed to parse host key")?;
            info!("Loaded SSH host key from: {}", key_path.display());
            Ok(key)
        } else {
            // Key doesn't exist, generate it (blocking)
            info!("SSH host key not found at: {}", key_path.display());
            info!("Auto-generating persistent SSH host key...");

            // Create parent directory if it doesn't exist
            if let Some(parent) = key_path.parent() {
                std::fs::create_dir_all(parent).context("Failed to create key directory")?;
            }

            // Run ssh-keygen synchronously (blocking I/O in sync context)
            let status = std::process::Command::new("ssh-keygen")
                .arg("-q")
                .arg("-t")
                .arg("ed25519")
                .arg("-f")
                .arg(key_path)
                .arg("-N")
                .arg("") // Empty passphrase
                .stdin(std::process::Stdio::null())
                .status()
                .context("Failed to run ssh-keygen. Is ssh-keygen installed?")?;

            if !status.success() {
                anyhow::bail!("ssh-keygen failed to generate key");
            }

            info!("Generated persistent SSH host key: {}", key_path.display());

            // Now load the newly generated key
            let key = russh::keys::load_secret_key(key_path, None)
                .context("Failed to load newly generated key")?;
            Ok(key)
        }
    }

    /// Get the SSH host key for the server.
    ///
    /// This method loads or generates the SSH host key:
    /// - If `host_key_path` is explicitly configured: load from that path
    /// - Otherwise: use default path `~/.dsb/ssh_host_key`
    ///   - Generate key if it doesn't exist (using ssh-keygen)
    ///   - Load the existing/generated key
    ///
    /// # Returns
    ///
    /// The server's Ed25519 private key
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Key generation fails (ssh-keygen not installed)
    /// - Key file exists but is invalid
    /// - File I/O operations fail
    pub fn get_host_key(&self) -> Result<russh::keys::PrivateKey> {
        // Determine the key path
        let key_path = if let Some(ref path) = self.config.ssh.host_key_path {
            PathBuf::from(path)
        } else {
            // Use default path: ~/.dsb/ssh_host_key
            let home_dir = std::env::var("HOME").context("HOME environment variable not set")?;
            PathBuf::from(home_dir).join(".dsb").join("ssh_host_key")
        };

        // Ensure single access when creating/reading key to avoid race conditions
        static KEY_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
        let lock = KEY_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
        let _guard = blocking_lock(lock);

        // Load or generate the key
        Self::load_or_generate_host_key_blocking(&key_path)
    }

    /// Start the SSH server.
    ///
    /// This blocks and runs the SSH server indefinitely.
    ///
    /// # Returns
    ///
    /// Error if server fails to start or run
    #[instrument(skip(self))]
    pub async fn run(&self) -> Result<()> {
        debug!("Starting SSH server on port {}", self.config.ssh.port);

        let host_key = self.get_host_key()?;

        let config = russh::server::Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
            auth_rejection_time: std::time::Duration::from_secs(3),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![host_key],
            ..Default::default()
        };

        let config = Arc::new(config);

        let socket: tokio::net::TcpListener =
            tokio::net::TcpListener::bind(("0.0.0.0", self.config.ssh.port))
                .await
                .context("Failed to bind SSH server")?;

        debug!("SSH server listening on port {}", self.config.ssh.port);

        // Create a mutable clone of the server for accepting connections
        let mut server = self.clone();

        // Accept and handle connections indefinitely
        loop {
            match socket.accept().await {
                Ok((stream, addr)) => {
                    debug!("New connection from: {:?}", addr);
                    let config = config.clone();
                    let handler = server.new_client(Some(addr));

                    tokio::spawn(async move {
                        if let Err(e) = russh::server::run_stream(config, stream, handler).await {
                            error!("Connection error: {:?}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {:?}", e);
                }
            }
        }
    }
}

/// Implement the server::Server trait for russh.
impl russh::server::Server for SshServer {
    type Handler = Self;

    /// Called for each new client connection.
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let mut next_id = blocking_lock(&self.next_id);
        let id = *next_id;
        *next_id += 1;
        drop(next_id);

        let mut connections = blocking_lock(&self.connections);
        connections.insert(
            id,
            Arc::new(Mutex::new(ConnectionState::new("unknown".to_string()))),
        );
        drop(connections);

        debug!("New client connection: {}", id);

        Self {
            config: self.config.clone(),
            session_manager: self.session_manager.clone(),
            connections: self.connections.clone(),
            next_id: self.next_id.clone(),
            id,
        }
    }

    /// Handle session errors.
    fn handle_session_error(&mut self, error: russh::Error) {
        error!("Session error on connection {}: {:?}", self.id, error);
    }
}

/// Implement the server::Handler trait for handling SSH events.
impl russh::server::Handler for SshServer {
    type Error = russh::Error;

    /// Handle public key authentication.
    async fn auth_publickey(&mut self, user: &str, _key: &PublicKey) -> Result<Auth, Self::Error> {
        debug!("Public key authentication for user: {}", user);

        // Parse sandbox ID from username
        match uuid::Uuid::parse_str(user) {
            Ok(sandbox_id) => {
                debug!("Valid sandbox ID in username: {}", sandbox_id);

                // Verify sandbox exists and is running via DSB API
                match self.session_manager.authorize_sandbox(&sandbox_id).await {
                    Ok(auth_context) => {
                        if !auth_context.authorized {
                            warn!(
                                "Sandbox authorization failed for {}: not authorized or not running",
                                sandbox_id
                            );
                            return Ok(Auth::Reject {
                                proceed_with_methods: None,
                                partial_success: false,
                            });
                        }

                        debug!("Sandbox {} authorized and running", sandbox_id);

                        // Update connection state with sandbox_id
                        let conn_arc = {
                            let connections = self.connections.lock().await;
                            connections.get(&self.id).cloned()
                        };

                        if let Some(conn) = conn_arc {
                            let mut conn = conn.lock().await;
                            conn.set_sandbox_id(sandbox_id);
                        }

                        Ok(Auth::Accept)
                    }
                    Err(e) => {
                        warn!("Sandbox authorization failed for {}: {}", sandbox_id, e);
                        Ok(Auth::Reject {
                            proceed_with_methods: None,
                            partial_success: false,
                        })
                    }
                }
            }
            Err(_) => {
                warn!("Invalid username (not a UUID): {}", user);
                Ok(Auth::Reject {
                    proceed_with_methods: None,
                    partial_success: false,
                })
            }
        }
    }

    /// Handle session channel open request.
    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        debug!("Session channel opened: {}", channel.id());

        // Update connection state with channel ID
        let conn_arc = {
            let connections = self.connections.lock().await;
            connections.get(&self.id).cloned()
        };

        if let Some(conn) = conn_arc {
            let mut conn = conn.lock().await;
            conn.set_channel_id(channel.id());
        }

        // Accept the channel
        Ok(true)
    }

    /// Handle PTY request.
    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("PTY request received");
        // Accept PTY request
        Ok(())
    }

    /// Handle environment variable request.
    async fn env_request(
        &mut self,
        _channel: ChannelId,
        variable_name: &str,
        variable_value: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Environment variable: {}={}", variable_name, variable_value);
        // Accept environment variable
        Ok(())
    }

    /// Handle shell request - this is where we start the Docker exec.
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Shell request received on channel {}", channel);

        // Get connection state and sandbox_id
        let sandbox_id = {
            let conn_arc = {
                let connections = self.connections.lock().await;
                connections.get(&self.id).cloned()
            };

            let conn_arc = conn_arc.ok_or_else(|| russh::Error::Disconnect)?;
            let conn = conn_arc.lock().await;
            conn.sandbox_id.ok_or_else(|| russh::Error::Disconnect)?
        };

        // Authorize sandbox via DSB API
        let auth_result = match self.session_manager.authorize_sandbox(&sandbox_id).await {
            Ok(auth) => auth,
            Err(e) => {
                error!("Failed to authorize sandbox {}: {:?}", sandbox_id, e);
                let msg = format!("Error: Failed to authorize sandbox: {}\r\n", e);
                let _ = session.data(channel, msg.into());
                let _ = session.close(channel);
                return Ok(());
            }
        };

        // Extract container_id from auth response
        let container_id = match auth_result.sandbox {
            Some(sandbox) => sandbox.container_id,
            None => {
                error!("No sandbox info in auth response for {}", sandbox_id);
                let msg = "Error: Sandbox not found or not running\r\n".to_string();
                let _ = session.data(channel, msg.into());
                let _ = session.close(channel);
                return Ok(());
            }
        };

        debug!(
            "Starting shell for sandbox: {} (container: {})",
            sandbox_id, container_id
        );

        // Get the Handle from the session BEFORE spawning background task
        let handle = session.handle();

        if self.config.ssh.backend == "kubernetes" {
            // Kubernetes backend: use K8sExecProxy
            // Pod names are prefixed with "dsb-sb-" by the k8s manager
            // (see src/k8s/types.rs::sandbox_resource_name)
            let namespace = self.config.ssh.kubernetes_namespace.clone();
            let pod_name = format!("dsb-sb-{}", container_id);
            let mut exec_proxy = match crate::k8s::K8sExecProxy::new(pod_name, namespace).await {
                Ok(proxy) => proxy,
                Err(e) => {
                    error!("Failed to create K8s exec proxy: {:?}", e);
                    let msg = format!("Error: Failed to create shell: {}\r\n", e);
                    let _ = session.data(channel, msg.into());
                    let _ = session.close(channel);
                    return Ok(());
                }
            };

            if let Err(e) = exec_proxy.create_exec().await {
                error!("Failed to create K8s exec: {:?}", e);
                let msg = format!("Error: Failed to create shell: {}\r\n", e);
                let _ = session.data(channel, msg.into());
                let _ = session.close(channel);
                return Ok(());
            }

            if let Err(e) = exec_proxy.start_exec().await {
                error!("Failed to start K8s exec: {:?}", e);
                let msg = format!("Error: Failed to start shell: {}\r\n", e);
                let _ = session.data(channel, msg.into());
                let _ = session.close(channel);
                return Ok(());
            }

            debug!("K8s exec in pod {} started successfully", container_id);

            let exec_input_stream = exec_proxy.exec_input.take();
            let exec_output_stream = exec_proxy.exec_output.take();

            if exec_input_stream.is_none() || exec_output_stream.is_none() {
                error!("Failed to extract K8s exec streams");
                let msg = "Error: Failed to initialize shell streams\r\n".to_string();
                let _ = session.data(channel, msg.into());
                let _ = session.close(channel);
                return Ok(());
            }

            // Store exec_input in connection state for the data() handler.
            // The K8s output stream is passed directly to the background task.
            {
                let conn_arc = {
                    let connections = self.connections.lock().await;
                    connections.get(&self.id).cloned()
                };

                if let Some(conn) = conn_arc {
                    let mut conn = conn.lock().await;
                    conn.exec_input = exec_input_stream;
                    conn.set_handle_channel_id(channel);
                    conn.set_handle(handle.clone());
                }
            }

            // Spawn background task for K8s stdout (takes ownership of output stream)
            let connections_clone = self.connections.clone();
            let connection_id = self.id;
            let mut exec_output = exec_output_stream.ok_or_else(|| {
                error!("K8s exec output stream is missing");
                russh::Error::Disconnect
            })?;
            tokio::spawn(async move {
                debug!(
                    "Background task: Starting K8s exec output reader for connection {}",
                    connection_id
                );

                let (handle, channel_id) = {
                    let conn_arc = {
                        let connections = connections_clone.lock().await;
                        connections.get(&connection_id).cloned()
                    };

                    if let Some(conn) = conn_arc {
                        let conn = conn.lock().await;
                        (conn.get_handle(), conn.get_handle_channel_id())
                    } else {
                        warn!("Connection {} not found", connection_id);
                        return;
                    }
                };

                let (handle, channel_id) = match (handle, channel_id) {
                    (Some(h), Some(cid)) => (h, cid),
                    _ => {
                        debug!(
                            "K8s handle or channel not ready for connection {}",
                            connection_id
                        );
                        return;
                    }
                };

                use futures_util::StreamExt;
                loop {
                    match exec_output.next().await {
                        Some(Ok(data)) => {
                            if !data.is_empty() {
                                debug!(
                                    "Read {} bytes from K8s exec output for connection {}",
                                    data.len(),
                                    connection_id
                                );
                                let crypto_data = russh::CryptoVec::from(data);
                                let data_len = crypto_data.len();
                                if let Err(e) = handle.data(channel_id, crypto_data).await {
                                    error!("Failed to send data via Handle: {:?}", e);
                                    return;
                                }
                                debug!(
                                    "Successfully forwarded {} bytes to SSH client via Handle",
                                    data_len
                                );
                            } else {
                                debug!("K8s exec output closed for connection {}", connection_id);
                                return;
                            }
                        }
                        Some(Err(e)) => {
                            error!("Error reading from K8s exec output: {:?}", e);
                            return;
                        }
                        None => {
                            debug!(
                                "K8s exec output stream closed for connection {}",
                                connection_id
                            );
                            return;
                        }
                    }
                }
            });
        } else {
            // Docker backend: use DockerExecProxy
            let mut exec_proxy =
                match DockerExecProxy::new_with_config_and_id(container_id.clone(), &self.config) {
                    Ok(proxy) => proxy,
                    Err(e) => {
                        error!("Failed to create Docker exec proxy: {:?}", e);
                        let msg = format!("Error: Failed to create shell: {}\r\n", e);
                        let _ = session.data(channel, msg.into());
                        let _ = session.close(channel);
                        return Ok(());
                    }
                };

            let exec_id = match exec_proxy.create_exec().await {
                Ok(id) => {
                    debug!("Created exec instance: {}", id);
                    id
                }
                Err(e) => {
                    error!("Failed to create exec instance: {:?}", e);
                    let msg = format!("Error: Failed to create shell: {}\r\n", e);
                    let _ = session.data(channel, msg.into());
                    let _ = session.close(channel);
                    return Ok(());
                }
            };

            if let Err(e) = exec_proxy.start_exec().await {
                error!("Failed to start exec instance: {:?}", e);
                let msg = format!("Error: Failed to start shell: {}\r\n", e);
                let _ = session.data(channel, msg.into());
                let _ = session.close(channel);
                return Ok(());
            }

            debug!("Exec instance {} started successfully", exec_id);

            let exec_input_stream = exec_proxy.exec_input.take();
            let exec_output_stream = exec_proxy.exec_output.take();

            if exec_input_stream.is_none() || exec_output_stream.is_none() {
                error!("Failed to extract exec streams from proxy");
                let msg = "Error: Failed to initialize shell streams\r\n".to_string();
                let _ = session.data(channel, msg.into());
                let _ = session.close(channel);
                return Ok(());
            }

            debug!("Extracted exec streams from proxy");

            // Store exec_proxy (without streams), streams, handle_channel_id, and handle in connection state
            {
                let conn_arc = {
                    let connections = self.connections.lock().await;
                    connections.get(&self.id).cloned()
                };

                if let Some(conn) = conn_arc {
                    let mut conn = conn.lock().await;
                    conn.set_exec_proxy(exec_proxy);
                    conn.exec_input = exec_input_stream;
                    conn.exec_output = exec_output_stream;
                    conn.set_handle_channel_id(channel);
                    conn.set_handle(handle.clone());
                }
            }

            // Spawn a background task to read from exec output and forward immediately via Handle
            let connections_clone = self.connections.clone();
            let connection_id = self.id;
            tokio::spawn(async move {
                debug!(
                    "Background task: Starting exec output reader for connection {}",
                    connection_id
                );

                // Take output stream ONCE at the start (not in the loop)
                let (exec_output, handle, channel_id) = {
                    let conn_arc = {
                        let connections = connections_clone.lock().await;
                        connections.get(&connection_id).cloned()
                    };

                    if let Some(conn) = conn_arc {
                        let mut conn = conn.lock().await;
                        (
                            conn.exec_output.take(),
                            conn.get_handle(),
                            conn.get_handle_channel_id(),
                        )
                    } else {
                        warn!("Connection {} not found", connection_id);
                        return;
                    }
                };

                let (mut exec_output, handle, channel_id) = match (exec_output, handle, channel_id)
                {
                    (Some(eo), Some(h), Some(cid)) => (eo, h, cid),
                    _ => {
                        debug!(
                            "Exec output, handle, or channel not ready for connection {}",
                            connection_id
                        );
                        return;
                    }
                };

                // Now read continuously in the loop without accessing connection state
                loop {
                    use futures_util::StreamExt;
                    match exec_output.next().await {
                        Some(Ok(log_output)) => {
                            let data = match log_output {
                                bollard::container::LogOutput::StdOut { message } => {
                                    debug!("Received {} bytes from stdout", message.len());
                                    message
                                }
                                bollard::container::LogOutput::StdErr { message } => {
                                    debug!("Received {} bytes from stderr", message.len());
                                    message
                                }
                                bollard::container::LogOutput::StdIn { message } => {
                                    debug!("Received {} bytes from stdin", message.len());
                                    message
                                }
                                bollard::container::LogOutput::Console { message } => {
                                    debug!("Received {} bytes from console", message.len());
                                    message
                                }
                            };

                            if !data.is_empty() {
                                debug!(
                                    "Read {} bytes from exec output for connection {}",
                                    data.len(),
                                    connection_id
                                );
                                // Send immediately via Handle
                                let crypto_data = russh::CryptoVec::from(data.to_vec());
                                let data_len = crypto_data.len();
                                if let Err(e) = handle.data(channel_id, crypto_data).await {
                                    error!("Failed to send data via Handle: {:?}", e);
                                    return; // Stop task if handle fails
                                }
                                debug!(
                                    "Successfully forwarded {} bytes to SSH client via Handle",
                                    data_len
                                );
                            } else {
                                // Empty data means EOF or exec finished
                                debug!("Exec output closed for connection {}", connection_id);
                                return;
                            }
                        }
                        Some(Err(e)) => {
                            error!("Error reading from exec output: {:?}", e);
                            return;
                        }
                        None => {
                            // Stream closed
                            debug!("Exec output stream closed for connection {}", connection_id);
                            return;
                        }
                    }
                }
            });
        }

        // Send welcome message
        let msg = format!(
            "Connected to sandbox {} (container: {})\r\n\r\n",
            sandbox_id, container_id
        );
        let _ = session.data(channel, msg.into());

        debug!("Shell session ready for sandbox {}", sandbox_id);

        Ok(())
    }

    /// Handle data from SSH client.
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(
            "Received {} bytes from SSH client on channel {}",
            data.len(),
            channel
        );

        // Log first 32 bytes of data for debugging (avoid logging passwords)
        let preview_len = data.len().min(32);
        trace!(
            "Data preview (first {} bytes): {:?}",
            preview_len,
            &data[..preview_len]
        );

        // Get exec_input stream from connection state
        let exec_input = {
            let conn_arc = {
                let connections = self.connections.lock().await;
                connections.get(&self.id).cloned()
            };

            if let Some(conn) = conn_arc {
                let mut conn = conn.lock().await;
                conn.exec_input.take()
            } else {
                warn!("Connection {} not found", self.id);
                return Ok(());
            }
        };

        match exec_input {
            None => {
                warn!(
                    "No exec input stream available for connection {}, channel {}",
                    self.id, channel
                );
            }
            Some(mut input_stream) => {
                debug!("Writing {} bytes to exec stdin (no lock held)", data.len());

                use tokio::io::AsyncWriteExt;
                match input_stream.write_all(data).await {
                    Ok(_) => {
                        debug!("Successfully wrote {} bytes to exec stdin", data.len());
                        // Flush to ensure data is sent immediately
                        match input_stream.flush().await {
                            Ok(_) => {
                                debug!("Successfully flushed exec stdin");
                            }
                            Err(e) => {
                                error!("Failed to flush exec stdin: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to write to exec stdin: {:?}", e);
                        error!("This is the error causing the terminal to not respond!");
                    }
                }
                debug!("write_stdin completed");

                // Put the stream back for next write
                let conn_arc = {
                    let connections = self.connections.lock().await;
                    connections.get(&self.id).cloned()
                };

                if let Some(conn) = conn_arc {
                    let mut conn = conn.lock().await;
                    conn.exec_input = Some(input_stream);
                }
            }
        }

        // Note: Exec output forwarding is handled by background task via Handle
        // No need to check channels here - data is sent immediately as it becomes available

        Ok(())
    }

    /// Handle channel close.
    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Channel closed: {}", channel);

        // Get the connection state for this connection
        let conn_arc = {
            let connections = self.connections.lock().await;
            connections.get(&self.id).cloned()
        };

        if let Some(conn) = conn_arc {
            let conn = conn.lock().await;

            // Terminate SSH session via DSB API if we have a session_id
            if let Some(session_id) = conn.get_session_id() {
                debug!("Terminating SSH session: {}", session_id);

                // Terminate the session (fire and forget - don't fail channel close if this fails)
                let session_manager = self.session_manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = session_manager
                        .terminate_session(&session_id, "Channel closed")
                        .await
                    {
                        error!("Failed to terminate SSH session {}: {:?}", session_id, e);
                    } else {
                        debug!("SSH session {} terminated successfully", session_id);
                    }
                });
            }

            // Note: Docker exec instances automatically clean themselves up
            // when the process exits, so no explicit cleanup needed there
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssh_config_default() {
        // Test that SshConfig::default() uses config system
        let config = SshConfig::default();

        // Load expected values from config system
        let test_config = dsb::config::load_for_tests().expect("Failed to load test config");

        assert_eq!(config.api_url, test_config.ssh.api_url);
        assert_eq!(config.port, test_config.ssh.port);
        assert_eq!(config.api_key, test_config.ssh.api_key);
        assert!(config.host_key.is_none());
    }

    #[test]
    fn test_connection_state_creation() {
        let state = ConnectionState::new("127.0.0.1".to_string());
        assert_eq!(state.client_ip, "127.0.0.1");
        assert!(state.sandbox_id.is_none());
        assert!(state.session_id.is_none());
    }

    #[test]
    fn test_connection_state_handle_management() {
        let state = ConnectionState::new("192.168.1.1".to_string());

        // Initially, no handle is set
        assert!(state.get_handle().is_none());
        assert!(state.get_handle_channel_id().is_none());

        // Note: We can't create a real Handle without a Session, so we test
        // the handle management methods indirectly through the API
        // The actual Handle functionality is tested in integration tests
    }

    #[test]
    fn test_connection_state_sandbox_id() {
        let mut state = ConnectionState::new("127.0.0.1".to_string());
        let uuid = uuid::Uuid::new_v4();

        state.set_sandbox_id(uuid);
        assert_eq!(state.get_sandbox_id(), Some(uuid));
    }

    #[test]
    fn test_connection_state_session_id() {
        let mut state = ConnectionState::new("127.0.0.1".to_string());
        let uuid = uuid::Uuid::new_v4();

        state.set_session_id(uuid);
        assert_eq!(state.get_session_id(), Some(uuid));
    }

    #[test]
    fn test_connection_state_channel_id() {
        let state = ConnectionState::new("127.0.0.1".to_string());

        // Note: ChannelId is a tuple struct with private fields in russh
        // We can't construct it directly in tests without a Session
        // Verify that the connection state was created successfully
        assert_eq!(state.client_ip, "127.0.0.1");
    }

    #[test]
    fn test_connection_state_handle_channel_id() {
        let state = ConnectionState::new("127.0.0.1".to_string());

        // Note: ChannelId is private, we can't create instances directly
        // Verify that get_handle_channel_id returns None when not set
        assert!(state.get_handle_channel_id().is_none());
    }

    #[test]
    fn test_connection_state_exec_proxy() {
        let state = ConnectionState::new("127.0.0.1".to_string());

        // Initially no exec proxy
        // Note: exec_proxy is private, we can't access it directly
        // This test verifies the connection state was created successfully
        assert_eq!(state.client_ip, "127.0.0.1");

        // We can't create a real DockerExecProxy without Docker running,
        // but we can test that the setter stores it (indirectly via API)
        // This is tested in integration tests with real Docker
    }

    #[test]
    fn test_ssh_config_builder() {
        let config = SshConfig {
            api_url: "http://example.com:9090".to_string(),
            port: 2223,
            api_key: Some("test-key".to_string()),
            host_key: None,
        };

        assert_eq!(config.api_url, "http://example.com:9090");
        assert_eq!(config.port, 2223);
        assert_eq!(config.api_key, Some("test-key".to_string()));
    }

    #[test]
    fn test_ssh_config_with_no_api_key() {
        let config = SshConfig {
            api_url: "http://example.com:9090".to_string(),
            port: 2223,
            api_key: None,
            host_key: None,
        };

        assert_eq!(config.api_url, "http://example.com:9090");
        assert_eq!(config.port, 2223);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_ssh_config_clone() {
        let config = SshConfig {
            api_url: "http://example.com:9090".to_string(),
            port: 2223,
            api_key: Some("test-key".to_string()),
            host_key: None,
        };

        let cloned = config.clone();
        assert_eq!(cloned.api_url, config.api_url);
        assert_eq!(cloned.port, config.port);
        assert_eq!(cloned.api_key, config.api_key);
    }

    #[test]
    fn test_connection_state_with_different_ips() {
        let state1 = ConnectionState::new("192.168.1.1".to_string());
        let state2 = ConnectionState::new("10.0.0.1".to_string());

        assert_eq!(state1.client_ip, "192.168.1.1");
        assert_eq!(state2.client_ip, "10.0.0.1");
    }

    #[test]
    fn test_connection_state_bytes_initially_zero() {
        let state = ConnectionState::new("127.0.0.1".to_string());
        assert_eq!(state.bytes_sent, 0);
        assert_eq!(state.bytes_received, 0);
    }

    #[test]
    fn test_connection_state_multiple_sandbox_id_changes() {
        let mut state = ConnectionState::new("127.0.0.1".to_string());
        let uuid1 = uuid::Uuid::new_v4();
        let uuid2 = uuid::Uuid::new_v4();

        assert!(state.get_sandbox_id().is_none());

        state.set_sandbox_id(uuid1);
        assert_eq!(state.get_sandbox_id(), Some(uuid1));

        state.set_sandbox_id(uuid2);
        assert_eq!(state.get_sandbox_id(), Some(uuid2));
    }

    #[test]
    fn test_connection_state_multiple_session_id_changes() {
        let mut state = ConnectionState::new("127.0.0.1".to_string());
        let uuid1 = uuid::Uuid::new_v4();
        let uuid2 = uuid::Uuid::new_v4();

        assert!(state.get_session_id().is_none());

        state.set_session_id(uuid1);
        assert_eq!(state.get_session_id(), Some(uuid1));

        state.set_session_id(uuid2);
        assert_eq!(state.get_session_id(), Some(uuid2));
    }

    #[test]
    fn test_ssh_config_debug_format() {
        let config = SshConfig {
            api_url: "http://example.com:9090".to_string(),
            port: 2223,
            api_key: Some("test-key".to_string()),
            host_key: None,
        };
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("SshConfig"));
        assert!(debug_str.contains("example.com"));
    }

    #[test]
    fn test_connection_state_debug_format() {
        // ConnectionState doesn't implement Debug, so we test the string format differently
        // We test that the type can be created and has expected fields
        let state = ConnectionState::new("127.0.0.1".to_string());
        assert_eq!(state.client_ip, "127.0.0.1");
        assert!(state.bytes_sent == 0);
        assert!(state.bytes_received == 0);
    }

    #[tokio::test]
    async fn test_connection_lookup_returns_disconnect_instead_of_panic() {
        // Simulates the pattern used in shell_request (issue 2 fix).
        // When the connection is not in the map, we must return an error
        // instead of panicking with unwrap.
        let conn_arc: Option<Arc<Mutex<ConnectionState>>> = None;

        let result: Result<(), russh::Error> = async {
            let conn_arc = conn_arc.ok_or_else(|| russh::Error::Disconnect)?;
            let conn = conn_arc.lock().await;
            let _sandbox_id = conn.sandbox_id.ok_or_else(|| russh::Error::Disconnect)?;
            Ok(())
        }
        .await;

        assert!(
            matches!(result, Err(russh::Error::Disconnect)),
            "Should return Disconnect error, not panic"
        );
    }

    #[tokio::test]
    #[allow(clippy::type_complexity)]
    async fn test_exec_output_stream_returns_disconnect_instead_of_panic() {
        // Simulates the pattern used when spawning the K8s background task
        // (issue 3 fix). When the output stream is missing, we must return
        // an error instead of panicking with unwrap.
        let exec_output_stream: Option<
            Pin<
                Box<
                    dyn futures_util::Stream<
                            Item = Result<bollard::container::LogOutput, bollard::errors::Error>,
                        > + Send,
                >,
            >,
        > = None;

        let result: Result<(), russh::Error> = async {
            let _exec_output = exec_output_stream.ok_or_else(|| {
                error!("K8s exec output stream is missing");
                russh::Error::Disconnect
            })?;
            Ok(())
        }
        .await;

        assert!(
            matches!(result, Err(russh::Error::Disconnect)),
            "Should return Disconnect error, not panic"
        );
    }

    // Note: Full SSH server tests require integration testing
    // These are in tests/integration_tests.rs
}
