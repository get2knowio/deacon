//! Container environment probing for lifecycle execution
//!
//! This module implements in-container environment probing to capture the user's
//! shell environment (PATH, etc.) for lifecycle command execution. This ensures
//! tools like nvm-installed Node.js are available in lifecycle phases.
//!
//! ## Probe Process
//!
//! 1. Detect the user's shell (from $SHELL, /etc/passwd, or fallback)
//! 2. Execute shell with login flags to source profile/rc files
//! 3. Capture environment variables via `env` command
//! 4. Merge with existing containerEnv/remoteEnv
//!
//! ## Probing Modes
//!
//! - `None`: No environment probing (use containerEnv as-is)
//! - `LoginShell`: Execute `shell -lc 'env'` to capture login environment
//! - `LoginInteractiveShell`: Execute `shell -lic 'env'` (interactive + login)
//!
//! ## Shell Selection
//!
//! - Prefer user's `$SHELL` environment variable
//! - Fall back to /etc/passwd entry for remoteUser
//! - Ultimate fallback: try zsh → bash → sh

use crate::docker::Docker;
use crate::errors::{DeaconError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// Environment probing modes for container
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ContainerProbeMode {
    /// No environment probing
    None,
    /// Login shell only (-l)
    #[default]
    LoginShell,
    /// Login + Interactive shell (-l -i)
    LoginInteractiveShell,
}

/// Result of container environment probe
#[derive(Debug, Clone)]
pub struct ContainerProbeResult {
    /// Environment variables captured from container shell
    pub env_vars: HashMap<String, String>,
    /// Shell used for probing
    pub shell_used: String,
    /// Number of variables captured
    pub var_count: usize,
}

/// Container environment prober
#[derive(Debug)]
pub struct ContainerEnvironmentProber {
    /// Timeout for probe execution (reserved for future use)
    #[allow(dead_code)]
    probe_timeout: Duration,
}

impl Default for ContainerEnvironmentProber {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerEnvironmentProber {
    /// Create a new container environment prober
    pub fn new() -> Self {
        Self {
            probe_timeout: Duration::from_secs(10),
        }
    }

    /// Create a new container environment prober with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            probe_timeout: timeout,
        }
    }

    /// Probe environment in container
    ///
    /// Executes shell probe in the container to capture environment variables
    /// that are set by shell initialization files (e.g., ~/.bashrc, ~/.zshrc)
    #[instrument(skip(self, docker))]
    pub async fn probe_container_environment<D>(
        &self,
        docker: &D,
        container_id: &str,
        mode: ContainerProbeMode,
        user: Option<&str>,
    ) -> Result<ContainerProbeResult>
    where
        D: Docker,
    {
        if mode == ContainerProbeMode::None {
            debug!("Container environment probing disabled (mode: None)");
            return Ok(ContainerProbeResult {
                env_vars: HashMap::new(),
                shell_used: "none".to_string(),
                var_count: 0,
            });
        }

        // Detect shell in container
        let shell = self
            .detect_container_shell(docker, container_id, user)
            .await?;
        info!("Detected shell in container: {}", shell);

        // Execute probe based on mode
        let env_output = self
            .execute_probe_in_container(docker, container_id, &shell, mode, user)
            .await?;

        // Parse environment output
        let env_vars = self.parse_env_output(&env_output)?;
        let var_count = env_vars.len();

        info!(
            "Container environment probe completed: shell={}, mode={:?}, variables_captured={}",
            shell, mode, var_count
        );

        Ok(ContainerProbeResult {
            env_vars,
            shell_used: shell,
            var_count,
        })
    }

    /// Detect the shell to use in the container
    ///
    /// Order of detection:
    /// 1. Check $SHELL environment variable (if valid and executable)
    /// 2. Check /etc/passwd for user's shell
    /// 3. Fallback chain: zsh → bash → sh
    pub async fn detect_container_shell<D>(
        &self,
        docker: &D,
        container_id: &str,
        user: Option<&str>,
    ) -> Result<String>
    where
        D: Docker,
    {
        // Try $SHELL first - but verify it's valid and executable
        if let Ok(shell) = self
            .exec_simple_command(docker, container_id, &["sh", "-c", "echo $SHELL"], user)
            .await
        {
            let shell = shell.trim();
            if !shell.is_empty() && shell != "/bin/sh" && shell != "sh" {
                // Verify the shell actually exists and is executable
                if self
                    .check_shell_exists(docker, container_id, shell, user)
                    .await
                    .unwrap_or(false)
                {
                    debug!("Using verified $SHELL from container: {}", shell);
                    return Ok(shell.to_string());
                } else {
                    debug!("$SHELL '{}' is not executable, trying alternatives", shell);
                }
            }
        }

        // Try reading from /etc/passwd if user is specified
        if let Some(user) = user {
            if let Ok(shell) = self
                .read_shell_from_passwd(docker, container_id, user, user)
                .await
            {
                // Verify this shell is also executable
                if self
                    .check_shell_exists(docker, container_id, &shell, Some(user))
                    .await
                    .unwrap_or(false)
                {
                    debug!(
                        "Using verified shell from /etc/passwd for user {}: {}",
                        user, shell
                    );
                    return Ok(shell);
                } else {
                    debug!(
                        "Shell from /etc/passwd '{}' is not executable, trying fallbacks",
                        shell
                    );
                }
            }
        }

        // Fallback chain: zsh → bash → sh
        for shell in &["/bin/zsh", "/usr/bin/zsh", "/bin/bash", "/bin/sh"] {
            if self
                .check_shell_exists(docker, container_id, shell, user)
                .await
                .unwrap_or(false)
            {
                debug!("Using fallback shell: {}", shell);
                return Ok(shell.to_string());
            }
        }

        // Ultimate fallback - sh should always exist
        debug!("Using ultimate fallback: sh");
        Ok("sh".to_string())
    }

    /// Read shell from /etc/passwd for a user
    async fn read_shell_from_passwd<D>(
        &self,
        docker: &D,
        container_id: &str,
        user: &str,
        exec_user: &str,
    ) -> Result<String>
    where
        D: Docker,
    {
        let output = self
            .exec_simple_command(
                docker,
                container_id,
                &[
                    "sh",
                    "-c",
                    &format!(
                        "getent passwd {} || grep '^{}:' /etc/passwd || echo ''",
                        user, user
                    ),
                ],
                Some(exec_user),
            )
            .await?;

        // Parse passwd line: username:x:uid:gid:comment:home:shell
        if let Some(line) = output.lines().next() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 7 {
                let shell = parts[6].trim();
                if !shell.is_empty() {
                    return Ok(shell.to_string());
                }
            }
        }

        Err(DeaconError::Internal(
            crate::errors::InternalError::Generic {
                message: format!("Could not read shell from /etc/passwd for user {}", user),
            },
        ))
    }

    /// Check if a shell exists in the container
    async fn check_shell_exists<D>(
        &self,
        docker: &D,
        container_id: &str,
        shell_path: &str,
        user: Option<&str>,
    ) -> Result<bool>
    where
        D: Docker,
    {
        use crate::docker::ExecConfig;

        let command = vec![
            "sh".to_string(),
            "-c".to_string(),
            format!("test -x {}", shell_path),
        ];

        let exec_config = ExecConfig {
            user: user.map(String::from),
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
        };

        // Use exit code to determine if shell exists (0 = exists, non-zero = doesn't exist)
        match docker.exec(container_id, &command, exec_config).await {
            Ok(result) => Ok(result.success && result.exit_code == 0),
            Err(_) => Ok(false),
        }
    }

    /// Execute probe command in container
    #[instrument(skip(self, docker))]
    async fn execute_probe_in_container<D>(
        &self,
        docker: &D,
        container_id: &str,
        shell: &str,
        mode: ContainerProbeMode,
        user: Option<&str>,
    ) -> Result<String>
    where
        D: Docker,
    {
        // Build shell command based on mode
        let shell_flags = match mode {
            ContainerProbeMode::None => {
                return Err(DeaconError::Internal(
                    crate::errors::InternalError::Generic {
                        message: "None mode should be handled earlier".to_string(),
                    },
                ))
            }
            ContainerProbeMode::LoginShell => "-lc",
            ContainerProbeMode::LoginInteractiveShell => "-lic",
        };

        let probe_cmd = format!("{} {} 'env'", shell, shell_flags);
        debug!("Executing probe command: {}", probe_cmd);

        self.exec_simple_command(docker, container_id, &["sh", "-c", &probe_cmd], user)
            .await
    }

    /// Execute a simple command in container and return stdout
    async fn exec_simple_command<D>(
        &self,
        docker: &D,
        container_id: &str,
        command: &[&str],
        user: Option<&str>,
    ) -> Result<String>
    where
        D: Docker,
    {
        use crate::docker::ExecConfig;

        let command_strings: Vec<String> = command.iter().map(|s| s.to_string()).collect();

        let exec_config = ExecConfig {
            user: user.map(String::from),
            working_dir: None,
            env: HashMap::new(),
            tty: false,
            interactive: false,
            detach: false,
        };

        // Note: Current Docker trait doesn't capture output, so we use a workaround
        // We'll need to enhance this when output capture is added to the trait
        // For now, we execute and check success
        let result = docker
            .exec(container_id, &command_strings, exec_config)
            .await?;

        if !result.success {
            return Err(DeaconError::Internal(
                crate::errors::InternalError::Generic {
                    message: format!(
                        "Container command failed with exit code {}",
                        result.exit_code
                    ),
                },
            ));
        }

        // TODO: Capture actual output when Docker trait supports it
        // For now, return empty string to indicate success
        warn!("Docker exec doesn't capture output yet - probe will be limited");
        Ok(String::new())
    }

    /// Parse environment output from shell
    ///
    /// Handles:
    /// - Standard KEY=VALUE pairs
    /// - Empty values (KEY=)
    /// - Values containing equals signs (KEY=val=ue)
    /// - Skips invalid lines and keys
    ///
    /// Note: Multiline values are challenging without proper quoting/escaping from the shell.
    /// This implementation handles single-line values robustly.
    fn parse_env_output(&self, output: &str) -> Result<HashMap<String, String>> {
        let mut env_vars = HashMap::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Find the first '=' which separates key from value
            if let Some(eq_pos) = line.find('=') {
                let key = &line[..eq_pos];

                // Only include if key is valid (non-empty and contains only valid characters)
                // Valid environment variable names: alphanumeric and underscore, cannot start with digit
                if !key.is_empty()
                    && key.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !key.chars().next().unwrap().is_ascii_digit()
                {
                    let value = &line[eq_pos + 1..];
                    env_vars.insert(key.to_string(), value.to_string());
                }
            }
        }

        Ok(env_vars)
    }

    /// Merge probed environment with existing containerEnv/remoteEnv
    ///
    /// Precedence order (highest to lowest):
    /// 1. remoteEnv (explicit user override)
    /// 2. containerEnv (from config)
    /// 3. probed_env (from shell initialization)
    pub fn merge_environments(
        &self,
        probed_env: &HashMap<String, String>,
        container_env: Option<&HashMap<String, String>>,
        remote_env: Option<&HashMap<String, String>>,
    ) -> HashMap<String, String> {
        let mut result = probed_env.clone();

        // Layer 2: containerEnv overrides probed
        if let Some(container_env) = container_env {
            for (key, value) in container_env {
                result.insert(key.clone(), value.clone());
            }
        }

        // Layer 3: remoteEnv overrides all
        if let Some(remote_env) = remote_env {
            for (key, value) in remote_env {
                let old_value = result.insert(key.clone(), value.clone());
                if let Some(old_val) = old_value {
                    debug!(
                        "Remote env variable '{}' overrode value '{}' with '{}'",
                        key, old_val, value
                    );
                }
            }
        }

        result
    }
}

/// Get shell command for lifecycle execution
///
/// Determines the appropriate shell and flags to use for executing lifecycle commands.
/// Defaults to login shell mode for best environment parity.
pub fn get_shell_command_for_lifecycle(
    shell: &str,
    command: &str,
    use_login_shell: bool,
) -> Vec<String> {
    if !use_login_shell {
        // Legacy mode: plain sh -c
        return vec!["sh".to_string(), "-c".to_string(), command.to_string()];
    }

    // Determine shell name for flag compatibility
    let shell_name = shell.split('/').next_back().unwrap_or(shell);

    match shell_name {
        "zsh" | "bash" => {
            // Use login shell with -l flag
            vec![shell.to_string(), "-lc".to_string(), command.to_string()]
        }
        _ => {
            // Fallback: try -lc, may not work for all shells
            vec![shell.to_string(), "-lc".to_string(), command.to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probe_mode_default() {
        assert_eq!(
            ContainerProbeMode::default(),
            ContainerProbeMode::LoginShell
        );
    }

    #[test]
    fn test_parse_env_output() {
        let prober = ContainerEnvironmentProber::new();
        let output = "PATH=/usr/bin:/bin\nHOME=/home/user\nINVALID_LINE\n=INVALID_KEY\nEMPTY=\n";

        let result = prober.parse_env_output(output).unwrap();

        assert_eq!(result.get("PATH"), Some(&"/usr/bin:/bin".to_string()));
        assert_eq!(result.get("HOME"), Some(&"/home/user".to_string()));
        assert_eq!(result.get("EMPTY"), Some(&"".to_string()));
        assert!(!result.contains_key("INVALID_KEY"));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_merge_environments_precedence() {
        let prober = ContainerEnvironmentProber::new();

        let mut probed_env = HashMap::new();
        probed_env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        probed_env.insert("HOME".to_string(), "/home/user".to_string());

        let mut container_env = HashMap::new();
        container_env.insert("PATH".to_string(), "/custom/bin:/usr/bin".to_string());
        container_env.insert("MY_VAR".to_string(), "container_value".to_string());

        let mut remote_env = HashMap::new();
        remote_env.insert("PATH".to_string(), "/remote/bin:/usr/bin".to_string());
        remote_env.insert("REMOTE_VAR".to_string(), "remote_value".to_string());

        let result =
            prober.merge_environments(&probed_env, Some(&container_env), Some(&remote_env));

        // Remote env should have highest precedence
        assert_eq!(
            result.get("PATH"),
            Some(&"/remote/bin:/usr/bin".to_string())
        );
        // Container env vars should be included
        assert_eq!(result.get("MY_VAR"), Some(&"container_value".to_string()));
        // Remote-only vars should be included
        assert_eq!(result.get("REMOTE_VAR"), Some(&"remote_value".to_string()));
        // Probed-only vars should be preserved
        assert_eq!(result.get("HOME"), Some(&"/home/user".to_string()));
    }

    #[test]
    fn test_merge_environments_container_overrides_probed() {
        let prober = ContainerEnvironmentProber::new();

        let mut probed_env = HashMap::new();
        probed_env.insert("VAR".to_string(), "probed".to_string());

        let mut container_env = HashMap::new();
        container_env.insert("VAR".to_string(), "container".to_string());

        let result = prober.merge_environments(&probed_env, Some(&container_env), None);

        // Container env should override probed
        assert_eq!(result.get("VAR"), Some(&"container".to_string()));
    }

    #[test]
    fn test_merge_environments_remote_overrides_all() {
        let prober = ContainerEnvironmentProber::new();

        let mut probed_env = HashMap::new();
        probed_env.insert("VAR".to_string(), "probed".to_string());

        let mut container_env = HashMap::new();
        container_env.insert("VAR".to_string(), "container".to_string());

        let mut remote_env = HashMap::new();
        remote_env.insert("VAR".to_string(), "remote".to_string());

        let result =
            prober.merge_environments(&probed_env, Some(&container_env), Some(&remote_env));

        // Remote env should override both container and probed
        assert_eq!(result.get("VAR"), Some(&"remote".to_string()));
    }

    #[test]
    fn test_parse_env_output_with_equals_in_value() {
        let prober = ContainerEnvironmentProber::new();
        let output = "KEY=value=with=equals\nANOTHER=normal";

        let result = prober.parse_env_output(output).unwrap();

        assert_eq!(result.get("KEY"), Some(&"value=with=equals".to_string()));
        assert_eq!(result.get("ANOTHER"), Some(&"normal".to_string()));
    }

    #[test]
    fn test_parse_env_output_empty_values() {
        let prober = ContainerEnvironmentProber::new();
        let output = "EMPTY=\nNONEMPTY=value\nALSO_EMPTY=";

        let result = prober.parse_env_output(output).unwrap();

        assert_eq!(result.get("EMPTY"), Some(&"".to_string()));
        assert_eq!(result.get("NONEMPTY"), Some(&"value".to_string()));
        assert_eq!(result.get("ALSO_EMPTY"), Some(&"".to_string()));
    }

    #[test]
    fn test_parse_env_output_invalid_keys_ignored() {
        let prober = ContainerEnvironmentProber::new();
        let output = "VALID=value\nINVALID-KEY=bad\n123START=bad\nVALID_KEY=good";

        let result = prober.parse_env_output(output).unwrap();

        assert_eq!(result.get("VALID"), Some(&"value".to_string()));
        assert_eq!(result.get("VALID_KEY"), Some(&"good".to_string()));
        assert!(!result.contains_key("INVALID-KEY"));
        assert!(!result.contains_key("123START"));
    }

    #[test]
    fn test_get_shell_command_legacy_mode() {
        let cmd = get_shell_command_for_lifecycle("/bin/bash", "echo hello", false);
        assert_eq!(cmd, vec!["sh", "-c", "echo hello"]);
    }

    #[test]
    fn test_get_shell_command_bash_login() {
        let cmd = get_shell_command_for_lifecycle("/bin/bash", "echo hello", true);
        assert_eq!(cmd, vec!["/bin/bash", "-lc", "echo hello"]);
    }

    #[test]
    fn test_get_shell_command_zsh_login() {
        let cmd = get_shell_command_for_lifecycle("/usr/bin/zsh", "echo hello", true);
        assert_eq!(cmd, vec!["/usr/bin/zsh", "-lc", "echo hello"]);
    }

    #[test]
    fn test_get_shell_command_sh_fallback() {
        let cmd = get_shell_command_for_lifecycle("/bin/sh", "echo hello", true);
        assert_eq!(cmd, vec!["/bin/sh", "-lc", "echo hello"]);
    }
}
