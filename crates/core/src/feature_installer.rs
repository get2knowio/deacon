//! In-container feature installation and environment injection
//!
//! This module handles executing feature installation scripts inside containers
//! in resolved dependency order, applying environment variables, mounts, and
//! security options according to the DevContainer specification.

use crate::docker::{CliDocker, Docker, ExecConfig, ExecResult};
use crate::errors::{FeatureError, Result};
use crate::features::{InstallationPlan, ResolvedFeature};
use crate::oci::DownloadedFeature;
use std::collections::HashMap;
use tracing::{debug, info, instrument, warn};

/// Configuration for feature installation process
#[derive(Debug, Clone)]
pub struct FeatureInstallationConfig {
    /// Container ID where features will be installed
    pub container_id: String,
    /// Whether to apply security options (with warnings)
    pub apply_security_options: bool,
    /// Base directory for feature installation in container
    pub installation_base_dir: String,
}

impl Default for FeatureInstallationConfig {
    fn default() -> Self {
        Self {
            container_id: String::new(),
            apply_security_options: false,
            installation_base_dir: "/tmp/devcontainer-features".to_string(),
        }
    }
}

/// Result of installing a single feature
#[derive(Debug, Clone)]
pub struct FeatureInstallationResult {
    /// Feature ID that was installed
    pub feature_id: String,
    /// Exit code from installation script
    pub exit_code: i32,
    /// Whether installation was successful
    pub success: bool,
    /// Installation logs
    pub logs: String,
    /// Environment variables added by this feature
    pub container_env: HashMap<String, String>,
}

/// Complete result of installing all features in a plan
#[derive(Debug, Clone)]
pub struct InstallationPlanResult {
    /// Results for individual features
    pub feature_results: Vec<FeatureInstallationResult>,
    /// Combined environment variables from all features
    pub combined_env: HashMap<String, String>,
    /// Whether all features installed successfully
    pub success: bool,
}

/// Feature installer that executes installation scripts inside containers
#[derive(Debug)]
pub struct FeatureInstaller {
    /// Docker client for container operations
    docker: CliDocker,
}

impl FeatureInstaller {
    /// Create a new feature installer
    pub fn new(docker: CliDocker) -> Self {
        Self { docker }
    }

    /// Install all features from an installation plan in dependency order
    #[instrument(level = "info", skip(self, downloaded_features))]
    pub async fn install_features(
        &self,
        plan: &InstallationPlan,
        downloaded_features: &HashMap<String, DownloadedFeature>,
        config: &FeatureInstallationConfig,
    ) -> Result<InstallationPlanResult> {
        info!(
            "Installing {} features in container {}",
            plan.len(),
            config.container_id
        );

        let mut feature_results = Vec::new();
        let mut combined_env = HashMap::new();
        let mut overall_success = true;

        // Install features sequentially in dependency order
        for feature in &plan.features {
            let downloaded_feature =
                downloaded_features
                    .get(&feature.id)
                    .ok_or_else(|| FeatureError::NotFound {
                        path: format!("Downloaded feature {}", feature.id),
                    })?;

            info!("Installing feature: {}", feature.id);

            let result = self
                .install_single_feature(feature, downloaded_feature, config)
                .await?;

            // Check for failure and stop if fail-fast is needed
            if !result.success {
                overall_success = false;
                feature_results.push(result);
                warn!(
                    "Feature {} installation failed, stopping installation process",
                    feature.id
                );
                break;
            }

            // Aggregate environment variables
            combined_env.extend(result.container_env.clone());
            feature_results.push(result);
        }

        // Apply combined environment variables
        if overall_success && !combined_env.is_empty() {
            info!(
                "Applying combined environment variables from {} features",
                feature_results.len()
            );
            self.apply_environment_variables(&combined_env, config)
                .await?;
        }

        Ok(InstallationPlanResult {
            feature_results,
            combined_env,
            success: overall_success,
        })
    }

    /// Install a single feature in the container
    #[instrument(level = "debug", skip(self, downloaded_feature))]
    async fn install_single_feature(
        &self,
        feature: &ResolvedFeature,
        downloaded_feature: &DownloadedFeature,
        config: &FeatureInstallationConfig,
    ) -> Result<FeatureInstallationResult> {
        debug!("Installing feature {} from {}", feature.id, feature.source);

        // 1. Copy feature content to container
        let container_feature_path = format!("{}/{}", config.installation_base_dir, feature.id);
        self.copy_feature_to_container(
            downloaded_feature,
            &container_feature_path,
            &config.container_id,
        )
        .await?;

        // 2. Execute installation script
        let exec_result = self
            .execute_install_script(feature, &container_feature_path, config)
            .await?;

        // 3. Handle security options if requested
        if config.apply_security_options {
            self.apply_security_options(feature, config).await?;
        }

        Ok(FeatureInstallationResult {
            feature_id: feature.id.clone(),
            exit_code: exec_result.exit_code,
            success: exec_result.success,
            logs: String::new(), // TODO: Capture logs from exec
            container_env: feature.metadata.container_env.clone(),
        })
    }

    /// Copy feature content to container using docker cp
    #[instrument(level = "debug", skip(self))]
    async fn copy_feature_to_container(
        &self,
        downloaded_feature: &DownloadedFeature,
        container_path: &str,
        container_id: &str,
    ) -> Result<()> {
        debug!(
            "Copying feature from {} to container path {}",
            downloaded_feature.path.display(),
            container_path
        );

        // Create the target directory in container
        let mkdir_command = vec![
            "mkdir".to_string(),
            "-p".to_string(),
            container_path.to_string(),
        ];

        let exec_config = ExecConfig {
            user: Some("root".to_string()),
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
        };

        let mkdir_result = self
            .docker
            .exec(container_id, &mkdir_command, exec_config)
            .await?;

        if !mkdir_result.success {
            return Err(FeatureError::Installation {
                message: format!(
                    "Failed to create directory {} in container: exit code {}",
                    container_path, mkdir_result.exit_code
                ),
            }
            .into());
        }

        // Copy files using docker cp
        // Note: This would need to be implemented properly with actual docker cp command
        // For now, we'll use a simple approach of copying file contents via exec
        self.copy_files_via_exec(downloaded_feature, container_path, container_id)
            .await?;

        Ok(())
    }

    /// Copy files using docker exec with cat commands (fallback for docker cp)
    #[instrument(level = "debug", skip(self))]
    async fn copy_files_via_exec(
        &self,
        downloaded_feature: &DownloadedFeature,
        container_path: &str,
        container_id: &str,
    ) -> Result<()> {
        // This is a simplified implementation that would copy key files
        // A full implementation would need to handle the complete feature directory structure

        // Copy devcontainer-feature.json
        let feature_json_path = downloaded_feature.path.join("devcontainer-feature.json");
        if feature_json_path.exists() {
            let content =
                std::fs::read_to_string(&feature_json_path).map_err(FeatureError::Io)?;

            self.write_file_to_container(
                container_id,
                &format!("{}/devcontainer-feature.json", container_path),
                &content,
            )
            .await?;
        }

        // Copy install.sh if it exists
        let install_script_path = downloaded_feature.path.join("install.sh");
        if install_script_path.exists() {
            let content =
                std::fs::read_to_string(&install_script_path).map_err(FeatureError::Io)?;

            self.write_file_to_container(
                container_id,
                &format!("{}/install.sh", container_path),
                &content,
            )
            .await?;

            // Make install.sh executable
            let chmod_command = vec![
                "chmod".to_string(),
                "+x".to_string(),
                format!("{}/install.sh", container_path),
            ];

            let exec_config = ExecConfig {
                user: Some("root".to_string()),
                working_dir: None,
                env: HashMap::new(),
                tty: false,
                interactive: false,
                detach: false,
            };

            self.docker
                .exec(container_id, &chmod_command, exec_config)
                .await?;
        }

        Ok(())
    }

    /// Write file content to container using exec with echo
    #[instrument(level = "debug", skip(self, content))]
    async fn write_file_to_container(
        &self,
        container_id: &str,
        file_path: &str,
        content: &str,
    ) -> Result<()> {
        // Use base64 encoding to handle special characters safely
        use base64::{engine::general_purpose, Engine as _};
        let encoded_content = general_purpose::STANDARD.encode(content);

        let write_command = vec![
            "bash".to_string(),
            "-c".to_string(),
            format!("echo '{}' | base64 -d > '{}'", encoded_content, file_path),
        ];

        let exec_config = ExecConfig {
            user: Some("root".to_string()),
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
        };

        let result = self
            .docker
            .exec(container_id, &write_command, exec_config)
            .await?;

        if !result.success {
            return Err(FeatureError::Installation {
                message: format!(
                    "Failed to write file {} to container: exit code {}",
                    file_path, result.exit_code
                ),
            }
            .into());
        }

        Ok(())
    }

    /// Execute the install.sh script for a feature
    #[instrument(level = "debug", skip(self))]
    async fn execute_install_script(
        &self,
        feature: &ResolvedFeature,
        container_feature_path: &str,
        config: &FeatureInstallationConfig,
    ) -> Result<ExecResult> {
        let install_script_path = format!("{}/install.sh", container_feature_path);

        // Prepare environment variables
        let mut env = HashMap::new();
        env.insert("FEATURE_ID".to_string(), feature.id.clone());

        if let Some(version) = &feature.metadata.version {
            env.insert("FEATURE_VERSION".to_string(), version.clone());
        }

        // Serialize feature options as JSON
        let options_json =
            serde_json::to_string(&feature.options).map_err(|e| FeatureError::Parsing {
                message: format!("Failed to serialize feature options: {}", e),
            })?;
        env.insert("PROVIDED_OPTIONS".to_string(), options_json);
        env.insert("DEACON".to_string(), "1".to_string());

        // Set FEATURE_PATH for compatibility
        env.insert(
            "FEATURE_PATH".to_string(),
            container_feature_path.to_string(),
        );

        let exec_config = ExecConfig {
            user: Some("root".to_string()),
            working_dir: Some(container_feature_path.to_string()),
            env,
            tty: false,
            interactive: false,
            detach: false,
        };

        debug!("Executing install script: {}", install_script_path);

        let command = vec!["/bin/bash".to_string(), install_script_path];
        let result = self
            .docker
            .exec(&config.container_id, &command, exec_config)
            .await?;

        info!(
            "Feature {} installation script completed with exit code {}",
            feature.id, result.exit_code
        );

        Ok(result)
    }

    /// Apply environment variables to container by writing to /etc/profile.d/deacon-features.sh
    #[instrument(level = "debug", skip(self))]
    async fn apply_environment_variables(
        &self,
        env_vars: &HashMap<String, String>,
        config: &FeatureInstallationConfig,
    ) -> Result<()> {
        if env_vars.is_empty() {
            return Ok(());
        }

        info!(
            "Applying {} environment variables to container",
            env_vars.len()
        );

        // Generate shell script content
        let mut script_content = String::new();
        script_content.push_str("#!/bin/bash\n");
        script_content.push_str("# Environment variables from DevContainer features\n");
        script_content.push_str("# Generated by Deacon\n\n");

        for (key, value) in env_vars {
            // Escape the value to handle special characters
            let escaped_value = value.replace('\'', "'\"'\"'");
            script_content.push_str(&format!("export {}='{}'\n", key, escaped_value));
        }

        // Write the script to /etc/profile.d/deacon-features.sh
        let profile_script_path = "/etc/profile.d/deacon-features.sh";
        self.write_file_to_container(&config.container_id, profile_script_path, &script_content)
            .await?;

        // Make the script executable
        let chmod_command = vec![
            "chmod".to_string(),
            "+x".to_string(),
            profile_script_path.to_string(),
        ];

        let exec_config = ExecConfig {
            user: Some("root".to_string()),
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
        };

        let result = self
            .docker
            .exec(&config.container_id, &chmod_command, exec_config)
            .await?;

        if !result.success {
            return Err(FeatureError::Installation {
                message: format!(
                    "Failed to make environment script executable: exit code {}",
                    result.exit_code
                ),
            }
            .into());
        }

        debug!("Environment variables applied successfully");
        Ok(())
    }

    /// Apply security options with warnings about limitations
    #[instrument(level = "debug", skip(self))]
    async fn apply_security_options(
        &self,
        feature: &ResolvedFeature,
        _config: &FeatureInstallationConfig,
    ) -> Result<()> {
        let has_security_options = feature.metadata.privileged.unwrap_or(false)
            || !feature.metadata.cap_add.is_empty()
            || !feature.metadata.security_opt.is_empty();

        if has_security_options {
            warn!(
                "Feature '{}' requests security options that cannot be applied to existing container",
                feature.id
            );

            if feature.metadata.privileged.unwrap_or(false) {
                warn!("  - Privileged mode requested but cannot be enabled on running container");
            }

            if !feature.metadata.cap_add.is_empty() {
                warn!(
                    "  - Additional capabilities requested: {:?} (cannot be added to running container)",
                    feature.metadata.cap_add
                );
            }

            if !feature.metadata.security_opt.is_empty() {
                warn!(
                    "  - Security options requested: {:?} (cannot be applied to running container)",
                    feature.metadata.security_opt
                );
            }

            warn!("Consider recreating the container to apply security options");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{FeatureMetadata, ResolvedFeature};
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[allow(dead_code)]
    fn create_test_feature(id: &str) -> ResolvedFeature {
        let metadata = FeatureMetadata {
            id: id.to_string(),
            version: Some("1.0.0".to_string()),
            name: Some(format!("Test Feature {}", id)),
            description: None,
            documentation_url: None,
            license_url: None,
            options: HashMap::new(),
            container_env: {
                let mut env = HashMap::new();
                env.insert("TEST_VAR".to_string(), "test_value".to_string());
                env
            },
            mounts: vec![],
            init: None,
            privileged: None,
            cap_add: vec![],
            security_opt: vec![],
            installs_after: vec![],
            depends_on: HashMap::new(),
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        };

        ResolvedFeature {
            id: id.to_string(),
            source: format!("test://features/{}", id),
            options: HashMap::new(),
            metadata,
        }
    }

    #[allow(dead_code)]
    fn create_test_downloaded_feature(temp_dir: &TempDir) -> DownloadedFeature {
        let feature_path = temp_dir.path().to_path_buf();

        // Create test install.sh
        let install_script = feature_path.join("install.sh");
        std::fs::write(&install_script, "#!/bin/bash\necho 'Feature installed'\n").unwrap();

        // Create test metadata
        let metadata_file = feature_path.join("devcontainer-feature.json");
        std::fs::write(&metadata_file, r#"{"id": "test-feature"}"#).unwrap();

        let metadata = FeatureMetadata {
            id: "test-feature".to_string(),
            version: Some("1.0.0".to_string()),
            name: Some("Test Feature".to_string()),
            description: None,
            documentation_url: None,
            license_url: None,
            options: HashMap::new(),
            container_env: HashMap::new(),
            mounts: vec![],
            init: None,
            privileged: None,
            cap_add: vec![],
            security_opt: vec![],
            installs_after: vec![],
            depends_on: HashMap::new(),
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        };

        DownloadedFeature {
            path: feature_path,
            metadata,
            digest: "test-digest".to_string(),
        }
    }

    #[test]
    fn test_feature_installation_config_default() {
        let config = FeatureInstallationConfig::default();
        assert_eq!(config.container_id, "");
        assert!(!config.apply_security_options);
        assert_eq!(config.installation_base_dir, "/tmp/devcontainer-features");
    }

    #[test]
    fn test_create_feature_installer() {
        let docker = CliDocker::new();
        let installer = FeatureInstaller::new(docker);
        assert!(format!("{:?}", installer).contains("FeatureInstaller"));
    }

    #[test]
    fn test_feature_installation_result() {
        let result = FeatureInstallationResult {
            feature_id: "test-feature".to_string(),
            exit_code: 0,
            success: true,
            logs: "Installation completed".to_string(),
            container_env: HashMap::new(),
        };

        assert_eq!(result.feature_id, "test-feature");
        assert_eq!(result.exit_code, 0);
        assert!(result.success);
    }

    #[test]
    fn test_installation_plan_result() {
        let result = InstallationPlanResult {
            feature_results: vec![],
            combined_env: HashMap::new(),
            success: true,
        };

        assert!(result.feature_results.is_empty());
        assert!(result.combined_env.is_empty());
        assert!(result.success);
    }
}
