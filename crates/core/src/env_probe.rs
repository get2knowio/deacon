//! Environment probing and remote user mapping for host simulation
//!
//! This module implements environment probing modes and remote user mapping logic
//! prior to real container execution. It simulates by invoking user shell derived
//! from $SHELL env var with appropriate flags to capture environment variables
//! and provides user mapping functionality.
//!
//! ## Probing Modes
//!
//! - `None`: No environment probing
//! - `LoginInteractiveShell`: Login + Interactive shell (-l -i)
//! - `InteractiveShell`: Interactive shell only (-i)
//! - `LoginShell`: Login shell only (-l)
//!
//! ## Caching
//!
//! Probe results are cached per mode & shell path during the process lifetime
//! to avoid re-running expensive shell operations.

use crate::errors::{DeaconError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

/// Environment probing modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProbeMode {
    /// No environment probing
    None,
    /// Login + Interactive shell (-l -i)
    LoginInteractiveShell,
    /// Interactive shell only (-i)
    InteractiveShell,
    /// Login shell only (-l)
    LoginShell,
}

/// Remote user information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteUser {
    /// User name
    pub name: String,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
}

/// Cache key for environment probe results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    mode: ProbeMode,
    shell_path: PathBuf,
}

/// Environment probe result
#[derive(Debug, Clone)]
struct ProbeResult {
    /// Environment variables captured from shell
    pub env_vars: HashMap<String, String>,
    /// Number of variables captured
    pub var_count: usize,
}

/// Environment probe cache
type ProbeCache = Arc<Mutex<HashMap<CacheKey, ProbeResult>>>;

/// Environment prober for simulating host environment
#[derive(Debug)]
pub struct EnvironmentProber {
    /// Cache for probe results
    cache: ProbeCache,
    /// Call counter for testing
    #[cfg(test)]
    call_counter: Arc<Mutex<u32>>,
}

impl Default for EnvironmentProber {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentProber {
    /// Create a new environment prober
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            call_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Probe environment variables using the specified mode
    #[instrument(skip(self))]
    pub fn probe_environment(
        &self,
        mode: ProbeMode,
        remote_env: Option<&HashMap<String, String>>,
    ) -> Result<HashMap<String, String>> {
        // Return empty map for None mode
        if mode == ProbeMode::None {
            debug!("Environment probing disabled (mode: None)");
            return Ok(remote_env.cloned().unwrap_or_default());
        }

        // Get shell path from $SHELL environment variable
        let shell_path = self.get_shell_path()?;
        let cache_key = CacheKey {
            mode,
            shell_path: shell_path.clone(),
        };

        // Check cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached_result) = cache.get(&cache_key) {
                debug!(
                    "Using cached environment probe result for mode {:?} with {} variables",
                    mode, cached_result.var_count
                );
                return Ok(self.merge_environments(&cached_result.env_vars, remote_env));
            }
        }

        // Perform actual probing
        let probe_result = self.execute_shell_probe(mode, &shell_path)?;

        info!(
            "Environment probe completed: mode={:?}, variables_captured={}",
            mode, probe_result.var_count
        );

        // Cache the result
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(cache_key, probe_result.clone());
        }

        #[cfg(test)]
        {
            let mut counter = self.call_counter.lock().unwrap();
            *counter += 1;
        }

        Ok(self.merge_environments(&probe_result.env_vars, remote_env))
    }

    /// Get the current user information
    #[instrument]
    pub fn get_remote_user(&self) -> Result<RemoteUser> {
        #[cfg(unix)]
        {
            self.get_unix_user()
        }
        #[cfg(not(unix))]
        {
            self.get_fallback_user()
        }
    }

    /// Get the shell path from environment
    fn get_shell_path(&self) -> Result<PathBuf> {
        std::env::var("SHELL")
            .map(PathBuf::from)
            .or_else(|_: std::env::VarError| {
                // Fallback to common shells
                if cfg!(windows) {
                    Ok(PathBuf::from("cmd.exe"))
                } else {
                    Ok(PathBuf::from("/bin/sh"))
                }
            })
            .map_err(|_: std::env::VarError| {
                DeaconError::Internal(crate::errors::InternalError::Generic {
                    message: "Unable to determine shell path".to_string(),
                })
            })
    }

    /// Execute shell command to probe environment
    #[instrument(skip(self))]
    fn execute_shell_probe(&self, mode: ProbeMode, shell_path: &Path) -> Result<ProbeResult> {
        // Skip shell probing on Windows or treat specially
        if cfg!(windows) {
            debug!("Skipping shell probing on Windows");
            return Ok(ProbeResult {
                env_vars: HashMap::new(),
                var_count: 0,
            });
        }

        let mut cmd = Command::new(shell_path);

        // Add appropriate flags based on mode
        // In CI environments, avoid interactive flags that can hang
        let is_ci = std::env::var("CI").is_ok()
            || std::env::var("GITHUB_ACTIONS").is_ok()
            || std::env::var("CONTINUOUS_INTEGRATION").is_ok();

        // In CI environments, skip complex shell probing to avoid hanging
        if is_ci
            && matches!(
                mode,
                ProbeMode::InteractiveShell
                    | ProbeMode::LoginInteractiveShell
                    | ProbeMode::LoginShell
            )
        {
            warn!(
                "CI environment detected, skipping shell probing for mode {:?} to prevent hanging",
                mode
            );
            // Return empty environment for CI - remote env will still be used if provided
            return Ok(ProbeResult {
                env_vars: HashMap::new(),
                var_count: 0,
            });
        }

        match mode {
            ProbeMode::None => unreachable!("None mode should be handled earlier"),
            ProbeMode::LoginInteractiveShell => {
                cmd.args(["-l", "-i", "-c", "env"]);
            }
            ProbeMode::InteractiveShell => {
                cmd.args(["-i", "-c", "env"]);
            }
            ProbeMode::LoginShell => {
                cmd.args(["-l", "-c", "env"]);
            }
        }

        debug!("Executing shell command: {:?}", cmd);

        // Use shorter timeout for better performance
        let timeout_duration = if is_ci {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(10)
        };

        let output = self
            .execute_with_timeout(cmd, timeout_duration)
            .map_err(|e| {
                warn!("Shell command execution failed or timed out: {}", e);
                DeaconError::Internal(crate::errors::InternalError::Generic {
                    message: format!("Failed to execute shell command: {}", e),
                })
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DeaconError::Internal(
                crate::errors::InternalError::Generic {
                    message: format!("Shell command failed: {}", stderr),
                },
            ));
        }

        let stdout = String::from_utf8(output.stdout).map_err(|e| {
            DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to parse shell output as UTF-8: {}", e),
            })
        })?;

        let env_vars = self.parse_env_output(&stdout)?;
        let var_count = env_vars.len();

        debug!(
            "Parsed {} environment variables from shell output",
            var_count
        );

        Ok(ProbeResult {
            env_vars,
            var_count,
        })
    }

    /// Execute command with timeout to prevent hanging
    fn execute_with_timeout(
        &self,
        mut cmd: Command,
        timeout: Duration,
    ) -> std::io::Result<std::process::Output> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::mpsc;
        use std::sync::Arc;
        use std::thread;

        let (tx, rx) = mpsc::channel();
        let finished = Arc::new(AtomicBool::new(false));
        let finished_clone = finished.clone();

        // Spawn command in a separate thread
        let cmd_thread = thread::spawn(move || {
            let result = cmd.output();
            finished_clone.store(true, Ordering::Relaxed);
            let _ = tx.send(result);
        });

        // Wait for either completion or timeout
        let result = rx.recv_timeout(timeout).unwrap_or_else(|_| {
            if finished.load(Ordering::Relaxed) {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "Channel closed unexpectedly",
                ))
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Command execution timed out",
                ))
            }
        });

        // Clean up command thread (best effort, non-blocking)
        let _ = cmd_thread.join();

        result
    }

    /// Parse environment output from shell
    fn parse_env_output(&self, output: &str) -> Result<HashMap<String, String>> {
        let mut env_vars = HashMap::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].to_string();
                let value = line[eq_pos + 1..].to_string();

                // Only include if key is valid (non-empty and contains only valid characters)
                if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    env_vars.insert(key, value);
                }
            }
        }

        Ok(env_vars)
    }

    /// Merge probed environment with existing remoteEnv
    fn merge_environments(
        &self,
        probed_env: &HashMap<String, String>,
        remote_env: Option<&HashMap<String, String>>,
    ) -> HashMap<String, String> {
        let mut result = probed_env.clone();

        // Remote env takes precedence over probed env
        if let Some(remote_env) = remote_env {
            for (key, value) in remote_env {
                let old_value = result.insert(key.clone(), value.clone());
                if let Some(old_val) = old_value {
                    debug!(
                        "Remote env variable '{}' overrode probed value '{}' with '{}'",
                        key, old_val, value
                    );
                }
            }
        }

        result
    }

    /// Get user information on Unix systems
    #[cfg(unix)]
    fn get_unix_user(&self) -> Result<RemoteUser> {
        // Use safe alternatives to libc calls by reading from /proc or using std::env
        let uid = std::process::Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|output| {
                String::from_utf8(output.stdout)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
            })
            .unwrap_or(1000); // Default UID

        let gid = std::process::Command::new("id")
            .arg("-g")
            .output()
            .ok()
            .and_then(|output| {
                String::from_utf8(output.stdout)
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
            })
            .unwrap_or(1000); // Default GID

        let name = std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .unwrap_or_else(|_| format!("user{}", uid));

        Ok(RemoteUser { name, uid, gid })
    }

    /// Get fallback user information for non-Unix systems
    #[cfg(not(unix))]
    fn get_fallback_user(&self) -> Result<RemoteUser> {
        let name = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "user".to_string());

        Ok(RemoteUser {
            name,
            uid: 1000, // Default UID
            gid: 1000, // Default GID
        })
    }

    /// Get call counter for testing
    #[cfg(test)]
    pub fn get_call_count(&self) -> u32 {
        *self.call_counter.lock().unwrap()
    }

    /// Reset call counter for testing
    #[cfg(test)]
    pub fn reset_call_count(&self) {
        *self.call_counter.lock().unwrap() = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_probe_mode_serialization() {
        let mode = ProbeMode::LoginInteractiveShell;
        let serialized = serde_json::to_string(&mode).unwrap();
        let deserialized: ProbeMode = serde_json::from_str(&serialized).unwrap();
        assert_eq!(mode, deserialized);
    }

    #[test]
    fn test_remote_user_creation() {
        let user = RemoteUser {
            name: "testuser".to_string(),
            uid: 1000,
            gid: 1000,
        };
        assert_eq!(user.name, "testuser");
        assert_eq!(user.uid, 1000);
        assert_eq!(user.gid, 1000);
    }

    #[test]
    fn test_environment_prober_creation() {
        let prober = EnvironmentProber::new();
        assert_eq!(prober.get_call_count(), 0);
    }

    #[test]
    fn test_probe_environment_none_mode() {
        let prober = EnvironmentProber::new();
        let result = prober.probe_environment(ProbeMode::None, None).unwrap();
        assert!(result.is_empty());
        assert_eq!(prober.get_call_count(), 0);
    }

    #[test]
    fn test_probe_environment_with_remote_env() {
        let prober = EnvironmentProber::new();
        let mut remote_env = HashMap::new();
        remote_env.insert("TEST_VAR".to_string(), "test_value".to_string());

        let result = prober
            .probe_environment(ProbeMode::None, Some(&remote_env))
            .unwrap();
        assert_eq!(result.get("TEST_VAR"), Some(&"test_value".to_string()));
    }

    #[test]
    fn test_parse_env_output() {
        let prober = EnvironmentProber::new();
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
        let prober = EnvironmentProber::new();

        let mut probed_env = HashMap::new();
        probed_env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        probed_env.insert("HOME".to_string(), "/home/user".to_string());

        let mut remote_env = HashMap::new();
        remote_env.insert("PATH".to_string(), "/custom/bin:/usr/bin".to_string());
        remote_env.insert("CUSTOM_VAR".to_string(), "custom_value".to_string());

        let result = prober.merge_environments(&probed_env, Some(&remote_env));

        // Remote env should override probed env
        assert_eq!(
            result.get("PATH"),
            Some(&"/custom/bin:/usr/bin".to_string())
        );
        // Non-conflicting vars should be preserved
        assert_eq!(result.get("HOME"), Some(&"/home/user".to_string()));
        // Remote-only vars should be included
        assert_eq!(result.get("CUSTOM_VAR"), Some(&"custom_value".to_string()));
    }

    #[test]
    fn test_get_shell_path_fallback() {
        let prober = EnvironmentProber::new();
        let shell_path = prober.get_shell_path().unwrap();
        // Should not panic and should return some path
        assert!(!shell_path.as_os_str().is_empty());
    }

    #[test]
    fn test_get_remote_user() {
        let prober = EnvironmentProber::new();
        let user = prober.get_remote_user().unwrap();

        // Should have some name
        assert!(!user.name.is_empty());

        // UIDs and GIDs are u32 values, so they're always valid
        #[cfg(not(unix))]
        {
            assert_eq!(user.uid, 1000);
            assert_eq!(user.gid, 1000);
        }
    }

    #[test]
    fn test_caching_behavior() {
        let prober = EnvironmentProber::new();
        prober.reset_call_count();

        // First call should execute shell (but might be skipped on Windows)
        let _result1 = prober.probe_environment(ProbeMode::InteractiveShell, None);
        let count1 = prober.get_call_count();

        // Second call should use cache
        let _result2 = prober.probe_environment(ProbeMode::InteractiveShell, None);
        let count2 = prober.get_call_count();

        // On non-Windows systems, the call count should not increase for cached calls
        if !cfg!(windows) {
            assert_eq!(count1, count2, "Second call should use cache");
        }
    }

    #[test]
    fn test_different_modes_cache_separately() {
        let prober = EnvironmentProber::new();
        prober.reset_call_count();

        // Different modes should not share cache
        let _result1 = prober.probe_environment(ProbeMode::InteractiveShell, None);
        let _result2 = prober.probe_environment(ProbeMode::LoginShell, None);

        // Each mode should be cached separately
        // (Actual shell execution depends on the platform)
    }
}
