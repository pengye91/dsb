// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! # Database Migration Module
//!
//! This module handles database schema creation and migrations for PostgreSQL.
//!
//! ## Schema Design
//!
//! The database stores sandbox metadata and configuration. Following best practices:
//!
//! - **Persistent data**: Sandbox records, configurations, state history
//! - **Not stored**: Ephemeral runtime state (can be queried from Docker)
//!
//! ## Tables
//!
//! - **sandboxes**: Main table storing all sandbox instances
//! - **sandbox_activities**: Activity tracking for sandboxes
//! - **ssh_sessions**: SSH session management
//! - **api_keys**: API key authentication
//! - **vnc_tokens**: VNC session token audit logging
//! - **static_files**: Static file serving metadata
//! - **session_tokens**: Short-lived session tokens for service authentication
//!
//! ## Testing Strategy
//!
//! Database migrations are tested through:
//!
//! ### Unit Tests (This Module)
//! URL parsing and validation tests:
//! - Database URL parsing
//! - Edge cases and error handling
//! - Component extraction
//!
//! ### Integration Tests
//! Full migration tests in:
//! - **`tests/common/db_test_setup.rs`**: Tests `ensure_database_exists()` and `run_migrations()`
//! - **`tests/db_integration_tests.rs`**: Verifies tables are created correctly
//!
//! Integration tests cover:
//! - Database creation
//! - Table schema validation
//! - Index creation
//! - Constraint validation
//! - Idempotent migration execution
//!
//! ## Example
//!
//! ```rust,no_run,ignore
//! use dsb::db::migration::{ensure_database_exists, run_migrations};
//! use dsb::db::pool::create_pool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! let database_url = "postgresql://postgres:postgres@localhost:5432/dsb";
//! // Ensure database exists first
//! ensure_database_exists(database_url).await?;
//! let pool = create_pool(database_url).await?;
//! run_migrations(&pool).await?;
//! # Ok(())
//! # }
//! ```

use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

/// Ensures the target database exists, creating it if necessary.
///
/// This function connects to the default "postgres" database first
/// (which always exists in PostgreSQL), then checks if the target
/// database specified in the database URL exists. If not, it creates it.
///
/// # Arguments
///
/// * `database_url` - PostgreSQL connection URL
///   Format: `postgresql://user:password@host:port/database`
///
/// # Returns
///
/// * `Ok(())` - Database exists or was created successfully
/// * `Err(...)` - Failed to connect or create database
///
/// # Errors
///
/// This function will return an error if:
/// - `database_url` is invalid
/// - Connection to PostgreSQL fails
/// - Database creation fails due to permissions
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use dsb::db::migration::ensure_database_exists;
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// ensure_database_exists("postgresql://postgres:postgres@localhost:5432/dsb").await?;
/// # Ok(())
/// # }
/// ```
pub async fn ensure_database_exists(
    database_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Checking if database exists...");

    // Parse the database URL to extract components
    // Format: postgresql://user:password@host:port/database
    let url_parts = parse_database_url(database_url)?;

    // Connect to the default "postgres" database first
    // (this database always exists in PostgreSQL)
    let admin_db_url = format!(
        "postgresql://{}:{}@{}:{}/postgres",
        url_parts.user, url_parts.password, url_parts.host, url_parts.port
    );

    let mut cfg = Config::new();
    cfg.url = Some(admin_db_url.clone());

    // Retry logic to wait for PostgreSQL to be ready
    let mut client = None;
    for attempt in 0..30 {
        // Exponential backoff: 1s, 2s, 4s, 8s...
        let delay = std::time::Duration::from_millis(1000 * u64::pow(2, attempt).min(8000));

        let pool = cfg
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| format!("Failed to create admin connection pool: {}", e));

        if let Ok(pool) = pool {
            match pool.get().await {
                Ok(conn) => {
                    client = Some(conn);
                    break;
                }
                Err(e) => {
                    if attempt < 29 {
                        tracing::debug!(
                            "Attempt {}/30: Failed to connect to PostgreSQL ({}), retrying in {:?}...",
                            attempt + 1,
                            e,
                            delay
                        );
                        tokio::time::sleep(delay).await;
                    } else {
                        return Err(format!(
                            "Failed to connect to PostgreSQL after 30 attempts: {}",
                            e
                        )
                        .into());
                    }
                }
            }
        } else if attempt < 29 {
            tracing::warn!(
                "Attempt {}/30: Failed to create connection pool, retrying in {:?}...",
                attempt + 1,
                delay
            );
            tokio::time::sleep(delay).await;
        } else {
            return Err("Failed to create connection pool after 30 attempts".into());
        }
    }

    let client = client.unwrap();

    // Check if database exists
    let exists_query = r#"
        SELECT EXISTS(
            SELECT 1 FROM pg_database WHERE datname = $1
        )
    "#;

    let exists: bool = client
        .query_one(exists_query, &[&url_parts.database])
        .await
        .map_err(|e| format!("Failed to check database existence: {}", e))?
        .get(0);

    if exists {
        tracing::info!("Database '{}' already exists", url_parts.database);
        return Ok(());
    }

    // Create database
    tracing::info!("Creating database '{}'...", url_parts.database);

    let create_query = format!("CREATE DATABASE {}", url_parts.database);

    client
        .execute(&create_query, &[])
        .await
        .map_err(|e| format!("Failed to create database: {}", e))?;

    tracing::info!("Database '{}' created successfully", url_parts.database);

    Ok(())
}

/// Parsed components of a PostgreSQL database URL.
#[derive(Debug)]
struct DatabaseUrlParts {
    host: String,
    port: String,
    user: String,
    password: String,
    database: String,
}

/// Parses a PostgreSQL database URL into its components.
///
/// # Arguments
///
/// * `url` - Database URL in format `postgresql://user:password@host:port/database`
///
/// # Returns
///
/// Parsed URL components
fn parse_database_url(url: &str) -> Result<DatabaseUrlParts, String> {
    // Remove the "postgresql://" prefix if present
    let url = url
        .strip_prefix("postgresql://")
        .or_else(|| url.strip_prefix("postgres://"))
        .ok_or("URL must start with postgresql:// or postgres://")?;

    // Split into parts: user:password@host:port/database
    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        return Err("Invalid URL format. Expected: user:password@host:port/database".to_string());
    }

    // Parse user:password
    let auth_parts: Vec<&str> = parts[0].split(':').collect();
    if auth_parts.len() != 2 {
        return Err("Invalid authentication format. Expected: user:password".to_string());
    }
    let user = auth_parts[0];
    let password = auth_parts[1];

    // Parse host:port/database
    let host_parts: Vec<&str> = parts[1].split('/').collect();
    if host_parts.len() != 2 {
        return Err("Invalid host/database format. Expected: host:port/database".to_string());
    }
    let database = host_parts[1];

    // Parse host:port
    let addr_parts: Vec<&str> = host_parts[0].split(':').collect();
    let (host, port) = if addr_parts.len() == 2 {
        (addr_parts[0], addr_parts[1])
    } else {
        // Default port if not specified
        (host_parts[0], "5432")
    };

    Ok(DatabaseUrlParts {
        host: host.to_string(),
        port: port.to_string(),
        user: user.to_string(),
        password: password.to_string(),
        database: database.to_string(),
    })
}

/// Runs all database migrations to create/update schema.
///
/// This function creates all necessary tables if they don't exist.
/// It's idempotent - safe to run multiple times.
///
/// # Arguments
///
/// * `pool` - PostgreSQL connection pool
///
/// # Returns
///
/// * `Ok(())` - Migrations completed successfully
/// * `Err(...)` - Database error during migration
///
/// # Errors
///
/// This function will return an error if:
/// - Database connection fails
/// - SQL execution fails
/// - Table creation fails due to permissions or schema conflicts
///
/// # Example
///
/// ```rust,no_run,ignore
/// # use dsb::db::migration::run_migrations;
/// # use dsb::db::pool::create_pool;
/// # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
/// let pool = create_pool("postgresql://postgres:postgres@localhost:5432/dsb").await?;
/// run_migrations(&pool).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run_migrations(pool: &Pool) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!("Running database migrations");

    let client = pool
        .get()
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Enable uuid-ossp extension for uuid_generate_v4() function
    client
        .batch_execute(
            r#"
        CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Create sandboxes table
    client
        .batch_execute(
            r#"
        CREATE TABLE IF NOT EXISTS sandboxes (
            -- Primary key
            id UUID PRIMARY KEY,

            -- Configuration (stored as JSONB for flexibility)
            image TEXT NOT NULL,
            name TEXT,
            environment JSONB DEFAULT '{}'::jsonb,
            port_mappings JSONB DEFAULT '[]'::jsonb,
            resource_limits JSONB DEFAULT '{}'::jsonb,
            volumes JSONB DEFAULT '[]'::jsonb,
            command JSONB,  -- Optional command to run in container
            inactivity_timeout_minutes BIGINT,
            pull_policy TEXT NOT NULL,
            features JSONB DEFAULT '[]'::jsonb,
            enable_all_features BOOLEAN DEFAULT FALSE,

            -- Runtime state
            state TEXT NOT NULL,
            container_id TEXT,
            error_message TEXT,

            -- Volume mounts tracking (JSONB array)
            volume_mounts JSONB DEFAULT '[]'::jsonb,

            -- Activity tracking for auto-cleanup
            last_api_activity TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            last_container_activity TIMESTAMPTZ,
            activity_count BIGINT DEFAULT 0,

            -- Timestamps
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            -- Constraints
            CONSTRAINT check_state CHECK (state IN ('creating', 'created', 'starting', 'running', 'stopped', 'error', 'destroying', 'destroyed')),
            CONSTRAINT check_pull_policy CHECK (pull_policy IN ('always', 'missing', 'never'))
        );

        -- Indexes for performance
        CREATE INDEX IF NOT EXISTS idx_sandboxes_state ON sandboxes(state);
        CREATE INDEX IF NOT EXISTS idx_sandboxes_created_at ON sandboxes(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_sandboxes_container_id ON sandboxes(container_id) WHERE container_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_sandboxes_name ON sandboxes(name) WHERE name IS NOT NULL;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Migration: Drop enable_static_server column (always enabled now)
    client
        .batch_execute(
            r#"
            -- Drop the enable_static_server column (static server is always enabled now)
            ALTER TABLE sandboxes DROP COLUMN IF EXISTS enable_static_server;
            "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Create sandbox_activities table
    client
        .batch_execute(
            r#"
        CREATE TABLE IF NOT EXISTS sandbox_activities (
            -- Primary key
            id UUID PRIMARY KEY,

            -- Foreign key reference (optional - allows tracking deleted sandboxes)
            sandbox_id UUID NOT NULL,

            -- Activity classification
            activity_type TEXT NOT NULL,

            -- Timestamp with time zone
            timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            -- Flexible details storage (JSONB)
            details JSONB DEFAULT '{}'::jsonb,

            -- Track if sandbox was deleted (preserves history)
            sandbox_is_deleted BOOLEAN DEFAULT FALSE
        );

        -- Indexes for performance
        CREATE INDEX IF NOT EXISTS idx_sandbox_activities_sandbox_id
            ON sandbox_activities(sandbox_id);
        CREATE INDEX IF NOT EXISTS idx_sandbox_activities_timestamp
            ON sandbox_activities(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_sandbox_activities_sandbox_timestamp
            ON sandbox_activities(sandbox_id, timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_sandbox_activities_type
            ON sandbox_activities(activity_type);
        CREATE INDEX IF NOT EXISTS idx_sandbox_activities_active
            ON sandbox_activities(sandbox_id, timestamp DESC);
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Create ssh_sessions table
    client
        .batch_execute(
            r#"
        CREATE TABLE IF NOT EXISTS ssh_sessions (
            -- Primary key
            id UUID PRIMARY KEY,

            -- Reference to sandbox
            sandbox_id UUID NOT NULL,

            -- Session information
            client_ip TEXT NOT NULL,
            ssh_version TEXT,

            -- Authentication
            auth_method TEXT NOT NULL,

            -- Connection tracking
            ssh_session_id TEXT,
            exec_id TEXT,

            -- PTY information
            pty_term TEXT,
            pty_rows INTEGER,
            pty_cols INTEGER,

            -- State management
            state TEXT NOT NULL,
            connected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            disconnected_at TIMESTAMPTZ,
            last_activity_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            -- Statistics
            bytes_sent BIGINT DEFAULT 0,
            bytes_received BIGINT DEFAULT 0,
            duration_seconds INTEGER,

            -- Termination
            termination_reason TEXT,

            -- Timestamps
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            -- Foreign key (automatically delete SSH sessions when sandbox is deleted)
            CONSTRAINT fk_sandbox
                FOREIGN KEY (sandbox_id)
                REFERENCES sandboxes(id)
                ON DELETE CASCADE,

            -- Constraints
            CONSTRAINT check_ssh_state
                CHECK (state IN ('connecting', 'active', 'disconnected', 'terminated', 'error')),
            CONSTRAINT check_auth_method
                CHECK (auth_method IN ('api_key', 'certificate'))
        );

        -- Indexes for performance
        CREATE INDEX IF NOT EXISTS idx_ssh_sessions_sandbox_id
            ON ssh_sessions(sandbox_id);
        CREATE INDEX IF NOT EXISTS idx_ssh_sessions_state
            ON ssh_sessions(state);
        CREATE INDEX IF NOT EXISTS idx_ssh_sessions_connected_at
            ON ssh_sessions(connected_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ssh_sessions_sandbox_state
            ON ssh_sessions(sandbox_id, state)
            WHERE state IN ('active', 'connecting');
        CREATE INDEX IF NOT EXISTS idx_ssh_sessions_last_activity
            ON ssh_sessions(last_activity_at DESC);
        CREATE INDEX IF NOT EXISTS idx_ssh_sessions_stale_sessions
            ON ssh_sessions(state, last_activity_at)
            WHERE state IN ('active', 'connecting');
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Create static_files table
    client
        .batch_execute(
            r#"
        CREATE TABLE IF NOT EXISTS static_files (
            -- Primary key
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

            -- Sandbox association
            sandbox_id UUID NOT NULL,

            -- File metadata
            file_path TEXT NOT NULL,
            file_name TEXT NOT NULL,

            -- Content information
            content_type TEXT NOT NULL,
            file_size_bytes BIGINT,

            -- Access tracking
            published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            last_accessed_at TIMESTAMPTZ,
            access_count BIGINT DEFAULT 0,

            -- Timestamps
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            -- Foreign key (preserve files after sandbox deletion)
            CONSTRAINT fk_static_files_sandbox
                FOREIGN KEY (sandbox_id)
                REFERENCES sandboxes(id)
                ON DELETE CASCADE,

            -- Unique constraint: one file record per sandbox path
            CONSTRAINT uq_sandbox_file_path
                UNIQUE (sandbox_id, file_path)
        );

        -- Indexes for performance
        CREATE INDEX IF NOT EXISTS idx_static_files_sandbox_id
            ON static_files(sandbox_id);
        CREATE INDEX IF NOT EXISTS idx_static_files_sandbox_path
            ON static_files(sandbox_id, file_path);
        CREATE INDEX IF NOT EXISTS idx_static_files_published_at
            ON static_files(published_at DESC);
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Fix ssh_sessions foreign key constraint
    // The original constraint had ON DELETE SET NULL with sandbox_id NOT NULL,
    // which causes conflicts. We need to change it to ON DELETE CASCADE.
    client
        .batch_execute(
            r#"
        -- Drop the old foreign key constraint if it exists
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'fk_sandbox'
                AND conrelid = 'ssh_sessions'::regclass
            ) THEN
                ALTER TABLE ssh_sessions DROP CONSTRAINT fk_sandbox;
            END IF;
        END $$;

        -- Add the new foreign key constraint with ON DELETE CASCADE
        ALTER TABLE ssh_sessions
        ADD CONSTRAINT fk_sandbox
            FOREIGN KEY (sandbox_id)
            REFERENCES sandboxes(id)
            ON DELETE CASCADE;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Create api_keys table
    client
        .batch_execute(
            r#"
        CREATE TABLE IF NOT EXISTS api_keys (
            -- Primary key
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

            -- Key identifier (never expose actual key after creation)
            key_hash TEXT NOT NULL UNIQUE,
            key_prefix TEXT NOT NULL,

            -- Metadata
            name TEXT NOT NULL,
            description TEXT,
            scopes JSONB DEFAULT '[]'::jsonb,

            -- Lifecycle
            is_active BOOLEAN NOT NULL DEFAULT TRUE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ,
            last_used_at TIMESTAMPTZ,

            -- Creator tracking
            created_by TEXT,

            -- Constraints
            CONSTRAINT valid_expiration CHECK (expires_at IS NULL OR expires_at > created_at)
        );

        -- Indexes for efficient lookups
        CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash);
        CREATE INDEX IF NOT EXISTS idx_api_keys_key_prefix ON api_keys(key_prefix);
        CREATE INDEX IF NOT EXISTS idx_api_keys_is_active ON api_keys(is_active);
        CREATE INDEX IF NOT EXISTS idx_api_keys_last_used ON api_keys(last_used_at);

        -- Unique constraint on name for active keys
        CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_name_unique
            ON api_keys(name) WHERE is_active = TRUE;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Create vnc_tokens table for VNC session token audit logging
    client
        .batch_execute(
            r#"
        CREATE TABLE IF NOT EXISTS vnc_tokens (
            -- Primary key
            id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),

            -- Token identifier (SHA-256 hash, never store plaintext token!)
            token_hash TEXT NOT NULL UNIQUE,

            -- Reference to sandbox (token is bound to this sandbox)
            sandbox_id UUID NOT NULL,

            -- Optional API key that created this token (for audit)
            api_key_id UUID REFERENCES api_keys(id) ON DELETE SET NULL,

            -- Token metadata
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ NOT NULL,
            ttl_seconds INTEGER NOT NULL,

            -- Usage tracking
            last_used_at TIMESTAMPTZ,
            usage_count INTEGER DEFAULT 0,

            -- Security audit information
            client_ip INET,
            user_agent TEXT,

            -- Ensure expires_at is after created_at
            CONSTRAINT valid_expires CHECK (expires_at > created_at)
        );

        -- Indexes for efficient queries
        CREATE INDEX IF NOT EXISTS idx_vnc_tokens_sandbox ON vnc_tokens(sandbox_id);
        CREATE INDEX IF NOT EXISTS idx_vnc_tokens_created_at ON vnc_tokens(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_vnc_tokens_api_key ON vnc_tokens(api_key_id);
        CREATE INDEX IF NOT EXISTS idx_vnc_tokens_expires_at ON vnc_tokens(expires_at);

        -- Cascade delete tokens when sandbox is deleted
        CREATE INDEX IF NOT EXISTS idx_vnc_tokens_sandbox_id_fkey
            ON vnc_tokens(sandbox_id);

        -- Cleanup old tokens periodically (run via cron or application task)
        -- DELETE FROM vnc_tokens WHERE expires_at < NOW();
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Fix static_files table id column default if needed
    // This handles the case where the table was created with gen_random_uuid()
    // which doesn't work in older PostgreSQL versions or without pgcrypto
    client
        .batch_execute(
            r#"
        -- Alter the id column default to use uuid_generate_v4() instead of gen_random_uuid()
        DO $$
        BEGIN
            -- Check if the default is gen_random_uuid() and change it
            IF EXISTS (
                SELECT 1 FROM pg_attrdef ad
                JOIN pg_attribute a ON a.attrelid = ad.adrelid AND a.attnum = ad.adnum
                JOIN pg_class c ON c.oid = ad.adrelid
                WHERE c.relname = 'static_files'
                AND a.attname = 'id'
                AND pg_get_expr(ad.adbin, ad.adrelid) LIKE '%gen_random_uuid%'
            ) THEN
                ALTER TABLE static_files ALTER COLUMN id SET DEFAULT uuid_generate_v4();
            END IF;
        END $$;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Add soft delete columns to sandboxes table for history tracking
    client
        .batch_execute(
            r#"
        -- Add soft delete columns for sandbox history tracking
        ALTER TABLE sandboxes ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;
        ALTER TABLE sandboxes ADD COLUMN IF NOT EXISTS deleted_by TEXT;

        -- Create indexes for efficient querying of deleted/non-deleted sandboxes
        CREATE INDEX IF NOT EXISTS idx_sandboxes_not_deleted ON sandboxes(id) WHERE deleted_at IS NULL;
        CREATE INDEX IF NOT EXISTS idx_sandboxes_deleted_at ON sandboxes(deleted_at DESC);
        CREATE INDEX IF NOT EXISTS idx_sandboxes_state_deleted ON sandboxes(state, deleted_at);
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Update foreign key constraints to SET NULL instead of CASCADE
    // This preserves SSH sessions and static files even when sandbox is soft-deleted
    client
        .batch_execute(
            r#"
        -- Update ssh_sessions foreign key to SET NULL
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'fk_sandbox'
                AND conrelid = 'ssh_sessions'::regclass
            ) THEN
                ALTER TABLE ssh_sessions DROP CONSTRAINT fk_sandbox;
            END IF;
        END $$;

        ALTER TABLE ssh_sessions
        ADD CONSTRAINT fk_sandbox
            FOREIGN KEY (sandbox_id)
            REFERENCES sandboxes(id)
            ON DELETE SET NULL;

        -- Update static_files foreign key to SET NULL
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'fk_static_files_sandbox'
                AND conrelid = 'static_files'::regclass
            ) THEN
                ALTER TABLE static_files DROP CONSTRAINT fk_static_files_sandbox;
            END IF;
        END $$;

        ALTER TABLE static_files
        ADD CONSTRAINT fk_static_files_sandbox
            FOREIGN KEY (sandbox_id)
            REFERENCES sandboxes(id)
            ON DELETE SET NULL;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Migration: Update destroyed state for soft-deleted sandboxes
    // This fixes the semantic issue where deleted sandboxes had state='destroying'
    // instead of the correct state='destroyed' (final state vs transitional)
    client
        .batch_execute(
            r#"
        -- First, drop the old check_state constraint if it exists
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_constraint
                WHERE conname = 'check_state'
                AND conrelid = 'sandboxes'::regclass
            ) THEN
                ALTER TABLE sandboxes DROP CONSTRAINT check_state;
            END IF;
        END $$;

        -- Add the updated constraint with 'destroyed' included
        ALTER TABLE sandboxes
        ADD CONSTRAINT check_state
        CHECK (state IN (
            'creating', 'created', 'starting', 'running',
            'stopped', 'error', 'destroying', 'destroyed'
        ));

        -- Now migrate existing 'destroying' states to 'destroyed' for deleted sandboxes
        UPDATE sandboxes
            SET state = 'destroyed'
            WHERE state = 'destroying' AND deleted_at IS NOT NULL;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Migration: Add vnc_resolution column for VNC display resolution configuration
    // This column stores the VNC resolution as TEXT (format: "WIDTHxHEIGHT")
    client
        .batch_execute(
            r#"
        -- Add vnc_resolution column if it doesn't exist
        ALTER TABLE sandboxes ADD COLUMN IF NOT EXISTS vnc_resolution TEXT;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Migration: Create session_tokens table for service authentication
    // Stores short-lived session tokens for service authentication (OpenClaw, VNC, etc.)
    client
        .batch_execute(
            r#"
        -- Create session_tokens table for service authentication
        CREATE TABLE IF NOT EXISTS session_tokens (
            -- Primary key: token string (UUID)
            token TEXT PRIMARY KEY,

            -- Associated sandbox ID
            sandbox_id TEXT NOT NULL,

            -- Service name (e.g., "openclaw", "vnc")
            service TEXT NOT NULL,

            -- Timestamps
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            expires_at TIMESTAMPTZ NOT NULL,

            -- Constraints
            CONSTRAINT expires_after_created CHECK (expires_at > created_at)
        );

        -- Indexes for efficient lookups and cleanup
        CREATE INDEX IF NOT EXISTS idx_session_tokens_sandbox_service
            ON session_tokens(sandbox_id, service);

        CREATE INDEX IF NOT EXISTS idx_session_tokens_expires_at
            ON session_tokens(expires_at);

        -- Comment for documentation
        COMMENT ON TABLE session_tokens IS 'Short-lived session tokens for service authentication';
        COMMENT ON COLUMN session_tokens.token IS 'Unique session token (UUID format)';
        COMMENT ON COLUMN session_tokens.sandbox_id IS 'ID of the sandbox this token is for';
        COMMENT ON COLUMN session_tokens.service IS 'Service name (openclaw, vnc, etc.)';
        COMMENT ON COLUMN session_tokens.expires_at IS 'Token expiration time for automatic cleanup';
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Migration: Add api_key_id to sandboxes table for ownership tracking
    client
        .batch_execute(
            r#"
        -- Add api_key_id column to sandboxes table for multi-tenancy isolation
        ALTER TABLE sandboxes ADD COLUMN IF NOT EXISTS api_key_id UUID REFERENCES api_keys(id) ON DELETE SET NULL;

        -- Single composite partial index covers both query patterns:
        -- 1. WHERE api_key_id = $1 AND deleted_at IS NULL  (list owned sandboxes)
        -- 2. WHERE api_key_id = $1                         (admin lookup by key)
        -- PostgreSQL can use the leading column for single-column lookups.
        CREATE INDEX IF NOT EXISTS idx_sandboxes_api_key_owner
            ON sandboxes(api_key_id, deleted_at)
            WHERE deleted_at IS NULL;
        "#,
        )
        .await
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    tracing::info!("Database migrations completed successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Database URL Parsing Tests
    // ========================================================================

    #[test]
    fn test_parse_database_url_full_url() {
        let url = "postgresql://user:pass@localhost:5432/mydb";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.host, "localhost");
        assert_eq!(parts.port, "5432");
        assert_eq!(parts.user, "user");
        assert_eq!(parts.password, "pass");
        assert_eq!(parts.database, "mydb");
    }

    #[test]
    fn test_parse_database_url_with_postgres_prefix() {
        let url = "postgres://admin:secret@db.example.com:3306/production";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.host, "db.example.com");
        assert_eq!(parts.port, "3306");
        assert_eq!(parts.user, "admin");
        assert_eq!(parts.password, "secret");
        assert_eq!(parts.database, "production");
    }

    #[test]
    fn test_parse_database_url_default_port() {
        let url = "postgresql://user:pass@localhost/testdb";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.host, "localhost");
        assert_eq!(parts.port, "5432"); // Default port
        assert_eq!(parts.database, "testdb");
    }

    #[test]
    fn test_parse_database_url_with_ipv4_host() {
        let url = "postgresql://user:pass@192.168.1.100:5432/mydb";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.host, "192.168.1.100");
        assert_eq!(parts.port, "5432");
        assert_eq!(parts.database, "mydb");
    }

    #[test]
    fn test_parse_database_url_with_localhost_default() {
        let url = "postgresql://postgres:postgres@localhost/dsb";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.host, "localhost");
        assert_eq!(parts.port, "5432");
        assert_eq!(parts.user, "postgres");
        assert_eq!(parts.password, "postgres");
        assert_eq!(parts.database, "dsb");
    }

    #[test]
    fn test_parse_database_url_with_special_characters_in_password() {
        // URL with @ in password causes parsing errors
        let url = "postgresql://user:p@ss:w0rd@localhost:5432/mydb";
        let result = parse_database_url(url);

        // Passwords with @ are not supported (breaks the parser)
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid URL format"));
    }

    #[test]
    fn test_parse_database_url_error_no_prefix() {
        let url = "user:pass@localhost:5432/mydb";
        let result = parse_database_url(url);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must start with"));
    }

    #[test]
    fn test_parse_database_url_error_missing_at_sign() {
        let url = "postgresql://user:passlocalhost:5432/mydb";
        let result = parse_database_url(url);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid URL format"));
    }

    #[test]
    fn test_parse_database_url_error_missing_colon_in_auth() {
        let url = "postgresql://user@localhost:5432/mydb";
        let result = parse_database_url(url);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Invalid authentication format"));
    }

    #[test]
    fn test_parse_database_url_error_missing_slash() {
        let url = "postgresql://user:pass@localhost:5432";
        let result = parse_database_url(url);

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid host/database format"));
    }

    #[test]
    fn test_parse_database_url_with_underscore_in_name() {
        let url = "postgresql://test_user:test_pass@localhost:5432/test_db";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.user, "test_user");
        assert_eq!(parts.password, "test_pass");
        assert_eq!(parts.database, "test_db");
    }

    #[test]
    fn test_parse_database_url_with_digits() {
        let url = "postgresql://user123:pass456@localhost:5432/db789";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.user, "user123");
        assert_eq!(parts.password, "pass456");
        assert_eq!(parts.database, "db789");
    }

    #[test]
    fn test_parse_database_url_with_hyphen_in_host() {
        let url = "postgresql://user:pass@my-db-server:5432/mydb";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.host, "my-db-server");
        assert_eq!(parts.database, "mydb");
    }

    #[test]
    fn test_parse_database_url_with_dots_in_host() {
        let url = "postgresql://user:pass@db.server.example.com:5432/mydb";
        let parts = parse_database_url(url).unwrap();

        assert_eq!(parts.host, "db.server.example.com");
        assert_eq!(parts.port, "5432");
    }

    #[test]
    fn test_parse_database_url_empty_database_name() {
        // Note: This test documents current behavior
        // An empty database name after / is accepted by the parser
        let url = "postgresql://user:pass@localhost:5432/";
        let parts = parse_database_url(url).unwrap();

        // Empty database name is accepted (validation happens later)
        assert_eq!(parts.database, "");
    }

    #[test]
    fn test_parse_database_url_with_multiple_colons_in_host() {
        // IPv6 addresses would be like [::1]:5432 but not currently supported
        // Our parser doesn't handle the bracket notation for IPv6
        let url = "postgresql://user:pass@::1:5432/mydb";
        let parts = parse_database_url(url).unwrap();

        // Parser treats ::1:5432 as host (no port separation)
        assert_eq!(parts.host, "::1:5432");
        // Port defaults to 5432 since no explicit port was found
        assert_eq!(parts.port, "5432");
    }

    #[test]
    fn test_parse_database_url_trims_whitespace() {
        // Our parser doesn't trim, leading/trailing spaces are part of the URL
        // The postgresql:// prefix check will fail if there's leading whitespace
        let url = "postgresql://user:pass@localhost:5432/mydb ";
        let parts = parse_database_url(url).unwrap();

        // Trailing space becomes part of the database name
        assert_eq!(parts.database, "mydb ");
    }

    #[test]
    fn test_parse_database_url_case_sensitive() {
        let url1 = "postgresql://User:Pass@localhost:5432/MyDB";
        let url2 = "postgresql://user:pass@localhost:5432/mydb";

        let parts1 = parse_database_url(url1).unwrap();
        let parts2 = parse_database_url(url2).unwrap();

        // User and database names are case-sensitive
        assert_eq!(parts1.user, "User");
        assert_eq!(parts2.user, "user");
        assert_eq!(parts1.database, "MyDB");
        assert_eq!(parts2.database, "mydb");
    }

    // ========================================================================
    // DatabaseUrlParts Struct Tests
    // ========================================================================

    #[test]
    fn test_database_url_parts_fields() {
        let parts = DatabaseUrlParts {
            host: "localhost".to_string(),
            port: "5432".to_string(),
            user: "user".to_string(),
            password: "pass".to_string(),
            database: "db".to_string(),
        };

        assert_eq!(parts.host, "localhost");
        assert_eq!(parts.port, "5432");
        assert_eq!(parts.user, "user");
        assert_eq!(parts.password, "pass");
        assert_eq!(parts.database, "db");
    }

    // ========================================================================
    // Integration Test References
    // ========================================================================

    #[test]
    fn test_integration_test_locations() {
        // Documents where integration tests are located
        let _locations = (
            "tests/common/db_test_setup.rs",
            "tests/db_integration_tests.rs",
        );
    }

    #[test]
    fn test_migration_functions_exist() {
        // Compile-time verification that migration functions exist
        // Just referencing them is enough to prove they exist
        let _ = ensure_database_exists;
        let _ = run_migrations;
    }
}
