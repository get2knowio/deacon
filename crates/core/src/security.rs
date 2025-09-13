//! Security option management and merging
//!
//! This module handles merging security options from configuration and features,
//! detecting conflicts, and providing warnings for security options that cannot
//! be applied to existing containers.

use crate::config::DevContainerConfig;
use crate::features::ResolvedFeature;
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
    /// Duplicate capabilities requested
    DuplicateCapabilities,
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

        // Start with configuration options
        result.privileged = config.privileged.unwrap_or(false);
        result.cap_add.extend(config.cap_add.clone());
        result.security_opt.extend(config.security_opt.clone());

        // Track sources for conflict detection
        let mut privileged_sources = Vec::new();
        if config.privileged.unwrap_or(false) {
            privileged_sources.push("config".to_string());
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
            for cap in &feature.metadata.cap_add {
                if !result.cap_add.contains(cap) {
                    result.cap_add.push(cap.clone());
                } else {
                    debug!(
                        "Duplicate capability '{}' requested by feature '{}'",
                        cap, feature.id
                    );
                }
            }

            // Handle security options
            result
                .security_opt
                .extend(feature.metadata.security_opt.clone());
        }

        // Detect duplicate capabilities
        let original_len = result.cap_add.len();
        result.cap_add.sort();
        result.cap_add.dedup();
        result.security_opt.sort();
        result.security_opt.dedup();

        // Only report conflicts if there were actual duplicates removed
        if result.cap_add.len() < original_len {
            debug!("Removed duplicate capabilities during merging");
        }

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
                    warn!("Security conflict: {}", conflict.description);
                }
                SecurityConflictType::DuplicateCapabilities => {
                    debug!("Security note: {}", conflict.description);
                }
                SecurityConflictType::SecurityOptConflict => {
                    warn!("Security conflict: {}", conflict.description);
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
            cap_add: vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()],
            security_opt: vec!["seccomp=unconfined".to_string()],
            ..Default::default()
        };

        let features = Vec::new();

        let security = SecurityOptions::merge_from_config_and_features(&config, &features);

        assert!(security.privileged);
        assert_eq!(security.cap_add, vec!["NET_ADMIN", "SYS_PTRACE"]); // Sorted
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
}
