//! Utility functions for the up command.
//!
//! This module contains:
//! - `check_for_disallowed_features` - Check for disallowed features
//! - `discover_id_labels_from_config` - Discover id-labels from configuration
//! - `apply_user_mapping` - Apply user mapping configuration

use anyhow::Result;
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::DeaconError;
use std::path::Path;
use tracing::{debug, instrument, warn};

/// Check if any features are disallowed and return an error if found.
///
/// Per FR-004: Configuration resolution MUST block disallowed Features.
///
/// This function checks features against a policy-defined list of disallowed features.
/// The disallowed list can be:
/// - Statically defined (DISALLOWED_FEATURES constant)
/// - Loaded from environment variable DEACON_DISALLOWED_FEATURES (comma-separated)
/// - Extended by policy enforcement systems
///
/// Returns Ok(()) if no disallowed features are found, or an error with the
/// disallowed feature ID if one is detected.
pub(crate) fn check_for_disallowed_features(features: &serde_json::Value) -> Result<()> {
    // Static list of disallowed features (currently empty - can be extended as needed)
    const DISALLOWED_FEATURES: &[&str] = &[];

    // Check for environment-based disallowed features
    let env_disallowed: Vec<String> = std::env::var("DEACON_DISALLOWED_FEATURES")
        .ok()
        .map(|s| s.split(',').map(|f| f.trim().to_string()).collect())
        .unwrap_or_default();

    debug!("Checking features against disallowed list");
    debug!("Static disallowed features: {:?}", DISALLOWED_FEATURES);
    debug!("Environment disallowed features: {:?}", env_disallowed);

    if let Some(features_obj) = features.as_object() {
        for (feature_id, _) in features_obj {
            // Check against static list
            if DISALLOWED_FEATURES.contains(&feature_id.as_str()) {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!("Feature '{}' is not allowed by policy", feature_id),
                    })
                    .into(),
                );
            }

            // Check against environment list
            if env_disallowed.contains(feature_id) {
                return Err(
                    DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                        message: format!(
                            "Feature '{}' is disallowed by DEACON_DISALLOWED_FEATURES",
                            feature_id
                        ),
                    })
                    .into(),
                );
            }

            debug!("Validated feature: {}", feature_id);
        }
    }

    Ok(())
}

/// Discover id-labels from configuration when not explicitly provided via CLI.
///
/// Per FR-004: Configuration resolution MUST discover id labels when not provided.
///
/// ID labels are used to uniquely identify containers for reconnection scenarios.
/// When not provided via --id-label flags, they can be derived from:
/// - Configuration metadata
/// - Workspace folder path
/// - Container name from config
///
/// Returns a list of (name, value) tuples representing discovered labels.
pub(crate) fn discover_id_labels_from_config(
    provided_labels: &[(String, String)],
    workspace_folder: &Path,
    config: &DevContainerConfig,
) -> Vec<(String, String)> {
    // If labels were provided via CLI, use those
    if !provided_labels.is_empty() {
        debug!("Using provided id-labels: {:?}", provided_labels);
        return provided_labels.to_vec();
    }

    // Otherwise, discover labels from context
    let mut labels = Vec::new();

    // Add workspace folder as a label (standard devcontainer practice)
    if let Ok(canonical_path) = workspace_folder.canonicalize() {
        labels.push((
            "devcontainer.local_folder".to_string(),
            canonical_path.to_string_lossy().to_string(),
        ));
        debug!(
            "Discovered id-label from workspace: devcontainer.local_folder={}",
            canonical_path.display()
        );
    }

    // Add config name as a label if available
    if let Some(name) = &config.name {
        labels.push(("devcontainer.config_name".to_string(), name.clone()));
        debug!(
            "Discovered id-label from config: devcontainer.config_name={}",
            name
        );
    }

    labels
}

/// Apply user mapping configuration to the container
#[instrument(skip(config))]
pub(crate) async fn apply_user_mapping(
    container_id: &str,
    config: &DevContainerConfig,
    workspace_folder: &Path,
) -> Result<()> {
    use deacon_core::user_mapping::{get_host_user_info, UserMappingConfig};

    debug!("Applying user mapping configuration");

    // Create user mapping configuration
    let mut user_config = UserMappingConfig::new(
        config.remote_user.clone(),
        config.container_user.clone(),
        config.update_remote_user_uid.unwrap_or(false),
    );

    // Add host user information if updateRemoteUserUID is enabled
    if user_config.update_remote_user_uid {
        match get_host_user_info() {
            Ok((uid, gid)) => {
                user_config = user_config.with_host_user(uid, gid);
                debug!("Host user: UID={}, GID={}", uid, gid);
            }
            Err(e) => {
                warn!("Failed to get host user info, skipping UID mapping: {}", e);
            }
        }
    }

    // Set workspace path for ownership adjustments
    if let Some(container_workspace_folder) = &config.workspace_folder {
        user_config = user_config.with_workspace_path(container_workspace_folder.clone());
    } else {
        // Default container workspace folder
        let workspace_name = workspace_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        user_config = user_config.with_workspace_path(format!("/workspaces/{}", workspace_name));
    }

    // T017: Apply user mapping if needed
    if user_config.needs_user_mapping() {
        debug!("User mapping required, applying configuration");

        // User mapping is applied via the user_mapping module
        // The actual UID/GID updates happen during container creation via DockerLifecycle::up()
        // which internally calls the UserMappingService when update_remote_user_uid is enabled.
        //
        // This ensures the remote user's UID/GID match the host user for proper file permissions.
        // The UserMappingService handles:
        // 1. Executing usermod/groupmod inside the container
        // 2. Updating file ownership in workspace folders
        // 3. Preserving shell and home directory settings

        debug!(
            "User mapping configured: remote_user={:?}, container_user={:?}, update_uid={}, workspace={}",
            user_config.remote_user,
            user_config.container_user,
            user_config.update_remote_user_uid,
            user_config.workspace_path.as_ref().unwrap_or(&"<none>".to_string())
        );

        // Note: The DockerLifecycle::up() implementation in container.rs handles the actual
        // user mapping execution. This function validates and prepares the configuration.
    }

    // T017: Log security options if configured
    // Security options (privileged, capAdd, securityOpt) are applied during container
    // creation by the Docker runtime. They are part of the config and passed to docker run/create.
    if config.privileged.unwrap_or(false) {
        debug!("Container will run in privileged mode");
    }
    if !config.cap_add.is_empty() {
        debug!("Container capabilities to add: {:?}", config.cap_add);
    }
    if !config.security_opt.is_empty() {
        debug!("Container security options: {:?}", config.security_opt);
    }

    Ok(())
}
