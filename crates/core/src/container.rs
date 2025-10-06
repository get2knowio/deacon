//! Container lifecycle management and hashing utilities
//!
//! This module provides container lifecycle operations including creation, starting,
//! reuse logic, and identification labels according to the DevContainer specification.

use crate::config::DevContainerConfig;
use crate::errors::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, instrument};

/// Container label schema for DevContainer identification
pub const LABEL_SOURCE: &str = "devcontainer.source";
pub const LABEL_WORKSPACE_HASH: &str = "devcontainer.workspaceHash";
pub const LABEL_CONFIG_HASH: &str = "devcontainer.configHash";
pub const LABEL_NAME: &str = "devcontainer.name";

/// Source identifier for containers created by deacon
pub const DEACON_SOURCE: &str = "deacon";

/// Container identification and configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerIdentity {
    /// Hash of the workspace path
    pub workspace_hash: String,
    /// Hash of the configuration content
    pub config_hash: String,
    /// Human-readable name
    pub name: Option<String>,
    /// Custom container name (overrides generated name)
    pub custom_name: Option<String>,
}

/// Container creation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerResult {
    /// Container ID
    #[serde(rename = "containerId")]
    pub container_id: String,
    /// Whether the container was reused
    pub reused: bool,
    /// Image ID used for the container
    #[serde(rename = "imageId")]
    pub image_id: String,
}

/// Container operations for lifecycle management
#[allow(async_fn_in_trait)]
pub trait ContainerOps {
    /// Find existing containers with matching workspace and config hashes
    async fn find_matching_containers(&self, identity: &ContainerIdentity) -> Result<Vec<String>>;

    /// Create a new container with the specified identity and configuration
    async fn create_container(
        &self,
        identity: &ContainerIdentity,
        config: &DevContainerConfig,
        workspace_path: &Path,
    ) -> Result<String>;

    /// Start a container by ID
    async fn start_container(&self, container_id: &str) -> Result<()>;

    /// Remove a container by ID
    async fn remove_container(&self, container_id: &str) -> Result<()>;

    /// Get container image ID
    async fn get_container_image(&self, container_id: &str) -> Result<String>;
}

impl ContainerIdentity {
    /// Create a new container identity from workspace path and configuration
    #[instrument(skip(config))]
    pub fn new(workspace_path: &Path, config: &DevContainerConfig) -> Self {
        Self::new_with_custom_name(workspace_path, config, None)
    }

    /// Create a new container identity with optional custom container name
    #[instrument(skip(config))]
    pub fn new_with_custom_name(
        workspace_path: &Path,
        config: &DevContainerConfig,
        custom_name: Option<String>,
    ) -> Self {
        let workspace_hash = Self::hash_workspace_path(workspace_path);
        let config_hash = Self::hash_config(config);
        let name = config.name.clone();

        debug!(
            workspace_hash = %workspace_hash,
            config_hash = %config_hash,
            name = ?name,
            custom_name = ?custom_name,
            "Created container identity"
        );

        Self {
            workspace_hash,
            config_hash,
            name,
            custom_name,
        }
    }

    /// Generate a deterministic hash from the workspace path
    fn hash_workspace_path(workspace_path: &Path) -> String {
        use crate::workspace::resolve_workspace_root;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Use worktree-aware resolution to get the canonical workspace root
        let canonical_path = resolve_workspace_root(workspace_path).unwrap_or_else(|_| {
            workspace_path
                .canonicalize()
                .unwrap_or_else(|_| workspace_path.to_path_buf())
        });

        let mut hasher = DefaultHasher::new();
        canonical_path.hash(&mut hasher);
        let hash = hasher.finish();

        // Use first 8 characters for short hash
        format!("{:016x}", hash)[..8].to_string()
    }

    /// Generate a deterministic hash from the configuration
    fn hash_config(config: &DevContainerConfig) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Create a normalized representation with deterministic key ordering for hashing
        let mut value = serde_json::to_value(config).unwrap_or(Value::Null);
        canonicalize_json(&mut value);
        let normalized = serde_json::to_string(&value).unwrap_or_default();

        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        let hash = hasher.finish();

        // Use first 8 characters for short hash
        format!("{:016x}", hash)[..8].to_string()
    }

    /// Generate a deterministic container name
    pub fn container_name(&self) -> String {
        // Use custom name if provided
        if let Some(ref custom_name) = self.custom_name {
            return custom_name.clone();
        }

        // Otherwise generate deterministic name
        let combined_hash = format!("{}{}", self.workspace_hash, self.config_hash);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        combined_hash.hash(&mut hasher);
        let short_hash = format!("{:x}", hasher.finish())[..8].to_string();
        format!("deacon-{}", short_hash)
    }

    /// Generate labels for the container
    pub fn labels(&self) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        labels.insert(LABEL_SOURCE.to_string(), DEACON_SOURCE.to_string());
        labels.insert(
            LABEL_WORKSPACE_HASH.to_string(),
            self.workspace_hash.clone(),
        );
        labels.insert(LABEL_CONFIG_HASH.to_string(), self.config_hash.clone());

        if let Some(ref name) = self.name {
            labels.insert(LABEL_NAME.to_string(), name.clone());
        }

        labels
    }

    /// Create a label selector string for finding matching containers
    pub fn label_selector(&self) -> String {
        format!(
            "{}={},{}={},{}={}",
            LABEL_SOURCE,
            DEACON_SOURCE,
            LABEL_WORKSPACE_HASH,
            self.workspace_hash,
            LABEL_CONFIG_HASH,
            self.config_hash
        )
    }
}

fn canonicalize_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map
                .iter_mut()
                .map(|(k, v)| (k.clone(), std::mem::take(v)))
                .collect();
            map.clear();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, mut val) in entries {
                canonicalize_json(&mut val);
                map.insert(key, val);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                canonicalize_json(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_container_identity_creation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = DevContainerConfig {
            name: Some("test-container".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let identity = ContainerIdentity::new(workspace_path, &config);

        assert!(!identity.workspace_hash.is_empty());
        assert!(!identity.config_hash.is_empty());
        assert_eq!(identity.name, Some("test-container".to_string()));
        assert_eq!(identity.workspace_hash.len(), 8);
        assert_eq!(identity.config_hash.len(), 8);
    }

    #[test]
    fn test_container_name_generation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let identity = ContainerIdentity::new(workspace_path, &config);
        let name = identity.container_name();

        assert!(name.starts_with("deacon-"));
        assert_eq!(name.len(), 15); // "deacon-" + 8 char hash
    }

    #[test]
    fn test_container_name_deterministic() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let identity1 = ContainerIdentity::new(workspace_path, &config);
        let identity2 = ContainerIdentity::new(workspace_path, &config);

        assert_eq!(identity1.container_name(), identity2.container_name());
    }

    #[test]
    fn test_labels_generation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = DevContainerConfig {
            name: Some("test-container".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let identity = ContainerIdentity::new(workspace_path, &config);
        let labels = identity.labels();

        assert_eq!(labels.get(LABEL_SOURCE), Some(&DEACON_SOURCE.to_string()));
        assert_eq!(
            labels.get(LABEL_WORKSPACE_HASH),
            Some(&identity.workspace_hash)
        );
        assert_eq!(labels.get(LABEL_CONFIG_HASH), Some(&identity.config_hash));
        assert_eq!(labels.get(LABEL_NAME), Some(&"test-container".to_string()));
    }

    #[test]
    fn test_label_selector() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let identity = ContainerIdentity::new(workspace_path, &config);
        let selector = identity.label_selector();

        assert!(selector.contains(&format!("{}={}", LABEL_SOURCE, DEACON_SOURCE)));
        assert!(selector.contains(&format!(
            "{}={}",
            LABEL_WORKSPACE_HASH, identity.workspace_hash
        )));
        assert!(selector.contains(&format!("{}={}", LABEL_CONFIG_HASH, identity.config_hash)));
    }

    #[test]
    fn test_config_hash_different_configs() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config1 = DevContainerConfig {
            name: Some("test1".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let config2 = DevContainerConfig {
            name: Some("test2".to_string()),
            image: Some("ubuntu:22.04".to_string()),
            ..Default::default()
        };

        let identity1 = ContainerIdentity::new(workspace_path, &config1);
        let identity2 = ContainerIdentity::new(workspace_path, &config2);

        assert_ne!(identity1.config_hash, identity2.config_hash);
    }

    #[test]
    fn test_workspace_hash_different_paths() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();

        let config = DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let identity1 = ContainerIdentity::new(temp_dir1.path(), &config);
        let identity2 = ContainerIdentity::new(temp_dir2.path(), &config);

        assert_ne!(identity1.workspace_hash, identity2.workspace_hash);
    }

    #[test]
    fn test_custom_container_name() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_path = temp_dir.path();

        let config = DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        let custom_name = Some("my-custom-container".to_string());
        let identity =
            ContainerIdentity::new_with_custom_name(workspace_path, &config, custom_name.clone());

        // Verify custom name is used
        assert_eq!(identity.container_name(), "my-custom-container");
        assert_eq!(identity.custom_name, custom_name);

        // Verify without custom name, generated name is used
        let identity_no_custom = ContainerIdentity::new(workspace_path, &config);
        assert!(identity_no_custom.container_name().starts_with("deacon-"));
        assert_eq!(identity_no_custom.custom_name, None);
    }

    #[test]
    fn test_hash_config_deterministic_with_maps() {
        let mut config = DevContainerConfig {
            name: Some("test".to_string()),
            image: Some("ubuntu:20.04".to_string()),
            ..Default::default()
        };

        config
            .remote_env
            .insert("ALPHA".to_string(), Some("1".to_string()));
        config
            .remote_env
            .insert("BETA".to_string(), Some("2".to_string()));

        let hash1 = ContainerIdentity::hash_config(&config);
        let hash2 = ContainerIdentity::hash_config(&config);

        assert_eq!(hash1, hash2);
    }
}
