//! Templates command implementation
//!
//! Implements the `deacon templates` subcommands for pulling and applying
//! DevContainer templates. Follows the CLI specification for template management.

use crate::cli::TemplateCommands;
use anyhow::Result;
use deacon_core::oci::{default_fetcher, TemplateRef};
use deacon_core::registry_parser::parse_registry_reference;
use std::path::PathBuf;
use tracing::{debug, info, instrument, warn};

/// Templates command arguments
#[derive(Debug, Clone)]
pub struct TemplatesArgs {
    pub command: TemplateCommands,
    #[allow(dead_code)] // Reserved for future use
    pub workspace_folder: Option<PathBuf>,
    #[allow(dead_code)] // Reserved for future use
    pub config_path: Option<PathBuf>,
}

/// Execute the templates command
#[instrument(level = "debug")]
pub async fn execute_templates(args: TemplatesArgs) -> Result<()> {
    match args.command {
        TemplateCommands::Pull { registry_ref, json } => {
            execute_templates_pull(&registry_ref, json).await
        }
        TemplateCommands::Apply {
            template,
            option,
            output,
            force,
            dry_run,
        } => execute_templates_apply(&template, &option, output.as_deref(), force, dry_run).await,
    }
}

/// Execute templates pull command
#[instrument(level = "debug")]
async fn execute_templates_pull(registry_ref: &str, json: bool) -> Result<()> {
    debug!("Pulling template from registry reference: {}", registry_ref);

    // Parse registry reference
    let (registry_url, namespace, name, tag) = parse_registry_reference(registry_ref)?;
    let tag = tag.unwrap_or_else(|| "latest".to_string());

    let template_ref = TemplateRef::new(registry_url, namespace, name, Some(tag));

    info!("Pulling template: {}", template_ref.reference());

    // Create OCI client and fetch from registry
    let fetcher =
        default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

    let downloaded_template = fetcher
        .fetch_template(&template_ref)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to pull template: {}", e))?;

    if json {
        let result = serde_json::json!({
            "command": "pull",
            "status": "success",
            "digest": downloaded_template.digest,
            "message": format!(
                "Successfully pulled {} to {}",
                template_ref.reference(),
                downloaded_template.path.display()
            ),
            "cachePath": downloaded_template.path.to_string_lossy(),
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Command: pull");
        println!("Status: success");
        println!("Digest: {}", downloaded_template.digest);
        println!("Cache Path: {}", downloaded_template.path.to_string_lossy());
        println!(
            "Message: Successfully pulled {} to {}",
            template_ref.reference(),
            downloaded_template.path.display()
        );
    }

    Ok(())
}

/// Execute templates apply command
#[instrument(level = "debug")]
async fn execute_templates_apply(
    template: &str,
    options: &[String],
    output: Option<&str>,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    use deacon_core::features::OptionValue;
    use deacon_core::templates::{apply_template, parse_template_metadata, ApplyOptions};
    use std::collections::HashMap;
    use std::fs;
    use std::path::Path;

    debug!(
        "Applying template: {} with {} options, output: {:?}, force: {}, dry_run: {}",
        template,
        options.len(),
        output,
        force,
        dry_run
    );

    // Determine output directory (default to current directory)
    let output_dir = match output {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir()?,
    };

    debug!("Output directory: {}", output_dir.display());

    // Create output directory if it doesn't exist (except in dry-run mode)
    if !output_dir.exists() && !dry_run {
        fs::create_dir_all(&output_dir)?;
        info!("Created output directory: {}", output_dir.display());
    }

    // Check if template is a local path or registry reference
    let template_path = Path::new(template);
    let (template_dir, metadata) = if template_path.exists() && template_path.is_dir() {
        // Local template directory
        debug!(
            "Using local template directory: {}",
            template_path.display()
        );
        let metadata_file = template_path.join("devcontainer-template.json");
        let metadata = parse_template_metadata(&metadata_file)
            .map_err(|e| anyhow::anyhow!("Failed to parse template metadata: {}", e))?;
        (template_path.to_path_buf(), metadata)
    } else {
        // Registry reference - fetch template first
        debug!("Fetching template from registry: {}", template);

        let (registry_url, namespace, name, tag) = parse_registry_reference(template)?;
        let template_ref = TemplateRef::new(registry_url, namespace, name, tag);

        let fetcher =
            default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

        let downloaded_template = fetcher
            .fetch_template(&template_ref)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch template: {}", e))?;

        let metadata_file = downloaded_template.path.join("devcontainer-template.json");
        let metadata = parse_template_metadata(&metadata_file)
            .map_err(|e| anyhow::anyhow!("Failed to parse template metadata: {}", e))?;

        (downloaded_template.path, metadata)
    };

    // Parse and validate template options
    let mut option_values = HashMap::new();
    for option_str in options {
        let parts: Vec<&str> = option_str.splitn(2, '=').collect();
        if parts.len() != 2 {
            return Err(anyhow::anyhow!(
                "Invalid option format '{}'. Use key=value format.",
                option_str
            ));
        }

        let key = parts[0].trim();
        let value = parts[1].trim();

        // Check if option is defined in template metadata
        if let Some(option_def) = metadata.options.get(key) {
            // Parse value according to option type
            let parsed_value = match option_def {
                deacon_core::features::FeatureOption::Boolean { .. } => {
                    match value.to_lowercase().as_str() {
                        "true" => OptionValue::Boolean(true),
                        "false" => OptionValue::Boolean(false),
                        _ => {
                            return Err(anyhow::anyhow!(
                            "Invalid boolean value '{}' for option '{}'. Use 'true' or 'false'.",
                            value, key
                        ))
                        }
                    }
                }
                deacon_core::features::FeatureOption::String { r#enum, .. } => {
                    // Validate against enum choices if specified
                    if let Some(enum_choices) = r#enum {
                        if !enum_choices.contains(&value.to_string()) {
                            return Err(anyhow::anyhow!(
                                "Invalid value '{}' for option '{}'. Valid choices: {:?}",
                                value,
                                key,
                                enum_choices
                            ));
                        }
                    }
                    OptionValue::String(value.to_string())
                }
            };

            // Validate the parsed value
            if let Err(err) = option_def.validate_value(&parsed_value) {
                return Err(anyhow::anyhow!(
                    "Invalid value '{}' for option '{}': {}",
                    value,
                    key,
                    err
                ));
            }

            option_values.insert(key.to_string(), parsed_value);
        } else {
            return Err(anyhow::anyhow!(
                "Unknown template option '{}'. Available options: {:?}",
                key,
                metadata.options.keys().collect::<Vec<_>>()
            ));
        }
    }

    // Add default values for unspecified options
    for (option_name, option_def) in &metadata.options {
        if !option_values.contains_key(option_name) {
            if let Some(default_value) = option_def.default_value() {
                option_values.insert(option_name.clone(), default_value);
            } else {
                return Err(anyhow::anyhow!(
                    "Missing required option '{}'. Provide a value with --option {}=<value> or define a default.",
                    option_name,
                    option_name,
                ));
            }
        }
    }

    // Log resolved options
    info!(
        "Template: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );
    for (key, value) in &option_values {
        debug!("Option '{}' = {:?}", key, value);
    }

    // Configure apply options
    let apply_options = ApplyOptions {
        options: option_values,
        overwrite: force,
        dry_run,
    };

    // Apply the template
    let result = apply_template(&template_dir, &output_dir, &apply_options)?;

    // Report results
    if dry_run {
        info!("DRY RUN: Would process {} files", result.files_processed);
    } else {
        info!("Successfully processed {} files", result.files_processed);
    }

    if result.files_skipped > 0 {
        info!(
            "Skipped {} existing files (use --force to overwrite)",
            result.files_skipped
        );
    }

    // Show actions taken/planned
    for action in &result.actions {
        match action {
            deacon_core::templates::PlannedAction::CopyFile {
                src,
                dest,
                has_substitutions,
            } => {
                let action_str = if dry_run { "Would copy" } else { "Copied" };
                let subst_str = if *has_substitutions {
                    " (with variable substitution)"
                } else {
                    ""
                };
                info!(
                    "{} {} -> {}{}",
                    action_str,
                    src.display(),
                    dest.display(),
                    subst_str
                );
            }
            deacon_core::templates::PlannedAction::SkipExistingFile { dest } => {
                info!("Skipped existing file: {}", dest.display());
            }
            deacon_core::templates::PlannedAction::OverwriteFile {
                src,
                dest,
                has_substitutions,
            } => {
                let action_str = if dry_run {
                    "Would overwrite"
                } else {
                    "Overwritten"
                };
                let subst_str = if *has_substitutions {
                    " (with variable substitution)"
                } else {
                    ""
                };
                info!(
                    "{} {} -> {}{}",
                    action_str,
                    src.display(),
                    dest.display(),
                    subst_str
                );
            }
        }
    }

    // Show substitution summary
    if !result.substitution_report.replacements.is_empty() {
        debug!(
            "Variable substitutions made: {}",
            result.substitution_report.replacements.len()
        );
        for (var, value) in &result.substitution_report.replacements {
            debug!("  ${{{}}}: {}", var, value);
        }
    }

    if !result.substitution_report.unknown_variables.is_empty() {
        warn!(
            "Unknown variables found: {:?}",
            result.substitution_report.unknown_variables
        );
    }

    info!(
        "Template application completed. Files processed: {}, skipped: {}",
        result.files_processed, result.files_skipped
    );

    Ok(())
}
