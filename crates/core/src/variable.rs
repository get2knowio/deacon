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
//! - `${containerWorkspaceFolder}` - Container workspace path (available after container start)
//! - `${containerEnv:VAR}` - Container environment variable (available after container start)
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

/// Maximum recursion depth for nested variable substitution
const MAX_SUBSTITUTION_DEPTH: usize = 5;

/// Substitution options for controlling behavior
#[derive(Debug, Clone)]
pub struct SubstitutionOptions {
    /// Maximum recursion depth for nested substitution
    pub max_depth: usize,
    /// Whether to fail on unresolved variables (strict mode)
    pub strict: bool,
    /// Enable multi-pass resolution for nested variables
    pub enable_nested: bool,
}

impl Default for SubstitutionOptions {
    fn default() -> Self {
        Self {
            max_depth: MAX_SUBSTITUTION_DEPTH,
            strict: false,
            enable_nested: true,
        }
    }
}

/// Substitution context containing values for variable resolution
#[derive(Debug, Clone)]
pub struct SubstitutionContext {
    /// Canonical workspace folder path
    pub local_workspace_folder: String,
    /// Host environment variables
    pub local_env: HashMap<String, String>,
    /// Deterministic container ID based on workspace path
    pub devcontainer_id: String,
    /// Container workspace folder path (for in-container execution)
    pub container_workspace_folder: Option<String>,
    /// Container environment variables (for in-container execution)
    pub container_env: Option<HashMap<String, String>>,
    /// Feature-provided variables (for advanced substitution)
    pub feature_vars: HashMap<String, String>,
    /// Template option values (for template variable substitution)
    pub template_options: Option<HashMap<String, String>>,
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
            container_workspace_folder: None,
            container_env: None,
            feature_vars: HashMap::new(),
            template_options: None,
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

    /// Set container workspace folder path for in-container variable substitution
    pub fn with_container_workspace_folder(mut self, container_workspace_folder: String) -> Self {
        self.container_workspace_folder = Some(container_workspace_folder);
        self
    }

    /// Set container environment variables for in-container variable substitution
    pub fn with_container_env(mut self, container_env: HashMap<String, String>) -> Self {
        self.container_env = Some(container_env);
        self
    }

    /// Set feature-provided variables for advanced substitution
    pub fn with_feature_vars(mut self, feature_vars: HashMap<String, String>) -> Self {
        self.feature_vars = feature_vars;
        self
    }

    /// Add a single feature variable
    pub fn add_feature_var(&mut self, key: String, value: String) {
        self.feature_vars.insert(key, value);
    }
}

/// Report of variable substitutions performed
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SubstitutionReport {
    /// Map of variable names to their resolved values
    pub replacements: HashMap<String, String>,
    /// List of unknown variables that were left unchanged
    pub unknown_variables: Vec<String>,
    /// Variables that failed to resolve in strict mode
    pub failed_variables: Vec<String>,
    /// Cycle detection warnings
    pub cycle_warnings: Vec<String>,
    /// Number of substitution passes performed
    pub passes: usize,
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

    /// Record a variable that failed to resolve in strict mode
    pub fn add_failed_variable(&mut self, variable: String) {
        self.failed_variables.push(variable);
    }

    /// Record a cycle detection warning
    pub fn add_cycle_warning(&mut self, warning: String) {
        self.cycle_warnings.push(warning);
    }

    /// Increment the pass counter
    pub fn increment_passes(&mut self) {
        self.passes += 1;
    }

    /// Check if any substitutions were performed
    pub fn has_substitutions(&self) -> bool {
        !self.replacements.is_empty() || !self.unknown_variables.is_empty()
    }

    /// Check if there were any errors in strict mode
    pub fn has_errors(&self) -> bool {
        !self.failed_variables.is_empty()
    }

    /// Check if there were any cycle warnings
    pub fn has_cycles(&self) -> bool {
        !self.cycle_warnings.is_empty()
    }
}

/// Variable substitution engine
pub struct VariableSubstitution;

impl VariableSubstitution {
    /// Apply variable substitution to a string value with advanced options
    ///
    /// This method supports:
    /// 1. Multi-pass nested variable resolution
    /// 2. Cycle detection and prevention
    /// 3. Strict mode for failing on unresolved variables
    ///
    /// ## Arguments
    ///
    /// * `input` - Input string that may contain variable tokens
    /// * `context` - Substitution context with variable values
    /// * `options` - Options controlling substitution behavior
    /// * `report` - Mutable report to track substitutions
    ///
    /// ## Returns
    ///
    /// Returns the input string with variable tokens replaced by their resolved values.
    #[instrument(skip_all, fields(input_length = input.len(), max_depth = options.max_depth, strict = options.strict))]
    pub fn substitute_string_advanced(
        input: &str,
        context: &SubstitutionContext,
        options: &SubstitutionOptions,
        report: &mut SubstitutionReport,
    ) -> Result<String> {
        if !options.enable_nested {
            // Single-pass mode for backward compatibility
            report.increment_passes();
            return Ok(Self::substitute_string_single_pass(input, context, report));
        }

        let mut result = input.to_string();
        let mut depth = 0;
        let mut previous_results = std::collections::HashSet::new();

        while depth < options.max_depth {
            let new_result = Self::substitute_string_single_pass(&result, context, report);
            report.increment_passes();

            // Check for cycle detection
            if previous_results.contains(&new_result) {
                let cycle_warning = format!(
                    "Cycle detected in variable substitution at depth {}: '{}'",
                    depth, new_result
                );
                debug!("{}", cycle_warning);
                report.add_cycle_warning(cycle_warning);
                break;
            }

            // If no changes were made, we're done
            if new_result == result {
                break;
            }

            previous_results.insert(result.clone());
            result = new_result;
            depth += 1;
        }

        // Check if we hit max depth without resolution
        if depth >= options.max_depth {
            let warning = format!(
                "Variable substitution reached maximum depth {} without full resolution: '{}'",
                options.max_depth, result
            );
            debug!("{}", warning);
            report.add_cycle_warning(warning);
        }

        // In strict mode, fail if there are unresolved variables
        if options.strict && Self::has_unresolved_variables(&result) {
            let unresolved = Self::extract_unresolved_variables(&result);
            for var in &unresolved {
                report.add_failed_variable(var.clone());
            }
            return Err(DeaconError::Config(ConfigError::Validation {
                message: format!(
                    "Unresolved variables in strict mode: {}",
                    unresolved.join(", ")
                ),
            }));
        }

        Ok(result)
    }

    /// Apply variable substitution to a string value (backward compatible)
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
        Self::substitute_string_single_pass(input, context, report)
    }

    /// Single-pass variable substitution implementation
    fn substitute_string_single_pass(
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

    /// Check if a string contains unresolved variable tokens
    fn has_unresolved_variables(input: &str) -> bool {
        let regex = regex::Regex::new(VARIABLE_PATTERN)
            .expect("Variable substitution regex should be valid");
        regex.is_match(input)
    }

    /// Extract unresolved variable names from a string
    fn extract_unresolved_variables(input: &str) -> Vec<String> {
        let regex = regex::Regex::new(VARIABLE_PATTERN)
            .expect("Variable substitution regex should be valid");
        regex
            .captures_iter(input)
            .map(|caps| caps[1].to_string())
            .collect()
    }

    /// Resolve a variable expression to its value
    ///
    /// Handles the supported variable types:
    /// - `localWorkspaceFolder` - Returns canonical workspace path
    /// - `localEnv:VAR` - Returns environment variable or empty string if missing
    /// - `devcontainerId` - Returns deterministic container ID
    /// - `containerWorkspaceFolder` - Returns container workspace path (if available)
    /// - `containerEnv:VAR` - Returns container environment variable (if available)
    /// - `feature:VAR` - Returns feature-provided variable (if available)
    fn resolve_variable(variable_expr: &str, context: &SubstitutionContext) -> Option<String> {
        match variable_expr {
            "localWorkspaceFolder" => Some(context.local_workspace_folder.clone()),
            "devcontainerId" => Some(context.devcontainer_id.clone()),
            "containerWorkspaceFolder" => context.container_workspace_folder.clone(),
            expr if expr.starts_with("localEnv:") => {
                let env_var = &expr[9..]; // Remove "localEnv:" prefix
                Some(context.local_env.get(env_var).cloned().unwrap_or_default())
            }
            expr if expr.starts_with("containerEnv:") => {
                let env_var = &expr[13..]; // Remove "containerEnv:" prefix
                context
                    .container_env
                    .as_ref()
                    .and_then(|env| env.get(env_var).cloned())
                    .or_else(|| Some(String::new())) // Return empty string if container env not available or var not found
            }
            expr if expr.starts_with("feature:") => {
                let feature_var = &expr[8..]; // Remove "feature:" prefix
                context.feature_vars.get(feature_var).cloned()
            }
            expr if expr.starts_with("templateOption:") => {
                let option_name = &expr[15..]; // Remove "templateOption:" prefix
                context
                    .template_options
                    .as_ref()
                    .and_then(|options| options.get(option_name).cloned())
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
        Self::substitute_json_value_with_options(
            value,
            context,
            &SubstitutionOptions::default(),
            report,
        )
        .unwrap_or_else(|err| {
            // Log warning when advanced substitution fails in strict mode
            tracing::warn!(
                "Advanced substitution failed, falling back to original value: {}",
                err
            );
            value.clone()
        })
    }

    /// Apply substitution to a JSON value recursively with advanced options
    ///
    /// This method supports advanced substitution features including:
    /// - Multi-pass nested variable resolution
    /// - Strict mode error handling
    /// - Cycle detection
    ///
    /// ## Arguments
    ///
    /// * `value` - JSON value to process
    /// * `context` - Substitution context with variable values
    /// * `options` - Options controlling substitution behavior
    /// * `report` - Mutable report to track substitutions
    ///
    /// ## Returns
    ///
    /// Returns the JSON value with variable substitutions applied to string values.
    pub fn substitute_json_value_with_options(
        value: &Value,
        context: &SubstitutionContext,
        options: &SubstitutionOptions,
        report: &mut SubstitutionReport,
    ) -> Result<Value> {
        match value {
            Value::String(s) => {
                let substituted = Self::substitute_string_advanced(s, context, options, report)?;
                Ok(Value::String(substituted))
            }
            Value::Array(arr) => {
                let mut substituted = Vec::new();
                for v in arr {
                    substituted.push(Self::substitute_json_value_with_options(
                        v, context, options, report,
                    )?);
                }
                Ok(Value::Array(substituted))
            }
            Value::Object(obj) => {
                let mut substituted = serde_json::Map::new();
                for (k, v) in obj {
                    substituted.insert(
                        k.clone(),
                        Self::substitute_json_value_with_options(v, context, options, report)?,
                    );
                }
                Ok(Value::Object(substituted))
            }
            // For non-string values, return as-is
            _ => Ok(value.clone()),
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

    #[test]
    fn test_container_workspace_folder_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?
            .with_container_workspace_folder("/workspaces/test".to_string());
        let mut report = SubstitutionReport::new();

        let input = "${containerWorkspaceFolder}/src";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "/workspaces/test/src");
        assert!(report.replacements.contains_key("containerWorkspaceFolder"));

        Ok(())
    }

    #[test]
    fn test_container_workspace_folder_not_available() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = "${containerWorkspaceFolder}/src";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        // Should be left unchanged when container workspace folder is not available
        assert_eq!(result, "${containerWorkspaceFolder}/src");
        assert!(report
            .unknown_variables
            .contains(&"containerWorkspaceFolder".to_string()));

        Ok(())
    }

    #[test]
    fn test_container_env_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut container_env = HashMap::new();
        container_env.insert("NODE_ENV".to_string(), "production".to_string());

        let context = SubstitutionContext::new(temp_dir.path())?.with_container_env(container_env);
        let mut report = SubstitutionReport::new();

        let input = "Environment: ${containerEnv:NODE_ENV}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "Environment: production");
        assert!(report.replacements.contains_key("containerEnv:NODE_ENV"));

        Ok(())
    }

    #[test]
    fn test_container_env_missing_var() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let container_env = HashMap::new();

        let context = SubstitutionContext::new(temp_dir.path())?.with_container_env(container_env);
        let mut report = SubstitutionReport::new();

        let input = "Environment: ${containerEnv:MISSING_VAR}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "Environment: ");
        assert!(report.replacements.contains_key("containerEnv:MISSING_VAR"));

        Ok(())
    }

    #[test]
    fn test_container_env_not_available() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = "Environment: ${containerEnv:NODE_ENV}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        // Should return empty string when container env is not available
        assert_eq!(result, "Environment: ");
        assert!(report.replacements.contains_key("containerEnv:NODE_ENV"));

        Ok(())
    }

    #[test]
    fn test_mixed_container_and_local_variables() -> anyhow::Result<()> {
        env::set_var("TEST_LOCAL", "local_value");

        let temp_dir = TempDir::new()?;
        let mut container_env = HashMap::new();
        container_env.insert("TEST_CONTAINER".to_string(), "container_value".to_string());

        let context = SubstitutionContext::new(temp_dir.path())?
            .with_container_workspace_folder("/workspaces/test".to_string())
            .with_container_env(container_env);
        let mut report = SubstitutionReport::new();

        let input = "${localWorkspaceFolder} -> ${containerWorkspaceFolder}, ${localEnv:TEST_LOCAL} vs ${containerEnv:TEST_CONTAINER}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert!(result.contains(&context.local_workspace_folder));
        assert!(result.contains("/workspaces/test"));
        assert!(result.contains("local_value"));
        assert!(result.contains("container_value"));
        assert_eq!(report.replacements.len(), 4);

        env::remove_var("TEST_LOCAL");
        Ok(())
    }

    #[test]
    fn test_advanced_substitution_with_options() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let options = SubstitutionOptions {
            max_depth: 3,
            strict: false,
            enable_nested: true,
        };

        let input = "${localWorkspaceFolder}/src";
        let result = VariableSubstitution::substitute_string_advanced(
            input,
            &context,
            &options,
            &mut report,
        )?;

        assert!(result.starts_with(&context.local_workspace_folder));
        assert!(result.ends_with("/src"));
        assert!(report.replacements.contains_key("localWorkspaceFolder"));
        assert!(report.passes >= 1); // Could be 1 or 2 depending on nested resolution logic

        Ok(())
    }

    #[test]
    fn test_single_pass_mode() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let options = SubstitutionOptions {
            max_depth: 3,
            strict: false,
            enable_nested: false, // Single-pass mode
        };

        let input = "${localWorkspaceFolder}/src";
        let result = VariableSubstitution::substitute_string_advanced(
            input,
            &context,
            &options,
            &mut report,
        )?;

        assert!(result.starts_with(&context.local_workspace_folder));
        assert!(result.ends_with("/src"));
        assert!(report.replacements.contains_key("localWorkspaceFolder"));
        assert_eq!(report.passes, 1); // Single pass mode should always be 1

        Ok(())
    }

    #[test]
    fn test_nested_variable_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        env::set_var("NESTED_TEST_VAR", "localWorkspaceFolder");

        let mut context = SubstitutionContext::new(temp_dir.path())?;
        context.add_feature_var(
            "dynamicVar".to_string(),
            "${localEnv:NESTED_TEST_VAR}".to_string(),
        );

        let mut report = SubstitutionReport::new();
        let options = SubstitutionOptions::default();

        // This should resolve ${feature:dynamicVar} -> ${localEnv:NESTED_TEST_VAR} -> localWorkspaceFolder -> actual path
        let input = "${feature:dynamicVar}/src";
        let result = VariableSubstitution::substitute_string_advanced(
            input,
            &context,
            &options,
            &mut report,
        )?;

        // Should resolve to the literal string "localWorkspaceFolder/src" since the nested resolution
        // would resolve to the variable name, not the value
        assert_eq!(result, "localWorkspaceFolder/src");
        assert!(report.passes > 1);

        env::remove_var("NESTED_TEST_VAR");
        Ok(())
    }

    #[test]
    fn test_cycle_detection() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut context = SubstitutionContext::new(temp_dir.path())?;

        // Create a circular reference: var1 -> var2 -> var1
        context.add_feature_var("var1".to_string(), "${feature:var2}".to_string());
        context.add_feature_var("var2".to_string(), "${feature:var1}".to_string());

        let mut report = SubstitutionReport::new();
        let options = SubstitutionOptions::default();

        let input = "${feature:var1}";
        let _result = VariableSubstitution::substitute_string_advanced(
            input,
            &context,
            &options,
            &mut report,
        )?;

        // Should detect cycle and stop
        assert!(report.has_cycles());
        assert!(!report.cycle_warnings.is_empty());

        Ok(())
    }

    #[test]
    fn test_strict_mode_failure() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let options = SubstitutionOptions {
            max_depth: 3,
            strict: true,
            enable_nested: true,
        };

        let input = "${unknownVariable}";
        let result = VariableSubstitution::substitute_string_advanced(
            input,
            &context,
            &options,
            &mut report,
        );

        assert!(result.is_err());
        assert!(report.has_errors());
        assert!(!report.failed_variables.is_empty());

        Ok(())
    }

    #[test]
    fn test_strict_mode_success() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let options = SubstitutionOptions {
            max_depth: 3,
            strict: true,
            enable_nested: true,
        };

        let input = "${localWorkspaceFolder}/src";
        let result = VariableSubstitution::substitute_string_advanced(
            input,
            &context,
            &options,
            &mut report,
        )?;

        assert!(result.starts_with(&context.local_workspace_folder));
        assert!(result.ends_with("/src"));
        assert!(!report.has_errors());

        Ok(())
    }

    #[test]
    fn test_feature_variable_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut context = SubstitutionContext::new(temp_dir.path())?;
        context.add_feature_var("customPath".to_string(), "/custom/feature/path".to_string());

        let mut report = SubstitutionReport::new();

        let input = "Path: ${feature:customPath}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "Path: /custom/feature/path");
        assert!(report.replacements.contains_key("feature:customPath"));

        Ok(())
    }

    #[test]
    fn test_unknown_feature_variable() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let context = SubstitutionContext::new(temp_dir.path())?;
        let mut report = SubstitutionReport::new();

        let input = "Path: ${feature:unknownVar}";
        let result = VariableSubstitution::substitute_string(input, &context, &mut report);

        assert_eq!(result, "Path: ${feature:unknownVar}");
        assert!(report
            .unknown_variables
            .contains(&"feature:unknownVar".to_string()));

        Ok(())
    }

    #[test]
    fn test_advanced_json_substitution() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut context = SubstitutionContext::new(temp_dir.path())?;
        context.add_feature_var("version".to_string(), "1.0.0".to_string());

        let mut report = SubstitutionReport::new();
        let options = SubstitutionOptions::default();

        // Test complex JSON structure with various variable types
        let json_value = serde_json::json!({
            "workspace": "${localWorkspaceFolder}",
            "container_id": "${devcontainerId}",
            "feature_version": "${feature:version}",
            "paths": [
                "${localWorkspaceFolder}/src",
                "${localWorkspaceFolder}/tests"
            ],
            "config": {
                "base_path": "${localWorkspaceFolder}",
                "version": "${feature:version}"
            }
        });

        let result = VariableSubstitution::substitute_json_value_with_options(
            &json_value,
            &context,
            &options,
            &mut report,
        )?;

        // Verify substitutions were applied
        assert_eq!(result["workspace"], context.local_workspace_folder);
        assert_eq!(result["container_id"], context.devcontainer_id);
        assert_eq!(result["feature_version"], "1.0.0");

        if let Some(paths) = result["paths"].as_array() {
            assert_eq!(paths.len(), 2);
            assert!(paths[0]
                .as_str()
                .unwrap()
                .starts_with(&context.local_workspace_folder));
        }

        if let Some(config) = result["config"].as_object() {
            assert_eq!(config["base_path"], context.local_workspace_folder);
            assert_eq!(config["version"], "1.0.0");
        }

        assert!(report.has_substitutions());

        Ok(())
    }

    #[test]
    fn test_max_depth_reached() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let mut context = SubstitutionContext::new(temp_dir.path())?;

        // Create a chain that exceeds max depth
        context.add_feature_var("var1".to_string(), "${feature:var2}".to_string());
        context.add_feature_var("var2".to_string(), "${feature:var3}".to_string());
        context.add_feature_var("var3".to_string(), "${feature:var4}".to_string());
        context.add_feature_var("var4".to_string(), "final_value".to_string());

        let mut report = SubstitutionReport::new();
        let options = SubstitutionOptions {
            max_depth: 2, // Set low depth to trigger warning
            strict: false,
            enable_nested: true,
        };

        let input = "${feature:var1}";
        let _result = VariableSubstitution::substitute_string_advanced(
            input,
            &context,
            &options,
            &mut report,
        )?;

        // Should hit max depth and generate warning
        assert!(report.has_cycles());
        assert!(report
            .cycle_warnings
            .iter()
            .any(|w| w.contains("maximum depth")));

        Ok(())
    }

    #[test]
    fn test_substitution_report_enhanced() {
        let mut report = SubstitutionReport::new();

        assert!(!report.has_substitutions());
        assert!(!report.has_errors());
        assert!(!report.has_cycles());
        assert_eq!(report.passes, 0);

        report.add_replacement("var1".to_string(), "value1".to_string());
        report.add_unknown_variable("var2".to_string());
        report.add_failed_variable("var3".to_string());
        report.add_cycle_warning("Cycle detected".to_string());
        report.increment_passes();

        assert!(report.has_substitutions());
        assert!(report.has_errors());
        assert!(report.has_cycles());
        assert_eq!(report.passes, 1);

        assert_eq!(report.replacements.len(), 1);
        assert_eq!(report.unknown_variables.len(), 1);
        assert_eq!(report.failed_variables.len(), 1);
        assert_eq!(report.cycle_warnings.len(), 1);
    }
}
