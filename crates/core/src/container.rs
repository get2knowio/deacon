//! Container lifecycle management and hashing utilities
//!
//! This module provides container lifecycle operations including creation, starting,
//! reuse logic, identification labels, and container selection utilities according
//! to the DevContainer specification.

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
        gpu_mode: crate::gpu::GpuMode,
    ) -> Result<String>;

    /// Start a container by ID
    async fn start_container(&self, container_id: &str) -> Result<()>;

    /// Remove a container by ID
    async fn remove_container(&self, container_id: &str) -> Result<()>;

    /// Get container image ID
    async fn get_container_image(&self, container_id: &str) -> Result<String>;

    /// Commit a container to create a new image
    async fn commit_container(&self, container_id: &str, image_tag: &str) -> Result<()>;
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

    // ContainerSelector tests

    #[test]
    fn test_label_parsing_valid() {
        let labels = vec!["key=value".to_string(), "foo=bar".to_string()];
        let result = ContainerSelector::parse_labels(&labels).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], ("key".to_string(), "value".to_string()));
        assert_eq!(result[1], ("foo".to_string(), "bar".to_string()));
    }

    #[test]
    fn test_label_parsing_with_equals_in_value() {
        let labels = vec!["key=value=with=equals".to_string()];
        let result = ContainerSelector::parse_labels(&labels).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            ("key".to_string(), "value=with=equals".to_string())
        );
    }

    #[test]
    fn test_label_parsing_invalid_no_equals() {
        let labels = vec!["invalid".to_string()];
        let result = ContainerSelector::parse_labels(&labels);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Unmatched argument format: id-label must match <name>=<value>."
        );
    }

    #[test]
    fn test_label_parsing_invalid_empty_key() {
        let labels = vec!["=value".to_string()];
        let result = ContainerSelector::parse_labels(&labels);
        assert!(result.is_err());
    }

    #[test]
    fn test_label_parsing_invalid_empty_value() {
        let labels = vec!["key=".to_string()];
        let result = ContainerSelector::parse_labels(&labels);
        assert!(result.is_err());
    }

    #[test]
    fn test_selector_validation_empty() {
        let selector = ContainerSelector {
            container_id: None,
            id_labels: vec![],
            workspace_folder: None,
        };
        let result = selector.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
        );
    }

    #[test]
    fn test_selector_validation_with_container_id() {
        let selector = ContainerSelector {
            container_id: Some("abc123".to_string()),
            id_labels: vec![],
            workspace_folder: None,
        };
        assert!(selector.validate().is_ok());
    }

    #[test]
    fn test_selector_validation_with_labels() {
        let selector = ContainerSelector {
            container_id: None,
            id_labels: vec![("app".to_string(), "web".to_string())],
            workspace_folder: None,
        };
        assert!(selector.validate().is_ok());
    }

    #[test]
    fn test_selector_validation_with_workspace() {
        let temp_dir = TempDir::new().unwrap();
        let selector = ContainerSelector {
            container_id: None,
            id_labels: vec![],
            workspace_folder: Some(temp_dir.path().to_path_buf()),
        };
        assert!(selector.validate().is_ok());
    }

    #[test]
    fn test_selector_new_with_valid_labels() {
        let selector = ContainerSelector::new(
            None,
            vec!["app=web".to_string(), "env=prod".to_string()],
            None,
            None,
        )
        .unwrap();
        assert_eq!(selector.id_labels.len(), 2);
        assert_eq!(
            selector.id_labels[0],
            ("app".to_string(), "web".to_string())
        );
        assert_eq!(
            selector.id_labels[1],
            ("env".to_string(), "prod".to_string())
        );
    }

    #[test]
    fn test_selector_new_with_invalid_labels() {
        let result = ContainerSelector::new(None, vec!["invalid".to_string()], None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_selector_uses_override_path_parent_when_workspace_missing() {
        let override_path =
            std::path::PathBuf::from("/workspace/.devcontainer/custom-override.json");

        let selector = ContainerSelector::new(None, vec![], None, Some(override_path)).unwrap();

        assert_eq!(
            selector.workspace_folder,
            Some(std::path::PathBuf::from("/workspace/.devcontainer"))
        );
    }

    #[test]
    fn test_compute_dev_container_id_basic() {
        let labels = vec![
            ("app".to_string(), "web".to_string()),
            ("env".to_string(), "prod".to_string()),
        ];
        let id = compute_dev_container_id(&labels);
        assert_eq!(id.len(), 12);
        // Should be hexadecimal
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_compute_dev_container_id_deterministic() {
        let labels = vec![
            ("app".to_string(), "web".to_string()),
            ("env".to_string(), "prod".to_string()),
        ];
        let id1 = compute_dev_container_id(&labels);
        let id2 = compute_dev_container_id(&labels);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_compute_dev_container_id_order_independent() {
        let labels1 = vec![
            ("app".to_string(), "web".to_string()),
            ("env".to_string(), "prod".to_string()),
        ];
        let labels2 = vec![
            ("env".to_string(), "prod".to_string()),
            ("app".to_string(), "web".to_string()),
        ];
        let id1 = compute_dev_container_id(&labels1);
        let id2 = compute_dev_container_id(&labels2);
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_compute_dev_container_id_changes_with_labels() {
        let labels1 = vec![
            ("app".to_string(), "web".to_string()),
            ("env".to_string(), "prod".to_string()),
        ];
        let labels2 = vec![
            ("app".to_string(), "web".to_string()),
            ("env".to_string(), "dev".to_string()),
        ];
        let labels3 = vec![("app".to_string(), "web".to_string())];

        let id1 = compute_dev_container_id(&labels1);
        let id2 = compute_dev_container_id(&labels2);
        let id3 = compute_dev_container_id(&labels3);

        // Changing label value should change ID
        assert_ne!(id1, id2);
        // Removing a label should change ID
        assert_ne!(id1, id3);
        assert_ne!(id2, id3);
    }

    #[test]
    fn test_compute_dev_container_id_empty_labels() {
        let labels = vec![];
        let id = compute_dev_container_id(&labels);
        assert_eq!(id.len(), 12);
        // Should still produce a valid ID (hash of empty string)
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}

/// Container selection criteria for targeting containers
///
/// Provides flexible container selection via direct ID, label filters, or workspace folder.
/// Used by commands like exec, read-configuration, run-user-commands, and set-up.
///
/// # Priority Order
///
/// When multiple selectors are provided, resolution follows this priority:
/// 1. Direct container ID (--container-id)
/// 2. Label-based lookup (--id-label)
/// 3. Workspace-based lookup (--workspace-folder)
///
/// # Examples
///
/// ```
/// use deacon_core::container::ContainerSelector;
/// use std::path::PathBuf;
///
/// // Create selector with container ID
/// let selector = ContainerSelector::new(
///     Some("abc123".to_string()),
///     vec![],
///     None,
///     None,
/// ).unwrap();
/// assert!(selector.validate().is_ok());
///
/// // Create selector with labels
/// let selector = ContainerSelector::new(
///     None,
///     vec!["app=myapp".to_string(), "env=prod".to_string()],
///     None,
///     None,
/// ).unwrap();
/// assert!(selector.validate().is_ok());
///
/// // Create selector from override config path when workspace isn't provided
/// let selector = ContainerSelector::new(
///     None,
///     vec!["app=myapp".to_string()],
///     None,
///     Some(PathBuf::from("/workspace/.devcontainer/override.json")),
/// ).unwrap();
/// assert!(selector.workspace_folder.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct ContainerSelector {
    /// Direct container ID
    pub container_id: Option<String>,

    /// Label filters (key=value pairs)
    pub id_labels: Vec<(String, String)>,

    /// Workspace folder (for workspace-based lookup)
    pub workspace_folder: Option<std::path::PathBuf>,
}

impl ContainerSelector {
    /// Create a new ContainerSelector from CLI arguments
    ///
    /// Parses id-label strings into (key, value) tuples and validates their format.
    ///
    /// # Arguments
    ///
    /// * `container_id` - Optional direct container ID
    /// * `id_label_strings` - Label strings in "key=value" format
    /// * `workspace_folder` - Optional workspace folder path (or derived from override config)
    /// * `override_config_path` - Optional override config path used to infer workspace when none is provided
    ///
    /// # Errors
    ///
    /// Returns error if any label string doesn't match the "key=value" format with non-empty parts
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::container::ContainerSelector;
    ///
    /// let selector = ContainerSelector::new(
    ///     Some("abc123".to_string()),
    ///     vec!["app=web".to_string()],
    ///     None,
    ///     None,
    /// ).unwrap();
    /// ```
    pub fn new(
        container_id: Option<String>,
        id_label_strings: Vec<String>,
        workspace_folder: Option<std::path::PathBuf>,
        override_config_path: Option<std::path::PathBuf>,
    ) -> anyhow::Result<Self> {
        let id_labels = Self::parse_labels(&id_label_strings)?;
        let workspace_folder = match (workspace_folder, override_config_path) {
            (Some(folder), _) => Some(folder),
            (None, Some(override_path)) => override_path.parent().map(|p| p.to_path_buf()),
            (None, None) => None,
        };
        Ok(Self {
            container_id,
            id_labels,
            workspace_folder,
        })
    }

    /// Validate that at least one selector is provided
    ///
    /// Ensures the selector can be used to identify a container by checking that
    /// at least one of container_id, id_labels, or workspace_folder is provided.
    ///
    /// # Errors
    ///
    /// Returns error if no selection criteria are provided
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::container::ContainerSelector;
    ///
    /// // Valid: has container_id
    /// let selector =
    ///     ContainerSelector::new(Some("abc123".to_string()), vec![], None, None).unwrap();
    /// assert!(selector.validate().is_ok());
    ///
    /// // Invalid: no selectors
    /// let selector = ContainerSelector::new(None, vec![], None, None).unwrap();
    /// assert!(selector.validate().is_err());
    /// ```
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.container_id.is_none()
            && self.id_labels.is_empty()
            && self.workspace_folder.is_none()
        {
            anyhow::bail!(
                "Missing required argument: One of --container-id, --id-label or --workspace-folder is required."
            );
        }
        Ok(())
    }

    /// Parse and validate label format
    ///
    /// Labels must be in "name=value" format with non-empty key and value parts.
    /// Uses regex pattern `^.+=.+$` to validate format.
    ///
    /// # Arguments
    ///
    /// * `labels` - Slice of label strings to parse
    ///
    /// # Returns
    ///
    /// Vector of (key, value) tuples
    ///
    /// # Errors
    ///
    /// Returns error if any label doesn't match the required format
    ///
    /// # Examples
    ///
    /// ```
    /// use deacon_core::container::ContainerSelector;
    ///
    /// // Valid labels
    /// let labels = vec!["key=value".to_string(), "app=web".to_string()];
    /// let parsed = ContainerSelector::parse_labels(&labels).unwrap();
    /// assert_eq!(parsed.len(), 2);
    /// assert_eq!(parsed[0], ("key".to_string(), "value".to_string()));
    ///
    /// // Invalid label (missing '=')
    /// let invalid = vec!["invalid".to_string()];
    /// assert!(ContainerSelector::parse_labels(&invalid).is_err());
    /// ```
    pub fn parse_labels(labels: &[String]) -> anyhow::Result<Vec<(String, String)>> {
        use regex::Regex;
        let regex = Regex::new(r"^.+=.+$").expect("Valid regex pattern");
        let mut result = Vec::new();
        for label in labels {
            if !regex.is_match(label) {
                anyhow::bail!("Unmatched argument format: id-label must match <name>=<value>.");
            }
            let parts: Vec<&str> = label.splitn(2, '=').collect();
            result.push((parts[0].to_string(), parts[1].to_string()));
        }
        Ok(result)
    }
}

/// Compute deterministic dev container ID from id-labels
///
/// Creates a deterministic 12-character hex ID from a set of id-labels.
/// The ID is independent of label order - labels are sorted before hashing.
/// Adding or removing labels changes the ID.
///
/// # Arguments
///
/// * `id_labels` - Slice of (key, value) tuples representing container labels
///
/// # Returns
///
/// A 12-character hexadecimal string representing the deterministic container ID
///
/// # Examples
///
/// ```
/// use deacon_core::container::compute_dev_container_id;
///
/// let labels = vec![
///     ("app".to_string(), "web".to_string()),
///     ("env".to_string(), "prod".to_string()),
/// ];
/// let id1 = compute_dev_container_id(&labels);
/// assert_eq!(id1.len(), 12);
///
/// // Order doesn't matter
/// let labels_reversed = vec![
///     ("env".to_string(), "prod".to_string()),
///     ("app".to_string(), "web".to_string()),
/// ];
/// let id2 = compute_dev_container_id(&labels_reversed);
/// assert_eq!(id1, id2);
/// ```
pub fn compute_dev_container_id(id_labels: &[(String, String)]) -> String {
    // Sort labels to ensure determinism regardless of order
    let mut sorted_labels = id_labels.to_vec();
    sorted_labels.sort_by(|a, b| match a.0.cmp(&b.0) {
        std::cmp::Ordering::Equal => a.1.cmp(&b.1),
        other => other,
    });

    // Create string representation: "key1=value1,key2=value2,..."
    let label_string = sorted_labels
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(",");

    // Use blake3 for stable, cross-platform hashing
    let hash = blake3::hash(label_string.as_bytes());
    let hex_digest = hash.to_hex();

    // Return first 12 characters of hex representation (lowercase)
    hex_digest.as_str()[..12].to_string()
}

/// Container lookup operations
///
/// These functions provide utilities for finding and inspecting containers
/// using various selection criteria.
/// Find container by exact ID match
///
/// Looks up a container by its exact ID using the Docker inspect API.
///
/// # Arguments
///
/// * `docker` - Docker client implementing the Docker trait
/// * `container_id` - Container ID to search for
///
/// # Returns
///
/// * `Ok(Some(ContainerInfo))` - Container found
/// * `Ok(None)` - Container not found (404)
/// * `Err(_)` - Docker API error
///
/// # Examples
///
/// ```no_run
/// use deacon_core::container::find_container_by_id;
/// use deacon_core::docker::CliDocker;
///
/// # async fn example() -> anyhow::Result<()> {
/// let docker = CliDocker::new();
/// let result = find_container_by_id(&docker, "abc123").await?;
/// match result {
///     Some(info) => println!("Found container: {}", info.id),
///     None => println!("Container not found"),
/// }
/// # Ok(())
/// # }
/// ```
#[instrument(skip(docker))]
pub async fn find_container_by_id<D>(
    docker: &D,
    container_id: &str,
) -> Result<Option<crate::docker::ContainerInfo>>
where
    D: crate::docker::Docker,
{
    debug!("Finding container by ID: {}", container_id);
    docker.inspect_container(container_id).await
}

/// Find containers matching all specified labels
///
/// Lists containers that match all provided label filters.
///
/// # Arguments
///
/// * `docker` - Docker client implementing the Docker trait
/// * `labels` - Slice of (key, value) tuples for label filters
///
/// # Returns
///
/// Vector of matching containers (may be empty)
///
/// # Examples
///
/// ```no_run
/// use deacon_core::container::find_containers_by_labels;
/// use deacon_core::docker::CliDocker;
///
/// # async fn example() -> anyhow::Result<()> {
/// let docker = CliDocker::new();
/// let labels = vec![
///     ("app".to_string(), "web".to_string()),
///     ("env".to_string(), "prod".to_string()),
/// ];
/// let containers = find_containers_by_labels(&docker, &labels).await?;
/// println!("Found {} matching containers", containers.len());
/// # Ok(())
/// # }
/// ```
#[instrument(skip(docker))]
pub async fn find_containers_by_labels<D>(
    docker: &D,
    labels: &[(String, String)],
) -> Result<Vec<crate::docker::ContainerInfo>>
where
    D: crate::docker::Docker,
{
    debug!("Finding containers by labels: {:?}", labels);

    if labels.is_empty() {
        return Ok(Vec::new());
    }

    // Build label selector string (comma-separated key=value pairs)
    let label_selector: Vec<String> = labels.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    let label_selector = label_selector.join(",");

    docker.list_containers(Some(&label_selector)).await
}

/// Resolve container using selector criteria
///
/// Resolves a container using the provided selector, following priority order:
/// 1. Direct container ID (highest priority)
/// 2. Label-based lookup
/// 3. Workspace-based lookup (TODO: implement when workspace labels defined - see issue #270)
///
/// **Priority Order Details:**
/// - **Container ID**: If provided, performs direct lookup ignoring other selectors
/// - **Labels**: If no ID but labels provided, finds containers matching all specified labels
/// - **Workspace**: TODO(#270) - When implemented, will query containers with workspace-specific labels
///
/// Returns the first matching container when using label-based lookup.
///
/// # Arguments
///
/// * `docker` - Docker client implementing the Docker trait
/// * `selector` - Container selection criteria
///
/// # Returns
///
/// * `Ok(Some(ContainerInfo))` - Container found and inspected
/// * `Ok(None)` - No matching container found
/// * `Err(_)` - Docker API error or invalid selector
///
/// # Examples
///
/// ```no_run
/// use deacon_core::container::{ContainerSelector, resolve_container};
/// use deacon_core::docker::CliDocker;
///
/// # async fn example() -> anyhow::Result<()> {
/// let docker = CliDocker::new();
/// let selector = ContainerSelector::new(
///     Some("abc123".to_string()),
///     vec![],
///     None,
///     None,
/// )?;
/// selector.validate()?;
///
/// match resolve_container(&docker, &selector).await? {
///     Some(info) => println!("Found container: {}", info.id),
///     None => eprintln!("Dev container not found."),
/// }
/// # Ok(())
/// # }
/// ```
#[instrument(skip(docker))]
pub async fn resolve_container<D>(
    docker: &D,
    selector: &ContainerSelector,
) -> Result<Option<crate::docker::ContainerInfo>>
where
    D: crate::docker::Docker,
{
    debug!("Resolving container with selector: {:?}", selector);

    // Priority 1: Direct container ID
    if let Some(ref container_id) = selector.container_id {
        debug!("Using container ID selector: {}", container_id);
        return find_container_by_id(docker, container_id).await;
    }

    // Priority 2: Label-based lookup
    if !selector.id_labels.is_empty() {
        debug!("Using label-based selector: {:?}", selector.id_labels);
        let containers = find_containers_by_labels(docker, &selector.id_labels).await?;
        if let Some(container) = containers.first() {
            debug!("Found container via labels: {}", container.id);
            return find_container_by_id(docker, &container.id).await;
        }
        return Ok(None);
    }

    // Priority 3: Workspace-based lookup
    // TODO(#270): Implement workspace-based container resolution
    // This would query containers with workspace-specific labels (e.g., workspace folder hash)
    // Priority: Low - most users will use direct ID or labels
    debug!("Workspace-based lookup not yet implemented");

    Ok(None)
}

/// Inspect container and return full metadata
///
/// Wrapper around Docker inspect that provides a more ergonomic API.
///
/// # Arguments
///
/// * `docker` - Docker client implementing the Docker trait
/// * `container_id` - Container ID to inspect
///
/// # Returns
///
/// Full container metadata
///
/// # Errors
///
/// Returns error if container doesn't exist or Docker API fails
///
/// # Examples
///
/// ```no_run
/// use deacon_core::container::inspect_container;
/// use deacon_core::docker::CliDocker;
///
/// # async fn example() -> anyhow::Result<()> {
/// let docker = CliDocker::new();
/// let info = inspect_container(&docker, "abc123").await?;
/// println!("Container state: {}", info.state);
/// # Ok(())
/// # }
/// ```
#[instrument(skip(docker))]
pub async fn inspect_container<D>(
    docker: &D,
    container_id: &str,
) -> Result<crate::docker::ContainerInfo>
where
    D: crate::docker::Docker,
{
    debug!("Inspecting container: {}", container_id);
    match docker.inspect_container(container_id).await? {
        Some(info) => Ok(info),
        None => Err(crate::errors::DockerError::ContainerNotFound {
            id: container_id.to_string(),
        }
        .into()),
    }
}
