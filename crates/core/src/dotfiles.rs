//! Dotfiles integration module
//!
//! This module provides functionality to clone dotfiles repositories and
//! execute their installation scripts, following the DevContainer CLI specification.
//!
//! ## Lifecycle Integration
//!
//! Dotfiles execute as part of the devcontainer lifecycle, specifically in the
//! `postCreate -> dotfiles -> postStart` boundary. The `DotfilesPhaseConfig` struct
//! and `execute_dotfiles_phase` function provide the integration point for the
//! `LifecycleOrchestrator`.
//!
//! ### Ordering Guarantees (per spec FR-001)
//!
//! The dotfiles phase:
//! - Runs exactly once during fresh `up` runs
//! - Runs after `postCreate` and before `postStart`
//! - Is skipped in prebuild mode (spec FR-006)
//! - Is skipped when `--skip-post-create` is provided (spec FR-005)
//! - Is skipped on resume runs if prior marker exists (spec FR-003)
//!
//! ### Example
//!
//! ```no_run
//! use deacon_core::dotfiles::{DotfilesPhaseConfig, execute_dotfiles_phase};
//!
//! # async fn example() -> deacon_core::errors::Result<()> {
//! let config = DotfilesPhaseConfig {
//!     repository: Some("https://github.com/user/dotfiles".to_string()),
//!     target_path: Some("/home/user/.dotfiles".to_string()),
//!     install_command: None,
//! };
//!
//! // Returns Ok(None) if no dotfiles configured (graceful skip)
//! // Returns Ok(Some(result)) on success
//! // Returns Err on failure
//! let result = execute_dotfiles_phase(&config).await?;
//! # Ok(())
//! # }
//! ```

use crate::errors::{GitError, Result};
use std::path::Path;
use tokio::process::Command;
use tracing::{debug, info, instrument, warn};

/// Configuration for dotfiles integration
#[derive(Debug, Clone)]
pub struct DotfilesConfiguration {
    /// Git repository URL to clone
    pub repository: String,
    /// Optional custom install command (overrides auto-detected scripts)
    pub install_command: Option<String>,
    /// Target path where dotfiles should be cloned
    pub target_path: String,
}

/// Options for dotfiles application (placeholder for future container user context)
#[derive(Debug, Clone, Default)]
pub struct DotfilesOptions {
    // Future fields for container user mapping, permissions, etc.
    // Currently a placeholder as noted in the issue
}

/// Result of dotfiles application
#[derive(Debug, Clone)]
pub struct DotfilesResult {
    /// Path where dotfiles were cloned
    pub target_path: String,
    /// Whether an install script was found and executed
    pub script_executed: bool,
    /// Name of the script that was executed (if any)
    pub script_name: Option<String>,
}

/// Check if git is available on the system
#[instrument]
async fn check_git_available() -> Result<()> {
    debug!("Checking git availability");

    let output = Command::new("git").arg("--version").output().await;

    match output {
        Ok(output) if output.status.success() => {
            debug!("Git is available");
            Ok(())
        }
        _ => {
            debug!("Git is not available or not installed");
            Err(GitError::NotInstalled.into())
        }
    }
}

/// Clone a git repository to the target directory
#[instrument]
async fn clone_repository(repo_url: &str, target: &Path, _options: &DotfilesOptions) -> Result<()> {
    info!("Starting to clone dotfiles repository: {}", repo_url);

    let output = Command::new("git")
        .arg("clone")
        .arg(repo_url)
        .arg(target)
        .output()
        .await
        .map_err(|e| GitError::CLIError(format!("Failed to execute git clone: {}", e)))?;

    if output.status.success() {
        info!(
            "Successfully cloned dotfiles repository to: {}",
            target.display()
        );
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(GitError::CloneFailed(stderr.to_string()).into())
    }
}

/// Detect install script in the cloned repository
#[instrument]
async fn detect_install_script(target: &Path) -> Option<String> {
    let script_names = ["install.sh", "setup.sh"];

    for script_name in &script_names {
        let script_path = target.join(script_name);
        debug!("Checking for install script: {}", script_path.display());

        if script_path.exists() && script_path.is_file() {
            debug!("Found install script: {}", script_name);
            return Some(script_name.to_string());
        }
    }

    debug!("No install script found");
    None
}

/// Execute install script using bash
#[instrument]
async fn execute_install_script(target: &Path, script_name: &str) -> Result<()> {
    let script_path = target.join(script_name);
    info!("Executing install script: {}", script_path.display());

    let output = Command::new("bash")
        .arg(&script_path)
        .current_dir(target)
        .output()
        .await
        .map_err(|e| GitError::CLIError(format!("Failed to execute install script: {}", e)))?;

    if output.status.success() {
        info!("Install script executed successfully");

        // Log stdout and stderr for debugging
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if !stdout.is_empty() {
            for line in stdout.lines() {
                debug!("[{}] stdout: {}", script_name, line);
            }
        }

        if !stderr.is_empty() {
            for line in stderr.lines() {
                debug!("[{}] stderr: {}", script_name, line);
            }
        }

        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(GitError::CLIError(format!("Install script failed: {}", stderr)).into())
    }
}

/// Apply dotfiles from a repository to a target directory
///
/// This function clones a dotfiles repository and optionally runs an installation script.
/// It follows the DevContainer CLI specification for dotfiles integration.
///
/// # Arguments
///
/// * `repo_url` - Git repository URL to clone
/// * `target` - Target directory where dotfiles should be cloned
/// * `options` - Configuration options (placeholder for future container user context)
///
/// # Returns
///
/// Returns a `DotfilesResult` containing information about the operation.
///
/// # Errors
///
/// Returns an error if:
/// - Git is not installed or not accessible
/// - Repository cloning fails
/// - Install script execution fails
#[instrument]
pub async fn apply_dotfiles(
    repo_url: &str,
    target: &Path,
    options: &DotfilesOptions,
) -> Result<DotfilesResult> {
    info!(
        "Applying dotfiles from {} to {}",
        repo_url,
        target.display()
    );

    // Check if git is available
    check_git_available().await?;

    // Clone the repository
    clone_repository(repo_url, target, options).await?;

    // Detect and execute install script
    let script_name = detect_install_script(target).await;
    let script_executed = if let Some(ref script) = script_name {
        execute_install_script(target, script).await?;
        true
    } else {
        info!("No install script found, dotfiles cloned without setup");
        false
    };

    info!("Dotfiles application completed");

    Ok(DotfilesResult {
        target_path: target.to_string_lossy().to_string(),
        script_executed,
        script_name,
    })
}

// =============================================================================
// Lifecycle Integration
// =============================================================================

/// Configuration for executing dotfiles as a lifecycle phase.
///
/// This struct captures the CLI arguments and configuration needed to execute
/// dotfiles during the `LifecyclePhase::Dotfiles` phase of container setup.
///
/// Per spec FR-001, dotfiles execute in the order:
/// `onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach`
#[derive(Debug, Clone, Default)]
pub struct DotfilesPhaseConfig {
    /// Git repository URL for dotfiles (None means no dotfiles configured)
    pub repository: Option<String>,
    /// Target path where dotfiles should be cloned (defaults based on user)
    pub target_path: Option<String>,
    /// Custom install command (fallback if no install.sh/setup.sh is auto-detected)
    pub install_command: Option<String>,
}

impl DotfilesPhaseConfig {
    /// Create a new empty configuration (no dotfiles)
    pub fn new() -> Self {
        Self::default()
    }

    /// Create configuration with a repository
    pub fn with_repository(mut self, repo: impl Into<String>) -> Self {
        self.repository = Some(repo.into());
        self
    }

    /// Set the target path for cloning
    pub fn with_target_path(mut self, path: impl Into<String>) -> Self {
        self.target_path = Some(path.into());
        self
    }

    /// Set a custom install command
    pub fn with_install_command(mut self, cmd: impl Into<String>) -> Self {
        self.install_command = Some(cmd.into());
        self
    }

    /// Check if dotfiles are configured
    pub fn is_configured(&self) -> bool {
        self.repository.is_some()
    }
}

/// Result of dotfiles phase execution in the lifecycle.
///
/// This extends `DotfilesResult` with lifecycle-specific information.
#[derive(Debug, Clone)]
pub struct DotfilesPhaseResult {
    /// Whether dotfiles were configured and attempted
    pub was_configured: bool,
    /// Whether the phase was skipped (no dotfiles configured)
    pub skipped: bool,
    /// Reason for skipping (if skipped)
    pub skip_reason: Option<String>,
    /// Underlying dotfiles result (if executed)
    pub result: Option<DotfilesResult>,
}

impl DotfilesPhaseResult {
    /// Create a result indicating dotfiles were skipped
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            was_configured: false,
            skipped: true,
            skip_reason: Some(reason.into()),
            result: None,
        }
    }

    /// Create a result indicating dotfiles executed successfully
    pub fn executed(result: DotfilesResult) -> Self {
        Self {
            was_configured: true,
            skipped: false,
            skip_reason: None,
            result: Some(result),
        }
    }
}

/// Execute dotfiles as a lifecycle phase.
///
/// This function is designed to be called from the `LifecycleOrchestrator` during
/// the `LifecyclePhase::Dotfiles` phase. It handles:
///
/// 1. **Graceful skip**: Returns `Ok(result)` with `skipped=true` if no dotfiles are configured
/// 2. **Execution**: Clones repository and runs install script if configured
/// 3. **Error propagation**: Returns `Err` if dotfiles execution fails
///
/// # Arguments
///
/// * `config` - Configuration specifying repository, target path, and install command
///
/// # Returns
///
/// * `Ok(DotfilesPhaseResult)` - Phase completed (check `skipped` field)
/// * `Err` - Phase failed and should halt lifecycle
///
/// # Example
///
/// ```no_run
/// use deacon_core::dotfiles::{DotfilesPhaseConfig, execute_dotfiles_phase};
///
/// # async fn example() -> deacon_core::errors::Result<()> {
/// // No dotfiles configured - returns skipped result
/// let empty_config = DotfilesPhaseConfig::new();
/// let result = execute_dotfiles_phase(&empty_config).await?;
/// assert!(result.skipped);
///
/// // Dotfiles configured - executes and returns result
/// let config = DotfilesPhaseConfig::new()
///     .with_repository("https://github.com/user/dotfiles")
///     .with_target_path("/home/user/.dotfiles");
/// let result = execute_dotfiles_phase(&config).await?;
/// assert!(!result.skipped);
/// # Ok(())
/// # }
/// ```
#[instrument(skip(config))]
pub async fn execute_dotfiles_phase(config: &DotfilesPhaseConfig) -> Result<DotfilesPhaseResult> {
    // Check if dotfiles are configured
    let repository = match &config.repository {
        Some(repo) => repo.clone(),
        None => {
            debug!("No dotfiles repository configured, skipping dotfiles phase");
            return Ok(DotfilesPhaseResult::skipped(
                "no dotfiles repository configured",
            ));
        }
    };

    // Determine target path (default to ~/.dotfiles if not specified)
    let target_path = config
        .target_path
        .clone()
        .unwrap_or_else(|| "~/.dotfiles".to_string());

    // Expand ~ to home directory for host-side execution
    let expanded_target = if target_path.starts_with('~') {
        if let Some(base_dirs) = directories_next::BaseDirs::new() {
            target_path.replacen('~', &base_dirs.home_dir().to_string_lossy(), 1)
        } else {
            warn!("Could not determine home directory, using target path as-is");
            target_path.clone()
        }
    } else {
        target_path.clone()
    };

    let target = std::path::Path::new(&expanded_target);

    info!(
        "Executing dotfiles phase: repository={}, target={}",
        repository,
        target.display()
    );

    // Execute dotfiles with custom install command if provided
    let options = DotfilesOptions::default();
    let result = apply_dotfiles(&repository, target, &options).await?;

    // If custom install command was provided and no script was auto-detected,
    // execute the custom command
    if let Some(ref custom_cmd) = config.install_command {
        if !result.script_executed {
            info!("Executing custom dotfiles install command: {}", custom_cmd);
            execute_custom_install_command(target, custom_cmd).await?;
        }
    }

    Ok(DotfilesPhaseResult::executed(result))
}

/// Execute a custom install command in the dotfiles directory
#[instrument]
async fn execute_custom_install_command(target: &Path, command: &str) -> Result<()> {
    let output = Command::new("bash")
        .arg("-c")
        .arg(command)
        .current_dir(target)
        .output()
        .await
        .map_err(|e| {
            GitError::CLIError(format!("Failed to execute custom install command: {}", e))
        })?;

    if output.status.success() {
        info!("Custom install command executed successfully");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(GitError::CLIError(format!("Custom install command failed: {}", stderr)).into())
    }
}

/// Check if the dotfiles phase should be skipped based on lifecycle context.
///
/// This helper function encapsulates the skip logic for the dotfiles phase:
/// - Prebuild mode: Skip dotfiles (spec FR-006)
/// - --skip-post-create flag: Skip dotfiles (spec FR-005)
/// - Resume with marker: Skip dotfiles (spec FR-003)
/// - No dotfiles configured: Skip with reason
///
/// # Arguments
///
/// * `config` - Dotfiles phase configuration
/// * `is_prebuild` - Whether running in prebuild mode
/// * `skip_post_create` - Whether --skip-post-create flag is set
/// * `has_prior_marker` - Whether a prior completion marker exists
///
/// # Returns
///
/// `Some(reason)` if phase should be skipped, `None` if it should execute.
pub fn should_skip_dotfiles_phase(
    config: &DotfilesPhaseConfig,
    is_prebuild: bool,
    skip_post_create: bool,
    has_prior_marker: bool,
) -> Option<&'static str> {
    if is_prebuild {
        return Some("prebuild mode");
    }
    if skip_post_create {
        return Some("--skip-post-create flag");
    }
    if has_prior_marker {
        return Some("prior completion marker");
    }
    if !config.is_configured() {
        return Some("no dotfiles configured");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::process::Command;

    // Note: Tests that execute bash scripts are marked #[cfg(unix)] because
    // dotfiles install scripts are designed for Unix/Linux environments.
    // The production code runs these scripts inside Linux containers.

    #[tokio::test]
    async fn test_check_git_available() {
        // This test assumes git is installed in the test environment
        let result = check_git_available().await;
        assert!(
            result.is_ok(),
            "Git should be available in test environment"
        );
    }

    #[tokio::test]
    async fn test_detect_install_script_with_install_sh() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create install.sh file
        let install_script = temp_path.join("install.sh");
        fs::write(&install_script, "#!/bin/bash\necho 'Installing dotfiles'").unwrap();

        let result = detect_install_script(temp_path).await;
        assert_eq!(result, Some("install.sh".to_string()));
    }

    #[tokio::test]
    async fn test_detect_install_script_with_setup_sh() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create setup.sh file
        let setup_script = temp_path.join("setup.sh");
        fs::write(&setup_script, "#!/bin/bash\necho 'Setting up dotfiles'").unwrap();

        let result = detect_install_script(temp_path).await;
        assert_eq!(result, Some("setup.sh".to_string()));
    }

    #[tokio::test]
    async fn test_detect_install_script_priority() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create both scripts - install.sh should have priority
        let install_script = temp_path.join("install.sh");
        let setup_script = temp_path.join("setup.sh");
        fs::write(&install_script, "#!/bin/bash\necho 'Installing dotfiles'").unwrap();
        fs::write(&setup_script, "#!/bin/bash\necho 'Setting up dotfiles'").unwrap();

        let result = detect_install_script(temp_path).await;
        assert_eq!(result, Some("install.sh".to_string()));
    }

    #[tokio::test]
    async fn test_detect_install_script_none_found() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let result = detect_install_script(temp_path).await;
        assert_eq!(result, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_execute_install_script_success() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        // Create a simple install script that creates a marker file
        let install_script = temp_path.join("install.sh");
        let marker_file = temp_path.join("marker.txt");
        let script_content = format!(
            "#!/bin/bash\necho 'Install executed' > '{}'",
            marker_file.display()
        );
        fs::write(&install_script, script_content).unwrap();

        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&install_script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&install_script, perms).unwrap();
        }

        let result = execute_install_script(temp_path, "install.sh").await;
        assert!(result.is_ok());

        // Check that marker file was created
        assert!(marker_file.exists());
        let content = fs::read_to_string(&marker_file).unwrap();
        assert!(content.contains("Install executed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_apply_dotfiles_with_local_fixture() {
        // Create a local git repository fixture for testing
        let fixture_dir = TempDir::new().unwrap();
        let fixture_path = fixture_dir.path();
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path();

        // Initialize git repository in fixture
        let init_result = Command::new("git")
            .arg("init")
            .current_dir(fixture_path)
            .output()
            .await;

        if init_result.is_err() {
            // Skip test if git is not available
            return;
        }

        // Configure git user for the test repo
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        // Create a simple dotfile and install script
        let bashrc_file = fixture_path.join(".bashrc");
        fs::write(&bashrc_file, "# Test bashrc\nexport TEST_VAR=1").unwrap();

        let install_script = fixture_path.join("install.sh");
        let marker_file = format!("{}/install_marker.txt", target_path.display());
        let script_content = format!("#!/bin/bash\necho 'Dotfiles installed' > '{}'", marker_file);
        fs::write(&install_script, script_content).unwrap();

        // Make script executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&install_script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&install_script, perms).unwrap();
        }

        // Add files to git and commit
        Command::new("git")
            .args(["add", "."])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        // Test apply_dotfiles
        let options = DotfilesOptions::default();
        let repo_url = format!("file://{}", fixture_path.display());

        let result = apply_dotfiles(&repo_url, target_path, &options).await;
        assert!(result.is_ok());

        let dotfiles_result = result.unwrap();
        assert_eq!(dotfiles_result.target_path, target_path.to_string_lossy());
        assert!(dotfiles_result.script_executed);
        assert_eq!(dotfiles_result.script_name, Some("install.sh".to_string()));

        // Check that files were cloned
        let cloned_bashrc = target_path.join(".bashrc");
        assert!(cloned_bashrc.exists());

        // Check that install script was executed
        let marker_path = std::path::Path::new(&marker_file);
        assert!(marker_path.exists());
        let content = fs::read_to_string(marker_path).unwrap();
        assert!(content.contains("Dotfiles installed"));
    }

    #[tokio::test]
    async fn test_apply_dotfiles_without_install_script() {
        // Create a local git repository fixture without install script
        let fixture_dir = TempDir::new().unwrap();
        let fixture_path = fixture_dir.path();
        let target_dir = TempDir::new().unwrap();
        let target_path = target_dir.path();

        // Initialize git repository in fixture
        let init_result = Command::new("git")
            .arg("init")
            .current_dir(fixture_path)
            .output()
            .await;

        if init_result.is_err() {
            // Skip test if git is not available
            return;
        }

        // Configure git user for the test repo
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        // Create only a dotfile (no install script)
        let bashrc_file = fixture_path.join(".bashrc");
        fs::write(&bashrc_file, "# Test bashrc without install script").unwrap();

        // Add file to git and commit
        Command::new("git")
            .args(["add", ".bashrc"])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(fixture_path)
            .output()
            .await
            .unwrap();

        // Test apply_dotfiles
        let options = DotfilesOptions::default();
        let repo_url = format!("file://{}", fixture_path.display());

        let result = apply_dotfiles(&repo_url, target_path, &options).await;
        assert!(result.is_ok());

        let dotfiles_result = result.unwrap();
        assert_eq!(dotfiles_result.target_path, target_path.to_string_lossy());
        assert!(!dotfiles_result.script_executed);
        assert_eq!(dotfiles_result.script_name, None);

        // Check that files were cloned
        let cloned_bashrc = target_path.join(".bashrc");
        assert!(cloned_bashrc.exists());
    }

    #[test]
    fn test_dotfiles_configuration() {
        let config = DotfilesConfiguration {
            repository: "https://github.com/user/dotfiles".to_string(),
            install_command: Some("./setup.sh".to_string()),
            target_path: "/home/user/.dotfiles".to_string(),
        };

        assert_eq!(config.repository, "https://github.com/user/dotfiles");
        assert_eq!(config.install_command, Some("./setup.sh".to_string()));
        assert_eq!(config.target_path, "/home/user/.dotfiles");
    }

    #[test]
    fn test_dotfiles_options_default() {
        let options = DotfilesOptions::default();
        // Currently no fields to test, but ensures Default trait works
        assert!(format!("{:?}", options).contains("DotfilesOptions"));
    }

    // =========================================================================
    // Lifecycle Integration Tests
    // =========================================================================

    #[test]
    fn test_dotfiles_phase_config_default() {
        let config = DotfilesPhaseConfig::new();
        assert!(config.repository.is_none());
        assert!(config.target_path.is_none());
        assert!(config.install_command.is_none());
        assert!(!config.is_configured());
    }

    #[test]
    fn test_dotfiles_phase_config_builder() {
        let config = DotfilesPhaseConfig::new()
            .with_repository("https://github.com/user/dotfiles")
            .with_target_path("/home/user/.dotfiles")
            .with_install_command("./setup.sh");

        assert_eq!(
            config.repository,
            Some("https://github.com/user/dotfiles".to_string())
        );
        assert_eq!(config.target_path, Some("/home/user/.dotfiles".to_string()));
        assert_eq!(config.install_command, Some("./setup.sh".to_string()));
        assert!(config.is_configured());
    }

    #[test]
    fn test_dotfiles_phase_config_is_configured() {
        // Not configured without repository
        let empty = DotfilesPhaseConfig::new();
        assert!(!empty.is_configured());

        // Configured with only repository
        let with_repo =
            DotfilesPhaseConfig::new().with_repository("https://github.com/user/dotfiles");
        assert!(with_repo.is_configured());

        // Target path alone does not make it configured
        let with_target_only = DotfilesPhaseConfig {
            repository: None,
            target_path: Some("/path".to_string()),
            install_command: None,
        };
        assert!(!with_target_only.is_configured());
    }

    #[test]
    fn test_dotfiles_phase_result_skipped() {
        let result = DotfilesPhaseResult::skipped("test reason");
        assert!(!result.was_configured);
        assert!(result.skipped);
        assert_eq!(result.skip_reason, Some("test reason".to_string()));
        assert!(result.result.is_none());
    }

    #[test]
    fn test_dotfiles_phase_result_executed() {
        let dotfiles_result = DotfilesResult {
            target_path: "/home/user/.dotfiles".to_string(),
            script_executed: true,
            script_name: Some("install.sh".to_string()),
        };

        let result = DotfilesPhaseResult::executed(dotfiles_result);
        assert!(result.was_configured);
        assert!(!result.skipped);
        assert!(result.skip_reason.is_none());
        assert!(result.result.is_some());

        let inner = result.result.unwrap();
        assert_eq!(inner.target_path, "/home/user/.dotfiles");
        assert!(inner.script_executed);
        assert_eq!(inner.script_name, Some("install.sh".to_string()));
    }

    #[test]
    fn test_should_skip_dotfiles_phase_prebuild() {
        let config = DotfilesPhaseConfig::new().with_repository("https://github.com/user/dotfiles");

        // Prebuild mode should skip
        let reason = should_skip_dotfiles_phase(&config, true, false, false);
        assert_eq!(reason, Some("prebuild mode"));
    }

    #[test]
    fn test_should_skip_dotfiles_phase_skip_post_create() {
        let config = DotfilesPhaseConfig::new().with_repository("https://github.com/user/dotfiles");

        // --skip-post-create should skip
        let reason = should_skip_dotfiles_phase(&config, false, true, false);
        assert_eq!(reason, Some("--skip-post-create flag"));
    }

    #[test]
    fn test_should_skip_dotfiles_phase_prior_marker() {
        let config = DotfilesPhaseConfig::new().with_repository("https://github.com/user/dotfiles");

        // Prior marker should skip
        let reason = should_skip_dotfiles_phase(&config, false, false, true);
        assert_eq!(reason, Some("prior completion marker"));
    }

    #[test]
    fn test_should_skip_dotfiles_phase_not_configured() {
        let config = DotfilesPhaseConfig::new();

        // No repository configured should skip
        let reason = should_skip_dotfiles_phase(&config, false, false, false);
        assert_eq!(reason, Some("no dotfiles configured"));
    }

    #[test]
    fn test_should_skip_dotfiles_phase_should_execute() {
        let config = DotfilesPhaseConfig::new().with_repository("https://github.com/user/dotfiles");

        // All conditions false, should execute
        let reason = should_skip_dotfiles_phase(&config, false, false, false);
        assert!(reason.is_none());
    }

    #[test]
    fn test_should_skip_dotfiles_phase_precedence() {
        let config = DotfilesPhaseConfig::new().with_repository("https://github.com/user/dotfiles");

        // Multiple reasons: prebuild takes precedence
        let reason = should_skip_dotfiles_phase(&config, true, true, true);
        assert_eq!(reason, Some("prebuild mode"));

        // Without prebuild, skip_post_create takes precedence
        let reason = should_skip_dotfiles_phase(&config, false, true, true);
        assert_eq!(reason, Some("--skip-post-create flag"));

        // Without prebuild and skip_post_create, prior_marker is used
        let reason = should_skip_dotfiles_phase(&config, false, false, true);
        assert_eq!(reason, Some("prior completion marker"));
    }

    #[tokio::test]
    async fn test_execute_dotfiles_phase_not_configured() {
        let config = DotfilesPhaseConfig::new();
        let result = execute_dotfiles_phase(&config).await.unwrap();

        assert!(result.skipped);
        assert_eq!(
            result.skip_reason,
            Some("no dotfiles repository configured".to_string())
        );
        assert!(result.result.is_none());
    }
}
