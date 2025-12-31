//! Integration tests for lifecycle command execution
//!
//! These tests verify that the lifecycle harness can execute commands,
//! capture output, handle errors, and maintain proper phase ordering.
//!
//! Note: These tests use Unix-specific APIs and are only compiled on Unix systems.
#![cfg(unix)]

use deacon_core::lifecycle::{run_phase, ExecutionContext, LifecycleCommands, LifecyclePhase};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

/// Test successful execution of a simple command
#[test]
fn test_run_phase_simple_command() {
    deacon_core::logging::init(None).ok(); // Initialize logging, ignore if already initialized

    let env = HashMap::new();
    let command_value = json!("echo 'Hello, World!'");
    let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();
    let ctx = ExecutionContext::new();

    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    assert!(result.success);
    assert_eq!(result.exit_codes, vec![0]);
    assert!(result.stdout.contains("Hello, World!"));
    assert!(result.stderr.is_empty());
}

/// Test execution of multiple commands in sequence
#[test]
fn test_run_phase_multiple_commands() {
    deacon_core::logging::init(None).ok();

    let env = HashMap::new();
    let command_value = json!(["echo 'First command'", "echo 'Second command'"]);
    let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();
    let ctx = ExecutionContext::new();

    let result = run_phase(LifecyclePhase::Initialize, &commands, &ctx).unwrap();

    assert!(result.success);
    assert_eq!(result.exit_codes, vec![0, 0]);
    assert!(result.stdout.contains("First command"));
    assert!(result.stdout.contains("Second command"));
}

/// Test that failing command halts execution and returns error
#[test]
fn test_run_phase_failing_command() {
    deacon_core::logging::init(None).ok();

    let env = HashMap::new();
    let command_value = json!(["echo 'This works'", "exit 1", "echo 'This should not run'"]);
    let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();
    let ctx = ExecutionContext::new();

    let result = run_phase(LifecyclePhase::OnCreate, &commands, &ctx);

    assert!(result.is_err());
    let error_message = result.unwrap_err().to_string();
    assert!(error_message.contains("Command failed in phase onCreate"));
    assert!(error_message.contains("exit code 1"));
}

/// Test command execution with environment variables
#[test]
fn test_run_phase_with_environment() {
    deacon_core::logging::init(None).ok();

    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let ctx =
        ExecutionContext::new().with_env("ANOTHER_VAR".to_string(), "another_value".to_string());

    let command_value = if cfg!(target_os = "windows") {
        json!("echo %TEST_VAR%_%ANOTHER_VAR%")
    } else {
        json!("echo $TEST_VAR:$ANOTHER_VAR")
    };

    let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();

    let result = run_phase(LifecyclePhase::PostStart, &commands, &ctx).unwrap();

    assert!(result.success);
    if cfg!(target_os = "windows") {
        assert!(result.stdout.contains("test_value_another_value"));
    } else {
        assert!(result.stdout.contains("test_value:another_value"));
    }
}

/// Test execution with working directory
#[test]
fn test_run_phase_with_working_directory() {
    deacon_core::logging::init(None).ok();

    let temp_dir = TempDir::new().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    // Create a test file in the temp directory
    let test_file = temp_path.join("test.txt");
    fs::write(&test_file, "test content").unwrap();

    let env = HashMap::new();
    let command_value = if cfg!(target_os = "windows") {
        json!("dir /b")
    } else {
        json!("ls")
    };

    let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();
    let ctx = ExecutionContext::new().with_working_directory(temp_path);

    let result = run_phase(LifecyclePhase::PostAttach, &commands, &ctx).unwrap();

    assert!(result.success);
    assert!(result.stdout.contains("test.txt"));
}

/// Test proper ordering and delineation of lifecycle phases
#[test]
fn test_lifecycle_phase_ordering() {
    deacon_core::logging::init(None).ok();

    let phases = vec![
        LifecyclePhase::Initialize,
        LifecyclePhase::OnCreate,
        LifecyclePhase::UpdateContent,
        LifecyclePhase::PostCreate,
        LifecyclePhase::PostStart,
        LifecyclePhase::PostAttach,
    ];

    let env = HashMap::new();

    for phase in phases {
        let phase_name = phase.as_str();
        let command_value = json!(format!("echo 'Executing {}'", phase_name));
        let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();
        let ctx = ExecutionContext::new();

        let result = run_phase(phase, &commands, &ctx).unwrap();

        assert!(result.success);
        assert!(result.stdout.contains(&format!("Executing {}", phase_name)));
    }
}

/// Test that stderr output is captured properly
#[test]
fn test_run_phase_stderr_capture() {
    deacon_core::logging::init(None).ok();

    let env = HashMap::new();
    let command_value = if cfg!(target_os = "windows") {
        json!("echo Error message 1>&2")
    } else {
        json!("echo 'Error message' >&2")
    };

    let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();
    let ctx = ExecutionContext::new();

    let result = run_phase(LifecyclePhase::PostCreate, &commands, &ctx).unwrap();

    assert!(result.success);
    assert!(result.stderr.contains("Error message"));
}

/// Test command normalization from JSON values
#[test]
fn test_command_normalization() {
    deacon_core::logging::init(None).ok();

    let env = HashMap::new();

    // Test string command
    let string_cmd = json!("echo 'single command'");
    let commands = LifecycleCommands::from_json_value(&string_cmd, &env).unwrap();
    assert_eq!(commands.commands.len(), 1);
    assert_eq!(commands.commands[0].command, "echo 'single command'");

    // Test array of commands
    let array_cmd = json!(["echo 'first'", "echo 'second'", "echo 'third'"]);
    let commands = LifecycleCommands::from_json_value(&array_cmd, &env).unwrap();
    assert_eq!(commands.commands.len(), 3);
    assert_eq!(commands.commands[0].command, "echo 'first'");
    assert_eq!(commands.commands[1].command, "echo 'second'");
    assert_eq!(commands.commands[2].command, "echo 'third'");

    // Test invalid format
    let invalid_cmd = json!(42);
    let result = LifecycleCommands::from_json_value(&invalid_cmd, &env);
    assert!(result.is_err());

    // Test invalid array element
    let invalid_array = json!(["echo 'valid'", 123]);
    let result = LifecycleCommands::from_json_value(&invalid_array, &env);
    assert!(result.is_err());
}

/// Test comprehensive scenario with script file execution
/// This creates a temporary script and tests more complex command execution
#[test]
fn test_run_phase_with_script_file() {
    deacon_core::logging::init(None).ok();

    let temp_dir = TempDir::new().unwrap();
    let script_path = if cfg!(target_os = "windows") {
        temp_dir.path().join("test_script.bat")
    } else {
        temp_dir.path().join("test_script.sh")
    };

    let script_content = if cfg!(target_os = "windows") {
        r#"@echo off
echo Starting script execution
echo Line 1 output
echo Line 2 output
echo Error line 1>&2
echo Line 3 output
exit 0
"#
    } else {
        r#"#!/bin/bash
echo "Starting script execution"
echo "Line 1 output"
echo "Line 2 output"  
echo "Error line" >&2
echo "Line 3 output"
exit 0
"#
    };

    fs::write(&script_path, script_content).unwrap();

    if !cfg!(target_os = "windows") {
        // Make script executable on Unix systems
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let env = HashMap::new();
    let command_value = if cfg!(target_os = "windows") {
        json!(format!("\"{}\"", script_path.display()))
    } else {
        json!(format!("bash \"{}\"", script_path.display()))
    };

    let commands = LifecycleCommands::from_json_value(&command_value, &env).unwrap();
    let ctx = ExecutionContext::new();

    let result = run_phase(LifecyclePhase::UpdateContent, &commands, &ctx).unwrap();

    assert!(result.success);
    assert_eq!(result.exit_codes, vec![0]);
    assert!(result.stdout.contains("Starting script execution"));
    assert!(result.stdout.contains("Line 1 output"));
    assert!(result.stdout.contains("Line 2 output"));
    assert!(result.stdout.contains("Line 3 output"));
    assert!(result.stderr.contains("Error line"));
}
