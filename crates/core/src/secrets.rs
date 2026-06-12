//! Secrets file parsing and management
//!
//! This module parses secrets files (JSON or `.env`), supports multiple files
//! with conflict resolution (later wins), and integrates with the redaction
//! system.
//!
//! ## File Format
//!
//! Two formats are accepted (auto-detected by a leading `{`):
//!
//! - **JSON** — a flat object `{"KEY": "value", ...}`, the format used by the
//!   reference CLI and the containers.dev spec. Non-string values are coerced to
//!   their string form.
//! - **`.env`** (`KEY=VALUE`, one per line) — deacon's original format, retained
//!   for backwards compatibility. Empty lines and `#` comments are ignored;
//!   values are taken literally (no quoting) and may contain spaces.
//!
//! ## Example
//!
//! ```text
//! # Database credentials
//! DB_PASSWORD=my-secret-password
//!
//! # API tokens
//! API_KEY=abc123xyz
//! GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz123456
//! ```

use crate::errors::{ConfigError, DeaconError, Result};
use crate::redaction::SecretRegistry;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{debug, instrument, warn};

/// A collection of secrets loaded from files
#[derive(Debug, Clone)]
pub struct SecretsCollection {
    /// Map of secret key to value
    secrets: HashMap<String, String>,
    /// Registry for redaction purposes
    redaction_registry: SecretRegistry,
}

impl SecretsCollection {
    /// Create a new empty secrets collection
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
            redaction_registry: SecretRegistry::new(),
        }
    }

    /// Load secrets from multiple files
    ///
    /// Files are processed in order, with later files taking precedence
    /// over earlier files for conflicting keys.
    ///
    /// ## Arguments
    ///
    /// * `file_paths` - Paths to secrets files
    ///
    /// ## Returns
    ///
    /// Returns a `SecretsCollection` with all secrets loaded and registered
    /// for redaction.
    #[instrument(skip_all)]
    pub fn load_from_files<P: AsRef<Path>>(file_paths: &[P]) -> Result<Self> {
        let mut collection = Self::new();

        for file_path in file_paths {
            let path = file_path.as_ref();
            debug!("Loading secrets from: {}", path.display());

            if !path.exists() {
                warn!("Secrets file not found: {}", path.display());
                continue;
            }

            let file_secrets = Self::parse_secrets_file(path)?;
            debug!(
                "Loaded {} secrets from {}",
                file_secrets.len(),
                path.display()
            );

            // Merge secrets (later files win on conflicts)
            for (key, value) in file_secrets {
                collection.secrets.insert(key, value);
            }
        }

        // Register all secret values for redaction
        for value in collection.secrets.values() {
            collection.redaction_registry.add_secret(value);
        }

        debug!(
            "Loaded {} total secrets from {} files",
            collection.secrets.len(),
            file_paths.len()
        );

        Ok(collection)
    }

    /// Parse a single secrets file
    ///
    /// ## Arguments
    ///
    /// * `file_path` - Path to the secrets file
    ///
    /// ## Returns
    ///
    /// Returns a HashMap of key-value pairs from the file.
    #[instrument(skip_all, fields(file = %file_path.display()))]
    fn parse_secrets_file(file_path: &Path) -> Result<HashMap<String, String>> {
        let content =
            fs::read_to_string(file_path).map_err(|e| DeaconError::Config(ConfigError::Io(e)))?;

        // Accept BOTH formats: the reference CLI / spec use a flat JSON object
        // (`{"KEY":"value"}`); deacon historically used `.env` (`KEY=VALUE`). A
        // leading `{` (after whitespace) selects JSON, otherwise fall back to
        // KEY=VALUE. The two are syntactically disjoint, so detection is
        // unambiguous and JSON support is purely additive (no `.env` regression).
        if content.trim_start().starts_with('{') {
            return Self::parse_secrets_json(file_path, &content);
        }

        let mut secrets = HashMap::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse KEY=VALUE format
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();

                if key.is_empty() {
                    warn!(
                        "Empty key found at {}:{}",
                        file_path.display(),
                        line_num + 1
                    );
                    continue;
                }

                debug!("Parsed secret key: {}", key);
                secrets.insert(key.to_string(), value.to_string());
            } else {
                warn!(
                    "Invalid format at {}:{} (expected KEY=VALUE): {}",
                    file_path.display(),
                    line_num + 1,
                    line
                );
            }
        }

        Ok(secrets)
    }

    /// Parse a flat JSON object secrets file (`{"KEY":"value", ...}`), matching
    /// the reference CLI / containers.dev spec. Non-string values are coerced to
    /// their string form (number/bool → literal, object/array → compact JSON,
    /// null → empty) so a value type doesn't hard-fail injection.
    fn parse_secrets_json(file_path: &Path, content: &str) -> Result<HashMap<String, String>> {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(content)
            .map_err(|e| {
                DeaconError::Config(ConfigError::Validation {
                    message: format!("Invalid JSON secrets file {}: {}", file_path.display(), e),
                })
            })?;

        let mut secrets = HashMap::new();
        for (key, value) in map {
            if key.is_empty() {
                warn!(
                    "Empty key found in JSON secrets file {}",
                    file_path.display()
                );
                continue;
            }
            let value = match value {
                serde_json::Value::String(s) => s,
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            debug!("Parsed secret key: {}", key);
            secrets.insert(key, value);
        }

        Ok(secrets)
    }

    /// Get all secrets as environment variables
    ///
    /// Returns the secrets as a HashMap suitable for merging with
    /// environment variables for variable substitution.
    pub fn as_env_vars(&self) -> &HashMap<String, String> {
        &self.secrets
    }

    /// Get a specific secret value
    pub fn get(&self, key: &str) -> Option<&String> {
        self.secrets.get(key)
    }

    /// Get the redaction registry
    pub fn redaction_registry(&self) -> &SecretRegistry {
        &self.redaction_registry
    }

    /// Check if the collection is empty
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Get the number of secrets
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// Get all secret keys (for logging purposes, values are redacted)
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.secrets.keys()
    }
}

impl Default for SecretsCollection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_empty_file() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.env");
        fs::write(&secrets_file, "").unwrap();

        let secrets = SecretsCollection::parse_secrets_file(&secrets_file)?;
        assert!(secrets.is_empty());
        Ok(())
    }

    #[test]
    fn test_parse_json_secrets_file() -> Result<()> {
        // Reference / spec format: a flat JSON object. Non-string values are
        // coerced to their string form.
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.json");
        fs::write(
            &secrets_file,
            r#"{ "MY_SECRET": "topsecret", "NUM": 42, "FLAG": true }"#,
        )
        .unwrap();

        let secrets = SecretsCollection::parse_secrets_file(&secrets_file)?;
        assert_eq!(secrets.len(), 3);
        assert_eq!(secrets.get("MY_SECRET"), Some(&"topsecret".to_string()));
        assert_eq!(secrets.get("NUM"), Some(&"42".to_string()));
        assert_eq!(secrets.get("FLAG"), Some(&"true".to_string()));
        Ok(())
    }

    #[test]
    fn test_json_detected_by_leading_brace_with_whitespace() -> Result<()> {
        // Leading whitespace before `{` still selects JSON (not KEY=VALUE).
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.txt");
        fs::write(&secrets_file, "\n  { \"K\": \"v\" }\n").unwrap();
        let secrets = SecretsCollection::parse_secrets_file(&secrets_file)?;
        assert_eq!(secrets.get("K"), Some(&"v".to_string()));
        Ok(())
    }

    #[test]
    fn test_invalid_json_secrets_file_errors() {
        // A `{`-leading file that isn't valid JSON fails fast (no silent
        // fallback to KEY=VALUE parsing).
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.json");
        fs::write(&secrets_file, r#"{ "MY_SECRET": "x",, }"#).unwrap();
        let result = SecretsCollection::parse_secrets_file(&secrets_file);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("Invalid JSON secrets file"),
            "expected JSON parse error, got: {msg}"
        );
    }

    #[test]
    fn test_parse_comments_and_empty_lines() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.env");
        let content = r#"
# This is a comment
# Another comment

KEY1=value1

# More comments
KEY2=value2
"#;
        fs::write(&secrets_file, content).unwrap();

        let secrets = SecretsCollection::parse_secrets_file(&secrets_file)?;
        assert_eq!(secrets.len(), 2);
        assert_eq!(secrets.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(secrets.get("KEY2"), Some(&"value2".to_string()));
        Ok(())
    }

    #[test]
    fn test_parse_values_with_spaces() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.env");
        let content = r#"
DB_PASSWORD=my secret password
API_URL=https://api.example.com/v1
"#;
        fs::write(&secrets_file, content).unwrap();

        let secrets = SecretsCollection::parse_secrets_file(&secrets_file)?;
        assert_eq!(
            secrets.get("DB_PASSWORD"),
            Some(&"my secret password".to_string())
        );
        assert_eq!(
            secrets.get("API_URL"),
            Some(&"https://api.example.com/v1".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_load_multiple_files_later_wins() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();

        let file1 = temp_dir.path().join("secrets1.env");
        fs::write(&file1, "KEY1=value1\nKEY2=value2\n").unwrap();

        let file2 = temp_dir.path().join("secrets2.env");
        fs::write(&file2, "KEY2=new_value2\nKEY3=value3\n").unwrap();

        let collection = SecretsCollection::load_from_files(&[file1, file2])?;

        assert_eq!(collection.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(collection.get("KEY2"), Some(&"new_value2".to_string())); // Later wins
        assert_eq!(collection.get("KEY3"), Some(&"value3".to_string()));
        assert_eq!(collection.len(), 3);
        Ok(())
    }

    #[test]
    fn test_missing_file_warning() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let missing_file = temp_dir.path().join("missing.env");

        let collection = SecretsCollection::load_from_files(&[missing_file])?;
        assert!(collection.is_empty());
        Ok(())
    }

    #[test]
    fn test_invalid_format_warning() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.env");
        let content = r#"
KEY1=value1
INVALID_LINE_WITHOUT_EQUALS
KEY2=value2
=empty_key
KEY3=value3
"#;
        fs::write(&secrets_file, content).unwrap();

        let secrets = SecretsCollection::parse_secrets_file(&secrets_file)?;
        // Should only get valid KEY=VALUE pairs, empty key line is ignored
        assert_eq!(secrets.len(), 3);
        assert_eq!(secrets.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(secrets.get("KEY2"), Some(&"value2".to_string()));
        assert_eq!(secrets.get("KEY3"), Some(&"value3".to_string()));
        Ok(())
    }

    #[test]
    fn test_redaction_registry_populated() -> Result<()> {
        let temp_dir = TempDir::new().unwrap();
        let secrets_file = temp_dir.path().join("secrets.env");
        fs::write(&secrets_file, "SECRET_KEY=very-secret-value\n").unwrap();

        let collection = SecretsCollection::load_from_files(&[secrets_file])?;

        // Redaction registry should have the secret value
        let registry = collection.redaction_registry();
        let redacted = registry.redact_text("Found very-secret-value in logs");
        assert!(redacted.contains("****"));
        assert!(!redacted.contains("very-secret-value"));
        Ok(())
    }
}
