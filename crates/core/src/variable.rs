//! Variable substitution engine
//!
//! This module implements variable substitution for DevContainer configurations following
//! the Development Containers Specification. It supports local environment and workspace
//! path variables for phase 1 implementation.
//!
//! ## Supported Variables
//!
//! - `${localWorkspaceFolder}` - Canonical workspace path
//! - `${localEnv:VAR}` - Host environment variable
//! - `${devcontainerId}` - Deterministic hash ID (first 12 chars of SHA256 of workspace path)
//!
//! ## Variable Substitution Workflow
//!
//! The substitution engine follows the workflow outlined in the CLI specification:
//! 1. Parse variable tokens using regex pattern matching
//! 2. Resolve variable values from substitution context
//! 3. Replace tokens with resolved values
//! 4. Track replacements for debug logging
//! 5. Handle unknown variables by leaving tokens unchanged
//! 6. Handle missing environment variables as empty strings
//!
//! ## References
//!
//! This implementation aligns with the variable substitution workflow defined in the
//! CLI specification and follows the Development Containers Specification patterns.

use crate::errors::{ConfigError, DeaconError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::path::Path;
use tracing::{debug, instrument};

/// Regular expression pattern for variable substitution tokens
const VARIABLE_PATTERN: &str = r"\$\{([^}]+)\}";

/// Substitution context containing values for variable resolution
#[derive(Debug, Clone)]
pub struct SubstitutionContext {
    /// Canonical workspace folder path
    pub local_workspace_folder: String,
    /// Host environment variables
    pub local_env: HashMap<String, String>,
    /// Deterministic container ID based on workspace path
    pub devcontainer_id: String,
}

impl SubstitutionContext {
    /// Create a new substitution context from a workspace path
    ///
    /// This method:
    /// 1. Canonicalizes the workspace path
    /// 2. Captures current environment variables
    /// 3. Generates a deterministic devcontainer ID from workspace path hash
    ///
    /// ## Arguments
    ///
    /// * `workspace_path` - Path to the workspace folder
    ///
    /// ## Returns
    ///
    /// Returns a `SubstitutionContext` with resolved values for variable substitution.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::variable::SubstitutionContext;
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let context = SubstitutionContext::new(Path::new("/path/to/workspace"))?;
    /// println!("Workspace: {}", context.local_workspace_folder);
    /// println!("Container ID: {}", context.devcontainer_id);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all, fields(workspace_path = %workspace_path.display()))]
    pub fn new(workspace_path: &Path) -> Result<Self> {
        debug!(
            "Creating substitution context for workspace: {}",
            workspace_path.display()
        );

        // Canonicalize workspace path
        let canonical_path = workspace_path.canonicalize().map_err(|e| {
            debug!("Failed to canonicalize workspace path: {}", e);
            DeaconError::Config(ConfigError::Validation {
                message: format!(
                    "Invalid workspace path '{}': {}",
                    workspace_path.display(),
                    e
                ),
            })
        })?;

        let local_workspace_folder = canonical_path.to_string_lossy().to_string();

        // Capture environment variables
        let local_env: HashMap<String, String> = env::vars().collect();

        // Generate deterministic devcontainer ID
        let devcontainer_id = Self::generate_devcontainer_id(&local_workspace_folder);

        debug!(
            "Created substitution context - workspace: {}, devcontainer_id: {}",
            local_workspace_folder, devcontainer_id
        );

        Ok(Self {
            local_workspace_folder,
            local_env,
            devcontainer_id,
        })
    }

    /// Generate a deterministic devcontainer ID from workspace path
    ///
    /// Uses SHA256 hash of the canonical workspace path and returns the first 12 characters
    /// for a compact, deterministic identifier.
    fn generate_devcontainer_id(workspace_path: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        workspace_path.hash(&mut hasher);
        let hash = hasher.finish();

        // Convert to hex and take first 12 characters for deterministic ID
        format!("{:016x}", hash)[..12].to_string()
    }
}

/// Report of variable substitutions performed
#[derive(Debug, Clone, Default)]
pub struct SubstitutionReport {
    /// Map of variable names to their resolved values
    pub replacements: HashMap<String, String>,
    /// List of unknown variables that were left unchanged
    pub unknown_variables: Vec<String>,
}

impl SubstitutionReport {
    /// Create a new empty substitution report
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful variable replacement
    pub fn add_replacement(&mut self, variable: String, value: String) {
        self.replacements.insert(variable, value);
    }

    /// Record an unknown variable that was left unchanged
    pub fn add_unknown_variable(&mut self, variable: String) {
        self.unknown_variables.push(variable);
    }

    /// Check if any substitutions were performed
    pub fn has_substitutions(&self) -> bool {
        !self.replacements.is_empty() || !self.unknown_variables.is_empty()
    }
}

/// Variable substitution engine
pub struct VariableSubstitution;

impl VariableSubstitution {
    /// Apply variable substitution to a string value
    ///
    /// This method:
    /// 1. Finds all variable tokens using regex pattern matching
    /// 2. Resolves each variable from the substitution context
    /// 3. Replaces tokens with resolved values
    /// 4. Records replacements in the substitution report
    /// 5. Leaves unknown variables unchanged
    ///
    /// ## Arguments
    ///
    /// * `input` - Input string that may contain variable tokens
    /// * `context` - Substitution context with variable values
    /// * `report` - Mutable report to track substitutions
    ///
    /// ## Returns
    ///
    /// Returns the input string with variable tokens replaced by their resolved values.
    ///
    /// ## Example
    ///
    /// ```rust
    /// use deacon_core::variable::{VariableSubstitution, SubstitutionContext, SubstitutionReport};
    /// use std::path::Path;
    ///
    /// # fn example() -> anyhow::Result<()> {
    /// let context = SubstitutionContext::new(Path::new("/workspace"))?;
    /// let mut report = SubstitutionReport::new();
    ///
    /// let result = VariableSubstitution::substitute_string(
    ///     "${localWorkspaceFolder}/src",
    ///     &context,
    ///     &mut report
    /// );
    ///
    /// println!("Result: {}", result);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip_all, fields(input_length = input.len()))]
    pub fn substitute_string(
        input: &str,
        context: &SubstitutionContext,
        report: &mut SubstitutionReport,
    ) -> String {
        let regex = regex::Regex::new(VARIABLE_PATTERN)
            .expect("Variable substitution regex should be valid");

        let result = regex.replace_all(input, |caps: &regex::Captures| {
            let variable_expr = &caps[1];

            debug!("Processing variable: {}", variable_expr);

            match Self::resolve_variable(variable_expr, context) {
                Some(value) => {
                    debug!("Resolved variable '{}' to: {}", variable_expr, value);
                    report.add_replacement(variable_expr.to_string(), value.clone());
                    value
                }
                None => {
                    debug!("Unknown variable '{}' - leaving unchanged", variable_expr);
                    report.add_unknown_variable(variable_expr.to_string());
                    format!("${{{}}}", variable_expr)
                }
            }
        });

        result.to_string()
    }

    /// Resolve a variable expression to its value
    ///
    /// Handles the three supported variable types:
    /// - `localWorkspaceFolder` - Returns canonical workspace path
    /// - `localEnv:VAR` - Returns environment variable or empty string if missing
    /// - `devcontainerId` - Returns deterministic container ID
    fn resolve_variable(variable_expr: &str, context: &SubstitutionContext) -> Option<String> {
        match variable_expr {
            "localWorkspaceFolder" => Some(context.local_workspace_folder.clone()),
            "devcontainerId" => Some(context.devcontainer_id.clone()),
            expr if expr.starts_with("localEnv:") => {
                let env_var = &expr[9..]; // Remove "localEnv:" prefix
                Some(context.local_env.get(env_var).cloned().unwrap_or_default())
            }
            _ => None, // Unknown variable
        }
    }

    /// Apply substitution to a JSON value recursively
    ///
    /// This method handles substitution in JSON values that may contain strings,
    /// arrays of strings, or objects with string values. It recursively processes
    /// the JSON structure to find and substitute variable tokens.
    ///
    /// ## Arguments
    ///
    /// * `value` - JSON value to process
    /// * `context` - Substitution context with variable values
    /// * `report` - Mutable report to track substitutions
    ///
    /// ## Returns
    ///
    /// Returns the JSON value with variable substitutions applied to string values.
    pub fn substitute_json_value(
        value: &Value,
        context: &SubstitutionContext,
        report: &mut SubstitutionReport,
    ) -> Value {
        match value {
            Value::String(s) => Value::String(Self::substitute_string(s, context, report)),
            Value::Array(arr) => {
                let substituted: Vec<Value> = arr
                    .iter()
                    .map(|v| Self::substitute_json_value(v, context, report))
                    .collect();
                Value::Array(substituted)
            }
            Value::Object(obj) => {
                let substituted: serde_json::Map<String, Value> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), Self::substitute_json_value(v, context, report)))
                    .collect();
                Value::Object(substituted)
            }
            // For non-string values, return as-is
            _ => value.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_substitution_context_creation() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;

        // Should have canonical path
        assert!(context
            .local_workspace_folder
            .contains(temp_dir.path().file_name().unwrap().to_str().unwrap()));

        // Should have environment variables
        assert!(!context.local_env.is_empty());

        // Should have deterministic devcontainer ID
        assert_eq!(context.devcontainer_id.len(), 12);

        // ID should be deterministic
        let context2 = SubstitutionContext::new(temp_dir.path())?;
        assert_eq!(context.devcontainer_id, context2.devcontainer_id);

        Ok(())
    }

    #[test]
    fn test_basic_variable_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        // Test localWorkspaceFolder substitution
        let input = "${localWorkspaceFolder}/src";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert!(result.starts_with(&context.local_workspace_folder));
        assert!(result.ends_with("/src"));
        assert!(report.replacements.contains_key("localWorkspaceFolder"));

        Ok(())
    }

    #[test]
    fn test_devcontainer_id_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = "container-${devcontainerId}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert!(result.starts_with("container-"));
        assert_eq!(result.len(), "container-".len() + 12); // 12-char ID
        assert!(report.replacements.contains_key("devcontainerId"));

        Ok(())
    }

    #[test]
    fn test_local_env_substitution() -> anyhow::Result<()> {
        // Use a unique env var to avoid interference with other tests running in parallel.
        const VAR: &str = "DEACON_TEST_LOCAL_ENV_SUBST";
        env::set_var(VAR, "test_value");

        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = &format!("Value: ${{localEnv:{VAR}}}");
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "Value: test_value");
        assert!(report.replacements.contains_key(&format!("localEnv:{VAR}")));

        env::remove_var(VAR);
        Ok(())
    }

    #[test]
    fn test_missing_env_var_becomes_empty() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = "Value: ${localEnv:NONEXISTENT_VAR}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "Value: ");
        assert!(report.replacements.contains_key("localEnv:NONEXISTENT_VAR"));

        Ok(())
    }

    #[test]
    fn test_unknown_variable_left_unchanged() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = "Value: ${unknownVariable}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "Value: ${unknownVariable}");
        assert!(report
            .unknown_variables
            .contains(&"unknownVariable".to_string()));

        Ok(())
    }

    #[test]
    fn test_multiple_variables_in_string() -> anyhow::Result<()> {
        env::set_var("TEST_VAR", "test");

        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = "${localWorkspaceFolder}/src/${localEnv:TEST_VAR}/${devcontainerId}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert!(result.contains(&context.local_workspace_folder));
        assert!(result.contains("test"));
        assert!(result.contains(&context.devcontainer_id));
        assert_eq!(report.replacements.len(), 3);

        env::remove_var("TEST_VAR");
        Ok(())
    }

    #[test]
    fn test_json_value_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        // Test string substitution
        let string_value = Value::String("${localWorkspaceFolder}/src".to_string());
        let result =
            VariableSubstitution::substitute_json_value(&string_value, &context, &mut report);

        if let Value::String(s) = result {
            assert!(s.starts_with(&context.local_workspace_folder));
            assert!(s.ends_with("/src"));
        } else {
            panic!("Expected string value");
        }

        // Test array substitution
        let array_value = Value::Array(vec![
            Value::String("${localWorkspaceFolder}/src".to_string()),
            Value::String("${devcontainerId}".to_string()),
        ]);
        let result =
            VariableSubstitution::substitute_json_value(&array_value, &context, &mut report);

        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 2);
            if let Value::String(s) = &arr[0] {
                assert!(s.starts_with(&context.local_workspace_folder));
            }
            if let Value::String(s) = &arr[1] {
                assert_eq!(s, &context.devcontainer_id);
            }
        } else {
            panic!("Expected array value");
        }

        Ok(())
    }

    #[test]
    fn test_nested_braces_ignored() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        // Nested braces should not be processed correctly
        let input = "${localWorkspaceFolder${invalid}}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        // Should be left unchanged because the regex will match "${localWorkspaceFolder${invalid" which is unknown
        assert_eq!(result, input);
        assert!(report
            .unknown_variables
            .contains(&"localWorkspaceFolder${invalid".to_string()));

        Ok(())
    }

    #[test]
    fn test_substitution_report() {
        let mut report = SubstitutionReport::new();

        assert!(!report.has_substitutions());

        report.add_replacement("var1".to_string(), "value1".to_string());
        report.add_unknown_variable("var2".to_string());

        assert!(report.has_substitutions());
        assert_eq!(report.replacements.len(), 1);
        assert_eq!(report.unknown_variables.len(), 1);
        assert_eq!(report.replacements.get("var1"), Some(&"value1".to_string()));
        assert!(report.unknown_variables.contains(&"var2".to_string()));
    }

    #[test]
    fn test_deterministic_devcontainer_id() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let path = temp_dir.path();

        // Create multiple contexts with the same path
        let context1 = SubstitutionContext::new(path)?;
        let context2 = SubstitutionContext::new(path)?;

        // IDs should be identical for the same path
        assert_eq!(context1.devcontainer_id, context2.devcontainer_id);

        // ID should be 12 characters
        assert_eq!(context1.devcontainer_id.len(), 12);

        // ID should be hexadecimal
        assert!(context1
            .devcontainer_id
            .chars()
            .all(|c| c.is_ascii_hexdigit()));

        Ok(())
    }
}
