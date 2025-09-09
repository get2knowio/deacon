//! Dotfiles integration module
//!
//! This module provides functionality to clone dotfiles repositories and
//! execute their installation scripts, following the DevContainer CLI specification.

use crate::errors::{GitError, Result};
use std::path::Path;
use tokio::process::Command;
use tracing::{debug, info, instrument};

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
#[derive(Debug)]
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
    
    let output = Command::new("git")
        .arg("--version")
        .output()
        .await;

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
        info!("Successfully cloned dotfiles repository to: {}", target.display());
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
    info!("Applying dotfiles from {} to {}", repo_url, target.display());
    
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::process::Command;

    #[tokio::test]
    async fn test_check_git_available() {
        // This test assumes git is installed in the test environment
        let result = check_git_available().await;
        assert!(result.is_ok(), "Git should be available in test environment");
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
        let script_content = format!(
            "#!/bin/bash\necho 'Dotfiles installed' > '{}'",
            marker_file
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
}