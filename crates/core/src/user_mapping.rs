//! User mapping and remote user handling for DevContainers
//!
//! This module provides functionality for creating and mapping users inside containers,
//! managing UID/GID synchronization with the host, and ensuring proper permissions
//! for workspace files and directories.
//!
//! ## Key Features
//!
//! - Create remote users inside containers
//! - Map UID/GID between host and container users when `updateRemoteUserUID` is enabled
//! - Ensure proper home directory setup and workspace ownership
//! - Execute commands as the correct user context
//!
//! ## User Mapping Workflow
//!
//! 1. Parse `remoteUser`, `containerUser`, and `updateRemoteUserUID` configuration
//! 2. Detect current container user state
//! 3. Create or modify user/group inside container as needed
//! 4. Set up home directory with correct ownership
//! 5. Adjust workspace mount permissions
//! 6. Configure execution context for lifecycle commands

use crate::docker::{Docker, ExecConfig, ExecResult};
use crate::errors::{DeaconError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, instrument, warn};

/// User information structure for container operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInfo {
    /// User name
    pub username: String,
    /// User ID (UID)
    pub uid: u32,
    /// Primary group ID (GID)
    pub gid: u32,
    /// Home directory path
    pub home_dir: String,
    /// Login shell
    pub shell: String,
}

impl UserInfo {
    /// Create a new UserInfo instance
    pub fn new(username: String, uid: u32, gid: u32, home_dir: String, shell: String) -> Self {
        Self {
            username,
            uid,
            gid,
            home_dir,
            shell,
        }
    }

    /// Get the default shell for a user (typically /bin/bash)
    pub fn default_shell() -> String {
        "/bin/bash".to_string()
    }

    /// Generate a home directory path for a username
    pub fn default_home_dir(username: &str) -> String {
        if username == "root" {
            "/root".to_string()
        } else {
            format!("/home/{}", username)
        }
    }
}

/// Configuration for user mapping operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserMappingConfig {
    /// Name of the remote user to create/use
    pub remote_user: Option<String>,
    /// Name of the container user
    pub container_user: Option<String>,
    /// Whether to update the remote user's UID to match host
    pub update_remote_user_uid: bool,
    /// Host user UID (detected from environment)
    pub host_uid: Option<u32>,
    /// Host user GID (detected from environment)
    pub host_gid: Option<u32>,
    /// Workspace path for ownership adjustments
    pub workspace_path: Option<String>,
}

impl UserMappingConfig {
    /// Create a new UserMappingConfig
    pub fn new(
        remote_user: Option<String>,
        container_user: Option<String>,
        update_remote_user_uid: bool,
    ) -> Self {
        Self {
            remote_user,
            container_user,
            update_remote_user_uid,
            host_uid: None,
            host_gid: None,
            workspace_path: None,
        }
    }

    /// Set host user information
    pub fn with_host_user(mut self, uid: u32, gid: u32) -> Self {
        self.host_uid = Some(uid);
        self.host_gid = Some(gid);
        self
    }

    /// Set workspace path for ownership adjustments
    pub fn with_workspace_path(mut self, path: String) -> Self {
        self.workspace_path = Some(path);
        self
    }

    /// Check if user mapping is required
    pub fn needs_user_mapping(&self) -> bool {
        self.remote_user.is_some()
    }

    /// Check if UID mapping is required
    pub fn needs_uid_mapping(&self) -> bool {
        self.update_remote_user_uid && self.host_uid.is_some()
    }

    /// Get the effective user to use for command execution
    pub fn effective_user(&self) -> Option<&str> {
        self.remote_user
            .as_deref()
            .or(self.container_user.as_deref())
    }
}

/// Result of user mapping operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserMappingResult {
    /// Information about the created or mapped user
    pub user_info: UserInfo,
    /// Whether a new user was created
    pub user_created: bool,
    /// Whether UID/GID was updated
    pub uid_updated: bool,
    /// Whether the UID remap was WANTED but refused because the target UID is
    /// already owned by another user in the container (see [`decide_uid_remap`]).
    pub uid_update_skipped_uid_taken: bool,
    /// Whether home directory was created
    pub home_created: bool,
    /// Whether workspace ownership was adjusted
    pub workspace_ownership_adjusted: bool,
}

/// What `updateRemoteUserUID` should do for one remote user.
///
/// This is the decision half of the remap, extracted so it can be exercised
/// without a container. It mirrors the reference CLI's
/// `scripts/updateUID.Dockerfile` branch-for-branch — see [`decide_uid_remap`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UidRemapDecision {
    /// Nothing to do — the user already carries the target ids (or the only
    /// change the guards would have allowed is a no-op).
    NoChange,
    /// Another user already owns the target UID; the remote user is left alone
    /// so it keeps being the identity the configuration named.
    SkippedUidTaken {
        /// Name of the user that already owns the target UID.
        existing_user: String,
    },
    /// Perform the remap with these ids.
    Remap {
        /// UID to move the remote user to.
        uid: u32,
        /// GID to move the remote user to. Equal to the remote user's CURRENT
        /// gid when another group already owns the target gid.
        gid: u32,
    },
}

/// Decide whether `updateRemoteUserUID` may remap a user, mirroring the
/// reference CLI's `scripts/updateUID.Dockerfile`:
///
/// ```sh
/// if [ -z "$OLD_UID" ]; then …
/// elif [ "$OLD_UID" = "$NEW_UID" -a "$OLD_GID" = "$NEW_GID" ]; then …
/// elif [ "$OLD_UID" != "$NEW_UID" -a -n "$EXISTING_USER" ]; then
///     echo "User with UID exists ($EXISTING_USER=$NEW_UID)."
/// else
///     if [ "$OLD_GID" != "$NEW_GID" -a -n "$EXISTING_GROUP" ]; then
///         echo "Group with GID exists ($EXISTING_GROUP=$NEW_GID)."
///         NEW_GID="$OLD_GID"
///     fi
///     …
/// fi
/// ```
///
/// `existing_user` is the name of whatever `/etc/passwd` entry already holds
/// `target_uid` (`EXISTING_USER`), and `existing_group` the `/etc/group` entry
/// holding `target_gid` (`EXISTING_GROUP`); both are `None` when the id is free.
/// Neither can name the remote user itself in the branch that consults it: the
/// UID guard only fires when `current_uid != target_uid`, and the GID fallback
/// only when `current_gid != target_gid`.
///
/// Without the UID guard the container ends up with two `/etc/passwd` entries
/// sharing one uid, and every name lookup for it resolves to the OTHER user —
/// see [issue #618](https://github.com/get2knowio/deacon/issues/618).
pub fn decide_uid_remap(
    current_uid: u32,
    current_gid: u32,
    target_uid: u32,
    target_gid: u32,
    existing_user: Option<&str>,
    existing_group: Option<&str>,
) -> UidRemapDecision {
    // `OLD_UID = NEW_UID -a OLD_GID = NEW_GID` — "UIDs and GIDs are the same".
    if current_uid == target_uid && current_gid == target_gid {
        return UidRemapDecision::NoChange;
    }

    // `OLD_UID != NEW_UID -a -n "$EXISTING_USER"` — "User with UID exists".
    if current_uid != target_uid {
        if let Some(existing_user) = existing_user {
            return UidRemapDecision::SkippedUidTaken {
                existing_user: existing_user.to_string(),
            };
        }
    }

    // `OLD_GID != NEW_GID -a -n "$EXISTING_GROUP"` — "Group with GID exists",
    // which keeps OLD_GID and remaps the uid only.
    let gid = if current_gid != target_gid && existing_group.is_some() {
        current_gid
    } else {
        target_gid
    };

    if current_uid == target_uid && current_gid == gid {
        // The GID fallback ate the only change there was to make.
        return UidRemapDecision::NoChange;
    }

    UidRemapDecision::Remap {
        uid: target_uid,
        gid,
    }
}

/// Error types specific to user mapping operations
#[derive(thiserror::Error, Debug)]
pub enum UserMappingError {
    #[error("Insufficient permissions to create user '{username}' - container must run as root")]
    InsufficientPermissions { username: String },

    #[error(
        "User '{username}' already exists with different UID {existing_uid}, cannot update to {target_uid}"
    )]
    UserExistsWithDifferentUid {
        username: String,
        existing_uid: u32,
        target_uid: u32,
    },

    #[error("Failed to create home directory '{home_dir}': {reason}")]
    HomeDirectoryCreationFailed { home_dir: String, reason: String },

    #[error("Failed to adjust workspace ownership: {reason}")]
    WorkspaceOwnershipFailed { reason: String },

    #[error("Command execution failed: {command} - {error}")]
    CommandExecutionFailed { command: String, error: String },

    #[error("Failed to parse user information: {reason}")]
    UserInfoParsingFailed { reason: String },
}

/// Trait for user mapping operations in containers
#[allow(async_fn_in_trait)]
pub trait UserMapper {
    /// Get information about the current user inside the container
    async fn get_current_user(&self, container_id: &str) -> Result<UserInfo>;

    /// Get information about a specific user by name
    async fn get_user_info(&self, container_id: &str, username: &str) -> Result<Option<UserInfo>>;

    /// Check if a user exists in the container
    async fn user_exists(&self, container_id: &str, username: &str) -> Result<bool>;

    /// Name of the `/etc/passwd` entry that currently owns `uid`, if any.
    ///
    /// This is the reference CLI's `EXISTING_USER` lookup; it is what stops
    /// `updateRemoteUserUID` from stamping a second user onto an occupied uid.
    async fn find_user_by_uid(&self, container_id: &str, uid: u32) -> Result<Option<String>>;

    /// Name of the `/etc/group` entry that currently owns `gid`, if any.
    ///
    /// The reference CLI's `EXISTING_GROUP` lookup.
    async fn find_group_by_gid(&self, container_id: &str, gid: u32) -> Result<Option<String>>;

    /// Create a new user in the container
    async fn create_user(
        &self,
        container_id: &str,
        username: &str,
        uid: Option<u32>,
        gid: Option<u32>,
        home_dir: Option<String>,
        shell: Option<String>,
    ) -> Result<UserInfo>;

    /// Update an existing user's UID/GID
    async fn update_user_uid(
        &self,
        container_id: &str,
        username: &str,
        new_uid: u32,
        new_gid: u32,
    ) -> Result<()>;

    /// Create a home directory for a user
    async fn create_home_directory(&self, container_id: &str, user_info: &UserInfo) -> Result<()>;

    /// Set ownership of workspace directory
    async fn set_workspace_ownership(
        &self,
        container_id: &str,
        workspace_path: &str,
        uid: u32,
        gid: u32,
    ) -> Result<()>;

    /// Execute a command as a specific user
    async fn execute_as_user(
        &self,
        container_id: &str,
        username: &str,
        command: &[String],
        env: Option<HashMap<String, String>>,
        working_dir: Option<String>,
    ) -> Result<String>;
}

/// User mapping service that implements the DevContainer user mapping workflow
pub struct UserMappingService<T: UserMapper> {
    user_mapper: T,
}

impl<T: UserMapper> UserMappingService<T> {
    /// Create a new UserMappingService
    pub fn new(user_mapper: T) -> Self {
        Self { user_mapper }
    }

    /// Apply user mapping configuration to a container
    ///
    /// This is the main entry point for user mapping operations. It:
    /// 1. Analyzes the configuration to determine what actions are needed
    /// 2. Creates or updates users as required
    /// 3. Sets up home directories and workspace ownership
    /// 4. Returns a summary of actions taken
    #[instrument(skip(self, config), fields(container_id = %container_id))]
    pub async fn apply_user_mapping(
        &self,
        container_id: &str,
        config: &UserMappingConfig,
    ) -> Result<UserMappingResult> {
        debug!(
            "Applying user mapping configuration to container {}",
            container_id
        );

        // Check if user mapping is needed
        if !config.needs_user_mapping() {
            debug!("No user mapping required");
            // Return current user info
            let current_user = self.user_mapper.get_current_user(container_id).await?;
            return Ok(UserMappingResult {
                user_info: current_user,
                user_created: false,
                uid_updated: false,
                uid_update_skipped_uid_taken: false,
                home_created: false,
                workspace_ownership_adjusted: false,
            });
        }

        let remote_user = config.remote_user.as_ref().unwrap();
        debug!("Remote user specified: {}", remote_user);

        // Check if user already exists
        let existing_user = self
            .user_mapper
            .get_user_info(container_id, remote_user)
            .await?;

        let mut result = UserMappingResult {
            user_info: UserInfo::new(
                remote_user.clone(),
                0,
                0,
                UserInfo::default_home_dir(remote_user),
                UserInfo::default_shell(),
            ),
            user_created: false,
            uid_updated: false,
            uid_update_skipped_uid_taken: false,
            home_created: false,
            workspace_ownership_adjusted: false,
        };

        match existing_user {
            Some(user_info) => {
                debug!(
                    "User {} already exists with UID {}",
                    remote_user, user_info.uid
                );
                result.user_info = user_info.clone();

                // Check if UID update is needed
                if config.needs_uid_mapping() {
                    let target_uid = config.host_uid.unwrap();
                    let target_gid = config.host_gid.unwrap_or(target_uid);

                    if user_info.uid != target_uid || user_info.gid != target_gid {
                        // The reference CLI refuses to move a user onto ids
                        // another entry already owns; ask the container who
                        // holds them before deciding (#618). Each lookup is
                        // only consulted by the branch that can be reached
                        // when the corresponding id actually differs, so we
                        // only pay for it then.
                        let existing_user = if user_info.uid != target_uid {
                            self.user_mapper
                                .find_user_by_uid(container_id, target_uid)
                                .await?
                        } else {
                            None
                        };
                        let existing_group = if user_info.gid != target_gid {
                            self.user_mapper
                                .find_group_by_gid(container_id, target_gid)
                                .await?
                        } else {
                            None
                        };

                        match decide_uid_remap(
                            user_info.uid,
                            user_info.gid,
                            target_uid,
                            target_gid,
                            existing_user.as_deref(),
                            existing_group.as_deref(),
                        ) {
                            UidRemapDecision::NoChange => {}
                            UidRemapDecision::SkippedUidTaken { existing_user } => {
                                // Mirrors the reference's
                                // `echo "User with UID exists ($EXISTING_USER=$NEW_UID)."`.
                                // Remapping anyway would give two /etc/passwd
                                // entries one uid, and every name lookup for
                                // it would resolve to `existing_user`.
                                warn!(
                                    "Skipping updateRemoteUserUID for '{}': UID {} is already owned by '{}'; keeping {}:{}",
                                    remote_user,
                                    target_uid,
                                    existing_user,
                                    user_info.uid,
                                    user_info.gid
                                );
                                result.uid_update_skipped_uid_taken = true;
                            }
                            UidRemapDecision::Remap { uid, gid } => {
                                if gid != target_gid {
                                    debug!(
                                        "GID {} is already owned by another group; keeping GID {} for user {}",
                                        target_gid, gid, remote_user
                                    );
                                }
                                debug!(
                                    "Updating user {} UID from {} to {} and GID from {} to {}",
                                    remote_user, user_info.uid, uid, user_info.gid, gid
                                );

                                self.user_mapper
                                    .update_user_uid(container_id, remote_user, uid, gid)
                                    .await?;

                                result.user_info.uid = uid;
                                result.user_info.gid = gid;
                                result.uid_updated = true;
                            }
                        }
                    }
                }
            }
            None => {
                debug!("Creating new user: {}", remote_user);

                // Determine UID/GID for new user
                let (uid, gid) = if config.needs_uid_mapping() {
                    let host_uid = config.host_uid.unwrap();
                    let host_gid = config.host_gid.unwrap_or(host_uid);
                    (Some(host_uid), Some(host_gid))
                } else {
                    (None, None) // Let system assign
                };

                let user_info = self
                    .user_mapper
                    .create_user(
                        container_id,
                        remote_user,
                        uid,
                        gid,
                        Some(UserInfo::default_home_dir(remote_user)),
                        Some(UserInfo::default_shell()),
                    )
                    .await?;

                result.user_info = user_info;
                result.user_created = true;
            }
        }

        // Ensure home directory exists and has correct ownership
        if !self
            .home_directory_exists(container_id, &result.user_info)
            .await?
        {
            debug!("Creating home directory: {}", result.user_info.home_dir);
            self.user_mapper
                .create_home_directory(container_id, &result.user_info)
                .await?;
            result.home_created = true;
        }

        // Set workspace ownership if specified.
        //
        // Skip when the target user is root (uid 0): chowning the workspace to
        // root is a no-op for access (root can already read/write everything)
        // and, because the workspace is a bind mount of a host directory,
        // `chown 0:0` inside the container rewrites the HOST directory's owner
        // to root — corrupting the developer's workspace (e.g. `remoteUser:
        // root` fixtures flipping the repo to root:root). Only adjust ownership
        // for a real, non-root target user.
        //
        // Skip for the same reason when the UID remap was REFUSED because the
        // host uid is taken (#618): the remote user then keeps an image-assigned
        // uid that has nothing to do with the host's, so chowning the bind mount
        // to it would take the developer's own workspace away from them. The
        // reference CLI does not chown the workspace at all — its `updateUID`
        // script only chowns `$HOME`, and only inside the branch that actually
        // remaps — so skipping here is also the closer behaviour.
        if let Some(ref workspace_path) = config.workspace_path {
            if result.user_info.uid == 0 {
                debug!(
                    "Skipping workspace ownership adjustment for {}: target user is root (uid 0)",
                    workspace_path
                );
            } else if result.uid_update_skipped_uid_taken {
                debug!(
                    "Skipping workspace ownership adjustment for {}: the UID remap was refused, \
                     so {} keeps its image-assigned uid {} and chowning a host bind mount to it \
                     would strip the host user's access",
                    workspace_path, result.user_info.username, result.user_info.uid
                );
            } else {
                debug!(
                    "Setting workspace ownership: {} -> {}:{}",
                    workspace_path, result.user_info.uid, result.user_info.gid
                );
                self.user_mapper
                    .set_workspace_ownership(
                        container_id,
                        workspace_path,
                        result.user_info.uid,
                        result.user_info.gid,
                    )
                    .await?;
                result.workspace_ownership_adjusted = true;
            }
        }

        debug!(
            "User mapping complete: user_created={}, uid_updated={}, home_created={}, workspace_adjusted={}",
            result.user_created,
            result.uid_updated,
            result.home_created,
            result.workspace_ownership_adjusted
        );

        Ok(result)
    }

    /// Check if a home directory exists for the user
    async fn home_directory_exists(
        &self,
        container_id: &str,
        user_info: &UserInfo,
    ) -> Result<bool> {
        // Use a simple test command to check if home directory exists
        let check_cmd = vec![
            "test".to_string(),
            "-d".to_string(),
            user_info.home_dir.clone(),
        ];

        match self
            .user_mapper
            .execute_as_user(container_id, "root", &check_cmd, None, None)
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false), // Directory doesn't exist or other error
        }
    }

    /// Execute a command as the configured user
    ///
    /// This method determines the correct user context for command execution
    /// based on the user mapping configuration.
    #[instrument(skip(self, config, command))]
    pub async fn execute_command_as_user(
        &self,
        container_id: &str,
        config: &UserMappingConfig,
        command: &[String],
        env: Option<HashMap<String, String>>,
        working_dir: Option<String>,
    ) -> Result<String> {
        let effective_user = config.effective_user().unwrap_or("root");

        debug!(
            "Executing command as user '{}': {:?}",
            effective_user, command
        );

        self.user_mapper
            .execute_as_user(container_id, effective_user, command, env, working_dir)
            .await
    }
}

/// Get the current host user UID and GID
///
/// This function detects the current user's UID and GID on the host system.
/// It's used to determine the target UID/GID when `updateRemoteUserUID` is enabled.
#[cfg(unix)]
pub async fn get_host_user_info() -> Result<(u32, u32)> {
    // Try environment variables first (fast path, no process spawn)
    if let Ok(uid_str) = std::env::var("UID") {
        if let Ok(uid) = uid_str.parse::<u32>() {
            let gid = std::env::var("GID")
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(uid);

            debug!("Host user info from environment: UID={}, GID={}", uid, gid);
            return Ok((uid, gid));
        }
    }

    // Fallback: use async tokio::process::Command to get UID/GID
    use tokio::process::Command;

    let output =
        Command::new("id")
            .arg("-u")
            .output()
            .await
            .map_err(|e| DeaconError::NotImplemented {
                feature: format!("Failed to get host UID: {}", e),
            })?;

    let uid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let uid = uid_str
        .parse::<u32>()
        .map_err(|e| DeaconError::NotImplemented {
            feature: format!("Failed to parse UID '{}': {}", uid_str, e),
        })?;

    let output =
        Command::new("id")
            .arg("-g")
            .output()
            .await
            .map_err(|e| DeaconError::NotImplemented {
                feature: format!("Failed to get host GID: {}", e),
            })?;

    let gid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let gid = gid_str
        .parse::<u32>()
        .map_err(|e| DeaconError::NotImplemented {
            feature: format!("Failed to parse GID '{}': {}", gid_str, e),
        })?;

    debug!("Host user info from id command: UID={}, GID={}", uid, gid);
    Ok((uid, gid))
}

/// Get the current host user UID and GID (Windows stub)
///
/// On Windows, this always returns an error since UID/GID mapping
/// is not applicable.
#[cfg(not(unix))]
pub async fn get_host_user_info() -> Result<(u32, u32)> {
    Err(DeaconError::NotImplemented {
        feature: "Host user UID/GID detection on non-Unix systems".to_string(),
    })
}

/// Docker-based implementation of [`UserMapper`] that executes commands via `docker exec`.
///
/// This bridges the `UserMapper` trait to the `Docker` trait, using container exec
/// calls to query and modify users inside running containers.
pub struct DockerUserMapper<T: Docker> {
    docker: T,
}

impl<T: Docker> DockerUserMapper<T> {
    /// Create a new `DockerUserMapper` wrapping the given Docker runtime.
    pub fn new(docker: T) -> Self {
        Self { docker }
    }

    /// Execute a command as root (silent, non-interactive) and return the `ExecResult`.
    async fn exec_silent(
        &self,
        container_id: &str,
        command: &[String],
        user: Option<&str>,
    ) -> Result<ExecResult> {
        let config = ExecConfig {
            user: user.map(|u| u.to_string()),
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
            silent: true,
            stdout_to_stderr: false,
            terminal_size: None,
        };
        self.docker.exec(container_id, command, config).await
    }

    /// Name of the first `database` record (a `/etc/passwd` or `/etc/group`
    /// path) whose third colon-separated field is `id`, or `None`.
    ///
    /// Reads the file directly rather than going through `getent`, because that
    /// is what the reference CLI's `updateUID.Dockerfile` does — its
    /// `EXISTING_USER` / `EXISTING_GROUP` are `sed` matches over `/etc/passwd`
    /// and `/etc/group`. NSS sources beyond the files are deliberately not
    /// consulted: the remap rewrites those two files, so they are the only
    /// place a collision can be created or observed.
    ///
    /// `awk` over `sed` for the same reason `update_user_uid` uses it: these are
    /// fixed-position colon-separated records, `awk` is in BusyBox, and there
    /// is no regex escaping to get wrong.
    async fn find_id_owner(
        &self,
        container_id: &str,
        database: &str,
        id: u32,
    ) -> Result<Option<String>> {
        let script = format!(
            "awk -F: -v id={} '$3==id {{ print $1; exit }}' {}",
            id, database
        );
        let cmd = vec!["sh".to_string(), "-c".to_string(), script];
        let result = self.exec_silent(container_id, &cmd, Some("root")).await?;
        if !result.success {
            return Err(UserMappingError::CommandExecutionFailed {
                command: format!("lookup of id {} in {}", id, database),
                error: result.stderr.trim().to_string(),
            }
            .into());
        }
        let name = result.stdout.trim();
        if name.is_empty() {
            Ok(None)
        } else {
            Ok(Some(name.to_string()))
        }
    }

    /// Parse `id -u -n` / `id` output into a `UserInfo`.
    fn parse_user_info_from_passwd(line: &str, username: &str) -> Result<UserInfo> {
        // Expected format from getent passwd: username:x:uid:gid:gecos:home:shell
        let parts: Vec<&str> = line.trim().split(':').collect();
        if parts.len() < 7 {
            return Err(UserMappingError::UserInfoParsingFailed {
                reason: format!(
                    "Expected passwd format (7+ colon-separated fields), got: {}",
                    line.trim()
                ),
            }
            .into());
        }
        let uid = parts[2]
            .parse::<u32>()
            .map_err(|e| UserMappingError::UserInfoParsingFailed {
                reason: format!("Cannot parse UID '{}': {}", parts[2], e),
            })?;
        let gid = parts[3]
            .parse::<u32>()
            .map_err(|e| UserMappingError::UserInfoParsingFailed {
                reason: format!("Cannot parse GID '{}': {}", parts[3], e),
            })?;
        let home_dir = parts[5].to_string();
        let shell = parts[6].to_string();

        Ok(UserInfo {
            username: username.to_string(),
            uid,
            gid,
            home_dir,
            shell,
        })
    }
}

// Convert UserMappingError into DeaconError for the Result type
impl From<UserMappingError> for DeaconError {
    fn from(err: UserMappingError) -> Self {
        DeaconError::Runtime(err.to_string())
    }
}

impl<T: Docker + Send + Sync> UserMapper for DockerUserMapper<T> {
    async fn get_current_user(&self, container_id: &str) -> Result<UserInfo> {
        // Get current user via `id -u -n` then fetch full info from passwd
        let cmd = vec!["id".to_string(), "-u".to_string(), "-n".to_string()];
        let result = self.exec_silent(container_id, &cmd, None).await?;
        if !result.success {
            return Err(UserMappingError::CommandExecutionFailed {
                command: "id -u -n".to_string(),
                error: result.stderr.trim().to_string(),
            }
            .into());
        }
        let username = result.stdout.trim().to_string();
        // Now get full info
        match self.get_user_info(container_id, &username).await? {
            Some(info) => Ok(info),
            None => {
                // Fallback: construct minimal info from `id`
                let uid_cmd = vec!["id".to_string(), "-u".to_string()];
                let gid_cmd = vec!["id".to_string(), "-g".to_string()];
                let uid_res = self.exec_silent(container_id, &uid_cmd, None).await?;
                let gid_res = self.exec_silent(container_id, &gid_cmd, None).await?;
                let uid = uid_res.stdout.trim().parse::<u32>().unwrap_or(0);
                let gid = gid_res.stdout.trim().parse::<u32>().unwrap_or(0);
                Ok(UserInfo::new(
                    username.clone(),
                    uid,
                    gid,
                    UserInfo::default_home_dir(&username),
                    UserInfo::default_shell(),
                ))
            }
        }
    }

    async fn get_user_info(&self, container_id: &str, username: &str) -> Result<Option<UserInfo>> {
        let cmd = vec![
            "getent".to_string(),
            "passwd".to_string(),
            username.to_string(),
        ];
        let result = self.exec_silent(container_id, &cmd, Some("root")).await?;
        if !result.success {
            // User does not exist (getent returns non-zero)
            return Ok(None);
        }
        let line = result.stdout.trim();
        if line.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self::parse_user_info_from_passwd(line, username)?))
    }

    async fn user_exists(&self, container_id: &str, username: &str) -> Result<bool> {
        let cmd = vec!["id".to_string(), "-u".to_string(), username.to_string()];
        let result = self.exec_silent(container_id, &cmd, Some("root")).await?;
        Ok(result.success)
    }

    async fn find_user_by_uid(&self, container_id: &str, uid: u32) -> Result<Option<String>> {
        self.find_id_owner(container_id, "/etc/passwd", uid).await
    }

    async fn find_group_by_gid(&self, container_id: &str, gid: u32) -> Result<Option<String>> {
        self.find_id_owner(container_id, "/etc/group", gid).await
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
        let home = home_dir.unwrap_or_else(|| UserInfo::default_home_dir(username));
        let shell_path = shell.unwrap_or_else(UserInfo::default_shell);

        // Create group first if gid is specified
        if let Some(gid_val) = gid {
            // Check if group with this GID already exists
            let check_cmd = vec![
                "getent".to_string(),
                "group".to_string(),
                gid_val.to_string(),
            ];
            let check_result = self
                .exec_silent(container_id, &check_cmd, Some("root"))
                .await?;
            if !check_result.success {
                // Group doesn't exist, create it
                let groupadd_cmd = vec![
                    "groupadd".to_string(),
                    "-g".to_string(),
                    gid_val.to_string(),
                    username.to_string(),
                ];
                let group_result = self
                    .exec_silent(container_id, &groupadd_cmd, Some("root"))
                    .await?;
                if !group_result.success {
                    warn!(
                        "groupadd failed (may be expected in minimal images): {}",
                        group_result.stderr.trim()
                    );
                    // Try addgroup (Alpine/BusyBox)
                    let addgroup_cmd = vec![
                        "addgroup".to_string(),
                        "-g".to_string(),
                        gid_val.to_string(),
                        username.to_string(),
                    ];
                    let alt_result = self
                        .exec_silent(container_id, &addgroup_cmd, Some("root"))
                        .await?;
                    if !alt_result.success {
                        debug!("addgroup also failed: {}", alt_result.stderr.trim());
                    }
                }
            }
        }

        // Create user with useradd (GNU) first, fall back to adduser (BusyBox/Alpine)
        let mut useradd_cmd = vec!["useradd".to_string()];
        if let Some(uid_val) = uid {
            useradd_cmd.extend(["--uid".to_string(), uid_val.to_string()]);
        }
        if let Some(gid_val) = gid {
            useradd_cmd.extend(["--gid".to_string(), gid_val.to_string()]);
        }
        useradd_cmd.extend([
            "--home-dir".to_string(),
            home.clone(),
            "--shell".to_string(),
            shell_path.clone(),
            "--create-home".to_string(),
            username.to_string(),
        ]);

        let result = self
            .exec_silent(container_id, &useradd_cmd, Some("root"))
            .await?;

        if !result.success {
            debug!(
                "useradd failed, trying adduser (BusyBox): {}",
                result.stderr.trim()
            );
            // BusyBox adduser fallback
            let mut adduser_cmd = vec![
                "adduser".to_string(),
                "-D".to_string(), // don't set password
                "-h".to_string(),
                home.clone(),
                "-s".to_string(),
                shell_path.clone(),
            ];
            if let Some(uid_val) = uid {
                adduser_cmd.extend(["-u".to_string(), uid_val.to_string()]);
            }
            if let Some(gid_val) = gid {
                adduser_cmd.extend(["-G".to_string(), username.to_string()]);
                // Ensure group exists for BusyBox too
                let _ = gid_val; // already handled above
            }
            adduser_cmd.push(username.to_string());

            let alt_result = self
                .exec_silent(container_id, &adduser_cmd, Some("root"))
                .await?;
            if !alt_result.success {
                return Err(UserMappingError::CommandExecutionFailed {
                    command: format!("useradd/adduser {}", username),
                    error: format!(
                        "useradd: {}; adduser: {}",
                        result.stderr.trim(),
                        alt_result.stderr.trim()
                    ),
                }
                .into());
            }
        }

        let final_uid = uid.unwrap_or(1000);
        let final_gid = gid.unwrap_or(final_uid);

        // Re-read actual user info from the container to get accurate values
        if let Some(info) = self.get_user_info(container_id, username).await? {
            Ok(info)
        } else {
            Ok(UserInfo::new(
                username.to_string(),
                final_uid,
                final_gid,
                home,
                shell_path,
            ))
        }
    }

    async fn update_user_uid(
        &self,
        container_id: &str,
        username: &str,
        new_uid: u32,
        new_gid: u32,
    ) -> Result<()> {
        debug!(
            "Updating UID/GID for user {} to {}:{}",
            username, new_uid, new_gid
        );

        // Update GID first with groupmod
        let groupmod_cmd = vec![
            "groupmod".to_string(),
            "-g".to_string(),
            new_gid.to_string(),
            username.to_string(),
        ];
        let gid_result = self
            .exec_silent(container_id, &groupmod_cmd, Some("root"))
            .await?;
        if !gid_result.success {
            // Non-fatal: group may not have same name as user, or groupmod unavailable
            debug!("groupmod failed (non-fatal): {}", gid_result.stderr.trim());
        }

        // Update UID with usermod
        let usermod_cmd = vec![
            "usermod".to_string(),
            "-u".to_string(),
            new_uid.to_string(),
            "-g".to_string(),
            new_gid.to_string(),
            username.to_string(),
        ];
        let uid_result = self
            .exec_silent(container_id, &usermod_cmd, Some("root"))
            .await?;
        if !uid_result.success {
            debug!(
                "usermod unavailable (likely BusyBox/Alpine), falling back to /etc/passwd + /etc/group edits: {}",
                uid_result.stderr.trim()
            );

            // BusyBox / Alpine fallback: shadow-utils' `usermod` isn't
            // present, so patch /etc/passwd and /etc/group directly with
            // `awk`. This mirrors what the upstream @devcontainers/cli
            // does on the same images. Home-directory chown happens
            // separately via `set_workspace_ownership` / lifecycle.
            //
            // Why awk and not sed: passwd/group lines are fixed-position
            // colon-separated records; awk is in BusyBox and avoids the
            // regex-escaping hazards of sed when the username contains
            // dots/dashes.
            let passwd_script = format!(
                "awk -F: -v OFS=: -v u={} -v nuid={} -v ngid={} \
                 '{{ if ($1==u) {{ $3=nuid; $4=ngid }} print }}' \
                 /etc/passwd > /etc/passwd.deacon.tmp && \
                 mv /etc/passwd.deacon.tmp /etc/passwd",
                username, new_uid, new_gid
            );
            let passwd_cmd = vec!["sh".to_string(), "-c".to_string(), passwd_script];
            let passwd_result = self
                .exec_silent(container_id, &passwd_cmd, Some("root"))
                .await?;
            if !passwd_result.success {
                return Err(UserMappingError::CommandExecutionFailed {
                    command: format!("/etc/passwd UID rewrite for {}", username),
                    error: passwd_result.stderr.trim().to_string(),
                }
                .into());
            }

            // Update the group entry by GID too (BusyBox groupmod likely
            // already failed silently above). Match the group by name
            // matching the user — common but not universal; if it doesn't
            // match we skip silently and trust group_result above.
            let group_script = format!(
                "awk -F: -v OFS=: -v g={} -v ngid={} \
                 '{{ if ($1==g) $3=ngid; print }}' \
                 /etc/group > /etc/group.deacon.tmp && \
                 mv /etc/group.deacon.tmp /etc/group",
                username, new_gid
            );
            let group_cmd = vec!["sh".to_string(), "-c".to_string(), group_script];
            let _ = self
                .exec_silent(container_id, &group_cmd, Some("root"))
                .await;

            // Re-own the user's home directory so the rewritten UID/GID
            // can read/write it. Best-effort — if the home dir doesn't
            // exist (yet) we skip.
            let chown_script = format!(
                "if getent passwd {u} >/dev/null 2>&1; then \
                   home=$(getent passwd {u} | cut -d: -f6); \
                   [ -d \"$home\" ] && chown -R {uid}:{gid} \"$home\" || true; \
                 fi",
                u = username,
                uid = new_uid,
                gid = new_gid
            );
            let chown_cmd = vec!["sh".to_string(), "-c".to_string(), chown_script];
            let _ = self
                .exec_silent(container_id, &chown_cmd, Some("root"))
                .await;
        }

        Ok(())
    }

    async fn create_home_directory(&self, container_id: &str, user_info: &UserInfo) -> Result<()> {
        let mkdir_cmd = vec![
            "mkdir".to_string(),
            "-p".to_string(),
            user_info.home_dir.clone(),
        ];
        let result = self
            .exec_silent(container_id, &mkdir_cmd, Some("root"))
            .await?;
        if !result.success {
            return Err(UserMappingError::HomeDirectoryCreationFailed {
                home_dir: user_info.home_dir.clone(),
                reason: result.stderr.trim().to_string(),
            }
            .into());
        }

        // Set ownership
        let chown_cmd = vec![
            "chown".to_string(),
            format!("{}:{}", user_info.uid, user_info.gid),
            user_info.home_dir.clone(),
        ];
        let chown_result = self
            .exec_silent(container_id, &chown_cmd, Some("root"))
            .await?;
        if !chown_result.success {
            warn!(
                "Failed to chown home directory: {}",
                chown_result.stderr.trim()
            );
        }

        Ok(())
    }

    async fn set_workspace_ownership(
        &self,
        container_id: &str,
        workspace_path: &str,
        uid: u32,
        gid: u32,
    ) -> Result<()> {
        // Use chown on the workspace directory (non-recursive to avoid long operations)
        let chown_cmd = vec![
            "chown".to_string(),
            format!("{}:{}", uid, gid),
            workspace_path.to_string(),
        ];
        let result = self
            .exec_silent(container_id, &chown_cmd, Some("root"))
            .await?;
        if !result.success {
            return Err(UserMappingError::WorkspaceOwnershipFailed {
                reason: format!(
                    "chown {}:{} {} failed: {}",
                    uid,
                    gid,
                    workspace_path,
                    result.stderr.trim()
                ),
            }
            .into());
        }

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
        let config = ExecConfig {
            user: Some(username.to_string()),
            working_dir,
            env: env.unwrap_or_default(),
            tty: false,
            interactive: false,
            detach: false,
            silent: true,
            stdout_to_stderr: false,
            terminal_size: None,
        };
        let result = self.docker.exec(container_id, command, config).await?;
        if !result.success {
            return Err(UserMappingError::CommandExecutionFailed {
                command: command.join(" "),
                error: result.stderr.trim().to_string(),
            }
            .into());
        }
        Ok(result.stdout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Mock implementation for testing
    struct MockUserMapper {
        users: HashMap<String, UserInfo>,
        /// Extra `/etc/group` entries not implied by `users`, keyed by gid.
        groups: HashMap<u32, String>,
        current_user: UserInfo,
    }

    impl MockUserMapper {
        fn new() -> Self {
            Self {
                users: HashMap::new(),
                groups: HashMap::new(),
                current_user: UserInfo::new(
                    "root".to_string(),
                    0,
                    0,
                    "/root".to_string(),
                    "/bin/bash".to_string(),
                ),
            }
        }

        fn with_user(mut self, user: UserInfo) -> Self {
            self.users.insert(user.username.clone(), user);
            self
        }

        fn with_group(mut self, gid: u32, name: &str) -> Self {
            self.groups.insert(gid, name.to_string());
            self
        }
    }

    impl UserMapper for MockUserMapper {
        async fn get_current_user(&self, _container_id: &str) -> Result<UserInfo> {
            Ok(self.current_user.clone())
        }

        async fn get_user_info(
            &self,
            _container_id: &str,
            username: &str,
        ) -> Result<Option<UserInfo>> {
            Ok(self.users.get(username).cloned())
        }

        async fn user_exists(&self, _container_id: &str, username: &str) -> Result<bool> {
            Ok(self.users.contains_key(username))
        }

        async fn find_user_by_uid(&self, _container_id: &str, uid: u32) -> Result<Option<String>> {
            Ok(self
                .users
                .values()
                .find(|u| u.uid == uid)
                .map(|u| u.username.clone()))
        }

        async fn find_group_by_gid(&self, _container_id: &str, gid: u32) -> Result<Option<String>> {
            // A user's primary group counts as a /etc/group entry too.
            Ok(self.groups.get(&gid).cloned().or_else(|| {
                self.users
                    .values()
                    .find(|u| u.gid == gid)
                    .map(|u| u.username.clone())
            }))
        }

        async fn create_user(
            &self,
            _container_id: &str,
            username: &str,
            uid: Option<u32>,
            gid: Option<u32>,
            home_dir: Option<String>,
            shell: Option<String>,
        ) -> Result<UserInfo> {
            let uid = uid.unwrap_or(1000);
            let gid = gid.unwrap_or(uid);
            let home_dir = home_dir.unwrap_or_else(|| UserInfo::default_home_dir(username));
            let shell = shell.unwrap_or_else(UserInfo::default_shell);

            Ok(UserInfo::new(
                username.to_string(),
                uid,
                gid,
                home_dir,
                shell,
            ))
        }

        async fn update_user_uid(
            &self,
            _container_id: &str,
            _username: &str,
            _new_uid: u32,
            _new_gid: u32,
        ) -> Result<()> {
            Ok(())
        }

        async fn create_home_directory(
            &self,
            _container_id: &str,
            _user_info: &UserInfo,
        ) -> Result<()> {
            Ok(())
        }

        async fn set_workspace_ownership(
            &self,
            _container_id: &str,
            _workspace_path: &str,
            _uid: u32,
            _gid: u32,
        ) -> Result<()> {
            Ok(())
        }

        async fn execute_as_user(
            &self,
            _container_id: &str,
            _username: &str,
            _command: &[String],
            _env: Option<HashMap<String, String>>,
            _working_dir: Option<String>,
        ) -> Result<String> {
            Ok("command output".to_string())
        }
    }

    #[tokio::test]
    async fn test_user_info_creation() {
        let user = UserInfo::new(
            "testuser".to_string(),
            1000,
            1000,
            "/home/testuser".to_string(),
            "/bin/bash".to_string(),
        );

        assert_eq!(user.username, "testuser");
        assert_eq!(user.uid, 1000);
        assert_eq!(user.gid, 1000);
        assert_eq!(user.home_dir, "/home/testuser");
        assert_eq!(user.shell, "/bin/bash");
    }

    #[tokio::test]
    async fn test_user_mapping_config() {
        let config = UserMappingConfig::new(Some("devuser".to_string()), None, true)
            .with_host_user(1001, 1001)
            .with_workspace_path("/workspace".to_string());

        assert!(config.needs_user_mapping());
        assert!(config.needs_uid_mapping());
        assert_eq!(config.effective_user(), Some("devuser"));
        assert_eq!(config.host_uid, Some(1001));
        assert_eq!(config.host_gid, Some(1001));
    }

    #[tokio::test]
    async fn test_no_user_mapping_needed() {
        let mapper = MockUserMapper::new();
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(None, None, false);

        let result = service
            .apply_user_mapping("container123", &config)
            .await
            .unwrap();

        assert!(!result.user_created);
        assert!(!result.uid_updated);
        assert!(!result.home_created);
        assert!(!result.workspace_ownership_adjusted);
        assert_eq!(result.user_info.username, "root");
    }

    #[tokio::test]
    async fn test_workspace_ownership_skipped_for_root() {
        // remoteUser: root resolves to uid 0. The workspace must NOT be chowned:
        // on a bind mount, `chown 0:0 <workspace>` rewrites the host directory
        // to root:root (e.g. flipping the repo workspace to root ownership).
        let root_user = UserInfo::new(
            "root".to_string(),
            0,
            0,
            "/root".to_string(),
            "/bin/bash".to_string(),
        );
        let mapper = MockUserMapper::new().with_user(root_user);
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(Some("root".to_string()), None, false)
            .with_workspace_path("/workspace".to_string());

        let result = service
            .apply_user_mapping("container123", &config)
            .await
            .unwrap();

        assert_eq!(result.user_info.uid, 0);
        assert!(
            !result.workspace_ownership_adjusted,
            "workspace ownership must not be adjusted for a root target user"
        );
    }

    #[tokio::test]
    async fn test_workspace_ownership_adjusted_for_nonroot() {
        // A real, non-root remote user still gets the workspace ownership
        // adjustment (the legitimate use case the guard preserves).
        let dev_user = UserInfo::new(
            "devuser".to_string(),
            1000,
            1000,
            "/home/devuser".to_string(),
            "/bin/bash".to_string(),
        );
        let mapper = MockUserMapper::new().with_user(dev_user);
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(Some("devuser".to_string()), None, false)
            .with_workspace_path("/workspace".to_string());

        let result = service
            .apply_user_mapping("container123", &config)
            .await
            .unwrap();

        assert_eq!(result.user_info.uid, 1000);
        assert!(
            result.workspace_ownership_adjusted,
            "workspace ownership should be adjusted for a non-root target user"
        );
    }

    #[tokio::test]
    async fn test_create_new_user() {
        let mapper = MockUserMapper::new();
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(Some("newuser".to_string()), None, true)
            .with_host_user(1002, 1002);

        let result = service
            .apply_user_mapping("container123", &config)
            .await
            .unwrap();

        assert!(result.user_created);
        assert!(!result.uid_updated); // New user created with correct UID
        assert_eq!(result.user_info.username, "newuser");
        assert_eq!(result.user_info.uid, 1002);
        assert_eq!(result.user_info.gid, 1002);
    }

    #[tokio::test]
    async fn test_update_existing_user_uid() {
        let existing_user = UserInfo::new(
            "existinguser".to_string(),
            1000,
            1000,
            "/home/existinguser".to_string(),
            "/bin/bash".to_string(),
        );

        let mapper = MockUserMapper::new().with_user(existing_user);
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(Some("existinguser".to_string()), None, true)
            .with_host_user(1003, 1003);

        let result = service
            .apply_user_mapping("container123", &config)
            .await
            .unwrap();

        assert!(!result.user_created);
        assert!(result.uid_updated);
        assert_eq!(result.user_info.username, "existinguser");
        assert_eq!(result.user_info.uid, 1003);
        assert_eq!(result.user_info.gid, 1003);
    }

    #[tokio::test]
    async fn test_existing_user_no_update_needed() {
        let existing_user = UserInfo::new(
            "correctuser".to_string(),
            1004,
            1004,
            "/home/correctuser".to_string(),
            "/bin/bash".to_string(),
        );

        let mapper = MockUserMapper::new().with_user(existing_user);
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(Some("correctuser".to_string()), None, true)
            .with_host_user(1004, 1004); // Same UID/GID

        let result = service
            .apply_user_mapping("container123", &config)
            .await
            .unwrap();

        assert!(!result.user_created);
        assert!(!result.uid_updated); // No update needed
        assert_eq!(result.user_info.username, "correctuser");
        assert_eq!(result.user_info.uid, 1004);
        assert_eq!(result.user_info.gid, 1004);
    }

    #[tokio::test]
    async fn test_execute_command_as_user() {
        let mapper = MockUserMapper::new();
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(Some("testuser".to_string()), None, false);

        let command = vec!["echo".to_string(), "hello".to_string()];
        let result = service
            .execute_command_as_user("container123", &config, &command, None, None)
            .await
            .unwrap();

        assert_eq!(result, "command output");
    }

    #[test]
    fn test_user_info_defaults() {
        assert_eq!(UserInfo::default_shell(), "/bin/bash");
        assert_eq!(UserInfo::default_home_dir("testuser"), "/home/testuser");
        assert_eq!(UserInfo::default_home_dir("root"), "/root");
    }

    /// SC-003: Root user (UID 0) is never modified when host UID matches
    #[tokio::test]
    async fn test_uid_update_skipped_for_root() {
        let root_user = UserInfo::new(
            "root".to_string(),
            0,
            0,
            "/root".to_string(),
            "/bin/sh".to_string(),
        );
        let mapper = MockUserMapper::new().with_user(root_user);
        let service = UserMappingService::new(mapper);
        let config =
            UserMappingConfig::new(Some("root".to_string()), None, true).with_host_user(0, 0);
        let result = service.apply_user_mapping("c1", &config).await.unwrap();
        assert!(!result.uid_updated);
        assert_eq!(result.user_info.username, "root");
        assert_eq!(result.user_info.uid, 0);
    }

    /// SC-005: UID already matching skips the update
    #[tokio::test]
    async fn test_uid_update_skipped_when_matching() {
        let vscode_user = UserInfo::new(
            "vscode".to_string(),
            1000,
            1000,
            "/home/vscode".to_string(),
            "/bin/bash".to_string(),
        );
        let mapper = MockUserMapper::new().with_user(vscode_user);
        let service = UserMappingService::new(mapper);
        let config = UserMappingConfig::new(Some("vscode".to_string()), None, true)
            .with_host_user(1000, 1000);
        let result = service.apply_user_mapping("c1", &config).await.unwrap();
        assert!(!result.uid_updated);
        assert_eq!(result.user_info.uid, 1000);
    }

    /// SC-002: updateRemoteUserUID=false skips the update entirely
    #[tokio::test]
    async fn test_uid_update_skipped_when_disabled() {
        let mapper = MockUserMapper::new();
        let service = UserMappingService::new(mapper);
        let config = UserMappingConfig::new(Some("vscode".to_string()), None, false);
        let result = service.apply_user_mapping("c1", &config).await.unwrap();
        assert!(!result.uid_updated);
        assert!(result.user_created); // user is created but UID not updated
    }

    /// SC-002: needs_uid_mapping returns false when update_remote_user_uid is false
    #[test]
    fn test_needs_uid_mapping_false_when_disabled() {
        let config = UserMappingConfig::new(Some("user".to_string()), None, false)
            .with_host_user(1000, 1000);
        assert!(!config.needs_uid_mapping());
    }

    /// SC-004: needs_uid_mapping returns false when no host UID is available
    #[test]
    fn test_needs_uid_mapping_false_without_host_uid() {
        let config = UserMappingConfig::new(Some("user".to_string()), None, true);
        assert!(!config.needs_uid_mapping());
    }

    // ---- #618: the reference's updateUID.Dockerfile guards -----------------

    /// Both ids already match: the reference's "UIDs and GIDs are the same"
    /// branch.
    #[test]
    fn decide_uid_remap_is_a_no_op_when_both_ids_match() {
        assert_eq!(
            decide_uid_remap(1000, 1000, 1000, 1000, Some("foo"), Some("foo")),
            UidRemapDecision::NoChange
        );
    }

    /// The UID half of #618: another `/etc/passwd` entry already owns the
    /// target uid, so the remap is refused OUTRIGHT — the gid is not touched
    /// either, which is what keeps the remote user a coherent identity.
    #[test]
    fn decide_uid_remap_refuses_when_another_user_owns_the_uid() {
        assert_eq!(
            decide_uid_remap(1002, 1002, 1000, 1000, Some("foo"), Some("foo")),
            UidRemapDecision::SkippedUidTaken {
                existing_user: "foo".to_string()
            }
        );
    }

    /// The GID half, which no parity case reaches: the uid is free but another
    /// group owns the target gid, so the reference keeps `OLD_GID` and remaps
    /// the uid only.
    #[test]
    fn decide_uid_remap_keeps_the_old_gid_when_another_group_owns_it() {
        assert_eq!(
            decide_uid_remap(1002, 1002, 1000, 1000, None, Some("staff")),
            UidRemapDecision::Remap {
                uid: 1000,
                gid: 1002
            }
        );
    }

    /// The GID fallback can eat the only change there was to make: the uid
    /// already matches and the gid cannot move, so nothing is left to do.
    #[test]
    fn decide_uid_remap_is_a_no_op_when_the_gid_fallback_undoes_the_only_change() {
        assert_eq!(
            decide_uid_remap(1000, 1002, 1000, 1000, None, Some("staff")),
            UidRemapDecision::NoChange
        );
    }

    /// Both ids free: the ordinary remap the guards must not get in the way of.
    #[test]
    fn decide_uid_remap_remaps_when_both_ids_are_free() {
        assert_eq!(
            decide_uid_remap(1002, 1002, 1000, 1000, None, None),
            UidRemapDecision::Remap {
                uid: 1000,
                gid: 1000
            }
        );
    }

    /// The `EXISTING_USER` lookup is only consulted when the uid actually
    /// moves — a user whose uid already matches but whose gid does not must
    /// still be able to have its gid remapped, even though it is itself the
    /// entry that owns the target uid.
    #[test]
    fn decide_uid_remap_moves_the_gid_when_only_the_gid_differs() {
        assert_eq!(
            decide_uid_remap(1000, 1002, 1000, 1000, Some("bar"), None),
            UidRemapDecision::Remap {
                uid: 1000,
                gid: 1000
            }
        );
    }

    /// End-to-end through the service: #618's shape — `bar` at 1002 with the
    /// host at 1000, which `foo` already owns. `bar` keeps 1002:1002.
    #[tokio::test]
    async fn test_uid_update_skipped_when_target_uid_is_taken() {
        let mapper = MockUserMapper::new()
            .with_user(UserInfo::new(
                "foo".to_string(),
                1000,
                1000,
                "/home/foo".to_string(),
                "/bin/sh".to_string(),
            ))
            .with_user(UserInfo::new(
                "bar".to_string(),
                1002,
                1002,
                "/home/bar".to_string(),
                "/bin/sh".to_string(),
            ));
        let service = UserMappingService::new(mapper);

        let config =
            UserMappingConfig::new(Some("bar".to_string()), None, true).with_host_user(1000, 1000);

        let result = service.apply_user_mapping("c1", &config).await.unwrap();

        assert!(!result.uid_updated);
        assert!(result.uid_update_skipped_uid_taken);
        assert_eq!(result.user_info.username, "bar");
        assert_eq!(result.user_info.uid, 1002);
        assert_eq!(result.user_info.gid, 1002);
    }

    /// A refused remap must NOT take the developer's workspace with it: `bar`
    /// keeps an image-assigned uid, so chowning the host bind mount to it would
    /// strip the host user's access. The reference chowns only `$HOME`, and
    /// only when it actually remaps.
    #[tokio::test]
    async fn test_workspace_ownership_skipped_when_uid_remap_is_refused() {
        let mapper = MockUserMapper::new()
            .with_user(UserInfo::new(
                "foo".to_string(),
                1000,
                1000,
                "/home/foo".to_string(),
                "/bin/sh".to_string(),
            ))
            .with_user(UserInfo::new(
                "bar".to_string(),
                1002,
                1002,
                "/home/bar".to_string(),
                "/bin/sh".to_string(),
            ));
        let service = UserMappingService::new(mapper);

        let config = UserMappingConfig::new(Some("bar".to_string()), None, true)
            .with_host_user(1000, 1000)
            .with_workspace_path("/workspaces/project".to_string());

        let result = service.apply_user_mapping("c1", &config).await.unwrap();

        assert!(result.uid_update_skipped_uid_taken);
        assert!(!result.workspace_ownership_adjusted);
    }

    /// End-to-end through the service: the uid is free, the gid is not, so the
    /// uid moves and the gid stays.
    #[tokio::test]
    async fn test_uid_update_keeps_old_gid_when_target_gid_is_taken() {
        let mapper = MockUserMapper::new()
            .with_user(UserInfo::new(
                "bar".to_string(),
                1002,
                1002,
                "/home/bar".to_string(),
                "/bin/sh".to_string(),
            ))
            .with_group(1000, "staff");
        let service = UserMappingService::new(mapper);

        let config =
            UserMappingConfig::new(Some("bar".to_string()), None, true).with_host_user(1000, 1000);

        let result = service.apply_user_mapping("c1", &config).await.unwrap();

        assert!(result.uid_updated);
        assert!(!result.uid_update_skipped_uid_taken);
        assert_eq!(result.user_info.uid, 1000);
        assert_eq!(result.user_info.gid, 1002);
    }
}
