use crate::core::types::{
    ActivityTracking, PortMapping, PullPolicy, ResourceLimits, Sandbox, SandboxConfig,
    SandboxState, VolumeMount,
};
use crate::db::store::SerializedSandboxFields;
use serde_json;

/// Serializes sandbox fields to JSONB for database storage.
///
/// This helper function reduces code duplication by handling all JSON serialization
/// in one place. It's used by both create and update operations.
///
/// # Arguments
///
/// * `sandbox` - The sandbox to serialize
///
/// # Returns
///
/// * `Ok(SerializedSandboxFields)` - All fields serialized successfully
/// * `Err(...)` - JSON serialization error
pub(crate) fn serialize_sandbox_fields(
    sandbox: &Sandbox,
) -> Result<SerializedSandboxFields, Box<dyn std::error::Error + Send + Sync>> {
    Ok(SerializedSandboxFields {
        environment: serde_json::to_value(&sandbox.config.environment)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        port_mappings: serde_json::to_value(&sandbox.config.port_mappings)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        resource_limits: serde_json::to_value(&sandbox.config.resource_limits)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        volumes: serde_json::to_value(&sandbox.config.volumes)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        volume_mounts: serde_json::to_value(&sandbox.volume_mounts)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        command: serde_json::to_value(&sandbox.config.command)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
        features: serde_json::to_value(&sandbox.config.features)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?,
    })
}

/// Deserialized JSONB fields from a database row.
///
/// This struct holds all the deserialized configuration fields, making it easier
/// to pass them between functions during the row-to-sandbox conversion process.
pub(crate) struct DeserializedSandboxFields {
    environment: std::collections::HashMap<String, String>,
    port_mappings: Vec<PortMapping>,
    resource_limits: ResourceLimits,
    volumes: Vec<VolumeMount>,
    volume_mounts: Vec<VolumeMount>,
    command: Option<Vec<String>>,
    features: Vec<String>,
}

/// Parses PullPolicy from a database row.
///
/// # Arguments
///
/// * `row` - The database row to parse from
///
/// # Returns
///
/// * `Ok(PullPolicy)` - Successfully parsed enum
/// * `Err(String)` - Invalid pull_policy value
pub(crate) fn parse_pull_policy(row: &tokio_postgres::Row) -> Result<PullPolicy, String> {
    let pull_policy_str: String = row.try_get("pull_policy").map_err(|e| e.to_string())?;
    match pull_policy_str.as_str() {
        "always" => Ok(PullPolicy::Always),
        "missing" => Ok(PullPolicy::Missing),
        "never" => Ok(PullPolicy::Never),
        _ => Err(format!("Invalid pull_policy: {}", pull_policy_str)),
    }
}

/// Parses SandboxState from a database row.
///
/// # Arguments
///
/// * `row` - The database row to parse from
///
/// # Returns
///
/// * `Ok(SandboxState)` - Successfully parsed enum
/// * `Err(String)` - Invalid state value
pub(crate) fn parse_sandbox_state(row: &tokio_postgres::Row) -> Result<SandboxState, String> {
    let state_str: String = row.try_get("state").map_err(|e| e.to_string())?;
    match state_str.as_str() {
        "creating" => Ok(SandboxState::Creating),
        "created" => Ok(SandboxState::Created),
        "starting" => Ok(SandboxState::Starting),
        "running" => Ok(SandboxState::Running),
        "stopped" => Ok(SandboxState::Stopped),
        "error" => Ok(SandboxState::Error),
        "destroying" => Ok(SandboxState::Destroying),
        "destroyed" => Ok(SandboxState::Destroyed),
        _ => Err(format!("Invalid state: {}", state_str)),
    }
}

/// Deserializes all JSONB fields from a database row.
///
/// This helper function extracts and deserializes all JSONB columns,
/// providing clear error messages for each field if deserialization fails.
///
/// # Arguments
///
/// * `row` - The database row to deserialize from
///
/// # Returns
///
/// * `Ok(DeserializedSandboxFields)` - All fields deserialized successfully
/// * `Err(String)` - Deserialization error with context
pub(crate) fn deserialize_jsonb_fields(
    row: &tokio_postgres::Row,
) -> Result<DeserializedSandboxFields, String> {
    // Deserialize environment
    let environment: serde_json::Value = row.try_get("environment").map_err(|e| e.to_string())?;
    let environment = serde_json::from_value(environment)
        .map_err(|e| format!("Failed to deserialize environment: {}", e))?;

    // Deserialize port_mappings
    let port_mappings: serde_json::Value =
        row.try_get("port_mappings").map_err(|e| e.to_string())?;
    let port_mappings = serde_json::from_value(port_mappings)
        .map_err(|e| format!("Failed to deserialize port_mappings: {}", e))?;

    // Deserialize resource_limits
    let resource_limits: serde_json::Value =
        row.try_get("resource_limits").map_err(|e| e.to_string())?;
    let resource_limits = serde_json::from_value(resource_limits)
        .map_err(|e| format!("Failed to deserialize resource_limits: {}", e))?;

    // Deserialize volumes
    let volumes: serde_json::Value = row.try_get("volumes").map_err(|e| e.to_string())?;
    let volumes = serde_json::from_value(volumes)
        .map_err(|e| format!("Failed to deserialize volumes: {}", e))?;

    // Deserialize volume_mounts
    let volume_mounts: serde_json::Value =
        row.try_get("volume_mounts").map_err(|e| e.to_string())?;
    let volume_mounts = serde_json::from_value(volume_mounts)
        .map_err(|e| format!("Failed to deserialize volume_mounts: {}", e))?;

    // Deserialize command
    // Note: After migration, the command column always exists. We use unwrap_or(Null)
    // for backward compatibility with any pre-migration database state, though in
    // practice all databases should have been migrated.
    let command: serde_json::Value = row.try_get("command").unwrap_or(serde_json::Value::Null);
    let command = if command.is_null() {
        None
    } else {
        Some(
            serde_json::from_value(command)
                .map_err(|e| format!("Failed to deserialize command: {}", e))?,
        )
    };

    // Deserialize features
    let features: serde_json::Value = row.try_get("features").unwrap_or(serde_json::Value::Null);
    let features = if features.is_null() {
        vec![]
    } else {
        serde_json::from_value(features)
            .map_err(|e| format!("Failed to deserialize features: {}", e))?
    };

    Ok(DeserializedSandboxFields {
        environment,
        port_mappings,
        resource_limits,
        volumes,
        volume_mounts,
        command,
        features,
    })
}

/// Parses ActivityTracking from a database row.
///
/// # Arguments
///
/// * `row` - The database row to parse from
///
/// # Returns
///
/// * `Ok(ActivityTracking)` - Successfully parsed activity tracking
/// * `Err(String)` - Error parsing activity fields
pub(crate) fn parse_activity_tracking(row: &tokio_postgres::Row) -> Result<ActivityTracking, String> {
    let count: i64 = row.try_get("activity_count").map_err(|e| e.to_string())?;

    Ok(ActivityTracking {
        last_api_activity: row
            .try_get("last_api_activity")
            .map_err(|e| e.to_string())?,
        last_container_activity: row
            .try_get("last_container_activity")
            .map_err(|e| e.to_string())?,
        activity_count: count as u64,
    })
}

/// Helper function to convert a database row to Sandbox struct.
///
/// This function orchestrates the conversion by delegating to specialized helpers:
/// - Enum parsing (pull_policy, state)
/// - JSONB deserialization
/// - Activity tracking parsing
/// - Final sandbox assembly
///
/// # Arguments
///
/// * `row` - The database row to convert
///
/// # Returns
///
/// * `Ok(Sandbox)` - Successfully converted sandbox
/// * `Err(String)` - Conversion error with context
pub(crate) fn row_to_sandbox(row: tokio_postgres::Row) -> Result<Sandbox, String> {
    // Parse enums using helper functions
    let pull_policy = parse_pull_policy(&row)?;
    let state = parse_sandbox_state(&row)?;

    // Deserialize all JSONB fields
    let jsonb_fields = deserialize_jsonb_fields(&row)?;

    // Parse activity tracking
    let activity = parse_activity_tracking(&row)?;

    // Extract simple fields
    let container_id: Option<String> = row.try_get("container_id").map_err(|e| e.to_string())?;
    let error_message: Option<String> = row.try_get("error_message").map_err(|e| e.to_string())?;
    let enable_all_features: bool = row.try_get("enable_all_features").unwrap_or(false);

    // Read vnc_resolution directly from TEXT column
    // Column is nullable in the database, so NULL is a legitimate value
    let vnc_resolution: Option<String> = row
        .try_get::<_, Option<String>>("vnc_resolution")
        .map_err(|e| e.to_string())?;

    // Convert i64 back to u64
    let timeout: Option<i64> = row
        .try_get("inactivity_timeout_minutes")
        .map_err(|e| e.to_string())?;
    let timeout = timeout.map(|v| v as u64);

    // Assemble final Sandbox struct
    Ok(Sandbox {
        id: row.try_get("id").map_err(|e| e.to_string())?,
        config: SandboxConfig {
            image: row.try_get("image").map_err(|e| e.to_string())?,
            name: row.try_get("name").map_err(|e| e.to_string())?,
            environment: jsonb_fields.environment,
            port_mappings: jsonb_fields.port_mappings,
            exposed_ports: vec![], // New field
            resource_limits: jsonb_fields.resource_limits,
            volumes: jsonb_fields.volumes,
            command: jsonb_fields.command,
            inactivity_timeout_minutes: timeout,
            pull_policy,
            features: jsonb_fields.features,
            enable_all_features,
            vnc_resolution,
        },
        state,
        container_id,
        created_at: row.try_get("created_at").map_err(|e| e.to_string())?,
        updated_at: row.try_get("updated_at").map_err(|e| e.to_string())?,
        error_message,
        volume_mounts: jsonb_fields.volume_mounts,
        activity,
        inactivity_timeout_minutes: timeout,
        // Column is nullable in the database, so NULL is a legitimate value
        deleted_at: row
            .try_get::<_, Option<chrono::DateTime<chrono::Utc>>>("deleted_at")
            .map_err(|e| e.to_string())?,
        // Column is nullable in the database, so NULL is a legitimate value
        deleted_by: row
            .try_get::<_, Option<String>>("deleted_by")
            .map_err(|e| e.to_string())?,
        // Column is nullable in the database, so NULL is a legitimate value
        api_key_id: row
            .try_get::<_, Option<uuid::Uuid>>("api_key_id")
            .map_err(|e| e.to_string())?,
    })
}
