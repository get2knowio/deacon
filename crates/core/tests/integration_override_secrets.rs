//! Integration test for override configuration and secrets functionality
//!
//! This test validates that:
//! - Override configuration takes precedence over base config
//! - Secrets are parsed correctly from files and available for variable substitution
//! - Variable substitution uses secrets as ${localEnv:SECRET_KEY}

use anyhow::Result;
use deacon_core::config::ConfigLoader;
use deacon_core::secrets::SecretsCollection;
use std::fs;
use tempfile::TempDir;

#[tokio::test]
async fn test_override_config_and_secrets_integration() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Create base configuration
    let base_config_path = temp_dir.path().join("devcontainer.json");
    let base_config_content = r#"{
        "name": "base-container",
        "image": "mcr.microsoft.com/devcontainers/base:ubuntu",
        "containerEnv": {
            "BASE_VAR": "base-value",
            "DB_PASSWORD": "${localEnv:DB_PASSWORD}",
            "API_URL": "${localEnv:API_URL}"
        }
    }"#;
    fs::write(&base_config_path, base_config_content)?;

    // Create override configuration
    let override_config_path = temp_dir.path().join("override.json");
    let override_config_content = r#"{
        "name": "override-container",
        "containerEnv": {
            "OVERRIDE_VAR": "override-value",
            "SECRET_TOKEN": "${localEnv:SECRET_TOKEN}"
        }
    }"#;
    fs::write(&override_config_path, override_config_content)?;

    // Create secrets file
    let secrets_path = temp_dir.path().join("secrets.env");
    let secrets_content = r#"
# Database credentials
DB_PASSWORD=super-secret-password

# API configuration  
API_URL=https://api.example.com/v1
SECRET_TOKEN=abc123xyz789

# This should be ignored
UNUSED_SECRET=ignored-value
"#;
    fs::write(&secrets_path, secrets_content)?;

    // Load secrets
    let secrets = SecretsCollection::load_from_files(&[&secrets_path])?;
    assert_eq!(secrets.len(), 4); // Including UNUSED_SECRET

    // Load configuration with overrides and secrets
    let (config, substitution_report) = ConfigLoader::load_with_overrides_and_substitution(
        &base_config_path,
        Some(&override_config_path),
        Some(&secrets),
        temp_dir.path(),
    )?;

    // Verify override config took precedence
    assert_eq!(config.name, Some("override-container".to_string()));

    // Verify base config fields are preserved
    assert_eq!(
        config.image,
        Some("mcr.microsoft.com/devcontainers/base:ubuntu".to_string())
    );

    // Verify environment variables are merged and substituted
    assert!(config.container_env.contains_key("BASE_VAR"));
    assert!(config.container_env.contains_key("OVERRIDE_VAR"));
    assert_eq!(
        config.container_env.get("BASE_VAR"),
        Some(&"base-value".to_string())
    );
    assert_eq!(
        config.container_env.get("OVERRIDE_VAR"),
        Some(&"override-value".to_string())
    );

    // Verify secrets were substituted
    assert_eq!(
        config.container_env.get("DB_PASSWORD"),
        Some(&"super-secret-password".to_string())
    );
    assert_eq!(
        config.container_env.get("API_URL"),
        Some(&"https://api.example.com/v1".to_string())
    );
    assert_eq!(
        config.container_env.get("SECRET_TOKEN"),
        Some(&"abc123xyz789".to_string())
    );

    // Verify substitution report shows the secret substitutions
    assert!(substitution_report
        .replacements
        .contains_key("localEnv:DB_PASSWORD"));
    assert!(substitution_report
        .replacements
        .contains_key("localEnv:API_URL"));
    assert!(substitution_report
        .replacements
        .contains_key("localEnv:SECRET_TOKEN"));

    // Verify redaction works
    let registry = secrets.redaction_registry();
    let redacted_log = registry.redact_text("Connecting with password super-secret-password");
    assert!(redacted_log.contains("****"));
    assert!(!redacted_log.contains("super-secret-password"));

    Ok(())
}

#[tokio::test]
async fn test_multiple_secrets_files_later_wins() -> Result<()> {
    let temp_dir = TempDir::new()?;

    let secrets1_path = temp_dir.path().join("secrets1.env");
    fs::write(&secrets1_path, "KEY1=value1\nKEY2=value2\n")?;

    let secrets2_path = temp_dir.path().join("secrets2.env");
    fs::write(&secrets2_path, "KEY2=new_value2\nKEY3=value3\n")?;

    let collection = SecretsCollection::load_from_files(&[&secrets1_path, &secrets2_path])?;

    assert_eq!(collection.get("KEY1"), Some(&"value1".to_string()));
    assert_eq!(collection.get("KEY2"), Some(&"new_value2".to_string())); // Later wins
    assert_eq!(collection.get("KEY3"), Some(&"value3".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_missing_secrets_file_graceful() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let missing_path = temp_dir.path().join("missing.env");

    // Should not error, just log warning and continue
    let collection = SecretsCollection::load_from_files(&[&missing_path])?;
    assert!(collection.is_empty());

    Ok(())
}
