// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! CLI Command Execution Tests
//!
//! Tests for the actual execution flow of CLI commands.

// ============================================================================
// Command Parsing End-to-End Tests
// ============================================================================

#[tokio::test]
async fn test_command_parsing_quoted_string() {
    use crate::cli::utils::parse_command_args;

    // Test the exact user case that was failing
    let command =
        vec!["sudo /usr/bin/supervisord -c /etc/supervisor/conf.d/supervisord.conf".to_string()];

    let parsed = parse_command_args(command);

    // Should be split into proper arguments
    assert_eq!(
        parsed,
        vec![
            "sudo",
            "/usr/bin/supervisord",
            "-c",
            "/etc/supervisor/conf.d/supervisord.conf"
        ]
    );

    // Verify it would serialize correctly to JSON
    let json = serde_json::json!(parsed);
    assert_eq!(
        json,
        serde_json::json!([
            "sudo",
            "/usr/bin/supervisord",
            "-c",
            "/etc/supervisor/conf.d/supervisord.conf"
        ])
    );
}

#[tokio::test]
async fn test_command_parsing_with_shell_operators() {
    use crate::cli::utils::parse_command_args;

    // Test commands with shell operators
    let command = vec!["sh -c 'echo hello && sleep infinity'".to_string()];

    let parsed = parse_command_args(command);

    // Should parse into: sh, -c, echo hello && sleep infinity
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0], "sh");
    assert_eq!(parsed[1], "-c");
    assert_eq!(parsed[2], "echo hello && sleep infinity");
}

#[tokio::test]
async fn test_command_parsing_empty_string() {
    use crate::cli::utils::parse_command_args;

    let command = vec!["".to_string()];
    let parsed = parse_command_args(command);

    // Empty string should result in empty vec (no arguments)
    assert!(parsed.is_empty());
}
