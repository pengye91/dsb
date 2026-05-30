// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
use crate::cli::commands::runner::CliContext;
use crate::cli::commands::types::Commands;

pub(crate) async fn run(
    ctx: &CliContext,
    cmd: Commands,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client = &ctx.client;
    let base_url = ctx.base_url.clone();
    let api_key = ctx.api_key.clone();
    let config = &ctx.config;
    match cmd {
        Commands::Create {
            image,
            name,
            pull,
            ports,
            cpu_shares,
            memory_mb,
            volumes,
            command,
            timeout,
            env,
            features,
            enable_all_features,
        } => {
            use indicatif::{ProgressBar, ProgressStyle};

            println!("Creating sandbox with image: {}", image);

            // Build request body
            let port_mappings: Vec<serde_json::Value> = ports
                .into_iter()
                .map(|(h, c)| {
                    serde_json::json!({
                        "host_port": h,
                        "container_port": c,
                        "protocol": "tcp"
                    })
                })
                .collect();

            let volumes_json: Vec<serde_json::Value> = volumes
                .into_iter()
                .map(|v| serde_json::to_value(v).unwrap())
                .collect();

            let mut body = serde_json::json!({
                "image": image,
            });

            if let Some(policy) = pull {
                body["pull_policy"] = serde_json::to_value(policy).unwrap();
            }

            if !port_mappings.is_empty() {
                body["port_mappings"] = serde_json::Value::Array(port_mappings);
            }

            if !volumes_json.is_empty() {
                body["volumes"] = serde_json::Value::Array(volumes_json);
            }

            if let Some(n) = name {
                body["name"] = serde_json::Value::String(n);
            }

            if let Some(cmd) = command {
                let parsed_cmd = crate::cli::utils::parse_command_args(cmd);
                body["command"] = serde_json::Value::Array(
                    parsed_cmd
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                );
            }

            let mut resource_limits = serde_json::Map::new();
            if let Some(cpu) = cpu_shares {
                resource_limits.insert(
                    "cpu_shares".to_string(),
                    serde_json::Value::Number(cpu.into()),
                );
            }
            if let Some(mem) = memory_mb {
                resource_limits.insert(
                    "memory_mb".to_string(),
                    serde_json::Value::Number(mem.into()),
                );
            }
            if !resource_limits.is_empty() {
                body["resource_limits"] = serde_json::Value::Object(resource_limits);
            }

            if let Some(t) = timeout {
                body["inactivity_timeout_minutes"] = serde_json::Value::Number(t.into());
            }

            if !env.is_empty() {
                let env_map: std::collections::HashMap<String, String> = env.into_iter().collect();
                body["environment"] = serde_json::to_value(env_map).unwrap();
            }

            if !features.is_empty() {
                body["features"] = serde_json::Value::Array(
                    features
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                );
            }

            if enable_all_features {
                body["enable_all_features"] = serde_json::Value::Bool(true);
            }

            // Create progress bar
            let pb = ProgressBar::new(100);
            pb.set_style(ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>7}% {msg:.cyan}")?
                .progress_chars("##-")
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈⠈⠈⠈⠉⠉⠉⠉⠉⠉ "));

            pb.set_message("Initializing...");

            let mut request = client.post(format!("{}/sandboxes/create-stream", base_url));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }

            match request.json(&body).send().await {
                Ok(mut response) => {
                    if response.status().is_success() {
                        let mut pb2: Option<ProgressBar> = None;
                        let mut pb3: Option<ProgressBar> = None;

                        'stream: loop {
                            match response.chunk().await {
                                Ok(Some(chunk)) => {
                                    let data = String::from_utf8_lossy(&chunk);
                                    for line in data.lines() {
                                        if line.starts_with("data:") {
                                            let json_str = line.trim_start_matches("data:").trim();
                                            if let Ok(event) =
                                                serde_json::from_str::<serde_json::Value>(json_str)
                                            {
                                                let event_type =
                                                    event.get("type").and_then(|t| t.as_str());

                                                match event_type {
                                                    Some("pulling") => {
                                                        let status = event
                                                            .get("status")
                                                            .and_then(|s| s.as_str())
                                                            .unwrap_or("Pulling...");
                                                        let current = event
                                                            .get("current")
                                                            .and_then(|v| v.as_u64());
                                                        let total = event
                                                            .get("total")
                                                            .and_then(|v| v.as_u64());

                                                        if let (Some(c), Some(t)) = (current, total)
                                                        {
                                                            if t > 0 {
                                                                pb.set_length(t);
                                                                pb.set_position(c);
                                                                let pct =
                                                                    (c as f64 / t as f64) * 100.0;
                                                                pb.set_message(format!(
                                                                    "{}: {:.0}%",
                                                                    status, pct
                                                                ));
                                                            }
                                                        } else if current.unwrap_or(0) == 0
                                                            && total.unwrap_or(0) == 0
                                                        {
                                                            // Image exists locally or already pulled
                                                            pb.set_length(100);
                                                            pb.set_position(100);
                                                            pb.set_message(status.to_string());
                                                        } else {
                                                            pb.set_message(format!(
                                                                "{}...",
                                                                status
                                                            ));
                                                        }
                                                        pb.tick();
                                                    }
                                                    Some("creating") => {
                                                        pb.finish_with_message("Image pulled");
                                                        println!();
                                                        let p = ProgressBar::new(100);
                                                        p.set_style(ProgressStyle::default_bar()
                                                            .template("{spinner:.green} [{elapsed_precise}] {msg:.cyan}")?
                                                            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈⠈⠈ "));
                                                        p.set_message("Creating container...");
                                                        p.tick();
                                                        pb2 = Some(p);
                                                    }
                                                    Some("starting") => {
                                                        if let Some(p) = pb2.take() {
                                                            p.finish_with_message(
                                                                "Container created",
                                                            );
                                                        }
                                                        println!();
                                                        let p = ProgressBar::new(100);
                                                        p.set_style(ProgressStyle::default_bar()
                                                            .template("{spinner:.green} [{elapsed_precise}] {msg:.cyan}")?
                                                            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈⠈⠈ "));
                                                        p.set_message("Starting container...");
                                                        p.tick();
                                                        pb3 = Some(p);
                                                    }
                                                    Some("ready") => {
                                                        let sandbox_id = event
                                                            .get("sandbox_id")
                                                            .and_then(|v| v.as_str())
                                                            .unwrap_or("unknown");

                                                        if let Some(p) = pb3.take() {
                                                            p.finish_with_message(
                                                                "Container started",
                                                            );
                                                        }

                                                        // Fetch full sandbox details
                                                        let mut get_request = client.get(format!(
                                                            "{}/sandboxes/{}",
                                                            base_url, sandbox_id
                                                        ));
                                                        if let Some(key) = &api_key {
                                                            get_request = get_request
                                                                .header("X-API-Key", key);
                                                        }

                                                        match get_request.send().await {
                                                            Ok(response) => {
                                                                if response.status().is_success() {
                                                                    if let Ok(sandbox) = response
                                                                        .json::<crate::core::types::SandboxResponse>()
                                                                        .await
                                                                    {
                                                                        crate::cli::display::print_sandbox_details(&sandbox);
                                                                    } else {
                                                                        // Fallback to basic display if JSON parsing fails
                                                                        println!();
                                                                        println!("✓ Sandbox ready!");
                                                                        println!("  ID: {}", sandbox_id);
                                                                    }
                                                                } else {
                                                                    // Fallback to basic display if request fails
                                                                    println!();
                                                                    println!("✓ Sandbox ready!");
                                                                    println!(
                                                                        "  ID: {}",
                                                                        sandbox_id
                                                                    );
                                                                }
                                                            }
                                                            Err(_) => {
                                                                // Fallback to basic display if request fails
                                                                println!();
                                                                println!("✓ Sandbox ready!");
                                                                println!("  ID: {}", sandbox_id);
                                                            }
                                                        }
                                                        break 'stream;
                                                    }
                                                    Some("error") => {
                                                        pb.abandon();
                                                        if let Some(p) = pb2.take() {
                                                            p.abandon();
                                                        }
                                                        if let Some(p) = pb3.take() {
                                                            p.abandon();
                                                        }
                                                        let msg = event
                                                            .get("message")
                                                            .and_then(|m| m.as_str())
                                                            .unwrap_or("Unknown error");
                                                        eprintln!("\n✗ Error: {}", msg);
                                                        std::process::exit(1);
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }
                                Ok(None) => {
                                    pb.abandon();
                                    eprintln!("\n✗ Stream ended unexpectedly");
                                    std::process::exit(1);
                                }
                                Err(e) => {
                                    pb.abandon();
                                    eprintln!("\n✗ Stream error: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                    } else {
                        let status = response.status();
                        let error = response
                            .text()
                            .await
                            .unwrap_or_else(|_| "Unknown error".to_string());

                        if let Ok(error_json) = serde_json::from_str::<serde_json::Value>(&error) {
                            eprintln!("Failed to create sandbox: {}", status);
                            if let Some(msg) = error_json.get("error") {
                                eprintln!("  Error: {}", msg);
                            }
                            if let Some(hint) = error_json.get("hint") {
                                eprintln!("  Hint: {}", hint);
                            }
                        } else {
                            eprintln!("Failed to create sandbox: {} - {}", status, error);
                        }
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to connect to server: {}", e);
                    eprintln!("  Hint: Make sure the server is running with 'dsb server'");
                    std::process::exit(1);
                }
            }
        }

        Commands::List {
            activity,
            state,
            image,
            include_deleted,
            created_after,
            created_before,
            page,
            per_page,
        } => {
            let mut request = client.get(format!("{}/sandboxes", base_url));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }

            // Add activity parameter if requested
            if activity {
                request = request.query(&[("include-activity", "true")]);
            }

            // Add filter parameters
            if let Some(state_filter) = &state {
                request = request.query(&[("state", state_filter.as_str())]);
            }
            if let Some(image_filter) = &image {
                request = request.query(&[("image", image_filter.as_str())]);
            }
            if include_deleted {
                request = request.query(&[("include_deleted", "true")]);
            }
            if let Some(after) = &created_after {
                request = request.query(&[("created_after", after.as_str())]);
            }
            if let Some(before) = &created_before {
                request = request.query(&[("created_before", before.as_str())]);
            }
            if let Some(p) = &page {
                request = request.query(&[("page", p.to_string().as_str())]);
            }
            if let Some(pp) = &per_page {
                request = request.query(&[("per_page", pp.to_string().as_str())]);
            }

            let response = request.send().await?;

            if response.status().is_success() {
                // Handle both legacy array and new paginated format
                let json_value: serde_json::Value = response.json().await?;

                let (sandboxes, pagination) = if let Some(data) = json_value.get("data") {
                    // New paginated format
                    let pag = json_value.get("pagination").unwrap();
                    (
                        data.as_array().unwrap_or(&vec![]).clone(),
                        Some(pag.clone()),
                    )
                } else {
                    // Legacy array format
                    (json_value.as_array().unwrap_or(&vec![]).clone(), None)
                };

                println!("Sandboxes:");
                if sandboxes.is_empty() {
                    println!("  (none)");
                } else {
                    for sb in sandboxes {
                        let deleted_indicator =
                            if sb.get("deleted_at").and_then(|v| v.as_str()).is_some() {
                                " [DELETED]"
                            } else {
                                ""
                            };
                        println!(
                            "  - {} | State: {} | Image: {}{}",
                            sb["id"], sb["state"], sb["config"]["image"], deleted_indicator
                        );
                        if activity {
                            if let Some(last_api) =
                                sb.get("activity").and_then(|a| a.get("last_api_activity"))
                            {
                                println!("    Last API Activity: {}", last_api);
                            }
                            if let Some(last_container) = sb
                                .get("activity")
                                .and_then(|a| a.get("last_container_activity"))
                                .and_then(|v| v.as_str())
                            {
                                println!("    Last Container Activity: {}", last_container);
                            }
                            if let Some(count) =
                                sb.get("activity").and_then(|a| a.get("activity_count"))
                            {
                                println!("    Activity Count: {}", count);
                            }
                        }
                    }
                }

                // Print pagination info if available
                if let Some(pag) = pagination {
                    println!();
                    println!(
                        "Page {} of {} ({} total)",
                        pag["page"], pag["total_pages"], pag["total"]
                    );
                    if pag["has_next"].as_bool().unwrap_or(false) {
                        println!("  Next page available");
                    }
                }
            } else {
                eprintln!("Failed to list sandboxes: {}", response.status());
                std::process::exit(1);
            }
        }

        Commands::Info { id } => {
            let mut request = client.get(format!("{}/sandboxes/{}", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.send().await?;

            if response.status().is_success() {
                let sandbox: crate::core::types::SandboxResponse = response.json().await?;
                crate::cli::display::print_sandbox_details(&sandbox);
            } else {
                eprintln!("Failed to get sandbox: {}", response.status());
                std::process::exit(1);
            }
        }

        Commands::Exec { id, command } => {
            if command.is_empty() {
                eprintln!("Error: No command specified");
                std::process::exit(1);
            }

            println!("Executing in sandbox {}: {:?}", id, command);

            let mut request = client.post(format!("{}/sandboxes/{}/exec", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }

            let body = serde_json::json!({
                "command": command
            });

            let response = request.json(&body).send().await?;

            if response.status().is_success() {
                let output: serde_json::Value = response.json().await?;
                if let Some(result) = output.get("output") {
                    println!("{}", result);
                } else if let Some(result) = output.get("result") {
                    println!("{}", result);
                } else {
                    println!("{}", output);
                }
            } else {
                let status = response.status();
                let error = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                eprintln!("Failed to exec: {} - {}", status, error);
                std::process::exit(1);
            }
        }

        Commands::Ssh { id } => {
            use crate::docker::exec_proxy::{
                DockerExecProxy, DockerExecProxyTrait, ExecConfig, ExecWriteStream,
            };
            use std::sync::Arc;
            use tokio::sync::mpsc;
            use tokio::task::JoinHandle;

            println!("Connecting to sandbox {}...", id);

            // Get sandbox info to find container ID
            let mut request = client.get(format!("{}/sandboxes/{}", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }

            let response = request.send().await?;

            if !response.status().is_success() {
                eprintln!("Failed to get sandbox: {}", response.status());
                std::process::exit(1);
            }

            let sandbox: serde_json::Value = response.json().await?;

            let container_id = match sandbox.get("container_id") {
                Some(id) => id,
                None => {
                    eprintln!("Sandbox is not running or has no container ID");
                    std::process::exit(1);
                }
            };

            let container_id: String = match container_id {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => {
                    eprintln!("Sandbox is not running");
                    std::process::exit(1);
                }
                _ => {
                    eprintln!("Invalid container ID format");
                    std::process::exit(1);
                }
            };

            if container_id.is_empty() {
                eprintln!("Sandbox is not running");
                std::process::exit(1);
            }

            println!("Connected to container: {}", container_id);
            println!("Press Ctrl+D to exit the shell");
            println!();

            // Connect to Docker via DockerManager (avoids direct bollard usage).
            // The Docker client is obtained from DockerManager::docker_client() so the
            // connection lifecycle stays consistent with the rest of the codebase.
            let docker_manager = crate::docker::DockerManager::new_with_config(config)
                .map_err(|e| format!("Failed to connect to Docker: {}", e))?;
            let docker = docker_manager.docker_client();

            let exec_proxy = Arc::new(DockerExecProxy::new_from_arc(docker));

            // Get terminal type from environment
            let term = std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string());

            // Try to use bash first, fall back to sh if not available
            let shell_command = vec!["bash".to_string()];

            let env_vars = vec![
                format!("TERM={}", term),
                "LANG=C.UTF-8".to_string(),
                "LC_ALL=C.UTF-8".to_string(),
            ];

            // Create exec with PTY
            let config = ExecConfig {
                container_id: container_id.clone(),
                command: shell_command,
                env: Some(env_vars),
                ..Default::default()
            };

            let exec_id = match exec_proxy.create_exec_pty(&config).await {
                Ok(id) => id,
                Err(e) => {
                    // If bash failed, try sh as fallback
                    eprintln!("Note: bash not available in container, trying sh...");
                    eprintln!("  For full shell features (history, tab completion), install bash:");
                    eprintln!("  apk add bash  # Alpine");
                    eprintln!("  apt-get install bash  # Debian/Ubuntu");
                    println!();

                    let sh_config = ExecConfig {
                        container_id: container_id.clone(),
                        command: vec!["sh".to_string()],
                        env: Some(vec![format!("TERM={}", term), "LANG=C.UTF-8".to_string()]),
                        ..Default::default()
                    };

                    exec_proxy.create_exec_pty(&sh_config).await.map_err(|e2| {
                        format!(
                            "Failed to create exec with sh: {}. Original bash error: {}",
                            e2, e
                        )
                    })?
                }
            };

            // Start exec and get stream
            let stream = exec_proxy
                .start_exec(&exec_id)
                .await
                .map_err(|e| format!("Failed to start exec: {}", e))?;

            // Set terminal to raw mode for interactive input
            let original_termios;
            #[cfg(unix)]
            {
                use libc::{tcgetattr, tcsetattr, TCSANOW};
                // SAFETY: tcgetattr/tcsetattr operate on file descriptor 0 (stdin), which is
                // always open in a CLI process. std::mem::zeroed is safe here because termios
                // is a C struct with no invalid bit patterns and is immediately overwritten
                // by tcgetattr before being read.
                unsafe {
                    let mut termios = std::mem::zeroed();
                    tcgetattr(0, &mut termios);
                    original_termios = Some(termios);
                    let mut raw = termios;
                    libc::cfmakeraw(&mut raw);
                    tcsetattr(0, TCSANOW, &raw);
                }
            }
            #[cfg(not(unix))]
            {
                original_termios = None;
            }

            // Split stream into read and write halves
            let (mut read_stream, write_stream) = stream.split();

            // Channel to signal shutdown
            let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

            // Spawn a task to handle window resize
            let exec_proxy_resize = exec_proxy.clone();
            let exec_id_resize = exec_id.clone();
            let _resize_handle: JoinHandle<()> = tokio::spawn(async move {
                #[cfg(unix)]
                {
                    use libc::{ioctl, winsize, STDOUT_FILENO, TIOCGWINSZ};
                    // SAFETY: ioctl with TIOCGWINSZ on STDOUT_FILENO is safe because stdout
                    // is always open in a CLI process. std::mem::zeroed is safe for winsize
                    // as it is a C struct with no invalid bit patterns and is immediately
                    // populated by the ioctl call before being read.
                    unsafe {
                        let mut size: winsize = std::mem::zeroed();
                        loop {
                            tokio::select! {
                                _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                                    if ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut size) == 0 {
                                        let _ = exec_proxy_resize.resize_pty(&exec_id_resize, size.ws_row, size.ws_col).await;
                                    }
                                }
                                _ = shutdown_rx.recv() => {
                                    break;
                                }
                            }
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    let _ = shutdown_rx.recv().await;
                }
            });

            // Spawn task to handle stdout
            // NOTE: LogOutput is a bollard type that leaks through DockerExecProxy's
            // ExecMultiplexedStream. A future refactor should abstract this into a
            // backend-agnostic frame type in the exec_proxy module. The K8s equivalent
            // would use kube::Api::exec pod streams with a similar frame enum.
            let stdout_handle = tokio::spawn(async move {
                use bollard::container::LogOutput;
                use tokio::io::{AsyncWriteExt, BufWriter};

                let mut stdout = BufWriter::new(tokio::io::stdout());

                loop {
                    match read_stream.read_frame().await {
                        Ok(Some(frame)) => match frame {
                            LogOutput::StdOut { message } | LogOutput::StdErr { message }
                                if !message.is_empty() =>
                            {
                                if stdout.write_all(&message).await.is_err() {
                                    break;
                                }
                                let _ = stdout.flush().await;
                            }
                            _ => {}
                        },
                        Ok(None) => {
                            break;
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
            });

            // Spawn task to handle stdin
            let shutdown_tx_clone = shutdown_tx.clone();
            let write_stream_for_stdin: Arc<ExecWriteStream> = Arc::new(write_stream);
            let _stdin_task: JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> =
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut stdin = tokio::io::stdin();
                    let mut buffer = [0u8; 1024];

                    loop {
                        match stdin.read(&mut buffer).await {
                            Ok(0) => {
                                // EOF - user pressed Ctrl+D
                                let _ = shutdown_tx_clone.send(()).await;
                                break;
                            }
                            Ok(n) => {
                                // Write to container
                                if let Err(e) = write_stream_for_stdin.write(&buffer[..n]).await {
                                    eprintln!("\nWrite error: {}", e);
                                    let _ = shutdown_tx_clone.send(()).await;
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("\nStdin read error: {}", e);
                                break;
                            }
                        }
                    }
                    Ok(())
                });

            // Wait for stdout task to finish
            let _ = stdout_handle.await;

            // Send shutdown signal
            let _ = shutdown_tx.send(()).await;

            // Restore terminal settings
            #[cfg(unix)]
            {
                if let Some(original) = original_termios {
                    use libc::{tcsetattr, TCSANOW};
                    // SAFETY: tcsetattr on file descriptor 0 (stdin) is safe because stdin
                    // is always open in a CLI process. The `original` termios was previously
                    // obtained via tcgetattr and is therefore a valid termios structure.
                    unsafe {
                        tcsetattr(0, TCSANOW, &original);
                    }
                }
            }

            println!("\nConnection closed");
        }

        Commands::Stop { id } => {
            println!("Stopping sandbox: {}", id);

            let mut request = client.post(format!("{}/sandboxes/{}/stop", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.send().await?;

            if response.status().is_success() {
                let sandbox: serde_json::Value = response.json().await?;
                println!("Sandbox stopped: {}", sandbox["id"]);
                println!("State: {}", sandbox["state"]);
            } else {
                eprintln!("Failed to stop sandbox: {}", response.status());
                std::process::exit(1);
            }
        }

        Commands::Delete { id } => {
            println!("Deleting sandbox: {}", id);

            let mut request = client.delete(format!("{}/sandboxes/{}", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.send().await?;

            if response.status().is_success() {
                println!("Sandbox deleted: {}", id);
            } else {
                eprintln!("Failed to delete sandbox: {}", response.status());
                std::process::exit(1);
            }
        }

        Commands::Restore { id } => {
            println!("Restoring sandbox: {}", id);

            let mut request = client.post(format!("{}/sandboxes/{}/restore", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.send().await?;

            if response.status().is_success() {
                println!("✓ Sandbox restored: {}", id);
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    if let Some(state) = body.get("state").and_then(|s| s.as_str()) {
                        println!("  State: {}", state);
                    }
                }
            } else {
                eprintln!("Failed to restore sandbox: {}", response.status());
                if let Ok(body) = response.json::<serde_json::Value>().await {
                    if let Some(error) = body.get("error") {
                        eprintln!("  Error: {}", error);
                    }
                }
                std::process::exit(1);
            }
        }

        Commands::Stats { id, stream } => {
            if stream {
                println!("Streaming stats for sandbox {} (Ctrl+C to stop):", id);

                let mut request = client.get(format!("{}/sandboxes/{}/stats-stream", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }

                match request.send().await {
                    Ok(mut response) => {
                        if response.status().is_success() {
                            // Read chunks as they arrive
                            loop {
                                match response.chunk().await {
                                    Ok(Some(chunk)) => {
                                        let data = String::from_utf8_lossy(&chunk);
                                        // Parse SSE format: "data: {...}\n\n"
                                        for line in data.lines() {
                                            if line.starts_with("data:") {
                                                let json_str =
                                                    line.trim_start_matches("data:").trim();
                                                if let Ok(stats) =
                                                    serde_json::from_str::<serde_json::Value>(
                                                        json_str,
                                                    )
                                                {
                                                    println!(
                                                        "CPU: {:.2}% | Memory: {:.2} MB ({:.2}%) | Network RX: {} bytes | TX: {} bytes | Block Read: {} bytes | Write: {} bytes",
                                                        stats["cpu_percent"].as_f64().unwrap_or(0.0),
                                                        stats["memory_usage_mb"].as_u64().unwrap_or(0),
                                                        stats["memory_percent"].as_f64().unwrap_or(0.0),
                                                        stats["network_rx_bytes"].as_u64().unwrap_or(0),
                                                        stats["network_tx_bytes"].as_u64().unwrap_or(0),
                                                        stats["block_read_bytes"].as_u64().unwrap_or(0),
                                                        stats["block_write_bytes"].as_u64().unwrap_or(0),
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        // Stream ended
                                        break;
                                    }
                                    Err(e) => {
                                        eprintln!("Stream error: {}", e);
                                        break;
                                    }
                                }
                            }
                        } else {
                            eprintln!("Failed to stream stats: {}", response.status());
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Request error: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                let mut request = client.get(format!("{}/sandboxes/{}/stats", base_url, id));
                if let Some(key) = &api_key {
                    request = request.header("X-API-Key", key);
                }
                let response = request.send().await?;

                if response.status().is_success() {
                    let stats: serde_json::Value = response.json().await?;
                    println!("Sandbox Statistics:");
                    println!(
                        "  CPU: {:.2}%",
                        stats["cpu_percent"].as_f64().unwrap_or(0.0)
                    );
                    println!(
                        "  Memory: {} MB / {} MB ({:.2}%)",
                        stats["memory_usage_mb"].as_u64().unwrap_or(0),
                        stats["memory_limit_mb"].as_u64().unwrap_or(0),
                        stats["memory_percent"].as_f64().unwrap_or(0.0)
                    );
                    println!(
                        "  Network RX: {} bytes",
                        stats["network_rx_bytes"].as_u64().unwrap_or(0)
                    );
                    println!(
                        "  Network TX: {} bytes",
                        stats["network_tx_bytes"].as_u64().unwrap_or(0)
                    );
                    println!(
                        "  Block Read: {} bytes",
                        stats["block_read_bytes"].as_u64().unwrap_or(0)
                    );
                    println!(
                        "  Block Write: {} bytes",
                        stats["block_write_bytes"].as_u64().unwrap_or(0)
                    );
                    println!("  Timestamp: {}", stats["timestamp"]);
                } else {
                    eprintln!("Failed to get stats: {}", response.status());
                    std::process::exit(1);
                }
            }
        }

        Commands::Cleanup { id } => {
            println!("Cleaning up sandbox: {}", id);

            let mut request = client.post(format!("{}/sandboxes/{}/cleanup", base_url, id));
            if let Some(key) = &api_key {
                request = request.header("X-API-Key", key);
            }
            let response = request.send().await?;

            if response.status().is_success() {
                println!("Sandbox cleaned up: {}", id);
            } else {
                let status = response.status();
                let error = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                eprintln!("Failed to cleanup sandbox: {} - {}", status, error);
                std::process::exit(1);
            }
        }

        _ => unreachable!(),
    }
    Ok(())
}
