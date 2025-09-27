//! Configuration command implementation
//!
//! Implements the `deacon config` subcommand for configuration management
//! including advanced variable substitution preview and validation.

use anyhow::Result;
use serde_json;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use deacon_core::config::ConfigLoader;
use deacon_core::errors::{ConfigError, DeaconError};
use deacon_core::observability::{config_resolve_span, TimedSpan};
use deacon_core::redaction::{redact_if_enabled, RedactionConfig};
use deacon_core::secrets::SecretsCollection;
use deacon_core::variable::{SubstitutionContext, SubstitutionOptions};

use crate::cli::{ConfigCommands, OutputFormat};

/// Config command arguments
#[derive(Debug, Clone)]
pub struct ConfigArgs {
    pub command: ConfigCommands,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub override_config_path: Option<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
    pub redaction_config: RedactionConfig,
}

/// Execute the config command
pub async fn execute_config(args: ConfigArgs) -> Result<()> {
    // Start standardized span for configuration resolution
    let timed_span = TimedSpan::new(config_resolve_span(
        args.workspace_folder.as_deref().unwrap_or(Path::new(".")),
    ));

    let result = {
        let _guard = timed_span.span().enter();

        info!("Starting config command execution");
        debug!("Config args: {:?}", args);

        match args.command {
            ConfigCommands::Substitute {
                dry_run,
                strict_substitution,
                max_depth,
                nested,
                output_format,
            } => {
                let substitute_args = ConfigSubstituteArgs {
                    dry_run,
                    strict_substitution,
                    max_depth,
                    enable_nested: nested,
                    output_format,
                    workspace_folder: args.workspace_folder,
                    config_path: args.config_path,
                    override_config_path: args.override_config_path,
                    secrets_files: args.secrets_files,
                    redaction_config: args.redaction_config.clone(),
                };
                execute_config_substitute(substitute_args).await
            }
        }
    };

    // Complete the timed span with duration
    timed_span.complete();
    result
}

/// Execute the config substitute command
#[derive(Debug)]
struct ConfigSubstituteArgs {
    dry_run: bool,
    strict_substitution: bool,
    max_depth: usize,
    enable_nested: bool,
    output_format: OutputFormat,
    workspace_folder: Option<PathBuf>,
    config_path: Option<PathBuf>,
    override_config_path: Option<PathBuf>,
    secrets_files: Vec<PathBuf>,
    redaction_config: RedactionConfig,
}

/// Execute the config substitute command
async fn execute_config_substitute(args: ConfigSubstituteArgs) -> Result<()> {
    info!("Starting config substitute command");

    // Determine workspace folder
    let workspace_folder = args.workspace_folder.as_deref().unwrap_or(Path::new("."));

    // Load secrets if provided
    let secrets = if !args.secrets_files.is_empty() {
        Some(SecretsCollection::load_from_files(&args.secrets_files)?)
    } else {
        None
    };

    // Load configuration
    let (config, substitution_report) = if let Some(config_path) = args.config_path.as_ref() {
        // For specified config, still apply overrides and substitution
        let base_config = ConfigLoader::load_from_path(config_path)?;
        let mut configs = vec![base_config];

        // Add override config if provided
        if let Some(override_path) = args.override_config_path.as_ref() {
            let override_config = ConfigLoader::load_from_path(override_path)?;
            configs.push(override_config);
        }

        let merged = deacon_core::config::ConfigMerger::merge_configs(&configs);

        // Apply variable substitution with secrets and advanced options
        let mut substitution_context = SubstitutionContext::new(workspace_folder)?;
        if let Some(ref secrets) = secrets {
            for (key, value) in secrets.as_env_vars() {
                substitution_context
                    .local_env
                    .insert(key.clone(), value.clone());
            }
        }

        let substitution_options = SubstitutionOptions {
            max_depth: args.max_depth,
            strict: args.strict_substitution,
            enable_nested: args.enable_nested,
        };

        // Use advanced substitution with specified options
        let mut report = deacon_core::variable::SubstitutionReport::new();
        let substituted_config = merged.apply_variable_substitution_advanced(
            &substitution_context,
            &substitution_options,
            &mut report,
        )?;

        (substituted_config, report)
    } else {
        // Discover configuration
        let config_location = ConfigLoader::discover_config(workspace_folder)?;
        if !config_location.exists() {
            return Err(DeaconError::Config(ConfigError::NotFound {
                path: config_location.path().to_string_lossy().to_string(),
            })
            .into());
        }

        // For discovered config, still apply overrides and substitution
        let base_config = ConfigLoader::load_from_path(config_location.path())?;
        let mut configs = vec![base_config];

        // Add override config if provided
        if let Some(override_path) = args.override_config_path.as_ref() {
            let override_config = ConfigLoader::load_from_path(override_path)?;
            configs.push(override_config);
        }

        let merged = deacon_core::config::ConfigMerger::merge_configs(&configs);

        // Apply variable substitution with secrets and advanced options
        let mut substitution_context = SubstitutionContext::new(workspace_folder)?;
        if let Some(ref secrets) = secrets {
            for (key, value) in secrets.as_env_vars() {
                substitution_context
                    .local_env
                    .insert(key.clone(), value.clone());
            }
        }

        let substitution_options = SubstitutionOptions {
            max_depth: args.max_depth,
            strict: args.strict_substitution,
            enable_nested: args.enable_nested,
        };

        // Use advanced substitution with specified options
        let mut report = deacon_core::variable::SubstitutionReport::new();
        let substituted_config = merged.apply_variable_substitution_advanced(
            &substitution_context,
            &substitution_options,
            &mut report,
        )?;

        (substituted_config, report)
    };

    // Prepare output data
    let output_data = ConfigSubstituteOutput {
        dry_run: args.dry_run,
        configuration: config,
        substitution_report: substitution_report.clone(),
        options: ConfigSubstituteOptions {
            strict_substitution: args.strict_substitution,
            max_depth: args.max_depth,
            enable_nested: args.enable_nested,
        },
    };

    // Output results based on format
    match args.output_format {
        OutputFormat::Json => {
            let mut json_output = serde_json::to_string_pretty(&output_data)?;
            // Apply redaction to JSON output
            json_output = redact_if_enabled(&json_output, &args.redaction_config);
            println!("{}", json_output);
        }
        OutputFormat::Text => {
            print_text_output(&output_data, &args.redaction_config);
        }
    }

    // Summary logging
    info!(
        "Completed config substitute: dry_run={} substitutions={} unknown={} cycles={} passes={}",
        args.dry_run,
        substitution_report.replacements.len(),
        substitution_report.unknown_variables.len(),
        substitution_report.cycle_warnings.len(),
        substitution_report.passes
    );

    Ok(())
}

/// Output structure for config substitute command
#[derive(Debug, serde::Serialize)]
struct ConfigSubstituteOutput {
    dry_run: bool,
    configuration: deacon_core::config::DevContainerConfig,
    substitution_report: deacon_core::variable::SubstitutionReport,
    options: ConfigSubstituteOptions,
}

/// Options used for substitution
#[derive(Debug, serde::Serialize)]
struct ConfigSubstituteOptions {
    strict_substitution: bool,
    max_depth: usize,
    enable_nested: bool,
}

/// Print text format output
fn print_text_output(output: &ConfigSubstituteOutput, redaction_config: &RedactionConfig) {
    println!("Configuration Substitution Results");
    println!("=================================");
    println!();

    if output.dry_run {
        println!("Mode: DRY RUN (preview only)");
    } else {
        println!("Mode: LIVE");
    }
    println!();

    println!("Substitution Options:");
    println!(
        "  Strict mode: {}",
        if output.options.strict_substitution {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!("  Max depth: {}", output.options.max_depth);
    println!(
        "  Nested resolution: {}",
        if output.options.enable_nested {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!("  Passes performed: {}", output.substitution_report.passes);
    println!();

    println!("Substitution Report:");
    if output.substitution_report.replacements.is_empty() {
        println!("  No variables substituted");
    } else {
        println!(
            "  Variables substituted ({}):",
            output.substitution_report.replacements.len()
        );
        for (var, value) in &output.substitution_report.replacements {
            // Apply redaction to sensitive values
            let redacted_value = redact_if_enabled(value, redaction_config);
            // Truncate long values for readability
            let display_value = if redacted_value.len() > 50 {
                format!("{}...", &redacted_value[..47])
            } else {
                redacted_value
            };
            println!("    {} -> {}", var, display_value);
        }
    }

    if !output.substitution_report.unknown_variables.is_empty() {
        println!();
        println!(
            "  Unknown variables ({}):",
            output.substitution_report.unknown_variables.len()
        );
        for var in &output.substitution_report.unknown_variables {
            println!("    {}", var);
        }
    }

    if !output.substitution_report.cycle_warnings.is_empty() {
        println!();
        println!(
            "  Cycle warnings ({}):",
            output.substitution_report.cycle_warnings.len()
        );
        for warning in &output.substitution_report.cycle_warnings {
            println!("    {}", warning);
        }
    }

    if !output.substitution_report.failed_variables.is_empty() {
        println!();
        println!(
            "  Failed variables (strict mode) ({}):",
            output.substitution_report.failed_variables.len()
        );
        for var in &output.substitution_report.failed_variables {
            println!("    {}", var);
        }
    }

    println!();
    println!(
        "Configuration Name: {}",
        output.configuration.name.as_deref().unwrap_or("(unnamed)")
    );
    println!(
        "Configuration Image: {}",
        output.configuration.image.as_deref().unwrap_or("(none)")
    );

    if output.dry_run {
        println!();
        println!("This was a dry run. No changes were applied.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_config_substitute_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "ubuntu:20.04",
            "workspaceFolder": "${localWorkspaceFolder}/src"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ConfigSubstituteArgs {
            dry_run: true,
            strict_substitution: false,
            max_depth: 5,
            enable_nested: true,
            output_format: OutputFormat::Json,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
        };

        let result = execute_config_substitute(args).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_config_substitute_strict_mode() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let config_content = r#"{
            "name": "test-container",
            "image": "ubuntu:20.04",
            "workspaceFolder": "${unknownVariable}/src"
        }"#;

        fs::write(&config_path, config_content).unwrap();

        let args = ConfigSubstituteArgs {
            dry_run: true,
            strict_substitution: true,
            max_depth: 5,
            enable_nested: true,
            output_format: OutputFormat::Json,
            workspace_folder: Some(temp_dir.path().to_path_buf()),
            config_path: Some(config_path),
            override_config_path: None,
            secrets_files: vec![],
            redaction_config: RedactionConfig::default(),
        };

        let result = execute_config_substitute(args).await;

        // Should fail in strict mode with unknown variable
        assert!(result.is_err());
    }
}
