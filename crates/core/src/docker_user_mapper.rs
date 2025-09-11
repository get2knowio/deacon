//! Docker implementation of user mapping functionality
//!
//! This module provides a concrete implementation of the UserMapper trait
//! that works with Docker containers via the Docker CLI.

use crate::docker::{CliDocker, Docker, ExecConfig};
use crate::errors::{DeaconError, DockerError, Result};
use crate::user_mapping::{UserInfo, UserMapper};
use async_trait::async_trait;
use std::collections::HashMap;
use tracing::{debug, warn};

/// Docker-based implementation of UserMapper
pub struct DockerUserMapper {
    docker: CliDocker,
}

impl DockerUserMapper {
    /// Create a new DockerUserMapper
    pub fn new(docker: CliDocker) -> Self {
        Self { docker }
    }

    /// Parse user information from /etc/passwd format
    fn parse_passwd_line(line: &str) -> Option<UserInfo> {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 7 {
            let username = parts[0].to_string();
            let uid = parts[2].parse::<u32>().ok()?;
            let gid = parts[3].parse::<u32>().ok()?;
            let home_dir = parts[5].to_string();
            let shell = parts[6].to_string();

            Some(UserInfo::new(username, uid, gid, home_dir, shell))
        } else {
            None
        }
    }

    /// Execute a command in the container and return success status
    async fn exec_command_success(&self, container_id: &str, command: &[String]) -> Result<bool> {
        debug!(
            "Executing command in container {}: {:?}",
            container_id, command
        );

        let config = ExecConfig {
            user: None,
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
        };

        let result = self
            .docker
            .exec(container_id, command, config)
            .await
            .map_err(|_e| DeaconError::Docker(DockerError::ExecFailed { code: 1 }))?;

        Ok(result.success)
    }

    /// Execute a command as a specific user
    async fn exec_command_as_user(
        &self,
        container_id: &str,
        username: &str,
        command: &[&str],
    ) -> Result<String> {
        // Build command to run as specific user
        let mut full_command = vec!["su", "-c"];
        let command_str = command.join(" ");
        full_command.push(&command_str);
        full_command.push(username);

        self.exec_command(container_id, &full_command).await
    }

    /// Check if we're running as root in the container
    async fn is_root(&self, container_id: &str) -> Result<bool> {
        let output = self.exec_command(container_id, &["id", "-u"]).await?;
        let uid = output.trim().parse::<u32>().unwrap_or(1000);
        Ok(uid == 0)
    }
}

#[async_trait]
impl UserMapper for DockerUserMapper {
    async fn get_current_user(&self, container_id: &str) -> Result<UserInfo> {
        debug!("Getting current user for container {}", container_id);

        // Get current user ID
        let uid_output = self.exec_command(container_id, &["id", "-u"]).await?;
        let uid = uid_output.trim().parse::<u32>().map_err(|e| {
            DeaconError::Docker(DockerError::CLIError(format!("Failed to parse UID: {}", e)))
        })?;

        // Get current user name
        let user_output = self.exec_command(container_id, &["whoami"]).await?;
        let username = user_output.trim();

        // Get user info from passwd
        if let Some(user_info) = self.get_user_info(container_id, username).await? {
            Ok(user_info)
        } else {
            // Fallback: construct basic user info
            let gid_output = self.exec_command(container_id, &["id", "-g"]).await?;
            let gid = gid_output.trim().parse::<u32>().unwrap_or(uid);

            Ok(UserInfo::new(
                username.to_string(),
                uid,
                gid,
                UserInfo::default_home_dir(username),
                UserInfo::default_shell(),
            ))
        }
    }

    async fn get_user_info(&self, container_id: &str, username: &str) -> Result<Option<UserInfo>> {
        debug!("Getting user info for {} in container {}", username, container_id);

        // Try to get user from /etc/passwd
        let passwd_output = self
            .exec_command(container_id, &["getent", "passwd", username])
            .await;

        match passwd_output {
            Ok(output) => {
                for line in output.lines() {
                    if let Some(user_info) = Self::parse_passwd_line(line.trim()) {
                        if user_info.username == username {
                            return Ok(Some(user_info));
                        }
                    }
                }
                Ok(None)
            }
            Err(_) => {
                // getent might not be available, try reading /etc/passwd directly
                let passwd_output = self
                    .exec_command(container_id, &["cat", "/etc/passwd"])
                    .await?;

                for line in passwd_output.lines() {
                    if let Some(user_info) = Self::parse_passwd_line(line.trim()) {
                        if user_info.username == username {
                            return Ok(Some(user_info));
                        }
                    }
                }
                Ok(None)
            }
        }
    }

    async fn user_exists(&self, container_id: &str, username: &str) -> Result<bool> {
        let user_info = self.get_user_info(container_id, username).await?;
        Ok(user_info.is_some())
    }

    async fn create_user(
        &self,
        container_id: &str,
        username: &str,
        uid: Option<u32>,
        gid: Option<u32>,
        home_dir: Option<String>,
        shell: Option<String>,
    ) -> Result<UserInfo> {
        debug!("Creating user {} in container {}", username, container_id);

        // Check if we have root permissions
        if !self.is_root(container_id).await? {
            return Err(DeaconError::Docker(DockerError::CLIError(format!(
                "Insufficient permissions to create user '{}' - container must run as root",
                username
            ))));
        }

        // Build useradd command as String vector
        let mut command = vec!["useradd".to_string()];

        // Add UID if specified
        if let Some(uid) = uid {
            command.push("-u".to_string());
            command.push(uid.to_string());
        }

        // Add GID if specified
        if let Some(gid) = gid {
            command.push("-g".to_string());
            command.push(gid.to_string());
        }

        // Add home directory if specified
        let home_dir = home_dir.unwrap_or_else(|| UserInfo::default_home_dir(username));
        command.push("-d".to_string());
        command.push(home_dir.clone());
        command.push("-m".to_string()); // Create home directory

        // Add shell if specified
        let shell = shell.unwrap_or_else(UserInfo::default_shell);
        command.push("-s".to_string());
        command.push(shell.clone());

        // Add username
        command.push(username.to_string());

        // Execute useradd command
        let success = self.exec_command_success(container_id, &command).await.map_err(|e| {
            warn!("Failed to create user {}: {}", username, e);
            DeaconError::Docker(DockerError::CLIError(format!(
                "Failed to create user '{}': {}",
                username, e
            )))
        })?;

        if !success {
            return Err(DeaconError::Docker(DockerError::CLIError(format!(
                "Failed to create user '{}'",
                username
            ))));
        }

        // Get the created user info
        self.get_user_info(container_id, username)
            .await?
            .ok_or_else(|| {
                DeaconError::Docker(DockerError::CLIError(format!(
                    "User '{}' was created but could not be found",
                    username
                )))
            })
    }

    async fn update_user_uid(
        &self,
        container_id: &str,
        username: &str,
        new_uid: u32,
        new_gid: u32,
    ) -> Result<()> {
        debug!(
            "Updating user {} UID to {} and GID to {} in container {}",
            username, new_uid, new_gid, container_id
        );

        // Check if we have root permissions
        if !self.is_root(container_id).await? {
            return Err(DeaconError::Docker(DockerError::CLIError(format!(
                "Insufficient permissions to update user '{}' - container must run as root",
                username
            ))));
        }

        // Update user UID
        let uid_str = new_uid.to_string();
        let gid_str = new_gid.to_string();
        let usermod_command = [
            "usermod",
            "-u",
            &uid_str,
            "-g",
            &gid_str,
            username,
        ];

        self.exec_command(container_id, &usermod_command)
            .await
            .map_err(|e| {
                warn!("Failed to update user {} UID/GID: {}", username, e);
                DeaconError::Docker(DockerError::CLIError(format!(
                    "Failed to update user '{}' UID/GID: {}",
                    username, e
                )))
            })?;

        Ok(())
    }

    async fn create_home_directory(&self, container_id: &str, user_info: &UserInfo) -> Result<()> {
        debug!(
            "Creating home directory {} for user {} in container {}",
            user_info.home_dir, user_info.username, container_id
        );

        // Create directory
        let mkdir_command = ["mkdir", "-p", &user_info.home_dir];
        self.exec_command(container_id, &mkdir_command).await?;

        // Set ownership
        let ownership_str = format!("{}:{}", user_info.uid, user_info.gid);
        let chown_command = ["chown", &ownership_str, &user_info.home_dir];
        self.exec_command(container_id, &chown_command).await?;

        // Set permissions
        let chmod_command = ["chmod", "755", &user_info.home_dir];
        self.exec_command(container_id, &chmod_command).await?;

        Ok(())
    }

    async fn set_workspace_ownership(
        &self,
        container_id: &str,
        workspace_path: &str,
        uid: u32,
        gid: u32,
    ) -> Result<()> {
        debug!(
            "Setting workspace ownership {} to {}:{} in container {}",
            workspace_path, uid, gid, container_id
        );

        // Set ownership recursively
        let ownership_str = format!("{}:{}", uid, gid);
        let chown_command = ["chown", "-R", &ownership_str, workspace_path];

        self.exec_command(container_id, &chown_command)
            .await
            .map_err(|e| {
                warn!("Failed to set workspace ownership: {}", e);
                DeaconError::Docker(DockerError::CLIError(format!(
                    "Failed to set workspace ownership: {}",
                    e
                )))
            })?;

        Ok(())
    }

    async fn execute_as_user(
        &self,
        container_id: &str,
        username: &str,
        command: &[String],
        env: Option<HashMap<String, String>>,
        working_dir: Option<String>,
    ) -> Result<String> {
        debug!(
            "Executing command as user {} in container {}: {:?}",
            username, container_id, command
        );

        // Convert command to string references
        let command_refs: Vec<&str> = command.iter().map(|s| s.as_ref()).collect();

        let config = ExecConfig {
            user: Some(username.to_string()),
            working_dir,
            env: env.unwrap_or_default(),
            tty: false,
            interactive: false,
            detach: false,
        };

        let result = self
            .docker
            .exec(container_id, command, config)
            .await
            .map_err(|e| DeaconError::Docker(DockerError::ExecFailed { code: 1 }))?;

        if !result.success {
            return Err(DeaconError::Docker(DockerError::ExecFailed {
                code: result.exit_code,
            }));
        }

        // Note: ExecResult doesn't include output, so this is a simplified implementation
        // In a real implementation, we'd need to capture stdout/stderr
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_passwd_line() {
        let line = "testuser:x:1000:1000:Test User:/home/testuser:/bin/bash";
        let user_info = DockerUserMapper::parse_passwd_line(line).unwrap();

        assert_eq!(user_info.username, "testuser");
        assert_eq!(user_info.uid, 1000);
        assert_eq!(user_info.gid, 1000);
        assert_eq!(user_info.home_dir, "/home/testuser");
        assert_eq!(user_info.shell, "/bin/bash");
    }

    #[test]
    fn test_parse_passwd_line_root() {
        let line = "root:x:0:0:root:/root:/bin/bash";
        let user_info = DockerUserMapper::parse_passwd_line(line).unwrap();

        assert_eq!(user_info.username, "root");
        assert_eq!(user_info.uid, 0);
        assert_eq!(user_info.gid, 0);
        assert_eq!(user_info.home_dir, "/root");
        assert_eq!(user_info.shell, "/bin/bash");
    }

    #[test]
    fn test_parse_passwd_line_invalid() {
        let line = "invalid:format";
        let user_info = DockerUserMapper::parse_passwd_line(line);
        assert!(user_info.is_none());
    }

    #[test]
    fn test_docker_user_mapper_creation() {
        let docker = CliDocker::new("docker");
        let mapper = DockerUserMapper::new(docker);
        // Just ensure it can be created
        assert!(true);
    }
}