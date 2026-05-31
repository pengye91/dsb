// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! SSH Session Cleanup and Monitoring Tests
//!
//! This test module verifies the enhanced SSH session cleanup and monitoring functionality,
//! including session statistics, stuck session detection, and orphaned session cleanup.
//!
//! # Running Tests
//!
//! ```bash
//! # Run SSH session cleanup tests
//! cargo test --test test_ssh_session_cleanup
//!
//! # Run specific test
//! cargo test test_ssh_session_statistics_query
//! ```

mod common;
use common::db_test_setup::TestDatabase;

use dsb::core::ssh_service::SshSessionService;
use dsb::db::ssh_sessions::{PostgresSshSessionStore, SshSessionStoreTrait};
use std::sync::Arc;

#[tokio::test]
async fn test_ssh_session_statistics_query() {
    // Test: Query SSH session statistics
    let db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    let store = PostgresSshSessionStore::new(db.pool);

    // Get initial statistics
    let stats = store
        .get_session_statistics()
        .await
        .expect("Failed to get session statistics");

    println!("✓ SSH Session Statistics:");
    println!("  Total sessions: {}", stats.total_sessions);
    println!("  Active sessions: {}", stats.active_sessions);
    println!("  Connecting sessions: {}", stats.connecting_sessions);
    println!("  Disconnected sessions: {}", stats.disconnected_sessions);
    println!("  Terminated sessions: {}", stats.terminated_sessions);
    println!("  Error sessions: {}", stats.error_sessions);
    println!("  Total bytes sent: {}", stats.total_bytes_sent);
    println!("  Total bytes received: {}", stats.total_bytes_received);
    println!("  Average duration: {:?}", stats.avg_duration_seconds);

    println!("✓ Test passed: SSH session statistics query");
}

#[tokio::test]
async fn test_stuck_connecting_sessions_detection() {
    // Test: Detect sessions stuck in connecting state
    // Note: This test verifies the query works but doesn't create fake sessions
    // due to foreign key constraints
    let db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    let store = PostgresSshSessionStore::new(db.pool);

    // Query for stuck connecting sessions (30 second timeout)
    let stuck_sessions = store
        .get_stuck_connecting_sessions(30)
        .await
        .expect("Failed to get stuck connecting sessions");

    println!(
        "✓ Found {} stuck connecting session(s)",
        stuck_sessions.len()
    );

    println!("✓ Test passed: Stuck connecting sessions detection query works");
}

#[tokio::test]
async fn test_orphaned_sessions_detection() {
    // Test: Detect orphaned sessions (sandbox no longer running)
    let db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    let store = PostgresSshSessionStore::new(db.pool);

    // Query for orphaned sessions
    let orphaned_sessions = store
        .get_orphaned_sessions()
        .await
        .expect("Failed to get orphaned sessions");

    println!("✓ Found {} orphaned session(s)", orphaned_sessions.len());

    println!("✓ Test passed: Orphaned sessions detection query works");
}

#[tokio::test]
async fn test_stale_sessions_detection() {
    // Test: Detect stale sessions (no activity for timeout period)
    let db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    let store = PostgresSshSessionStore::new(db.pool);

    // Query for stale sessions (300 second timeout)
    let stale_sessions = store
        .get_stale_sessions(300)
        .await
        .expect("Failed to get stale sessions");

    println!("✓ Found {} stale session(s)", stale_sessions.len());

    println!("✓ Test passed: Stale sessions detection query works");
}

#[tokio::test]
async fn test_session_service_statistics() {
    // Test: Query statistics through SSH service
    let db = TestDatabase::new()
        .await
        .expect("Failed to create test database");

    let ssh_store =
        Arc::new(PostgresSshSessionStore::new(db.pool)) as Arc<dyn SshSessionStoreTrait>;

    let service = SshSessionService::new(ssh_store);

    // Get statistics through service
    let stats = service
        .get_statistics()
        .await
        .expect("Failed to get session statistics from service");

    println!("✓ SSH Session Statistics (via service):");
    println!("  Total sessions: {}", stats.total_sessions);
    println!("  Active sessions: {}", stats.active_sessions);
    println!("  Connecting sessions: {}", stats.connecting_sessions);
    println!("  Disconnected sessions: {}", stats.disconnected_sessions);
    println!("  Terminated sessions: {}", stats.terminated_sessions);
    println!("  Error sessions: {}", stats.error_sessions);

    println!("✓ Test passed: Session service statistics query");
}
