//! Templates command implementation
//!
//! Implements the `deacon templates` subcommands for metadata, publishing, and documentation
//! generation of DevContainer templates. Follows the CLI specification for template management.

use crate::cli::TemplateCommands;
use anyhow::Result;
use deacon_core::oci::{default_fetcher, TemplateRef};
use deacon_core::registry_parser::parse_registry_reference;
use deacon_core::templates::parse_template_metadata;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tempfile;
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

/// Result of a templates command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatesResult {
    /// Command that was executed
    pub command: String,
    /// Status of the operation (success/failure)
    pub status: String,
    /// Optional digest for publish operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// Optional size information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Optional message with additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional cache path for pulled artifacts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_path: Option<String>,
}

/// Execute the templates command
#[instrument(level = "debug")]
pub async fn execute_templates(args: TemplatesArgs) -> Result<()> {
    match args.command {
        TemplateCommands::Metadata { path } => execute_templates_metadata(&path).await,
        TemplateCommands::Pull { registry_ref, json } => {
            execute_templates_pull(&registry_ref, json).await
        }
        TemplateCommands::Publish {
            path,
            registry,
            dry_run,
            username,
            password_stdin,
        } => {
            execute_templates_publish(
                &path,
                &registry,
                dry_run,
                username.as_deref(),
                password_stdin,
            )
            .await
        }
        TemplateCommands::GenerateDocs { path, output } => {
            execute_templates_generate_docs(&path, &output).await
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

/// Execute templates metadata command
#[instrument(level = "debug")]
async fn execute_templates_metadata(path: &str) -> Result<()> {
    debug!("Getting template metadata for path: {}", path);

    let template_path = Path::new(path);

    // Parse template metadata from devcontainer-template.json
    let metadata_file = template_path.join("devcontainer-template.json");
    let metadata = parse_template_metadata(&metadata_file)
        .map_err(|e| anyhow::anyhow!("Failed to parse template metadata: {}", e))?;

    info!(
        "Template metadata: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    // Create JSON summary with required fields
    let summary = serde_json::json!({
        "id": metadata.id,
        "name": metadata.name,
        "options": metadata.options,
        "recommendedFeatures": metadata.recommended_features
    });

    // Print JSON summary to stdout
    println!("{}", serde_json::to_string_pretty(&summary)?);

    Ok(())
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

    let result = TemplatesResult {
        command: "pull".to_string(),
        status: "success".to_string(),
        digest: Some(downloaded_template.digest),
        size: None, // Size not available in DownloadedTemplate
        message: Some(format!(
            "Successfully pulled {} to {}",
            template_ref.reference(),
            downloaded_template.path.display()
        )),
        cache_path: Some(downloaded_template.path.to_string_lossy().into_owned()),
    };

    output_result(&result, json)?;
    Ok(())
}

/// Execute templates publish command
#[instrument(level = "debug")]
async fn execute_templates_publish(
    path: &str,
    registry: &str,
    dry_run: bool,
    username: Option<&str>,
    password_stdin: bool,
) -> Result<()> {
    debug!(
        "Publishing template at path: {} to registry: {} (dry_run: {})",
        path, registry, dry_run
    );

    // Handle authentication credentials if provided
    if let Some(_username) = username {
        // TODO: Implement credential setting in OCI client
        debug!("Username provided for authentication: {}", _username);
    }
    if password_stdin {
        // TODO: Implement reading password from stdin
        debug!("Password will be read from stdin");
    }

    let template_path = Path::new(path);

    // Parse template metadata
    let metadata_file = template_path.join("devcontainer-template.json");
    let metadata = parse_template_metadata(&metadata_file)
        .map_err(|e| anyhow::anyhow!("Failed to parse template metadata: {}", e))?;

    info!(
        "Publishing template: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    if dry_run {
        debug!("Dry run mode - would publish to registry: {}", registry);

        let result = TemplatesResult {
            command: "publish".to_string(),
            status: "success".to_string(),
            digest: Some(
                "sha256:dryrun0000000000000000000000000000000000000000000000000000000000"
                    .to_string(),
            ),
            size: Some(1024), // Mock size
            message: Some(format!("Dry run completed - would publish to {}", registry)),
            cache_path: None,
        };

        output_result(&result, true)?; // Always output as JSON for programmatic use
        return Ok(());
    }

    // Parse registry reference from the registry parameter
    // Format: [registry]/[namespace]/[name]:[tag]
    let (registry_url, namespace, name, tag) = parse_registry_reference(registry)?;

    let template_ref = TemplateRef::new(
        registry_url.clone(),
        namespace.clone(),
        name.clone(),
        tag.clone(),
    );

    // Create template package
    let temp_dir = tempfile::tempdir()?;
    let (_digest, _size) =
        create_template_package(template_path, temp_dir.path(), &metadata.id).await?;

    // Read the created tar file for publishing
    let tar_path = temp_dir.path().join(format!("{}.tar", metadata.id));
    let tar_data = std::fs::read(&tar_path)?;

    // Create OCI client and publish to registry
    let fetcher =
        default_fetcher().map_err(|e| anyhow::anyhow!("Failed to create OCI client: {}", e))?;

    debug!("Publishing to OCI registry: {}", template_ref.reference());
    let publish_result = fetcher
        .publish_template(&template_ref, tar_data.into(), &metadata)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to publish template: {}", e))?;

    let result = TemplatesResult {
        command: "publish".to_string(),
        status: "success".to_string(),
        digest: Some(publish_result.digest),
        size: Some(publish_result.size),
        message: Some(format!(
            "Successfully published {} to {}",
            template_ref.reference(),
            registry_url
        )),
        cache_path: None,
    };

    output_result(&result, true)?; // Always output as JSON for programmatic use
    Ok(())
}

/// Execute templates generate-docs command
#[instrument(level = "debug")]
async fn execute_templates_generate_docs(path: &str, output_dir: &str) -> Result<()> {
    debug!(
        "Generating docs for template at path: {} to output: {}",
        path, output_dir
    );

    let template_path = Path::new(path);
    let output_path = Path::new(output_dir);

    // Parse template metadata
    let metadata_file = template_path.join("devcontainer-template.json");
    let metadata = parse_template_metadata(&metadata_file)
        .map_err(|e| anyhow::anyhow!("Failed to parse template metadata: {}", e))?;

    info!(
        "Generating docs for template: {} ({})",
        metadata.id,
        metadata.name.as_deref().unwrap_or("No name")
    );

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_path)?;

    // Generate README fragment
    let readme_content = generate_readme_fragment(&metadata)?;
    let readme_path = output_path.join("README-template.md");
    std::fs::write(&readme_path, readme_content)?;

    info!("Generated documentation at: {}", readme_path.display());

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

/// Create a template package (tar archive with OCI manifest)
async fn create_template_package(
    template_path: &Path,
    output_path: &Path,
    template_id: &str,
) -> Result<(String, u64)> {
    use sha2::{Digest, Sha256};
    use std::fs::File;
    use std::io::Read;
    use tar::Builder;

    debug!("Creating template package for: {}", template_id);

    // Create tar archive
    let tar_filename = format!("{}.tar", template_id);
    let tar_path = output_path.join(&tar_filename);
    let tar_file = File::create(&tar_path)?;
    let mut builder = Builder::new(tar_file);

    // Add all files from template directory to tar (excluding build artifacts)
    for entry in std::fs::read_dir(template_path)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = path.file_name().unwrap().to_string_lossy();

        // Skip common build artifacts and hidden files
        if file_name.starts_with('.')
            || file_name == "node_modules"
            || file_name == "target"
            || file_name == "dist"
            || file_name == "build"
        {
            continue;
        }

        if path.is_file() {
            builder.append_path_with_name(&path, file_name.as_ref())?;
        } else if path.is_dir() {
            builder.append_dir_all(file_name.as_ref(), &path)?;
        }
    }
    builder.finish()?;

    // Calculate digest and size
    let mut file = File::open(&tar_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    hasher.update(&buffer);
    let digest = format!("sha256:{:x}", hasher.finalize());
    let size = buffer.len() as u64;

    // Create OCI manifest with template annotation
    let _manifest = serde_json::json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "annotations": {
            "org.opencontainers.image.title": template_id,
            "dev.containers.type": "template"
        },
        "config": {
            "mediaType": "application/vnd.devcontainers.template.config.v1+json",
            "size": 0,
            "digest": "sha256:placeholder"
        },
        "layers": [{
            "mediaType": "application/vnd.oci.image.layer.v1.tar",
            "size": size,
            "digest": digest.clone()
        }]
    });

    // For now, we simulate the registry push
    debug!(
        "Template package created with digest: {}, size: {} bytes",
        digest, size
    );

    Ok((digest, size))
}

/// Generate README fragment with template options and usage
fn generate_readme_fragment(metadata: &deacon_core::templates::TemplateMetadata) -> Result<String> {
    use std::fmt::Write;

    let mut content = String::new();

    // Title
    writeln!(
        &mut content,
        "# {}",
        metadata.name.as_deref().unwrap_or(&metadata.id)
    )?;

    // Description
    if let Some(description) = &metadata.description {
        writeln!(&mut content)?;
        writeln!(&mut content, "{}", description)?;
    }

    // Options section
    if !metadata.options.is_empty() {
        writeln!(&mut content)?;
        writeln!(&mut content, "## Options")?;
        writeln!(&mut content)?;

        // Create deterministic order by sorting keys
        let mut option_keys: Vec<_> = metadata.options.keys().collect();
        option_keys.sort();

        for option_name in option_keys {
            let option = &metadata.options[option_name];
            writeln!(&mut content, "### {}", option_name)?;

            match option {
                deacon_core::features::FeatureOption::Boolean {
                    description,
                    default,
                    ..
                } => {
                    if let Some(desc) = description {
                        writeln!(&mut content, "{}", desc)?;
                    }
                    writeln!(&mut content, "- Type: `boolean`")?;
                    if let Some(default_val) = default {
                        writeln!(&mut content, "- Default: `{}`", default_val)?;
                    }
                }
                deacon_core::features::FeatureOption::String {
                    description,
                    default,
                    r#enum,
                    ..
                } => {
                    if let Some(desc) = description {
                        writeln!(&mut content, "{}", desc)?;
                    }
                    writeln!(&mut content, "- Type: `string`")?;
                    if let Some(default_val) = default {
                        writeln!(&mut content, "- Default: `{}`", default_val)?;
                    }
                    if let Some(allowed_values) = r#enum {
                        writeln!(
                            &mut content,
                            "- Allowed values: {}",
                            allowed_values
                                .iter()
                                .map(|v| format!("`{}`", v))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )?;
                    }
                }
            }
            writeln!(&mut content)?;
        }
    }

    // Usage section
    writeln!(&mut content, "## Usage")?;
    writeln!(&mut content)?;
    writeln!(&mut content, "```json")?;
    writeln!(&mut content, "{{")?;
    writeln!(
        &mut content,
        "  \"image\": \"mcr.microsoft.com/devcontainers/base:ubuntu\","
    )?;
    writeln!(&mut content, "  \"features\": {{")?;
    writeln!(&mut content, "    \"{}\": {{}}", metadata.id)?;
    writeln!(&mut content, "  }}")?;
    writeln!(&mut content, "}}")?;
    writeln!(&mut content, "```")?;

    Ok(content)
}

/// Output result in JSON format
fn output_result(result: &TemplatesResult, json: bool) -> Result<()> {
    if json {
        let json_output = serde_json::to_string_pretty(result)?;
        println!("{}", json_output);
    } else {
        println!("Command: {}", result.command);
        println!("Status: {}", result.status);
        if let Some(ref digest) = result.digest {
            println!("Digest: {}", digest);
        }
        if let Some(size) = result.size {
            println!("Size: {} bytes", size);
        }
        if let Some(ref cache_path) = result.cache_path {
            println!("Cache Path: {}", cache_path);
        }
        if let Some(ref message) = result.message {
            println!("Message: {}", message);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_templates_result_json_serialization() {
        let result = TemplatesResult {
            command: "publish".to_string(),
            status: "success".to_string(),
            digest: Some("sha256:abc123".to_string()),
            size: Some(1024),
            message: Some("Published successfully".to_string()),
            cache_path: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"command\":\"publish\""));
        assert!(json.contains("\"status\":\"success\""));
        assert!(json.contains("\"digest\":\"sha256:abc123\""));
    }

    #[tokio::test]
    async fn test_create_template_package() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("test-template");

        // Create template directory with minimal files
        fs::create_dir_all(&template_dir).unwrap();
        fs::write(
            template_dir.join("devcontainer-template.json"),
            r#"{"id": "test-template", "version": "1.0.0"}"#,
        )
        .unwrap();
        fs::write(
            template_dir.join("Dockerfile"),
            "FROM ubuntu:latest\nRUN apt-get update",
        )
        .unwrap();

        let output_dir = temp_dir.path().join("output");
        fs::create_dir_all(&output_dir).unwrap();

        let (digest, size) = create_template_package(&template_dir, &output_dir, "test-template")
            .await
            .unwrap();

        assert!(digest.starts_with("sha256:"));
        assert!(size > 0);
    }

    #[test]
    fn test_generate_readme_fragment() {
        use deacon_core::features::FeatureOption;
        use deacon_core::templates::TemplateMetadata;
        use std::collections::HashMap;

        let mut options = HashMap::new();
        options.insert(
            "enableFeature".to_string(),
            FeatureOption::Boolean {
                default: Some(true),
                description: Some("Enable the feature".to_string()),
            },
        );

        let metadata = TemplateMetadata {
            id: "test-template".to_string(),
            version: Some("1.0.0".to_string()),
            name: Some("Test Template".to_string()),
            description: Some("A test template".to_string()),
            documentation_url: None,
            license_url: None,
            options,
            recommended_features: None,
            files: None,
            platforms: None,
            publisher: None,
            keywords: None,
        };

        let readme = generate_readme_fragment(&metadata).unwrap();

        assert!(readme.contains("# Test Template"));
        assert!(readme.contains("A test template"));
        assert!(readme.contains("## Options"));
        assert!(readme.contains("### enableFeature"));
        assert!(readme.contains("Type: `boolean`"));
        assert!(readme.contains("Default: `true`"));
        assert!(readme.contains("## Usage"));
        assert!(readme.contains("test-template"));
    }
}
