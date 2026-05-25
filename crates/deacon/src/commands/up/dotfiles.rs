//! Dotfiles installation for the up command.
//!
//! **DEPRECATED**: As of T009, dotfiles execution has been integrated into
//! `container_lifecycle.rs` to ensure correct lifecycle ordering:
//! `postCreate -> dotfiles -> postStart` (per spec SC-001).
//!
//! This module is kept for reference but the `execute_dotfiles_installation`
//! function is no longer called. It may be removed in a future release.
//!
//! This module contains:
//! - `execute_dotfiles_installation` - Install dotfiles in container (DEPRECATED)

use super::args::UpArgs;
use anyhow::Result;
use deacon_core::config::DevContainerConfig;
use deacon_core::docker::{CliDocker, Docker, ExecConfig};
use std::collections::HashMap;
use tracing::{debug, info, instrument};

/// Execute dotfiles installation in the container if dotfiles flags are provided.
///
/// **DEPRECATED**: This function is no longer used. Dotfiles execution is now
/// integrated into `container_lifecycle.rs` via `execute_dotfiles_in_container`.
/// See T009 implementation for details.
///
/// T015: Dotfiles integration with container-side execution.
/// Per specs/001-up-gap-spec/ User Story 2:
/// - Dotfiles run after postCreate and before postStart (corrected ordering per SC-001)
/// - Dotfiles are user-specific and should NOT run in prebuild mode
/// - Uses git to clone repository inside container and executes install script
///
/// # Arguments
/// * `container_id` - Container to execute dotfiles installation in
/// * `config` - Devcontainer configuration
/// * `args` - Up command arguments containing dotfiles flags
///
/// # Returns
/// Ok(()) if dotfiles installation succeeds or if no dotfiles are configured.
/// Error if dotfiles installation fails.
#[deprecated(
    since = "0.1.5",
    note = "Use container_lifecycle.rs dotfiles integration instead (T009)"
)]
#[allow(dead_code)]
#[instrument(skip(config, args))]
pub(crate) async fn execute_dotfiles_installation(
    container_id: &str,
    config: &DevContainerConfig,
    args: &UpArgs,
    force_pty: bool,
) -> Result<()> {
    // Check if dotfiles repository is configured
    let dotfiles_repo = match &args.dotfiles_repository {
        Some(repo) => repo.clone(),
        None => {
            debug!("No dotfiles repository configured, skipping dotfiles installation");
            return Ok(());
        }
    };

    info!("Installing dotfiles from repository: {}", dotfiles_repo);

    // Determine target path for dotfiles
    // Default to user's home directory if not specified
    let remote_user = config
        .remote_user
        .as_ref()
        .or(config.container_user.as_ref())
        .unwrap_or(&"root".to_string())
        .clone();

    let default_target_path = if remote_user == "root" {
        "/root/.dotfiles".to_string()
    } else {
        format!("/home/{}/.dotfiles", remote_user)
    };

    let target_path = args
        .dotfiles_target_path
        .as_ref()
        .unwrap_or(&default_target_path)
        .clone();

    debug!(
        "Installing dotfiles to container path: {} as user: {}",
        target_path, remote_user
    );

    // Initialize Docker client
    let docker = CliDocker::with_path(args.docker_path.clone());

    let exec_config = ExecConfig {
        user: Some(remote_user.clone()),
        working_dir: None,
        env: HashMap::new(),
        tty: force_pty,
        interactive: false,
        detach: false,
        silent: false,
        terminal_size: None,
    };

    // T015: Step 0 - Check if dotfiles directory already exists (idempotency)
    let check_exists_command = vec!["test".to_string(), "-d".to_string(), target_path.clone()];

    let exists_result = docker
        .exec(container_id, &check_exists_command, exec_config.clone())
        .await?;

    // test -d returns exit code 0 if directory exists, 1 if not
    let dotfiles_exist = exists_result.success;
    debug!(
        "Directory exists check result: exit_code={}, success={}, dotfiles_exist={}",
        exists_result.exit_code, exists_result.success, dotfiles_exist
    );

    if dotfiles_exist {
        info!(
            "Dotfiles directory already exists at {}, removing to clone fresh",
            target_path
        );
        // Remove existing directory to ensure fresh clone
        let remove_command = vec!["rm".to_string(), "-rf".to_string(), target_path.clone()];

        debug!("Executing remove command: rm -rf {}", target_path);
        let remove_result = docker
            .exec(container_id, &remove_command, exec_config.clone())
            .await?;

        debug!(
            "Remove command result: success={}, exit_code={}, stdout={}, stderr={}",
            remove_result.success,
            remove_result.exit_code,
            remove_result.stdout,
            remove_result.stderr
        );

        if !remove_result.success {
            return Err(anyhow::anyhow!(
                "Failed to remove existing dotfiles directory (exit code {}): {}{}",
                remove_result.exit_code,
                remove_result.stdout,
                remove_result.stderr
            ));
        }

        debug!("Dotfiles directory removed successfully");
    }

    // T015: Step 1 - Clone dotfiles repository inside container using docker exec
    info!("Cloning dotfiles repository inside container");
    let clone_command = vec![
        "git".to_string(),
        "clone".to_string(),
        dotfiles_repo.clone(),
        target_path.clone(),
    ];

    let clone_result = docker
        .exec(container_id, &clone_command, exec_config.clone())
        .await?;

    // Check if git clone was successful
    if !clone_result.success {
        return Err(anyhow::anyhow!(
            "Failed to clone dotfiles repository (exit code {}): {}{}. Ensure git is installed and the repository URL is valid.",
            clone_result.exit_code,
            clone_result.stdout,
            clone_result.stderr
        ));
    }

    info!("Dotfiles repository cloned successfully");

    // T015: Step 2 - Determine and execute install script
    let install_command = if let Some(custom_command) = &args.dotfiles_install_command {
        // Use custom install command
        debug!("Using custom dotfiles install command: {}", custom_command);
        let parsed = shell_words::split(custom_command)
            .map_err(|e| anyhow::anyhow!("Invalid dotfiles install command syntax: {}", e))?;
        if parsed.is_empty() {
            return Err(anyhow::anyhow!(
                "Dotfiles install command is empty after parsing"
            ));
        }
        Some(parsed)
    } else {
        // Auto-detect install script
        debug!("Auto-detecting install script in dotfiles repository");

        let install_script = format!("{}/install.sh", target_path);
        let setup_script = format!("{}/setup.sh", target_path);
        let has_install = docker
            .exec(
                container_id,
                &["test".to_string(), "-f".to_string(), install_script],
                exec_config.clone(),
            )
            .await
            .map(|r| r.success)
            .unwrap_or(false);
        let has_setup = if has_install {
            false
        } else {
            docker
                .exec(
                    container_id,
                    &["test".to_string(), "-f".to_string(), setup_script],
                    exec_config.clone(),
                )
                .await
                .map(|r| r.success)
                .unwrap_or(false)
        };

        if has_install {
            debug!("Auto-detected install script: install.sh");
            Some(vec!["bash".to_string(), format!("{}/install.sh", target_path)])
        } else if has_setup {
            debug!("Auto-detected install script: setup.sh");
            Some(vec!["bash".to_string(), format!("{}/setup.sh", target_path)])
        } else {
            debug!("No install script found in dotfiles repository");
            None
        }
    };

    // T015: Step 3 - Execute install command if present
    if let Some(install_command) = install_command {
        info!("Executing dotfiles install command: {:?}", install_command);
        let mut install_exec_config = exec_config;
        install_exec_config.working_dir = Some(target_path.clone());

        let install_result = docker
            .exec(container_id, &install_command, install_exec_config)
            .await?;

        // Check if install command was successful
        if !install_result.success {
            return Err(anyhow::anyhow!(
                "Dotfiles install script failed (exit code {}): {}{}",
                install_result.exit_code,
                install_result.stdout,
                install_result.stderr
            ));
        }

        info!("Dotfiles install command completed successfully");
    } else {
        info!("No install script to execute, dotfiles cloned only");
    }

    Ok(())
}
