use super::parsers::*;
use super::*;
use crate::core::types::{PullPolicy, VolumeMount};
use clap::Parser;

// ============================================================================
// Port Mapping Parser Tests
// ============================================================================

#[test]
fn test_parse_port_mapping_valid() {
    let result = parse_port_mapping("8080:80");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), (8080, 80));
}

#[test]
fn test_parse_port_mapping_different_ports() {
    let test_cases = vec![
        ("8080:80", (8080, 80)),
        ("3000:3000", (3000, 3000)),
        ("443:8443", (443, 8443)),
        ("1:65535", (1, 65535)),
        ("65535:1", (65535, 1)),
    ];

    for (input, expected) in test_cases {
        let result = parse_port_mapping(input);
        assert!(result.is_ok(), "Failed for: {}", input);
        assert_eq!(result.unwrap(), expected);
    }
}

#[test]
fn test_parse_port_mapping_invalid_format() {
    let test_cases = vec![
        "8080",         // Missing colon
        "8080:80:8081", // Too many colons
        "",             // Empty
        "abc:def",      // Non-numeric
        ":80",          // Missing host port
        "8080:",        // Missing container port
        "8080 : 80",    // Spaces
    ];

    for input in test_cases {
        let result = parse_port_mapping(input);
        assert!(result.is_err(), "Should fail for: {}", input);
    }
}

#[test]
fn test_parse_port_mapping_boundary_values() {
    // Valid boundary values
    assert!(parse_port_mapping("1:1").is_ok());
    assert!(parse_port_mapping("65535:65535").is_ok());
}

#[test]
fn test_parse_port_mapping_zero_port() {
    let result = parse_port_mapping("0:80");
    // Port 0 might be valid in some contexts, but u16 can represent it
    assert_eq!(result.unwrap(), (0, 80));
}

#[test]
fn test_parse_port_mapping_large_numbers() {
    let result = parse_port_mapping("65535:65535");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), (65535, 65535));
}

// ============================================================================
// Volume String Parser Tests
// ============================================================================

#[test]
fn test_parse_volume_bind_mount_basic() {
    let result = parse_volume_string("/host/path:/container/path");
    assert!(result.is_ok());

    let volume = result.unwrap();
    match volume {
        VolumeMount::Bind {
            host_path,
            container_path,
            read_only,
        } => {
            assert_eq!(host_path, "/host/path");
            assert_eq!(container_path, "/container/path");
            assert!(!read_only);
        }
        _ => panic!("Expected Bind variant"),
    }
}

#[test]
fn test_parse_volume_bind_mount_readonly() {
    let result = parse_volume_string("/host/path:/container/path:ro");
    assert!(result.is_ok());

    let volume = result.unwrap();
    match volume {
        VolumeMount::Bind {
            host_path,
            container_path,
            read_only,
        } => {
            assert_eq!(host_path, "/host/path");
            assert_eq!(container_path, "/container/path");
            assert!(read_only);
        }
        _ => panic!("Expected Bind variant"),
    }
}

#[test]
fn test_parse_volume_named_volume() {
    let result = parse_volume_string("my_volume:/container/path");
    assert!(result.is_ok());

    let volume = result.unwrap();
    match volume {
        VolumeMount::Named {
            name,
            container_path,
            read_only,
        } => {
            assert_eq!(name, "my_volume");
            assert_eq!(container_path, "/container/path");
            assert!(!read_only);
        }
        _ => panic!("Expected Named variant"),
    }
}

#[test]
fn test_parse_volume_named_volume_readonly() {
    let result = parse_volume_string("my_volume:/container/path:ro");
    assert!(result.is_ok());

    let volume = result.unwrap();
    match volume {
        VolumeMount::Named {
            name,
            container_path,
            read_only,
        } => {
            assert_eq!(name, "my_volume");
            assert_eq!(container_path, "/container/path");
            assert!(read_only);
        }
        _ => panic!("Expected Named variant"),
    }
}

#[test]
fn test_parse_volume_relative_path() {
    let result = parse_volume_string("./relative:/container/path");
    assert!(result.is_ok());

    let volume = result.unwrap();
    match volume {
        VolumeMount::Bind { host_path, .. } => {
            assert_eq!(host_path, "./relative");
        }
        _ => panic!("Expected Bind variant for relative path"),
    }
}

#[test]
fn test_parse_volume_windows_style_path() {
    let result = parse_volume_string("C:\\host:path:/container/path");
    assert!(result.is_ok());

    let volume = result.unwrap();
    // "C:" doesn't start with / or ., so it's treated as a named volume
    // Split by ":" gives: ["C", "\\host", "path", "/container", "path"]
    // container_path = parts[1] = "\\host" (since no :ro flag)
    match volume {
        VolumeMount::Named {
            name,
            container_path,
            read_only,
        } => {
            assert_eq!(name, "C");
            assert_eq!(container_path, "\\host");
            assert!(!read_only);
        }
        _ => panic!("Expected Named variant for C: path"),
    }
}

#[test]
fn test_parse_volume_invalid_format() {
    let test_cases = vec![
        "/host/path",    // Missing container path
        "",              // Empty
        "only_one_part", // Single component
    ];

    for input in test_cases {
        let result = parse_volume_string(input);
        assert!(result.is_err(), "Should fail for: {}", input);
    }
}

#[test]
fn test_parse_volume_with_special_characters() {
    let result = parse_volume_string("/host/path with spaces:/container/path");
    assert!(result.is_ok());
}

#[test]
fn test_parse_volume_unicode_path() {
    let result = parse_volume_string("/主机/路径:/容器/路径");
    assert!(result.is_ok());
}

// ============================================================================
// Pull Policy Parser Tests
// ============================================================================

#[test]
fn test_parse_pull_policy_always() {
    let test_cases = vec![
        "always", "ALWAYS", "Always", "aLwAyS", // Case-insensitive
    ];

    for input in test_cases {
        let result = parse_pull_policy(input);
        assert!(result.is_ok(), "Failed for: {}", input);
        assert_eq!(result.unwrap(), PullPolicy::Always);
    }
}

#[test]
fn test_parse_pull_policy_missing() {
    let test_cases = vec!["missing", "MISSING", "Missing", "mIsSiNg"];

    for input in test_cases {
        let result = parse_pull_policy(input);
        assert!(result.is_ok(), "Failed for: {}", input);
        assert_eq!(result.unwrap(), PullPolicy::Missing);
    }
}

#[test]
fn test_parse_pull_policy_never() {
    let test_cases = vec!["never", "NEVER", "Never", "nEvEr"];

    for input in test_cases {
        let result = parse_pull_policy(input);
        assert!(result.is_ok(), "Failed for: {}", input);
        assert_eq!(result.unwrap(), PullPolicy::Never);
    }
}

#[test]
fn test_parse_pull_policy_invalid() {
    let test_cases = vec![
        "invalid",
        "sometimes",
        "auto",
        "pull",
        "",
        "always extra", // Extra text
    ];

    for input in test_cases {
        let result = parse_pull_policy(input);
        assert!(result.is_err(), "Should fail for: {}", input);
    }
}

#[test]
fn test_parse_pull_policy_empty_string() {
    let result = parse_pull_policy("");
    assert!(result.is_err());
}

#[test]
fn test_parse_pull_policy_with_whitespace() {
    let result = parse_pull_policy(" always ");
    assert!(result.is_err(), "Should fail with whitespace");
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_parse_port_mapping_unicode_error() {
    let result = parse_port_mapping("端口:80");
    assert!(result.is_err());
}

#[test]
fn test_parse_volume_multiple_colons_in_path() {
    // This gets split into multiple parts by ":"
    // "C" "\\path" "to" "file" "/container" "path"
    // Since "C" doesn't start with / or ., it's treated as a named volume
    // container_path = parts[1] = "\\path" (second part)
    let result = parse_volume_string("C:\\path:to:file:/container/path");
    assert!(result.is_ok());

    let volume = result.unwrap();
    match volume {
        VolumeMount::Named {
            name,
            container_path,
            read_only,
        } => {
            assert_eq!(name, "C");
            assert_eq!(container_path, "\\path");
            assert!(!read_only);
        }
        _ => panic!("Expected Named variant"),
    }
}

#[test]
fn test_parse_volume_with_rw_flag() {
    // Currently only :ro is supported, but let's test the behavior
    let result = parse_volume_string("/host:/container:rw");
    // This should work but read_only should be false since only "ro" is checked
    assert!(result.is_ok());
}

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_config_loading() {
    // Test that config can be loaded
    let result = crate::config::load();
    // Should succeed with defaults or from file/env
    assert!(result.is_ok());
}

// ============================================================================
// Error Message Tests
// ============================================================================

#[test]
fn test_port_mapping_error_messages() {
    let test_cases = vec![
        ("8080", "Invalid port mapping format"),
        ("8080:80:8081", "Invalid port mapping format"),
        ("abc:def", "Invalid host port"),
        (":80", "Invalid host port"),
        ("8080:", "Invalid container port"),
    ];

    for (input, expected_msg) in test_cases {
        let result = parse_port_mapping(input);
        assert!(result.is_err(), "Should fail for: {}", input);
        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains(expected_msg),
            "Error '{}' should contain '{}'",
            error_msg,
            expected_msg
        );
    }
}

#[test]
fn test_volume_error_messages() {
    let test_cases = vec![
        ("/host/path", "Invalid volume format"),
        ("", "Invalid volume format"),
        ("only_one_part", "Invalid volume format"),
    ];

    for (input, expected_msg) in test_cases {
        let result = parse_volume_string(input);
        assert!(result.is_err(), "Should fail for: {}", input);
        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains(expected_msg),
            "Error '{}' should contain '{}'",
            error_msg,
            expected_msg
        );
    }
}

#[test]
fn test_pull_policy_error_messages() {
    let test_cases = vec!["invalid", "sometimes", "auto", "pull"];

    for input in test_cases {
        let result = parse_pull_policy(input);
        assert!(result.is_err(), "Should fail for: {}", input);
        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains("Invalid pull policy"),
            "Error '{}' should contain 'Invalid pull policy'",
            error_msg
        );
        assert!(
            error_msg.contains(input),
            "Error '{}' should contain the input '{}'",
            error_msg,
            input
        );
        assert!(
            error_msg.contains("always"),
            "Error '{}' should mention valid options",
            error_msg
        );
    }
}

// ============================================================================
// Property-Based Style Tests (Edge Cases)
// ============================================================================

#[test]
fn test_port_mapping_symmetry() {
    // If host and container ports are swapped, they should be different
    let result1 = parse_port_mapping("8080:80");
    let result2 = parse_port_mapping("80:8080");

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let (h1, c1) = result1.unwrap();
    let (h2, c2) = result2.unwrap();

    assert_eq!(h1, 8080);
    assert_eq!(c1, 80);
    assert_eq!(h2, 80);
    assert_eq!(c2, 8080);

    // They should not be equal
    assert_ne!((h1, c1), (h2, c2));
}

#[test]
fn test_volume_read_only_flag_parsing() {
    // Test that :ro flag is correctly detected
    let bind_ro = parse_volume_string("/host:/container:ro").unwrap();
    let bind_rw = parse_volume_string("/host:/container").unwrap();

    match bind_ro {
        VolumeMount::Bind { read_only, .. } => assert!(read_only),
        _ => panic!("Expected Bind mount"),
    }

    match bind_rw {
        VolumeMount::Bind { read_only, .. } => assert!(!read_only),
        _ => panic!("Expected Bind mount"),
    }
}

#[test]
fn test_volume_path_reconstruction_with_colons() {
    // Windows-style path with multiple colons should be reconstructed correctly
    let result = parse_volume_string("C:\\path:to:file:/container/path");
    assert!(result.is_ok());

    let volume = result.unwrap();
    match volume {
        VolumeMount::Named { name, .. } => {
            // First part becomes the name since "C" doesn't start with / or .
            assert_eq!(name, "C");
        }
        _ => panic!("Expected Named variant for Windows path"),
    }
}

#[test]
fn test_pull_policy_case_insensitivity() {
    // All these should parse to the same policy
    let policies = [
        "always", "ALWAYS", "Always", "aLwAyS",
        "always ", // Note: with trailing space, should fail
    ];

    for input in policies.iter().take(4) {
        let result = parse_pull_policy(input);
        assert!(result.is_ok(), "Should succeed for: {}", input);
        assert_eq!(result.unwrap(), PullPolicy::Always);
    }

    // With trailing space should fail
    let result = parse_pull_policy("always ");
    assert!(result.is_err(), "Should fail with trailing space");
}

// ============================================================================
// Environment Variable Parser Tests
// ============================================================================

#[test]
fn test_parse_env_var_valid() {
    let result = parse_env_var("FOO=bar");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), ("FOO".to_string(), "bar".to_string()));
}

#[test]
fn test_parse_env_var_with_equals_in_value() {
    let result = parse_env_var("FOO=bar=baz");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), ("FOO".to_string(), "bar=baz".to_string()));
}

#[test]
fn test_parse_env_var_multiple() {
    let test_cases = vec![
        ("FOO=bar", ("FOO".to_string(), "bar".to_string())),
        (
            "DATABASE_URL=postgres://localhost/db",
            (
                "DATABASE_URL".to_string(),
                "postgres://localhost/db".to_string(),
            ),
        ),
        ("EMPTY=", ("EMPTY".to_string(), "".to_string())),
    ];

    for (input, expected) in test_cases {
        let result = parse_env_var(input);
        assert!(result.is_ok(), "Failed for: {}", input);
        assert_eq!(result.unwrap(), expected);
    }
}

#[test]
fn test_parse_env_var_missing_equals() {
    let test_cases = vec!["FOO", "FOO BAR", ""];

    for input in test_cases {
        let result = parse_env_var(input);
        assert!(result.is_err(), "Should fail for: {}", input);
        let error_msg = result.unwrap_err();
        assert!(
            error_msg.contains("Invalid env format"),
            "Error '{}' should mention format",
            error_msg
        );
    }
}

#[test]
fn test_parse_env_var_empty_key() {
    let result = parse_env_var("=value");
    assert!(result.is_err());
    let error_msg = result.unwrap_err();
    assert!(error_msg.contains("cannot be empty"));
}

// ============================================================================
// Web Command Parsing Tests
// ============================================================================

#[test]
fn test_web_search_command_parsing() {
    let cli = Cli::try_parse_from([
        "dsb",
        "--searxng-api-url",
        "http://localhost:8888/search",
        "web",
        "search",
        "rust async await",
        "--engine",
        "duckduckgo",
        "--num-results",
        "5",
    ])
    .expect("web search command should parse");

    assert_eq!(
        cli.searxng_api_url.as_deref(),
        Some("http://localhost:8888/search")
    );

    match cli.command {
        Commands::Web { action } => match action {
            WebCommands::Search {
                query,
                engine,
                num_results,
            } => {
                assert_eq!(query, "rust async await");
                assert_eq!(engine, Some(WebSearchEngine::Duckduckgo));
                assert_eq!(num_results, 5);
            }
            _ => panic!("Expected web search action"),
        },
        _ => panic!("Expected web command"),
    }
}

#[test]
fn test_web_fetch_command_parsing() {
    let cli = Cli::try_parse_from([
        "dsb",
        "web",
        "fetch",
        "sandbox-123",
        "https://example.com",
        "--format",
        "html",
        "--css-selector",
        "main",
        "--search-query",
        "distributed sandboxes",
        "--max-length",
        "2048",
        "--keep-open",
        "--timeout",
        "90",
    ])
    .expect("web fetch command should parse");

    match cli.command {
        Commands::Web { action } => match action {
            WebCommands::Fetch {
                sandbox_id,
                url,
                format,
                css_selector,
                search_query,
                max_length,
                keep_open,
                timeout,
                ..
            } => {
                assert_eq!(sandbox_id, "sandbox-123");
                assert_eq!(url, "https://example.com");
                assert_eq!(format, WebFetchFormat::Html);
                assert_eq!(css_selector.as_deref(), Some("main"));
                assert_eq!(search_query.as_deref(), Some("distributed sandboxes"));
                assert_eq!(max_length, Some(2048));
                assert!(keep_open);
                assert_eq!(timeout, Some(90));
            }
            _ => panic!("Expected web fetch action"),
        },
        _ => panic!("Expected web command"),
    }
}

#[test]
fn test_render_web_search_results_empty() {
    let response = serde_json::json!({ "results": [] });
    assert_eq!(
        render_web_search_results(&response),
        "No search results found."
    );
}

#[test]
fn test_truncate_search_results_rejects_zero() {
    let mut response = serde_json::json!({ "results": [] });
    let error = truncate_search_results(&mut response, 0).unwrap_err();
    assert!(error.contains("num-results"));
}
