#![cfg(feature = "full")]
//! Integration tests for template application workflow
//!
//! Tests the template apply command with option resolution, file materialization,
//! and variable substitution

use assert_cmd::prelude::*;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

#[tokio::test]
async fn test_template_apply_with_options() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;

    // Create template directory structure
    let template_dir = temp_dir.path().join("test_template");
    fs::create_dir_all(&template_dir)?;
    fs::create_dir_all(template_dir.join("src"))?;

    // Create template metadata
    let metadata = serde_json::json!({
        "id": "test-template",
        "name": "Test Template",
        "description": "A test template with options",
        "options": {
            "projectName": {
                "type": "string",
                "default": "my-project",
                "description": "Name of the project"
            },
            "enableDebug": {
                "type": "boolean",
                "default": false,
                "description": "Enable debug mode"
            },
            "version": {
                "type": "string",
                "enum": ["v1", "v2", "v3"],
                "default": "v2",
                "description": "Version to use"
            }
        }
    });
    fs::write(
        template_dir.join("devcontainer-template.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    // Create template files with templateOption variables
    fs::write(
        template_dir.join("README.md"),
        "# ${templateOption:projectName}\n\nDebug: ${templateOption:enableDebug}\nVersion: ${templateOption:version}\nWorkspace: ${localWorkspaceFolder}",
    )?;

    fs::write(
        template_dir.join("src").join("config.json"),
        r#"{"name": "${templateOption:projectName}", "debug": ${templateOption:enableDebug}, "version": "${templateOption:version}"}"#,
    )?;

    // Create workspace directory
    let workspace_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir)?;

    // Test template application with options
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args([
        "templates",
        "apply",
        template_dir.to_str().unwrap(),
        "--output",
        workspace_dir.to_str().unwrap(),
        "--option",
        "projectName=awesome-app",
        "--option",
        "enableDebug=true",
        "--option",
        "version=v3",
    ]);

    let output = cmd.output()?;

    // Check command succeeded
    if !output.status.success() {
        panic!(
            "Command failed with exit code {:?}:\nstdout: {}\nstderr: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Verify files were created
    assert!(workspace_dir.join("README.md").exists());
    assert!(workspace_dir.join("src").join("config.json").exists());

    // Verify template option substitution
    let readme_content = fs::read_to_string(workspace_dir.join("README.md"))?;

    assert!(readme_content.contains("# awesome-app"));
    assert!(readme_content.contains("Debug: true"));
    assert!(readme_content.contains("Version: v3"));

    // Use canonicalized path for comparison since that's what the variable substitution uses
    let canonical_workspace = workspace_dir.canonicalize()?;
    assert!(readme_content.contains(&format!("Workspace: {}", canonical_workspace.display())));

    let config_content = fs::read_to_string(workspace_dir.join("src").join("config.json"))?;
    assert!(config_content.contains(r#""name": "awesome-app""#));
    assert!(config_content.contains(r#""debug": true"#));
    assert!(config_content.contains(r#""version": "v3""#));

    Ok(())
}

#[tokio::test]
async fn test_template_apply_dry_run() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;

    // Create minimal template
    let template_dir = temp_dir.path().join("test_template");
    fs::create_dir_all(&template_dir)?;

    let metadata = serde_json::json!({
        "id": "test-template",
        "name": "Test Template"
    });
    fs::write(
        template_dir.join("devcontainer-template.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    fs::write(template_dir.join("test.txt"), "Hello World")?;

    let workspace_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir)?;

    // Test dry run mode
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args([
        "templates",
        "apply",
        template_dir.to_str().unwrap(),
        "--output",
        workspace_dir.to_str().unwrap(),
        "--dry-run",
    ]);

    let output = cmd.output()?;
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check for dry run indicators in either stdout or stderr
    let combined_output = format!("{}{}", stdout, stderr);
    assert!(
        combined_output.contains("DRY RUN") || combined_output.contains("Would copy"),
        "Expected dry run output but got:\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Verify no files were actually created
    assert!(!workspace_dir.join("test.txt").exists());

    Ok(())
}

#[tokio::test]
async fn test_template_apply_option_validation() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;

    // Create template with enum option
    let template_dir = temp_dir.path().join("test_template");
    fs::create_dir_all(&template_dir)?;

    let metadata = serde_json::json!({
        "id": "test-template",
        "name": "Test Template",
        "options": {
            "mode": {
                "type": "string",
                "enum": ["dev", "prod"],
                "default": "dev"
            }
        }
    });
    fs::write(
        template_dir.join("devcontainer-template.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    fs::write(
        template_dir.join("config.txt"),
        "mode: ${templateOption:mode}",
    )?;

    let workspace_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir)?;

    // Test invalid enum value
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args([
        "templates",
        "apply",
        template_dir.to_str().unwrap(),
        "--output",
        workspace_dir.to_str().unwrap(),
        "--option",
        "mode=invalid",
    ]);

    let output = cmd.output()?;
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid value"));
    assert!(stderr.contains("Valid choices"));

    Ok(())
}

#[tokio::test]
async fn test_template_apply_missing_required_option() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;

    // Create template with required option (no default)
    let template_dir = temp_dir.path().join("test_template");
    fs::create_dir_all(&template_dir)?;

    let metadata = serde_json::json!({
        "id": "test-template",
        "name": "Test Template",
        "options": {
            "requiredOption": {
                "type": "string",
                "description": "This is required"
                // No default value
            },
            "optionalOption": {
                "type": "string",
                "default": "default-value",
                "description": "This is optional"
            }
        }
    });
    fs::write(
        template_dir.join("devcontainer-template.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    fs::write(
        template_dir.join("config.txt"),
        "required: ${templateOption:requiredOption}",
    )?;

    let workspace_dir = temp_dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir)?;

    // Test missing required option
    let mut cmd = Command::cargo_bin("deacon")?;
    cmd.args([
        "templates",
        "apply",
        template_dir.to_str().unwrap(),
        "--output",
        workspace_dir.to_str().unwrap(),
        // Not providing requiredOption
        "--option",
        "optionalOption=custom-value",
    ]);

    let output = cmd.output()?;
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Missing required option"));
    assert!(stderr.contains("requiredOption"));

    Ok(())
}
