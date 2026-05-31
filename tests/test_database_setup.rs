// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Database setup integration tests
//!
//! Tests the PostgreSQL testcontainers setup.
//! These tests use testcontainers to automatically spin up PostgreSQL instances
//! for testing, requiring no manual Docker setup.

mod common;

use common::TestDatabase;

#[tokio::test]
async fn test_create_test_database() {
    let db = TestDatabase::new().await.expect("Failed to create test DB");

    // Verify we can query
    let client = db.pool.get().await.expect("Failed to get connection");
    let rows = client.query("SELECT 1", &[]).await.expect("Query failed");
    let value: i32 = rows.first().expect("Failed to get row").get(0);

    assert_eq!(value, 1);
}
