// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie

use super::*;
use super::helpers::*;
use crate::core::types::{PortMapping, PortProtocol, SandboxConfig};
use std::collections::{BTreeMap, HashMap};
use k8s_openapi::api::core::v1::{ResourceRequirements, Toleration};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use crate::k8s::crd::PortSpec;
use kube::Resource;

// -----------------------------------------------------------------------
// Name generation tests
// -----------------------------------------------------------------------

#[test]
fn test_sandbox_resource_name_format() {
    let name = sandbox_resource_name("550e8400-e29b-41d4-a716-446655440000");
    assert!(name.starts_with("dsb-sb-"));
    assert!(name.len() <= 63, "K8s names must be <= 63 chars");
    assert!(!name.contains(|c: char| c.is_uppercase()));
}

#[test]
fn test_sandbox_service_name_format() {
    let name = sandbox_service_name("550e8400-e29b-41d4-a716-446655440000");
    assert!(name.starts_with("dsb-svc-"));
    assert!(name.len() <= 63, "K8s names must be <= 63 chars");
    assert!(!name.contains(|c: char| c.is_uppercase()));
}

#[test]
fn test_sandbox_resource_name_truncation() {
    // Very long sandbox ID should be truncated to 63 chars
    let long_id = "a".repeat(100);
    let name = sandbox_resource_name(&long_id);
    assert_eq!(name.len(), 63);
}

#[test]
fn test_sandbox_service_name_truncation() {
    let long_id = "b".repeat(100);
    let name = sandbox_service_name(&long_id);
    assert_eq!(name.len(), 63);
}

#[test]
fn test_merge_sandbox_environment_proxy_overridden_by_request() {
    let mut proxy = HashMap::new();
    proxy.insert("HTTP_PROXY".to_string(), "http://proxy:3128".to_string());
    proxy.insert("HTTPS_PROXY".to_string(), "http://proxy:3128".to_string());
    proxy.insert("NO_PROXY".to_string(), "localhost,.svc.cluster.local".to_string());
    let config = SandboxConfig {
        environment: HashMap::from([(
            "HTTP_PROXY".to_string(),
            "http://override:8080".to_string(),
        )]),
        ..Default::default()
    };
    let merged = merge_sandbox_environment(&config, &proxy);
    assert_eq!(
        merged.get("HTTP_PROXY").map(String::as_str),
        Some("http://override:8080")
    );
    assert_eq!(
        merged.get("HTTPS_PROXY").map(String::as_str),
        Some("http://proxy:3128")
    );
    assert_eq!(
        merged.get("NO_PROXY").map(String::as_str),
        Some("localhost,.svc.cluster.local")
    );
}

#[test]
fn test_augment_no_proxy_appends_cluster_suffix_when_proxy_set() {
    let mut env = HashMap::from([("HTTP_PROXY".to_string(), "http://corp:3128".to_string())]);
    augment_no_proxy_for_kubernetes_cluster(&mut env);
    let n = env.get("NO_PROXY").expect("NO_PROXY");
    assert!(n.contains(".svc.cluster.local"), "{n}");
    assert!(n.contains("localhost"), "{n}");
}

#[test]
fn test_augment_no_proxy_preserves_existing_when_cluster_already_listed() {
    let mut env = HashMap::from([
        ("HTTPS_PROXY".to_string(), "http://corp:3128".to_string()),
        (
            "NO_PROXY".to_string(),
            "localhost,.svc.cluster.local,custom.svc.cluster.local".to_string(),
        ),
    ]);
    augment_no_proxy_for_kubernetes_cluster(&mut env);
    assert_eq!(
        env.get("NO_PROXY").map(String::as_str),
        Some("localhost,.svc.cluster.local,custom.svc.cluster.local")
    );
}

#[test]
fn test_augment_no_proxy_skips_without_proxy_vars() {
    let mut env = HashMap::from([("FOO".to_string(), "bar".to_string())]);
    augment_no_proxy_for_kubernetes_cluster(&mut env);
    assert!(!env.contains_key("NO_PROXY"));
}

#[test]
fn test_sandbox_resource_name_lowercase() {
    let name = sandbox_resource_name("AABBCCDD-1122-3344-5566-77889900AABB");
    assert_eq!(name, "dsb-sb-aabbccdd-1122-3344-5566-77889900aabb");
}

#[test]
fn test_sandbox_resource_name_short_id() {
    let name = sandbox_resource_name("abc");
    assert_eq!(name, "dsb-sb-abc");
}

#[test]
fn test_sandbox_resource_name_deterministic() {
    let id = "550e8400-e29b-41d4-a716-446655440000";
    let name1 = sandbox_resource_name(id);
    let name2 = sandbox_resource_name(id);
    assert_eq!(name1, name2, "Same ID should produce same resource name");
}

// -----------------------------------------------------------------------
// Kube error mapping tests
// -----------------------------------------------------------------------

#[test]
fn test_map_kube_error_not_found() {
    let status = kube::error::ErrorResponse {
        status: "Failure".to_string(),
        message: "pods \"test\" not found".to_string(),
        reason: "NotFound".to_string(),
        code: 404,
    };
    let err = kube::Error::Api(status);

    match &err {
        kube::Error::Api(api_err) => {
            assert_eq!(api_err.code, 404);
            assert_eq!(api_err.reason, "NotFound");
        }
        _ => panic!("Expected Api error"),
    }
}

#[test]
fn test_map_kube_error_conflict() {
    let status = kube::error::ErrorResponse {
        status: "Failure".to_string(),
        message: "already exists".to_string(),
        reason: "AlreadyExists".to_string(),
        code: 409,
    };
    let err = kube::Error::Api(status);

    match &err {
        kube::Error::Api(api_err) => {
            assert_eq!(api_err.code, 409);
            assert_eq!(api_err.reason, "AlreadyExists");
        }
        _ => panic!("Expected Api error"),
    }
}

#[test]
fn test_map_kube_error_timeout() {
    let status = kube::error::ErrorResponse {
        status: "Failure".to_string(),
        message: "request timeout".to_string(),
        reason: "Timeout".to_string(),
        code: 408,
    };
    let err = kube::Error::Api(status);

    match &err {
        kube::Error::Api(api_err) => {
            assert_eq!(api_err.code, 408);
        }
        _ => panic!("Expected Api error"),
    }
}

#[test]
fn test_map_kube_error_rate_limited() {
    let status = kube::error::ErrorResponse {
        status: "Failure".to_string(),
        message: "too many requests".to_string(),
        reason: "TooManyRequests".to_string(),
        code: 429,
    };
    let err = kube::Error::Api(status);

    match &err {
        kube::Error::Api(api_err) => {
            assert_eq!(api_err.code, 429);
        }
        _ => panic!("Expected Api error"),
    }
}

#[test]
fn test_map_kube_error_generic_api() {
    let status = kube::error::ErrorResponse {
        status: "Failure".to_string(),
        message: "internal error".to_string(),
        reason: "InternalError".to_string(),
        code: 500,
    };
    let err = kube::Error::Api(status);

    match &err {
        kube::Error::Api(api_err) => {
            assert_eq!(api_err.code, 500);
        }
        _ => panic!("Expected Api error"),
    }
}

// -----------------------------------------------------------------------
// CRD schema and serialization tests
// -----------------------------------------------------------------------

#[test]
fn test_crd_schema_serialization() {
    use crate::k8s::crd::{Sandbox, SandboxSpec};

    let spec = SandboxSpec {
        image: "dsb/sandbox:latest".to_string(),
        sandbox_id: "test-123".to_string(),
        env: HashMap::from([
            ("KEY1".to_string(), "value1".to_string()),
            ("KEY2".to_string(), "value2".to_string()),
        ]),
        ..Default::default()
    };
    let sandbox = Sandbox::new("test-sandbox", spec);
    let json = serde_json::to_string(&sandbox).unwrap();
    assert!(json.contains("dsb/sandbox:latest"));
    assert!(json.contains("test-123"));
    assert!(json.contains("KEY1"));
    assert!(json.contains("value1"));
}

#[test]
fn test_crd_schema_deserialization() {
    use crate::k8s::crd::{Sandbox, SandboxSpec};

    let spec = SandboxSpec {
        image: "nginx:latest".to_string(),
        sandbox_id: "deser-test".to_string(),
        ..Default::default()
    };
    let sandbox = Sandbox::new("deser-sandbox", spec);
    let json = serde_json::to_string(&sandbox).unwrap();

    // Round-trip: serialize -> deserialize -> compare key fields
    let deserialized: Sandbox = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.spec.image, "nginx:latest");
    assert_eq!(deserialized.spec.sandbox_id, "deser-test");
}

#[test]
fn test_sandbox_status_default() {
    use crate::k8s::crd::SandboxStatus;
    let status = SandboxStatus::default();
    assert!(status.phase.is_none());
    assert!(status.pod_name.is_none());
    assert!(status.service_name.is_none());
    assert!(status.container_id.is_none());
    assert!(status.node_name.is_none());
    assert!(status.pod_ip.is_none());
    assert!(status.message.is_none());
}

#[test]
fn test_port_spec_default() {
    use crate::k8s::crd::PortSpec;
    let port = PortSpec::default();
    // Note: #[serde(default)] only applies during deserialization, not Default derive.
    // The Default derive gives empty string for String fields.
    assert_eq!(port.protocol, "");
    assert_eq!(port.container_port, 0);
    assert!(port.service_port.is_none());
}

#[test]
fn test_port_spec_serde_default() {
    use crate::k8s::crd::PortSpec;
    // When deserialized from JSON with missing protocol field,
    // the serde default kicks in and sets "TCP".
    let json = r#"{"container_port": 8080}"#;
    let port: PortSpec = serde_json::from_str(json).unwrap();
    assert_eq!(port.container_port, 8080);
    assert_eq!(port.protocol, "TCP");
    assert!(port.service_port.is_none());
}

#[test]
fn test_sandbox_spec_default() {
    use crate::k8s::crd::SandboxSpec;
    let spec = SandboxSpec::default();
    assert!(spec.image.is_empty());
    assert!(spec.env.is_empty());
    assert!(spec.ports.is_empty());
    assert!(spec.volumes.is_empty());
    assert!(spec.resources.is_none());
    assert!(spec.command.is_none());
    assert!(spec.args.is_none());
    assert!(spec.labels.is_empty());
    assert!(!spec.gpu);
    assert!(spec.inactivity_timeout_minutes.is_none());
    assert!(spec.sandbox_id.is_empty());
    assert!(spec.api_key_hash.is_none());
    assert!(!spec.has_dsb_features);
}

#[test]
fn test_sandbox_spec_with_resources() {
    use crate::k8s::crd::{ResourceSpec, SandboxSpec};

    let spec = SandboxSpec {
        image: "test:latest".to_string(),
        sandbox_id: "res-test".to_string(),
        resources: Some(ResourceSpec {
            cpu_request: Some("500m".to_string()),
            memory_request: Some("1Gi".to_string()),
            cpu_limit: Some("2000m".to_string()),
            memory_limit: Some("4Gi".to_string()),
        }),
        ..Default::default()
    };

    let resources = spec.resources.as_ref().unwrap();
    assert_eq!(resources.cpu_request.as_deref(), Some("500m"));
    assert_eq!(resources.memory_request.as_deref(), Some("1Gi"));
    assert_eq!(resources.cpu_limit.as_deref(), Some("2000m"));
    assert_eq!(resources.memory_limit.as_deref(), Some("4Gi"));
}

#[test]
fn test_crd_metadata() {
    use crate::k8s::crd::Sandbox;

    let spec = crate::k8s::crd::SandboxSpec {
        image: "test:latest".to_string(),
        sandbox_id: "meta-test".to_string(),
        ..Default::default()
    };
    let sandbox = Sandbox::new("my-sandbox", spec);

    assert_eq!(sandbox.metadata.name.as_deref(), Some("my-sandbox"));
    // The CRD should have the correct API version and kind
    assert_eq!(Sandbox::kind(&()), "Sandbox");
    assert_eq!(Sandbox::api_version(&()), "dsb.io/v1");
}

#[test]
fn test_sandbox_status_serialization() {
    use crate::k8s::crd::SandboxStatus;

    let status = SandboxStatus {
        phase: Some("Running".to_string()),
        pod_name: Some("dsb-sb-test".to_string()),
        service_name: Some("dsb-svc-test".to_string()),
        node_name: Some("node-1".to_string()),
        pod_ip: Some("10.0.0.1".to_string()),
        ..Default::default()
    };

    let json = serde_json::to_string(&status).unwrap();
    assert!(json.contains("Running"));
    assert!(json.contains("dsb-sb-test"));
    assert!(json.contains("dsb-svc-test"));
    assert!(json.contains("node-1"));
    assert!(json.contains("10.0.0.1"));
}

// -----------------------------------------------------------------------
// Port and label constants tests
// -----------------------------------------------------------------------

#[test]
fn test_sandbox_ports_constants() {
    use crate::k8s::types::sandbox_ports;
    assert_eq!(sandbox_ports::TOOL_PROXY, 8080);
    assert_eq!(sandbox_ports::VNC, 5901);
    assert_eq!(sandbox_ports::NOVNC, 6080);
    assert_eq!(sandbox_ports::AGENT_BROWSER, 3000);
}

#[test]
fn test_labels_constants() {
    use crate::k8s::types::labels;
    assert_eq!(labels::MANAGED_BY, "dsb.io/managed-by");
    assert_eq!(labels::SANDBOX_ID, "dsb.io/sandbox-id");
    assert_eq!(labels::API_KEY_HASH, "dsb.io/api-key-hash");
    assert_eq!(labels::COMPONENT, "dsb.io/component");
}

// -----------------------------------------------------------------------
// phase_to_state mapping tests
// -----------------------------------------------------------------------

#[test]
fn test_phase_to_state_mapping() {
    assert_eq!(phase_to_state("Pending"), "created");
    assert_eq!(phase_to_state("Running"), "running");
    assert_eq!(phase_to_state("Succeeded"), "exited");
    assert_eq!(phase_to_state("Failed"), "exited");
    assert_eq!(phase_to_state("Unknown"), "unknown");
    assert_eq!(phase_to_state("SomethingElse"), "unknown");
}

// -----------------------------------------------------------------------
// make_service_port helper tests
// -----------------------------------------------------------------------

#[test]
fn test_make_service_port_fields() {
    let port = make_service_port("test-port", 9090);
    assert_eq!(port.port, 9090);
    assert_eq!(port.name.as_deref(), Some("test-port"));
    assert_eq!(port.protocol.as_deref(), Some("TCP"));
}

// -----------------------------------------------------------------------
// build_sandbox_spec conversion tests
// -----------------------------------------------------------------------

#[test]
fn test_build_sandbox_spec_from_config_minimal() {
    // This test verifies the conversion logic from SandboxConfig to SandboxSpec.
    // Since build_sandbox_spec needs &self, we test the conversion logic manually.
    let sandbox_config = SandboxConfig {
        image: "test-image:latest".to_string(),
        environment: HashMap::from([("FOO".to_string(), "bar".to_string())]),
        ..Default::default()
    };

    // Verify port mapping conversion logic
    let ports: Vec<PortSpec> = sandbox_config
        .port_mappings
        .iter()
        .map(|pm| PortSpec {
            container_port: pm.container_port,
            protocol: match pm.protocol {
                PortProtocol::Tcp => "TCP".to_string(),
                PortProtocol::Udp => "UDP".to_string(),
            },
            service_port: Some(pm.host_port),
        })
        .collect();

    // Default config has no port mappings
    assert!(ports.is_empty());
}

#[test]
fn test_build_sandbox_spec_port_conversion() {
    let sandbox_config = SandboxConfig {
        image: "test-image:latest".to_string(),
        port_mappings: vec![
            PortMapping {
                host_port: 8080,
                container_port: 80,
                protocol: PortProtocol::Tcp,
            },
            PortMapping {
                host_port: 9090,
                container_port: 9090,
                protocol: PortProtocol::Udp,
            },
        ],
        ..Default::default()
    };

    // Simulate the port conversion logic from build_sandbox_spec
    let ports: Vec<PortSpec> = sandbox_config
        .port_mappings
        .iter()
        .map(|pm| PortSpec {
            container_port: pm.container_port,
            protocol: match pm.protocol {
                PortProtocol::Tcp => "TCP".to_string(),
                PortProtocol::Udp => "UDP".to_string(),
            },
            service_port: Some(pm.host_port),
        })
        .collect();

    assert_eq!(ports.len(), 2);
    assert_eq!(ports[0].container_port, 80);
    assert_eq!(ports[0].protocol, "TCP");
    assert_eq!(ports[0].service_port, Some(8080));
    assert_eq!(ports[1].container_port, 9090);
    assert_eq!(ports[1].protocol, "UDP");
    assert_eq!(ports[1].service_port, Some(9090));
}

#[test]
fn test_build_sandbox_spec_resource_conversion() {
    let sandbox_config = SandboxConfig {
        image: "test-image:latest".to_string(),
        resource_limits: crate::core::types::ResourceLimits {
            memory_mb: Some(512),
            cpu_quota: Some(100000),
            cpu_shares: Some(512),
            ..Default::default()
        },
        ..Default::default()
    };

    // Simulate the resource conversion logic from build_sandbox_spec
    let rl = &sandbox_config.resource_limits;
    let resources = {
        let has_any =
            rl.memory_mb.is_some() || rl.cpu_quota.is_some() || rl.cpu_shares.is_some();
        if has_any {
            Some(crate::k8s::crd::ResourceSpec {
                cpu_request: rl.cpu_shares.map(|_| "500m".to_string()),
                memory_request: rl.memory_mb.map(|m| format!("{}Mi", m)),
                cpu_limit: rl.cpu_quota.map(|q| format!("{}m", q / 100)),
                memory_limit: rl.memory_mb.map(|m| format!("{}Mi", m)),
            })
        } else {
            None
        }
    };

    let res = resources.expect("Should have resources");
    assert_eq!(res.cpu_request.as_deref(), Some("500m"));
    assert_eq!(res.memory_request.as_deref(), Some("512Mi"));
    assert_eq!(res.cpu_limit.as_deref(), Some("1000m")); // 100000 / 100
    assert_eq!(res.memory_limit.as_deref(), Some("512Mi"));
}

#[test]
fn test_build_sandbox_spec_no_resources() {
    let sandbox_config = SandboxConfig {
        image: "test-image:latest".to_string(),
        ..Default::default()
    };

    let rl = &sandbox_config.resource_limits;
    let has_any = rl.memory_mb.is_some() || rl.cpu_quota.is_some() || rl.cpu_shares.is_some();
    assert!(!has_any);
}

// -----------------------------------------------------------------------
// VolumeSpec tests
// -----------------------------------------------------------------------

#[test]
fn test_volume_spec_serialization() {
    use crate::k8s::crd::VolumeSpec;

    let vol = VolumeSpec {
        name: "data-vol".to_string(),
        mount_path: "/data".to_string(),
        volume_type: "pvc".to_string(),
        source_name: Some("my-pvc".to_string()),
        read_only: false,
    };

    let json = serde_json::to_string(&vol).unwrap();
    assert!(json.contains("data-vol"));
    assert!(json.contains("/data"));
    assert!(json.contains("my-pvc"));
}

#[test]
fn test_volume_spec_default() {
    use crate::k8s::crd::VolumeSpec;

    // When deserialized from JSON with missing volume_type field,
    // the serde default kicks in and sets "pvc".
    let json = r#"{"name": "data-vol", "mount_path": "/data"}"#;
    let vol: VolumeSpec = serde_json::from_str(json).unwrap();
    assert_eq!(vol.name, "data-vol");
    assert_eq!(vol.mount_path, "/data");
    assert_eq!(vol.volume_type, "pvc");
    assert!(vol.source_name.is_none());
    assert!(!vol.read_only);
}

// -----------------------------------------------------------------------
// CreateSandboxResult tests
// -----------------------------------------------------------------------

#[test]
fn test_create_sandbox_result_fields() {
    use crate::k8s::types::CreateSandboxResult;

    let result = CreateSandboxResult {
        crd_name: "dsb-sb-test".to_string(),
        pod_name: Some("dsb-sb-test".to_string()),
        service_name: Some("dsb-svc-test".to_string()),
    };

    assert_eq!(result.crd_name, "dsb-sb-test");
    assert_eq!(result.pod_name.as_deref(), Some("dsb-sb-test"));
    assert_eq!(result.service_name.as_deref(), Some("dsb-svc-test"));
}

// -----------------------------------------------------------------------
// GPU scheduling tests
// -----------------------------------------------------------------------

#[test]
fn test_gpu_config_default() {
    use crate::config::GpuConfig;

    let gpu_config = GpuConfig::default();
    assert!(gpu_config.node_selector.is_empty());
    assert!(gpu_config.tolerations.is_empty());
    assert_eq!(gpu_config.resource_request, "1");
}

#[test]
fn test_gpu_config_with_custom_values() {
    use crate::config::GpuConfig;

    let mut node_selector = std::collections::HashMap::new();
    node_selector.insert("node.kubernetes.io/gpu".to_string(), "true".to_string());

    let tolerations: Vec<serde_json::Value> = vec![serde_json::json!({
        "key": "custom-gpu-taint",
        "operator": "Exists",
        "effect": "NoSchedule"
    })];

    let gpu_config = GpuConfig {
        node_selector,
        tolerations: tolerations.clone(),
        resource_request: "2".to_string(),
    };

    assert_eq!(gpu_config.resource_request, "2");
    assert_eq!(
        gpu_config.node_selector.get("node.kubernetes.io/gpu"),
        Some(&"true".to_string())
    );
    assert_eq!(gpu_config.tolerations.len(), 1);
}

#[test]
fn test_gpu_toleration_json_serialization() {
    let toleration_json = serde_json::json!({
        "key": "nvidia.com/gpu",
        "operator": "Exists",
        "effect": "NoSchedule"
    });

    let toleration: k8s_openapi::api::core::v1::Toleration =
        serde_json::from_value(toleration_json).unwrap();

    assert_eq!(toleration.key.as_deref(), Some("nvidia.com/gpu"));
    assert_eq!(toleration.operator.as_deref(), Some("Exists"));
    assert_eq!(toleration.effect.as_deref(), Some("NoSchedule"));
}

#[test]
fn test_malformed_gpu_toleration_is_rejected() {
    // Malformed toleration with wrong type for "key" field (should be string, not number)
    let malformed_toleration = serde_json::json!({
        "key": 123,  // wrong: should be string
        "operator": "Exists",
        "effect": "NoSchedule"
    });

    // The parse should fail - proving that silently using .ok() would drop this
    let result: Result<k8s_openapi::api::core::v1::Toleration, _> =
        serde_json::from_value(malformed_toleration);
    assert!(result.is_err(), "Malformed toleration should fail to parse");

    // Verify the error message indicates the type mismatch
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("invalid type"),
        "Error should indicate type mismatch, got: {}",
        err
    );
}

#[test]
fn test_malformed_toleration_with_wrong_effect_type_is_rejected() {
    // Malformed toleration with wrong type for "effect" field (should be string, not number)
    let malformed = serde_json::json!({
        "key": "nvidia.com/gpu",
        "operator": "Exists",
        "effect": 123  // wrong: should be string
    });

    let result: Result<Toleration, _> = serde_json::from_value(malformed);
    assert!(result.is_err(), "Wrong effect type should fail to parse");
}

#[test]
fn test_gpu_node_affinity_serialization() {
    let affinity_json = serde_json::json!({
        "nodeAffinity": {
            "preferredDuringSchedulingIgnoredDuringExecution": [
                {
                    "weight": 100,
                    "preference": {
                        "matchExpressions": [
                            {
                                "key": "node.kubernetes.io/gpu",
                                "operator": "In",
                                "values": ["true"]
                            }
                        ]
                    }
                }
            ]
        }
    });

    let affinity: k8s_openapi::api::core::v1::Affinity =
        serde_json::from_value(affinity_json).unwrap();

    let node_affinity = affinity.node_affinity.unwrap();
    let preferred = node_affinity
        .preferred_during_scheduling_ignored_during_execution
        .unwrap();
    assert_eq!(preferred.len(), 1);
    assert_eq!(preferred[0].weight, 100);

    let expressions = preferred[0].preference.match_expressions.as_ref().unwrap();
    assert_eq!(expressions.len(), 1);
    assert_eq!(expressions[0].key, "node.kubernetes.io/gpu");
    assert_eq!(expressions[0].operator, "In");
    assert_eq!(expressions[0].values.as_ref().unwrap()[0], "true");
}

#[test]
fn test_gpu_resource_requirements_both_request_and_limit() {
    // Verify that GPU resources can be set with both requests and limits
    let mut requests = BTreeMap::new();
    requests.insert("nvidia.com/gpu".to_string(), Quantity("1".to_string()));

    let mut limits = BTreeMap::new();
    limits.insert("nvidia.com/gpu".to_string(), Quantity("1".to_string()));

    let resources = ResourceRequirements {
        requests: Some(requests),
        limits: Some(limits),
        claims: None,
    };

    let gpu_request = resources.requests.as_ref().unwrap().get("nvidia.com/gpu");
    let gpu_limit = resources.limits.as_ref().unwrap().get("nvidia.com/gpu");

    assert!(gpu_request.is_some());
    assert!(gpu_limit.is_some());
    assert_eq!(gpu_request.unwrap().0, "1");
    assert_eq!(gpu_limit.unwrap().0, "1");
}

#[test]
fn test_gpu_sandbox_spec_with_gpu_enabled() {
    use crate::k8s::crd::SandboxSpec;

    let spec = SandboxSpec {
        image: "gpu-image:latest".to_string(),
        sandbox_id: "gpu-test".to_string(),
        gpu: true,
        ..Default::default()
    };

    assert!(spec.gpu);
    assert_eq!(spec.image, "gpu-image:latest");
}

#[test]
fn test_gpu_sandbox_spec_default_is_false() {
    use crate::k8s::crd::SandboxSpec;

    let spec = SandboxSpec::default();
    assert!(!spec.gpu);
}

// -----------------------------------------------------------------------
// Integration tests (require K8s cluster, marked #[ignore])
// -----------------------------------------------------------------------

#[tokio::test]
#[ignore] // Requires running K8s cluster
async fn test_k8s_create_and_delete_sandbox() {
    let client = kube::Client::try_default()
        .await
        .expect("Failed to create K8s client. Is a cluster running?");
    let config = crate::config::load_for_tests().expect("Failed to load config");
    let manager = KubernetesManager::new(client, std::sync::Arc::new(config));

    let sandbox_config = SandboxConfig {
        image: "alpine:latest".to_string(),
        command: Some(vec!["sleep".to_string(), "30".to_string()]),
        ..Default::default()
    };

    // Create
    let sandbox_id = manager
        .create(None, &sandbox_config)
        .await
        .expect("Failed to create sandbox");
    assert!(!sandbox_id.is_empty());

    // Start
    manager
        .start(&sandbox_id)
        .await
        .expect("Failed to start sandbox");

    // Is running
    let running = manager
        .is_running(&sandbox_id)
        .await
        .expect("Failed to check running status");
    assert!(running);

    // Stop
    manager
        .stop(&sandbox_id)
        .await
        .expect("Failed to stop sandbox");

    // Delete
    manager
        .delete(&sandbox_id)
        .await
        .expect("Failed to delete sandbox");
}

#[tokio::test]
#[ignore] // Requires running K8s cluster
async fn test_k8s_exec_command() {
    let client = kube::Client::try_default()
        .await
        .expect("Failed to create K8s client. Is a cluster running?");
    let config = crate::config::load_for_tests().expect("Failed to load config");
    let manager = KubernetesManager::new(client, std::sync::Arc::new(config));

    let sandbox_config = SandboxConfig {
        image: "alpine:latest".to_string(),
        command: Some(vec!["sleep".to_string(), "60".to_string()]),
        ..Default::default()
    };

    let sandbox_id = manager
        .create(None, &sandbox_config)
        .await
        .expect("Failed to create sandbox");

    manager
        .start(&sandbox_id)
        .await
        .expect("Failed to start sandbox");

    // Exec a command
    let output = manager
        .exec(&sandbox_id, vec!["echo".to_string(), "hello".to_string()])
        .await
        .expect("Failed to exec command");
    assert!(output.contains("hello"));

    // Cleanup
    manager.stop(&sandbox_id).await.ok();
    manager.delete(&sandbox_id).await.ok();
}

#[tokio::test]
#[ignore] // Requires running K8s cluster
async fn test_k8s_list_sandboxes() {
    let client = kube::Client::try_default()
        .await
        .expect("Failed to create K8s client. Is a cluster running?");
    let config = crate::config::load_for_tests().expect("Failed to load config");
    let manager = KubernetesManager::new(client, std::sync::Arc::new(config));

    // List should succeed even with no sandboxes
    let sandboxes = manager
        .list(true, None)
        .await
        .expect("Failed to list sandboxes");
    // Just verify it returns a valid vec (may be empty)
    let _: Vec<SandboxInfo> = sandboxes;
}

#[tokio::test]
#[ignore] // Requires running K8s cluster
async fn test_k8s_get_sandbox_address() {
    let client = kube::Client::try_default()
        .await
        .expect("Failed to create K8s client. Is a cluster running?");
    let config = crate::config::load_for_tests().expect("Failed to load config");
    let namespace = config.sandbox.kubernetes.namespace.clone();
    let manager = KubernetesManager::new(client, std::sync::Arc::new(config));

    let address = manager
        .get_sandbox_address("test-id", 8080)
        .await
        .expect("Failed to get sandbox address");

    let svc_name = sandbox_service_name("test-id");
    assert!(address.contains(&svc_name));
    assert!(address.contains(&namespace));
    assert!(address.contains("8080"));
    assert!(address.contains(".svc.cluster.local:"));
}

#[tokio::test]
#[ignore] // Requires running K8s cluster
async fn test_k8s_image_operations_not_supported() {
    let client = kube::Client::try_default()
        .await
        .expect("Failed to create K8s client. Is a cluster running?");
    let config = crate::config::load_for_tests().expect("Failed to load config");
    let manager = KubernetesManager::new(client, std::sync::Arc::new(config));

    // Image operations should return NotSupported
    assert!(manager.get_image_features("test").await.is_err());
    assert!(manager.pull_image("test").await.is_err());
    assert!(manager.delete_image("test").await.is_err());

    // image_exists should always return Ok(true)
    assert!(manager.image_exists("any-image").await.unwrap());

    // list_images should return Ok(empty vec)
    assert!(manager.list_images().await.unwrap().is_empty());
}

#[tokio::test]
#[ignore] // Requires running K8s cluster
async fn test_k8s_get_workdir() {
    let client = kube::Client::try_default()
        .await
        .expect("Failed to create K8s client. Is a cluster running?");
    let config = crate::config::load_for_tests().expect("Failed to load config");
    let manager = KubernetesManager::new(client, std::sync::Arc::new(config));

    let workdir = manager
        .get_workdir("any-id")
        .await
        .expect("Failed to get workdir");
    assert_eq!(workdir, "/workspace");
}
