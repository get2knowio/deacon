//! Lifecycle command execution harness
//!
//! This module provides execution harness for lifecycle commands (initialize, onCreate,
//! postCreate, postStart, postAttach) with host-only simulation for phases before
//! container support.
//!
//! References: CLI-SPEC.md "Container Lifecycle Management"

use crate::errors::{DeaconError, Result};
use crate::redaction::{RedactionConfig, redact_if_enabled};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tracing::{debug, error, info, instrument, warn};

/// Lifecycle phases representing different stages of container setup
///
/// References: CLI-SPEC.md "Lifecycle Commands"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LifecyclePhase {
    /// Host-side initialization
    Initialize,
    /// Container creation setup
    OnCreate,
    /// Content synchronization
    UpdateContent,
    /// Post-creation configuration
    PostCreate,
    /// Container startup tasks
    PostStart,
    /// Attachment preparation
    PostAttach,
}

impl LifecyclePhase {
    /// Get the phase name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecyclePhase::Initialize => "initialize",
            LifecyclePhase::OnCreate => "onCreate",
            LifecyclePhase::UpdateContent => "updateContent",
            LifecyclePhase::PostCreate => "postCreate",
            LifecyclePhase::PostStart => "postStart",
            LifecyclePhase::PostAttach => "postAttach",
        }
    }
}

/// Commands to execute for lifecycle phases
#[derive(Debug, Clone)]
pub struct LifecycleCommands {
    /// Command strings and environment variables
    pub commands: Vec<CommandTemplate>,
}

/// Template for creating commands with environment
#[derive(Debug, Clone)]
pub struct CommandTemplate {
    /// The command string to execute
    pub command: String,
    /// Environment variables for this command
    pub env_vars: HashMap<String, String>,
}

impl LifecycleCommands {
    /// Create new lifecycle commands from JSON value (string or array of strings)
    pub fn from_json_value(value: &Value, env_vars: &HashMap<String, String>) -> Result<Self> {
        let commands = match value {
            Value::String(cmd) => {
                vec![CommandTemplate {
                    command: cmd.clone(),
                    env_vars: env_vars.clone(),
                }]
            }
            Value::Array(cmds) => {
                let mut commands = Vec::new();
                for cmd_value in cmds {
                    if let Value::String(cmd) = cmd_value {
                        commands.push(CommandTemplate {
                            command: cmd.clone(),
                            env_vars: env_vars.clone(),
                        });
                    } else {
                        return Err(DeaconError::Lifecycle(format!(
                            "Invalid command in array: expected string, got {:?}",
                            cmd_value
                        )));
                    }
                }
                commands
            }
            _ => {
                return Err(DeaconError::Lifecycle(format!(
                    "Invalid command format: expected string or array of strings, got {:?}",
                    value
                )));
            }
        };

        Ok(Self { commands })
    }
}

/// Execution context for lifecycle commands
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Environment variables to pass to commands
    pub environment: HashMap<String, String>,
    /// Working directory for command execution
    pub working_directory: Option<std::path::PathBuf>,
    /// Timeout for command execution (placeholder, not enforced yet)
    pub timeout: Option<std::time::Duration>,
    /// Redaction configuration for sensitive output filtering
    pub redaction_config: RedactionConfig,
}

impl ExecutionContext {
    /// Create new execution context
    pub fn new() -> Self {
        Self {
            environment: HashMap::new(),
            working_directory: None,
            timeout: None, // TODO: Implement timeout enforcement
            redaction_config: RedactionConfig::default(),
        }
    }

    /// Add environment variable
    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.environment.insert(key, value);
        self
    }

    /// Set working directory
    pub fn with_working_directory(mut self, dir: std::path::PathBuf) -> Self {
        self.working_directory = Some(dir);
        self
    }

    /// Set timeout (placeholder for future implementation)
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set redaction configuration
    pub fn with_redaction_config(mut self, config: RedactionConfig) -> Self {
        self.redaction_config = config;
        self
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of lifecycle command execution
#[derive(Debug, Clone)]
pub struct LifecycleResult {
    /// Exit codes from executed commands
    pub exit_codes: Vec<i32>,
    /// Combined stdout from all commands
    pub stdout: String,
    /// Combined stderr from all commands  
    pub stderr: String,
    /// Whether all commands succeeded
    pub success: bool,
}

impl LifecycleResult {
    /// Create new result
    pub fn new() -> Self {
        Self {
            exit_codes: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            success: true,
        }
    }

    /// Add command result
    pub fn add_command_result(&mut self, exit_code: i32, stdout: String, stderr: String) {
        self.exit_codes.push(exit_code);
        if !stdout.is_empty() {
            if !self.stdout.is_empty() {
                self.stdout.push('\n');
            }
            self.stdout.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !self.stderr.is_empty() {
                self.stderr.push('\n');
            }
            self.stderr.push_str(&stderr);
        }
        if exit_code != 0 {
            self.success = false;
        }
    }
}

impl Default for LifecycleResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a lifecycle phase with the given commands and context
///
/// This function runs commands sequentially and captures their output.
/// If any command fails, execution halts and returns an error with phase context.
#[instrument(skip(commands, ctx), fields(phase = %phase.as_str()))]
pub fn run_phase(
    phase: LifecyclePhase,
    commands: &LifecycleCommands,
    ctx: &ExecutionContext,
) -> Result<LifecycleResult> {
    info!("Starting lifecycle phase: {}", phase.as_str());

    let mut result = LifecycleResult::new();

    for (i, command_template) in commands.commands.iter().enumerate() {
        debug!(
            "Executing command {} of {} for phase {}: {}",
            i + 1,
            commands.commands.len(),
            phase.as_str(),
            command_template.command
        );

        // Create the actual command from the template
        let mut command = if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", &command_template.command]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", &command_template.command]);
            cmd
        };

        // Set working directory if specified
        if let Some(ref dir) = ctx.working_directory {
            command.current_dir(dir);
        }

        // Add environment variables from template and context
        for (key, value) in &command_template.env_vars {
            command.env(key, value);
        }
        for (key, value) in &ctx.environment {
            command.env(key, value);
        }

        // Configure stdio
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        // Execute command
        let mut child = command.spawn().map_err(|e| {
            DeaconError::Lifecycle(format!(
                "Failed to spawn command for phase {}: {}",
                phase.as_str(),
                e
            ))
        })?;

        // Capture stdout line by line
        let stdout_reader = BufReader::new(child.stdout.take().unwrap());
        let stderr_reader = BufReader::new(child.stderr.take().unwrap());

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        // Read stdout
        for line in stdout_reader.lines() {
            let line =
                line.map_err(|e| DeaconError::Lifecycle(format!("Failed to read stdout: {}", e)))?;
            
            // Apply redaction to the line before logging
            let redacted_line = redact_if_enabled(&line, &ctx.redaction_config);
            info!("[{}] stdout: {}", phase.as_str(), redacted_line);
            stdout_lines.push(line); // Store original for result, log redacted
        }

        // Read stderr
        for line in stderr_reader.lines() {
            let line =
                line.map_err(|e| DeaconError::Lifecycle(format!("Failed to read stderr: {}", e)))?;
            
            // Apply redaction to the line before logging
            let redacted_line = redact_if_enabled(&line, &ctx.redaction_config);
            info!("[{}] stderr: {}", phase.as_str(), redacted_line);
            stderr_lines.push(line); // Store original for result, log redacted
        }

        // Wait for command to complete
        let exit_status = child.wait().map_err(|e| {
            DeaconError::Lifecycle(format!(
                "Failed to wait for command in phase {}: {}",
                phase.as_str(),
                e
            ))
        })?;

        let exit_code = exit_status.code().unwrap_or(-1);
        let stdout = stdout_lines.join("\n");
        let stderr = stderr_lines.join("\n");

        // Apply redaction to the combined output for the result
        let redacted_stdout = redact_if_enabled(&stdout, &ctx.redaction_config);
        let redacted_stderr = redact_if_enabled(&stderr, &ctx.redaction_config);

        debug!("Command completed with exit code: {}", exit_code);

        result.add_command_result(exit_code, redacted_stdout, redacted_stderr);

        // If command failed, halt execution and return error with phase context
        if exit_code != 0 {
            error!(
                "Command failed in phase {} with exit code {}",
                phase.as_str(),
                exit_code
            );
            return Err(DeaconError::Lifecycle(format!(
                "Command failed in phase {} with exit code {}: Command: {}",
                phase.as_str(),
                exit_code,
                command_template.command
            )));
        }
    }

    info!("Completed lifecycle phase: {}", phase.as_str());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_lifecycle_phase_as_str() {
        assert_eq!(LifecyclePhase::Initialize.as_str(), "initialize");
        assert_eq!(LifecyclePhase::OnCreate.as_str(), "onCreate");
        assert_eq!(LifecyclePhase::PostCreate.as_str(), "postCreate");
        assert_eq!(LifecyclePhase::PostStart.as_str(), "postStart");
        assert_eq!(LifecyclePhase::PostAttach.as_str(), "postAttach");
        assert_eq!(LifecyclePhase::UpdateContent.as_str(), "updateContent");
    }

    #[test]
    fn test_lifecycle_commands_from_string() {
        let env = HashMap::new();
        let value = json!("echo 'hello world'");
        let commands = LifecycleCommands::from_json_value(&value, &env).unwrap();
        assert_eq!(commands.commands.len(), 1);
        assert_eq!(commands.commands[0].command, "echo 'hello world'");
    }

    #[test]
    fn test_lifecycle_commands_from_array() {
        let env = HashMap::new();
        let value = json!(["echo 'hello'", "echo 'world'"]);
        let commands = LifecycleCommands::from_json_value(&value, &env).unwrap();
        assert_eq!(commands.commands.len(), 2);
        assert_eq!(commands.commands[0].command, "echo 'hello'");
        assert_eq!(commands.commands[1].command, "echo 'world'");
    }

    #[test]
    fn test_lifecycle_commands_invalid_format() {
        let env = HashMap::new();
        let value = json!(42);
        let result = LifecycleCommands::from_json_value(&value, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_context_creation() {
        let ctx = ExecutionContext::new()
            .with_env("TEST_VAR".to_string(), "test_value".to_string())
            .with_working_directory("/tmp".into());

        assert_eq!(
            ctx.environment.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
        assert_eq!(ctx.working_directory, Some("/tmp".into()));
    }

    #[test]
    fn test_lifecycle_result_creation() {
        let mut result = LifecycleResult::new();
        assert!(result.success);
        assert!(result.exit_codes.is_empty());

        result.add_command_result(0, "output".to_string(), "".to_string());
        assert!(result.success);
        assert_eq!(result.exit_codes, vec![0]);
        assert_eq!(result.stdout, "output");

        result.add_command_result(1, "".to_string(), "error".to_string());
        assert!(!result.success);
        assert_eq!(result.exit_codes, vec![0, 1]);
        assert_eq!(result.stderr, "error");
    }
}
