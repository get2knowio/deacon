//! Security option management and merging
//!
//! This module handles merging security options from configuration and features,
//! detecting conflicts, and providing warnings for security options that cannot
//! be applied to existing containers.

use crate::config::DevContainerConfig;
use crate::features::ResolvedFeature;
use std::collections::BTreeSet;
use tracing::{debug, warn};

/// Merged security options from configuration and features
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityOptions {
    /// Whether to run in privileged mode
    pub privileged: bool,
    /// Linux capabilities to add
    pub cap_add: Vec<String>,
    /// Security options
    pub security_opt: Vec<String>,
    /// Conflicts detected during merging
    pub conflicts: Vec<SecurityConflict>,
}

/// Represents a conflict between security options
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityConflict {
    /// Type of conflict
    pub conflict_type: SecurityConflictType,
    /// Description of the conflict
    pub description: String,
    /// Features involved in the conflict
    pub features: Vec<String>,
}

/// Types of security conflicts
#[derive(Debug, Clone, PartialEq)]
pub enum SecurityConflictType {
    /// Conflicting privileged settings
    PrivilegedConflict,
    /// Conflicting security options
    SecurityOptConflict,
}

impl SecurityOptions {
    /// Create new empty security options
    pub fn new() -> Self {
        Self {
            privileged: false,
            cap_add: Vec::new(),
            security_opt: Vec::new(),
            conflicts: Vec::new(),
        }
    }

    /// Normalize a capability name to uppercase and trim whitespace
    pub fn normalize_capability(cap: &str) -> String {
        cap.trim().to_uppercase()
    }

    /// Normalize and deduplicate capabilities
    pub fn normalize_capabilities(caps: &[String]) -> Vec<String> {
        let normalized: BTreeSet<String> = caps
            .iter()
            .map(|cap| Self::normalize_capability(cap))
            .filter(|cap| !cap.is_empty())
            .collect();
        normalized.into_iter().collect()
    }

    /// Normalize and deduplicate security options
    pub fn normalize_security_opts(opts: &[String]) -> Vec<String> {
        let normalized: BTreeSet<String> = opts
            .iter()
            .map(|opt| opt.trim().to_string())
            .filter(|opt| !opt.is_empty())
            .collect();
        normalized.into_iter().collect()
    }

    /// Merge security options from configuration and features
    pub fn merge_from_config_and_features(
        config: &DevContainerConfig,
        features: &[ResolvedFeature],
    ) -> Self {
        debug!(
            "Merging security options from config and {} features",
            features.len()
        );

        let mut result = Self::new();
        let mut conflicts = Vec::new();

        // Start with configuration options (normalized)
        result.privileged = config.privileged.unwrap_or(false);
        result.cap_add = Self::normalize_capabilities(&config.cap_add);
        result.security_opt = Self::normalize_security_opts(&config.security_opt);

        // Track sources for conflict detection
        let mut privileged_sources = Vec::new();
        if config.privileged.unwrap_or(false) {
            privileged_sources.push("config".to_string());
        }

        // Track security option sources for conflict detection
        let mut security_opt_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for opt in &result.security_opt {
            if let Some(key) = opt.split('=').next() {
                security_opt_map.insert(key.to_string(), "config".to_string());
            }
        }

        // Merge from features
        for feature in features {
            // Handle privileged setting
            if let Some(feature_privileged) = feature.metadata.privileged {
                if feature_privileged {
                    if result.privileged && !privileged_sources.is_empty() {
                        // Potential conflict - multiple sources want privileged
                        debug!(
                            "Multiple sources requesting privileged mode: {} and feature {}",
                            privileged_sources.join(", "),
                            feature.id
                        );
                    }
                    result.privileged = true;
                    privileged_sources.push(format!("feature:{}", feature.id));
                } else if result.privileged {
                    // Explicit conflict: one wants privileged, another doesn't
                    conflicts.push(SecurityConflict {
                        conflict_type: SecurityConflictType::PrivilegedConflict,
                        description: format!(
                            "Feature '{}' explicitly sets privileged=false, but other sources require privileged=true",
                            feature.id
                        ),
                        features: vec![feature.id.clone()],
                    });
                }
            }

            // Handle capabilities
            let feature_caps = Self::normalize_capabilities(&feature.metadata.cap_add);
            for cap in feature_caps {
                if !result.cap_add.contains(&cap) {
                    result.cap_add.push(cap);
                }
            }

            // Handle security options and detect conflicts
            let feature_opts = Self::normalize_security_opts(&feature.metadata.security_opt);
            for opt in feature_opts {
                if let Some(key) = opt.split('=').next() {
                    if let Some(existing_source) = security_opt_map.get(key) {
                        // Check if this is a different value for the same key
                        let existing_opt = result
                            .security_opt
                            .iter()
                            .find(|existing| existing.starts_with(&format!("{}=", key)));
                        if let Some(existing_opt) = existing_opt {
                            if existing_opt != &opt {
                                conflicts.push(SecurityConflict {
                                    conflict_type: SecurityConflictType::SecurityOptConflict,
                                    description: format!(
                                        "Conflicting security option '{}': '{}' from {} vs '{}' from feature '{}'",
                                        key, existing_opt, existing_source, opt, feature.id
                                    ),
                                    features: vec![feature.id.clone()],
                                });
                                // Use the feature value (last writer wins)
                                if let Some(pos) = result
                                    .security_opt
                                    .iter()
                                    .position(|x| x.starts_with(&format!("{}=", key)))
                                {
                                    result.security_opt[pos] = opt.clone();
                                }
                            }
                        }
                    } else {
                        result.security_opt.push(opt.clone());
                    }
                    security_opt_map.insert(key.to_string(), format!("feature:{}", feature.id));
                } else {
                    // Option without '=' separator, just add it
                    if !result.security_opt.contains(&opt) {
                        result.security_opt.push(opt);
                    }
                }
            }
        }

        // Final sort for deterministic output
        result.cap_add.sort();
        result.security_opt.sort();

        result.conflicts = conflicts;

        debug!(
            "Merged security options: privileged={}, cap_add={:?}, security_opt={:?}, conflicts={}",
            result.privileged,
            result.cap_add,
            result.security_opt,
            result.conflicts.len()
        );

        result
    }

    /// Check if any security options are configured
    pub fn has_security_options(&self) -> bool {
        self.privileged || !self.cap_add.is_empty() || !self.security_opt.is_empty()
    }

    /// Log warnings for conflicts
    pub fn log_conflicts(&self) {
        for conflict in &self.conflicts {
            match conflict.conflict_type {
                SecurityConflictType::PrivilegedConflict => {
                    warn!(
                        "Security conflict: {} (sources: {:?})",
                        conflict.description, conflict.features
                    );
                }
                SecurityConflictType::SecurityOptConflict => {
                    warn!(
                        "Security conflict: {} (sources: {:?})",
                        conflict.description, conflict.features
                    );
                }
            }
        }
    }

    /// Warn if security options cannot be applied to existing container
    pub fn warn_if_post_create_application(&self, container_id: &str) {
        if !self.has_security_options() {
            return;
        }

        warn!(
            "Security options requested for existing container '{}' cannot be applied:",
            container_id
        );

        if self.privileged {
            warn!("  - Privileged mode cannot be enabled on running container");
        }

        if !self.cap_add.is_empty() {
            warn!(
                "  - Additional capabilities cannot be added to running container: {:?}",
                self.cap_add
            );
        }

        if !self.security_opt.is_empty() {
            warn!(
                "  - Security options cannot be applied to running container: {:?}",
                self.security_opt
            );
        }

        warn!("Consider recreating the container to apply security options");
    }

    /// Generate Docker command arguments for security options
    pub fn to_docker_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if self.privileged {
            args.push("--privileged".to_string());
        }

        for cap in &self.cap_add {
            args.push("--cap-add".to_string());
            args.push(cap.clone());
        }

        for security_opt in &self.security_opt {
            args.push("--security-opt".to_string());
            args.push(security_opt.clone());
        }

        args
    }
}

impl Default for SecurityOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{FeatureMetadata, ResolvedFeature};
    use std::collections::HashMap;

    fn create_test_feature(
        id: &str,
        privileged: Option<bool>,
        cap_add: Vec<String>,
    ) -> ResolvedFeature {
        ResolvedFeature {
            id: id.to_string(),
            source: format!("test://features/{}", id),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: id.to_string(),
                version: Some("1.0.0".to_string()),
                name: Some(format!("Test Feature {}", id)),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                mounts: Vec::new(),
                init: None,
                privileged,
                cap_add,
                security_opt: Vec::new(),
                entrypoint: None,
                installs_after: Vec::new(),
                depends_on: HashMap::new(),
                on_create_command: None,
                update_content_command: None,
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
            },
        }
    }

    #[test]
    fn test_merge_empty() {
        let config = DevContainerConfig::default();
        let features = Vec::new();

        let security = SecurityOptions::merge_from_config_and_features(&config, &features);

        assert!(!security.privileged);
        assert!(security.cap_add.is_empty());
        assert!(security.security_opt.is_empty());
        assert!(security.conflicts.is_empty());
        assert!(!security.has_security_options());
    }

    #[test]
    fn test_merge_config_only() {
        let config = DevContainerConfig {
            privileged: Some(true),
            cap_add: vec!["SYS_PTRACE".to_string(), "net_admin".to_string()], // Test normalization
            security_opt: vec!["seccomp=unconfined".to_string()],
            ..Default::default()
        };

        let features = Vec::new();

        let security = SecurityOptions::merge_from_config_and_features(&config, &features);

        assert!(security.privileged);
        assert_eq!(security.cap_add, vec!["NET_ADMIN", "SYS_PTRACE"]); // Normalized and sorted
        assert_eq!(security.security_opt, vec!["seccomp=unconfined"]);
        assert!(security.conflicts.is_empty());
        assert!(security.has_security_options());
    }

    #[test]
    fn test_merge_features_only() {
        let config = DevContainerConfig::default();
        let features = vec![
            create_test_feature("feature1", Some(true), vec!["SYS_PTRACE".to_string()]),
            create_test_feature("feature2", None, vec!["NET_ADMIN".to_string()]),
        ];

        let security = SecurityOptions::merge_from_config_and_features(&config, &features);

        assert!(security.privileged);
        assert_eq!(security.cap_add, vec!["NET_ADMIN", "SYS_PTRACE"]);
        assert!(security.security_opt.is_empty());
        assert!(security.conflicts.is_empty());
    }

    #[test]
    fn test_merge_duplicate_capabilities() {
        let config = DevContainerConfig {
            cap_add: vec!["SYS_PTRACE".to_string()],
            ..Default::default()
        };

        let features = vec![
            create_test_feature("feature1", None, vec!["SYS_PTRACE".to_string()]),
            create_test_feature("feature2", None, vec!["NET_ADMIN".to_string()]),
        ];

        let security = SecurityOptions::merge_from_config_and_features(&config, &features);

        // Should deduplicate capabilities
        assert_eq!(security.cap_add, vec!["NET_ADMIN", "SYS_PTRACE"]);
        // No conflicts should be detected since we handle deduplication internally
        assert_eq!(security.conflicts.len(), 0);
    }

    #[test]
    fn test_docker_args_generation() {
        let mut security = SecurityOptions::new();
        security.privileged = true;
        security.cap_add = vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()];
        security.security_opt = vec!["seccomp=unconfined".to_string()];

        let args = security.to_docker_args();

        assert_eq!(
            args,
            vec![
                "--privileged",
                "--cap-add",
                "SYS_PTRACE",
                "--cap-add",
                "NET_ADMIN",
                "--security-opt",
                "seccomp=unconfined"
            ]
        );
    }

    #[test]
    fn test_empty_docker_args() {
        let security = SecurityOptions::new();
        let args = security.to_docker_args();
        assert!(args.is_empty());
    }

    #[test]
    fn test_capability_normalization() {
        // Test case-insensitive deduplication and normalization
        let config = DevContainerConfig {
            cap_add: vec![
                "net_admin".to_string(),
                "NET_ADMIN".to_string(),
                " sys_ptrace ".to_string(),
            ],
            ..Default::default()
        };

        let security = SecurityOptions::merge_from_config_and_features(&config, &[]);

        // Should be normalized to uppercase and deduplicated
        assert_eq!(security.cap_add, vec!["NET_ADMIN", "SYS_PTRACE"]);
    }

    #[test]
    fn test_privileged_conflict() {
        let config = DevContainerConfig {
            privileged: Some(true),
            ..Default::default()
        };

        let feature = ResolvedFeature {
            id: "conflicting-feature".to_string(),
            source: "test://features/conflicting-feature".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "conflicting-feature".to_string(),
                version: Some("1.0.0".to_string()),
                name: Some("Conflicting Feature".to_string()),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                mounts: Vec::new(),
                init: None,
                privileged: Some(false), // Explicit conflict
                cap_add: Vec::new(),
                security_opt: Vec::new(),
                entrypoint: None,
                installs_after: Vec::new(),
                depends_on: HashMap::new(),
                on_create_command: None,
                update_content_command: None,
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
            },
        };

        let security = SecurityOptions::merge_from_config_and_features(&config, &[feature]);

        // Should still be privileged (config wins) but detect conflict
        assert!(security.privileged);
        assert_eq!(security.conflicts.len(), 1);
        assert_eq!(
            security.conflicts[0].conflict_type,
            SecurityConflictType::PrivilegedConflict
        );
        assert!(security.conflicts[0]
            .description
            .contains("conflicting-feature"));
    }

    #[test]
    fn test_security_opt_conflict() {
        let config = DevContainerConfig {
            security_opt: vec!["seccomp=unconfined".to_string()],
            ..Default::default()
        };

        let feature = ResolvedFeature {
            id: "feature-with-seccomp".to_string(),
            source: "test://features/feature-with-seccomp".to_string(),
            options: HashMap::new(),
            metadata: FeatureMetadata {
                id: "feature-with-seccomp".to_string(),
                version: Some("1.0.0".to_string()),
                name: Some("Feature with Seccomp".to_string()),
                description: None,
                documentation_url: None,
                license_url: None,
                options: HashMap::new(),
                container_env: HashMap::new(),
                mounts: Vec::new(),
                init: None,
                privileged: None,
                cap_add: Vec::new(),
                security_opt: vec!["seccomp=profile:default".to_string()], // Conflicts with config
                entrypoint: None,
                installs_after: Vec::new(),
                depends_on: HashMap::new(),
                on_create_command: None,
                update_content_command: None,
                post_create_command: None,
                post_start_command: None,
                post_attach_command: None,
            },
        };

        let security = SecurityOptions::merge_from_config_and_features(&config, &[feature]);

        // Feature value should win (last writer wins)
        assert_eq!(security.security_opt, vec!["seccomp=profile:default"]);
        assert_eq!(security.conflicts.len(), 1);
        assert_eq!(
            security.conflicts[0].conflict_type,
            SecurityConflictType::SecurityOptConflict
        );
        assert!(security.conflicts[0].description.contains("seccomp"));
        assert!(security.conflicts[0]
            .description
            .contains("feature-with-seccomp"));
    }

    #[test]
    fn test_normalization_functions() {
        // Test normalize_capability
        assert_eq!(
            SecurityOptions::normalize_capability(" net_admin "),
            "NET_ADMIN"
        );
        assert_eq!(
            SecurityOptions::normalize_capability("SYS_PTRACE"),
            "SYS_PTRACE"
        );

        // Test normalize_capabilities
        let caps = vec![
            "net_admin".to_string(),
            " NET_ADMIN ".to_string(),
            "sys_ptrace".to_string(),
            "".to_string(), // Empty should be filtered
        ];
        let normalized = SecurityOptions::normalize_capabilities(&caps);
        assert_eq!(normalized, vec!["NET_ADMIN", "SYS_PTRACE"]);

        // Test normalize_security_opts
        let opts = vec![
            " seccomp=unconfined ".to_string(),
            "apparmor=unconfined".to_string(),
            "".to_string(), // Empty should be filtered
        ];
        let normalized = SecurityOptions::normalize_security_opts(&opts);
        assert_eq!(
            normalized,
            vec!["apparmor=unconfined", "seccomp=unconfined"]
        );
    }
}
