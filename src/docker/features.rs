// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2025-2026 Tom Xie
//! # Feature Detection from Docker Images
//!
//! This module provides functionality to inspect Docker images and extract
//! feature metadata from labels.

use crate::core::features::{
    build_feature_profile, FeatureProfile, FeatureSelection, ImageFeatureLabel, DSB_FEATURES_LABEL,
};
use bollard::Docker;
use std::sync::Arc;
use tracing::{debug, warn};

/// Feature profile detector
///
/// This struct is responsible for inspecting Docker images and extracting
/// feature metadata from the `com.dsb.features` label.
pub struct FeatureDetector {
    docker: Arc<Docker>,
}

impl FeatureDetector {
    /// Create a new feature detector
    ///
    /// # Arguments
    ///
    /// * `docker` - Bollard Docker client
    pub fn new(docker: Arc<Docker>) -> Self {
        Self { docker }
    }

    /// Detect feature profile from image by inspecting labels
    ///
    /// This method inspects the specified Docker image, extracts the
    /// `com.dsb.features` label, parses it as JSON, and determines which
    /// features should be enabled based on the user's selection.
    ///
    /// # Arguments
    ///
    /// * `image` - Docker image name (e.g., "nginx:latest")
    /// * `feature_selection` - User's feature selection request
    ///
    /// # Returns
    ///
    /// * `Ok(FeatureProfile)` - Detected feature profile (may be empty if no labels)
    /// * `Err(Box<dyn Error>)` - Error inspecting image (not for parsing errors)
    ///
    /// # Behavior
    ///
    /// - If the image doesn't have a `com.dsb.features` label, returns an empty profile
    /// - If the label contains invalid JSON, logs a warning and returns an empty profile
    /// - If the image cannot be inspected, returns an error (fail fast)
    pub async fn detect_from_image(
        &self,
        image: &str,
        feature_selection: &FeatureSelection,
    ) -> Result<FeatureProfile, crate::docker::DockerManagerError> {
        debug!("Detecting features for image: {}", image);

        // Inspect the image
        let inspect = self.docker.inspect_image(image).await?;

        // Get image labels
        let labels = inspect.config.and_then(|c| c.labels).unwrap_or_default();

        // Look for DSB features label
        let features_json = match labels.get(DSB_FEATURES_LABEL) {
            Some(json) => json,
            None => {
                debug!("No {} label found on image {}", DSB_FEATURES_LABEL, image);
                return Ok(FeatureProfile::empty());
            }
        };

        debug!("Found features label: {}", features_json);

        // Parse the JSON label
        let label: ImageFeatureLabel = match serde_json::from_str(features_json) {
            Ok(label) => label,
            Err(e) => {
                warn!("Failed to parse features label from image {}: {}", image, e);
                return Ok(FeatureProfile::empty());
            }
        };

        debug!("Parsed feature label version: {}", label.version);

        let profile = build_feature_profile(label, feature_selection);
        debug!("Enabled features: {:?}", profile.enabled_features);
        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn enabled_features(
        available_features: HashMap<String, crate::core::features::FeatureDefinition>,
        selection: &FeatureSelection,
    ) -> Vec<String> {
        build_feature_profile(
            ImageFeatureLabel {
                version: "1.0".to_string(),
                features: available_features,
                default_command: None,
            },
            selection,
        )
        .enabled_features
    }

    #[test]
    fn test_determine_enabled_features_all_defaults() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            crate::core::features::FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "browser".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Browser".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "optional".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Optional feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: false,
            },
        );

        let selection = FeatureSelection::new(); // No explicit selection
        let enabled = enabled_features(available_features, &selection);

        // Should enable vnc and browser (both have enabled_by_default: true)
        assert_eq!(enabled.len(), 2);
        assert!(enabled.contains(&"vnc".to_string()));
        assert!(enabled.contains(&"browser".to_string()));
        assert!(!enabled.contains(&"optional".to_string()));
    }

    #[test]
    fn test_determine_enabled_features_explicit() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            crate::core::features::FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "browser".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Browser".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "optional".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Optional feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: false,
            },
        );

        let selection = FeatureSelection::new().with_enabled(vec!["optional".to_string()]);
        let enabled = enabled_features(available_features, &selection);

        // Should only enable optional (explicitly requested)
        assert_eq!(enabled.len(), 1);
        assert!(enabled.contains(&"optional".to_string()));
    }

    #[test]
    fn test_determine_enabled_features_disabled() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            crate::core::features::FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "browser".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Browser".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );

        let selection = FeatureSelection::new().with_disabled(vec!["vnc".to_string()]);
        let enabled = enabled_features(available_features, &selection);

        // Should only enable browser (vnc explicitly disabled)
        assert_eq!(enabled.len(), 1);
        assert!(enabled.contains(&"browser".to_string()));
        assert!(!enabled.contains(&"vnc".to_string()));
    }

    #[test]
    fn test_determine_enabled_features_enable_all() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            crate::core::features::FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "browser".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Browser".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "optional".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Optional feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: false,
            },
        );

        let selection = FeatureSelection::new().enable_all();
        let enabled = enabled_features(available_features, &selection);

        // Should enable vnc and browser (both have enabled_by_default: true)
        // But not optional (enabled_by_default: false)
        assert_eq!(enabled.len(), 2);
        assert!(enabled.contains(&"vnc".to_string()));
        assert!(enabled.contains(&"browser".to_string()));
        assert!(!enabled.contains(&"optional".to_string()));
    }

    #[test]
    fn test_determine_enabled_features_sorted_and_deduped() {
        let mut available_features = HashMap::new();
        // Insert features in reverse alphabetical order to test sorting
        available_features.insert(
            "zebra".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Zebra feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "alpha".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Alpha feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "beta".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Beta feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );

        let selection = FeatureSelection::new();
        let enabled = enabled_features(available_features, &selection);

        // Should be sorted alphabetically
        assert_eq!(enabled, vec!["alpha", "beta", "zebra"]);
    }

    #[test]
    fn test_determine_enabled_features_mixed_explicit_and_default() {
        let mut available_features = HashMap::new();
        available_features.insert(
            "vnc".to_string(),
            crate::core::features::FeatureDefinition {
                description: "VNC server".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "browser".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Browser".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: true,
            },
        );
        available_features.insert(
            "custom".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Custom feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: false,
            },
        );
        available_features.insert(
            "optional".to_string(),
            crate::core::features::FeatureDefinition {
                description: "Optional feature".to_string(),
                ports: vec![],
                env: HashMap::new(),
                volumes: vec![],
                static_server: None,
                enabled_by_default: false,
            },
        );

        // Explicitly enable custom (not default) and vnc (default)
        let selection = FeatureSelection::new().with_enabled(vec!["custom".to_string()]);
        let enabled = enabled_features(available_features, &selection);

        // Should only enable custom (explicit), not vnc or browser (when explicit selection is given, defaults are disabled)
        assert_eq!(enabled.len(), 1);
        assert!(enabled.contains(&"custom".to_string()));
    }

    #[test]
    fn test_determine_enabled_features_empty_selection_empty_features() {
        let available_features = HashMap::new();
        let selection = FeatureSelection::new();
        let enabled = enabled_features(available_features, &selection);

        // Should return empty list
        assert!(enabled.is_empty());
    }
}
