//! DevContainer templates system
//!
//! This module handles template metadata parsing and application with variable substitution.
//! It supports the core template workflow defined in the CLI specification:
//! - Template metadata parsing from devcontainer-template.json
//! - File copying with variable substitution
//! - Dry-run mode for previewing changes
//! - Overwrite protection

use crate::errors::{Result, TemplateError};
use crate::features::{FeatureOption, OptionValue};
use crate::variable::{SubstitutionContext, SubstitutionReport, VariableSubstitution};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument, warn};

/// Template metadata structure representing devcontainer-template.json
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateMetadata {
    /// Template identifier (required)
    pub id: String,

    /// Template version
    #[serde(default)]
    pub version: Option<String>,

    /// Human-readable name
    #[serde(default)]
    pub name: Option<String>,

    /// Template description
    #[serde(default)]
    pub description: Option<String>,

    /// Documentation URL
    #[serde(default, rename = "documentationURL")]
    pub documentation_url: Option<String>,

    /// License URL
    #[serde(default, rename = "licenseURL")]
    pub license_url: Option<String>,

    /// Template options (reusing FeatureOption for consistency)
    #[serde(default)]
    pub options: HashMap<String, FeatureOption>,

    /// Recommended features (raw, as specified in issue)
    #[serde(default)]
    pub recommended_features: Option<serde_json::Value>,

    /// File list (optional, added during packaging)
    #[serde(default)]
    pub files: Option<Vec<String>>,

    /// Supported platforms
    #[serde(default)]
    pub platforms: Option<Vec<String>>,

    /// Template publisher
    #[serde(default)]
    pub publisher: Option<String>,

    /// Search keywords
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
}

impl TemplateMetadata {
    /// Validate the template metadata
    pub fn validate(&self) -> std::result::Result<(), TemplateError> {
        // Required field validation
        if self.id.is_empty() {
            return Err(TemplateError::Validation {
                message: "Template id is required and cannot be empty".to_string(),
            });
        }

        // Validate option defaults
        for (option_name, option_def) in &self.options {
            if let Some(default_value) = option_def.default_value() {
                if let Err(err) = option_def.validate_value(&default_value) {
                    return Err(TemplateError::Validation {
                        message: format!(
                            "Default value for option '{}' is invalid: {}",
                            option_name, err
                        ),
                    });
                }
            }
        }

        Ok(())
    }
}

/// Template application options
#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    /// Template option values
    pub options: HashMap<String, OptionValue>,
    /// Allow overwriting existing files (default: false)
    pub overwrite: bool,
    /// Dry run mode - plan actions without executing (default: false)
    pub dry_run: bool,
}

/// Planned action for template application
#[derive(Debug, Clone, PartialEq)]
pub enum PlannedAction {
    /// Copy a file from source to destination
    CopyFile {
        src: PathBuf,
        dest: PathBuf,
        has_substitutions: bool,
    },
    /// Skip copying file because it already exists
    SkipExistingFile { dest: PathBuf },
    /// Overwrite existing file
    OverwriteFile {
        src: PathBuf,
        dest: PathBuf,
        has_substitutions: bool,
    },
}

/// Template application result
#[derive(Debug)]
pub struct ApplyResult {
    /// Actions that were planned/executed
    pub actions: Vec<PlannedAction>,
    /// Variable substitution report
    pub substitution_report: SubstitutionReport,
    /// Number of files processed
    pub files_processed: usize,
    /// Number of files skipped
    pub files_skipped: usize,
}

/// Parse template metadata from a devcontainer-template.json file
#[instrument(level = "debug")]
pub fn parse_template_metadata(path: &Path) -> Result<TemplateMetadata> {
    debug!("Parsing template metadata from: {}", path.display());

    // Check if file exists
    if !path.exists() {
        return Err(TemplateError::NotFound {
            path: path.display().to_string(),
        }
        .into());
    }

    // Read file content
    let content = fs::read_to_string(path).map_err(TemplateError::Io)?;

    // Parse JSON
    let metadata: TemplateMetadata =
        serde_json::from_str(&content).map_err(|e| TemplateError::Parsing {
            message: e.to_string(),
        })?;

    debug!(
        "Parsed template: id={}, name={:?}",
        metadata.id, metadata.name
    );

    // Log options
    for (option_name, option_def) in &metadata.options {
        debug!("Option '{}': {:?}", option_name, option_def);
    }

    // Validate metadata
    metadata.validate()?;

    Ok(metadata)
}

/// Apply template from source directory to destination workspace
#[instrument(level = "info", skip(apply_options))]
pub fn apply_template(
    src_dir: &Path,
    dest_workspace: &Path,
    apply_options: &ApplyOptions,
) -> Result<ApplyResult> {
    info!(
        "Applying template from {} to {}",
        src_dir.display(),
        dest_workspace.display()
    );

    // Validate source directory exists
    if !src_dir.exists() {
        return Err(TemplateError::NotFound {
            path: src_dir.display().to_string(),
        }
        .into());
    }

    // Create substitution context for the destination workspace
    let mut context = SubstitutionContext::new(dest_workspace)?;

    // Add template options to substitution context
    if !apply_options.options.is_empty() {
        let template_options: HashMap<String, String> = apply_options
            .options
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string()))
            .collect();
        context.template_options = Some(template_options);
    }

    let mut substitution_report = SubstitutionReport::new();

    // Plan actions by walking source directory
    let mut actions = Vec::new();
    let mut files_processed = 0;
    let mut files_skipped = 0;

    plan_template_application(
        src_dir,
        dest_workspace,
        src_dir,
        apply_options,
        &mut actions,
        &mut files_processed,
        &mut files_skipped,
    )?;

    info!(
        "Planned {} actions ({} files to process, {} files to skip)",
        actions.len(),
        files_processed,
        files_skipped
    );

    // Execute actions if not in dry run mode
    if !apply_options.dry_run {
        execute_planned_actions(&actions, &context, &mut substitution_report)?;
        info!("Template application completed successfully");
    } else {
        info!("Dry run completed - no files were modified");
    }

    Ok(ApplyResult {
        actions,
        substitution_report,
        files_processed,
        files_skipped,
    })
}

/// Recursively plan template application actions
#[instrument(level = "debug", skip_all)]
fn plan_template_application(
    current_src: &Path,
    dest_workspace: &Path,
    src_root: &Path,
    apply_options: &ApplyOptions,
    actions: &mut Vec<PlannedAction>,
    files_processed: &mut usize,
    files_skipped: &mut usize,
) -> Result<()> {
    let entries = fs::read_dir(current_src).map_err(TemplateError::Io)?;

    for entry in entries {
        let entry = entry.map_err(TemplateError::Io)?;
        let src_path = entry.path();

        // Calculate relative path from source root
        let relative_path =
            src_path
                .strip_prefix(src_root)
                .map_err(|_| TemplateError::Validation {
                    message: format!(
                        "Failed to calculate relative path for {}",
                        src_path.display()
                    ),
                })?;

        // Skip template metadata file
        if src_path.file_name() == Some(std::ffi::OsStr::new("devcontainer-template.json")) {
            debug!("Skipping template metadata file: {}", src_path.display());
            continue;
        }

        let dest_path = dest_workspace.join(relative_path);

        if src_path.is_dir() {
            // Recursively process subdirectory
            if !dest_path.exists() && !apply_options.dry_run {
                fs::create_dir_all(&dest_path).map_err(TemplateError::Io)?;
                debug!("Created directory: {}", dest_path.display());
            }

            plan_template_application(
                &src_path,
                dest_workspace,
                src_root,
                apply_options,
                actions,
                files_processed,
                files_skipped,
            )?;
        } else {
            // Plan file copy action
            let action = if dest_path.exists() {
                if apply_options.overwrite {
                    *files_processed += 1;
                    PlannedAction::OverwriteFile {
                        src: src_path.clone(),
                        dest: dest_path,
                        has_substitutions: should_apply_substitution(&src_path),
                    }
                } else {
                    *files_skipped += 1;
                    PlannedAction::SkipExistingFile { dest: dest_path }
                }
            } else {
                *files_processed += 1;
                PlannedAction::CopyFile {
                    src: src_path.clone(),
                    dest: dest_path,
                    has_substitutions: should_apply_substitution(&src_path),
                }
            };

            actions.push(action);
        }
    }

    Ok(())
}

/// Execute planned actions
#[instrument(level = "debug", skip_all)]
fn execute_planned_actions(
    actions: &[PlannedAction],
    context: &SubstitutionContext,
    substitution_report: &mut SubstitutionReport,
) -> Result<()> {
    for action in actions {
        match action {
            PlannedAction::CopyFile {
                src,
                dest,
                has_substitutions,
            }
            | PlannedAction::OverwriteFile {
                src,
                dest,
                has_substitutions,
            } => {
                // Ensure destination directory exists
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(TemplateError::Io)?;
                }

                if *has_substitutions {
                    copy_file_with_substitution(src, dest, context, substitution_report)?;
                } else {
                    fs::copy(src, dest).map_err(TemplateError::Io)?;
                }

                debug!("Copied file: {} -> {}", src.display(), dest.display());
            }
            PlannedAction::SkipExistingFile { dest } => {
                debug!("Skipped existing file: {}", dest.display());
            }
        }
    }

    Ok(())
}

/// Copy file with variable substitution applied to text content
#[instrument(level = "debug", skip_all)]
fn copy_file_with_substitution(
    src: &Path,
    dest: &Path,
    context: &SubstitutionContext,
    substitution_report: &mut SubstitutionReport,
) -> Result<()> {
    // Read source file as string (assume text file for substitution)
    let content = fs::read_to_string(src).map_err(|e| {
        warn!(
            "Failed to read file as text for substitution: {} - {}",
            src.display(),
            e
        );
        TemplateError::Io(e)
    })?;

    // Apply variable substitution
    let substituted_content =
        VariableSubstitution::substitute_string(content.as_str(), context, substitution_report);

    // Write substituted content to destination
    fs::write(dest, substituted_content).map_err(TemplateError::Io)?;

    Ok(())
}

/// Determine if variable substitution should be applied to a file
/// For now, we apply substitution to common text file extensions
fn should_apply_substitution(path: &Path) -> bool {
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        matches!(
            extension.to_lowercase().as_str(),
            "json"
                | "yaml"
                | "yml"
                | "sh"
                | "bash"
                | "zsh"
                | "txt"
                | "md"
                | "dockerfile"
                | "conf"
                | "config"
                | "rs"
                | "py"
                | "js"
                | "ts"
        )
    } else {
        // Files without extension - check if they have common names that should be substituted
        if let Some(filename) = path.file_name().and_then(|name| name.to_str()) {
            matches!(
                filename.to_lowercase().as_str(),
                "dockerfile" | "makefile" | "readme" | ".gitignore" | ".dockerignore"
            )
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_parse_minimal_template_metadata() {
        let minimal_template = r#"
        {
            "id": "test-template"
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(minimal_template.as_bytes()).unwrap();

        let metadata = parse_template_metadata(temp_file.path()).unwrap();
        assert_eq!(metadata.id, "test-template");
        assert_eq!(metadata.name, None);
        assert_eq!(metadata.options.len(), 0);
        assert_eq!(metadata.files, None);
    }

    #[test]
    fn test_parse_template_with_options() {
        let template_with_options = r#"
        {
            "id": "test-template",
            "name": "Test Template",
            "description": "A test template",
            "documentationURL": "https://example.com/docs",
            "options": {
                "enableFeature": {
                    "type": "boolean",
                    "default": true,
                    "description": "Enable the feature"
                },
                "version": {
                    "type": "string",
                    "enum": ["latest", "stable"],
                    "default": "stable",
                    "description": "Version to install"
                }
            },
            "recommendedFeatures": {
                "ghcr.io/devcontainers/features/git:1": {}
            },
            "files": ["src/main.rs", "Dockerfile"]
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(template_with_options.as_bytes())
            .unwrap();

        let metadata = parse_template_metadata(temp_file.path()).unwrap();
        assert_eq!(metadata.id, "test-template");
        assert_eq!(metadata.name, Some("Test Template".to_string()));
        assert_eq!(metadata.options.len(), 2);
        assert!(metadata.recommended_features.is_some());
        assert_eq!(metadata.files.as_ref().unwrap().len(), 2);

        // Check boolean option
        let enable_option = metadata.options.get("enableFeature").unwrap();
        match enable_option {
            FeatureOption::Boolean { default, .. } => {
                assert_eq!(*default, Some(true));
            }
            _ => panic!("Expected boolean option"),
        }

        // Check string option with enum
        let version_option = metadata.options.get("version").unwrap();
        match version_option {
            FeatureOption::String {
                default, r#enum, ..
            } => {
                assert_eq!(*default, Some("stable".to_string()));
                assert_eq!(r#enum.as_ref().unwrap(), &vec!["latest", "stable"]);
            }
            _ => panic!("Expected string option"),
        }
    }

    #[test]
    fn test_parse_invalid_template_schema() {
        let invalid_template = r#"
        {
            "id": "",
            "options": {
                "badOption": {
                    "type": "string",
                    "enum": ["value1", "value2"],
                    "default": "invalid_default"
                }
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_template.as_bytes()).unwrap();

        let result = parse_template_metadata(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = parse_template_metadata(Path::new("/nonexistent/path/template.json"));
        assert!(result.is_err());

        if let Err(crate::errors::DeaconError::Template(TemplateError::NotFound { .. })) = result {
            // Expected
        } else {
            panic!("Expected NotFound error");
        }
    }

    #[test]
    fn test_apply_options_default() {
        let options = ApplyOptions::default();
        assert!(!options.overwrite);
        assert!(!options.dry_run);
        assert_eq!(options.options.len(), 0);
    }

    #[test]
    fn test_should_apply_substitution() {
        // Text files that should have substitution
        assert!(should_apply_substitution(Path::new("config.json")));
        assert!(should_apply_substitution(Path::new("docker-compose.yml")));
        assert!(should_apply_substitution(Path::new("script.sh")));
        assert!(should_apply_substitution(Path::new("README.md")));
        assert!(should_apply_substitution(Path::new("Dockerfile")));
        assert!(should_apply_substitution(Path::new("Makefile")));

        // Binary files that should not have substitution
        assert!(!should_apply_substitution(Path::new("image.png")));
        assert!(!should_apply_substitution(Path::new("binary.exe")));
        assert!(!should_apply_substitution(Path::new("data.bin")));
        assert!(!should_apply_substitution(Path::new("unknown")));
    }

    #[test]
    fn test_apply_template_dry_run() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let src_dir = temp_dir.path().join("template");
        let dest_dir = temp_dir.path().join("workspace");

        // Create template structure
        fs::create_dir_all(&src_dir)?;
        fs::create_dir_all(&dest_dir)?;

        // Create template metadata
        let metadata = r#"
        {
            "id": "test-template",
            "name": "Test Template"
        }
        "#;
        fs::write(src_dir.join("devcontainer-template.json"), metadata)?;

        // Create template files
        let dockerfile_content = "FROM ubuntu:20.04\nWORKDIR ${localWorkspaceFolder}";
        fs::write(src_dir.join("Dockerfile"), dockerfile_content)?;

        let readme_content = "# Project\nWorkspace: ${localWorkspaceFolder}";
        fs::write(src_dir.join("README.md"), readme_content)?;

        // Apply template in dry run mode
        let options = ApplyOptions {
            dry_run: true,
            ..Default::default()
        };

        let result = apply_template(&src_dir, &dest_dir, &options)?;

        // Check dry run results
        assert_eq!(result.files_processed, 2);
        assert_eq!(result.files_skipped, 0);
        assert_eq!(result.actions.len(), 2);

        // Verify no files were actually created
        assert!(!dest_dir.join("Dockerfile").exists());
        assert!(!dest_dir.join("README.md").exists());

        // Check actions
        for action in &result.actions {
            match action {
                PlannedAction::CopyFile {
                    src,
                    dest,
                    has_substitutions,
                } => {
                    assert!(src.exists());
                    assert!(!dest.exists());
                    // Should mark text files for substitution
                    if src.file_name().unwrap() == "Dockerfile"
                        || src.file_name().unwrap() == "README.md"
                    {
                        assert!(*has_substitutions);
                    }
                }
                _ => panic!("Expected CopyFile action in dry run"),
            }
        }

        Ok(())
    }

    #[test]
    fn test_apply_template_with_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let src_dir = temp_dir.path().join("template");
        let dest_dir = temp_dir.path().join("workspace");

        // Create template structure
        fs::create_dir_all(&src_dir)?;
        fs::create_dir_all(&dest_dir)?;

        // Create template metadata (should be excluded from copying)
        let metadata = r#"
        {
            "id": "test-template",
            "name": "Test Template"
        }
        "#;
        fs::write(src_dir.join("devcontainer-template.json"), metadata)?;

        // Create template files with variables
        let dockerfile_content = "FROM ubuntu:20.04\nWORKDIR ${localWorkspaceFolder}\nCOPY ${localWorkspaceFolder}/src .";
        fs::write(src_dir.join("Dockerfile"), dockerfile_content)?;

        let config_content = r#"{"workspaceFolder": "${localWorkspaceFolder}"}"#;
        fs::write(src_dir.join("config.json"), config_content)?;

        // Binary file (should be copied without substitution)
        fs::write(src_dir.join("binary.bin"), b"\x00\x01\x02\x03")?;

        // Apply template
        let options = ApplyOptions::default();
        let result = apply_template(&src_dir, &dest_dir, &options)?;

        // Check results
        assert_eq!(result.files_processed, 3);
        assert_eq!(result.files_skipped, 0);
        assert!(result.substitution_report.has_substitutions());

        // Verify files were created and substituted
        let dockerfile = fs::read_to_string(dest_dir.join("Dockerfile"))?;
        assert!(dockerfile.contains(&dest_dir.to_string_lossy().to_string()));
        assert!(!dockerfile.contains("${localWorkspaceFolder}"));

        let config = fs::read_to_string(dest_dir.join("config.json"))?;
        assert!(config.contains(&dest_dir.to_string_lossy().to_string()));
        assert!(!config.contains("${localWorkspaceFolder}"));

        // Binary file should be unchanged
        let binary = fs::read(dest_dir.join("binary.bin"))?;
        assert_eq!(binary, b"\x00\x01\x02\x03");

        // Template metadata should not be copied
        assert!(!dest_dir.join("devcontainer-template.json").exists());

        Ok(())
    }

    #[test]
    fn test_apply_template_overwrite_protection() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let src_dir = temp_dir.path().join("template");
        let dest_dir = temp_dir.path().join("workspace");

        // Create template structure
        fs::create_dir_all(&src_dir)?;
        fs::create_dir_all(&dest_dir)?;

        // Create template files
        fs::write(src_dir.join("Dockerfile"), "FROM ubuntu:20.04")?;

        // Create existing file in destination
        fs::write(dest_dir.join("Dockerfile"), "FROM alpine:latest")?;

        // Apply template without overwrite
        let options = ApplyOptions::default();
        let result = apply_template(&src_dir, &dest_dir, &options)?;

        // Check that existing file was not overwritten
        assert_eq!(result.files_processed, 0);
        assert_eq!(result.files_skipped, 1);

        let content = fs::read_to_string(dest_dir.join("Dockerfile"))?;
        assert_eq!(content, "FROM alpine:latest");

        // Apply template with overwrite
        let options = ApplyOptions {
            overwrite: true,
            ..Default::default()
        };
        let result = apply_template(&src_dir, &dest_dir, &options)?;

        // Check that existing file was overwritten
        assert_eq!(result.files_processed, 1);
        assert_eq!(result.files_skipped, 0);

        let content = fs::read_to_string(dest_dir.join("Dockerfile"))?;
        assert_eq!(content, "FROM ubuntu:20.04");

        Ok(())
    }

    #[test]
    fn test_apply_template_subdirectories() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let src_dir = temp_dir.path().join("template");
        let dest_dir = temp_dir.path().join("workspace");

        // Create template structure with subdirectories
        fs::create_dir_all(&src_dir)?;
        fs::create_dir_all(&dest_dir)?;
        fs::create_dir_all(src_dir.join("src"))?;
        fs::create_dir_all(src_dir.join("config"))?;

        // Create template files in subdirectories
        fs::write(
            src_dir.join("src/main.rs"),
            "fn main() { /* ${localWorkspaceFolder} */ }",
        )?;
        fs::write(
            src_dir.join("config/app.conf"),
            "workspace=${localWorkspaceFolder}",
        )?;

        // Apply template
        let options = ApplyOptions::default();
        let result = apply_template(&src_dir, &dest_dir, &options)?;

        // Check results
        assert_eq!(result.files_processed, 2);
        assert_eq!(result.files_skipped, 0);

        // Verify subdirectories and files were created
        assert!(dest_dir.join("src").is_dir());
        assert!(dest_dir.join("config").is_dir());
        assert!(dest_dir.join("src/main.rs").exists());
        assert!(dest_dir.join("config/app.conf").exists());

        // Verify substitution in subdirectory files
        let main_rs = fs::read_to_string(dest_dir.join("src/main.rs"))?;
        assert!(main_rs.contains(&dest_dir.to_string_lossy().to_string()));

        let app_conf = fs::read_to_string(dest_dir.join("config/app.conf"))?;
        assert!(app_conf.contains(&dest_dir.to_string_lossy().to_string()));

        Ok(())
    }

    #[test]
    fn test_apply_template_nonexistent_source() {
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_src = temp_dir.path().join("nonexistent");
        let dest_dir = temp_dir.path().join("workspace");

        let options = ApplyOptions::default();
        let result = apply_template(&nonexistent_src, &dest_dir, &options);

        assert!(result.is_err());
        if let Err(crate::errors::DeaconError::Template(TemplateError::NotFound { .. })) = result {
            // Expected
        } else {
            panic!("Expected NotFound error");
        }
    }
}
