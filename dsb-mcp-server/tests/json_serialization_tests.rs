// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! JSON serialization tests
//!
//! Validate that data structures can serialize/deserialize JSON correctly

use dsb_mcp_server::dsb_client::{Sandbox, VolumeMount};

#[test]
fn test_sandbox_deserialization_from_api() {
    let json = r#"{
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "state": "running",
        "config": {
            "image": "python:3.12",
            "name": "test",
            "environment": {}
        },
        "container_id": "container-123",
        "created_at": "2025-01-08T10:00:00Z",
        "updated_at": "2025-01-08T10:00:00Z"
    }"#;

    let sandbox: Sandbox = serde_json::from_str(json).unwrap();
    assert_eq!(sandbox.state, "running");
    assert_eq!(sandbox.config.image, "python:3.12");
    assert_eq!(sandbox.config.name, Some("test".to_string()));
    assert_eq!(sandbox.container_id, Some("container-123".to_string()));
}

#[test]
fn test_sandbox_deserialization_legacy_camel_case() {
    let json = r#"{
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "state": "running",
        "config": {
            "image": "python:3.12",
            "name": "test",
            "environment": {}
        },
        "containerId": "container-123",
        "createdAt": "2025-01-08T10:00:00Z",
        "updatedAt": "2025-01-08T10:00:00Z"
    }"#;

    let sandbox: Sandbox = serde_json::from_str(json).unwrap();
    assert_eq!(sandbox.container_id, Some("container-123".to_string()));
}

#[test]
fn test_volume_mount_serialization_with_required_host_path() {
    // host_path is now required (not optional) - this is the fix for create_sandbox
    let volume = VolumeMount {
        r#type: "bind".to_string(),
        host_path: "/host/path".to_string(), // Required field
        container_path: "/container/path".to_string(),
        read_only: false,
    };

    let json = serde_json::to_string(&volume).unwrap();
    assert!(json.contains("\"host_path\""));
    assert!(json.contains("/host/path"));

    let deserialized: VolumeMount = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.host_path, "/host/path");
    assert_eq!(deserialized.container_path, "/container/path");
    assert_eq!(deserialized.r#type, "bind");
    assert!(!deserialized.read_only);
}

#[test]
fn test_empty_sandbox_list() {
    let json = r#"[]"#;
    let sandboxes: Vec<Sandbox> = serde_json::from_str(json).unwrap();
    assert_eq!(sandboxes.len(), 0);
}

#[test]
fn test_multiple_sandboxes_in_list() {
    let json = r#"[
        {
            "id": "123e4567-e89b-12d3-a456-426614174000",
            "state": "running",
            "config": {"image": "python:3.12", "name": "sandbox-1", "environment": {}},
            "container_id": "container-1",
            "created_at": "2025-01-08T10:00:00Z",
            "updated_at": "2025-01-08T10:00:00Z"
        },
        {
            "id": "123e4567-e89b-12d3-a456-426614174001",
            "state": "stopped",
            "config": {"image": "node:20", "name": "sandbox-2", "environment": {}},
            "container_id": "container-2",
            "created_at": "2025-01-08T11:00:00Z",
            "updated_at": "2025-01-08T11:00:00Z"
        }
    ]"#;

    let sandboxes: Vec<Sandbox> = serde_json::from_str(json).unwrap();
    assert_eq!(sandboxes.len(), 2);
    assert_eq!(sandboxes[0].state, "running");
    assert_eq!(sandboxes[1].state, "stopped");
}
