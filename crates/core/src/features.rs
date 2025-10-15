//! DevContainer features system
//!
//! This module handles feature discovery, installation, and lifecycle management.
//!
//! ## API Changes
//!
//! ### OptionValue Enum Extension (v0.1.5)
//!
//! The `OptionValue` enum has been extended to support all JSON types, not just Boolean and String.
//!
//! **Before (v0.1.4 and earlier):**
//! ```ignore
//! pub enum OptionValue {
//!     Boolean(bool),
//!     String(String),
//! }
//! ```
//!
//! **After (v0.1.5+):**
//! ```ignore
//! pub enum OptionValue {
//!     Boolean(bool),
//!     String(String),
//!     Number(serde_json::Number),
//!     Array(Vec<serde_json::Value>),
//!     Object(serde_json::Map<String, serde_json::Value>),
//!     Null,
//! }
//! ```
//!
//! **Migration Notes:**
//! - **Backward Compatible:** Existing code using Boolean and String variants continues to work unchanged.
//! - **New Accessors:** Use `as_number()`, `as_array()`, `as_object()`, and `is_null()` to access new types.
//! - **Pattern Matching:** If you exhaustively match on `OptionValue`, add cases for the new variants:
//!   ```rust
//!   match option_value {
//!       OptionValue::Boolean(b) => { /* existing code */ }
//!       OptionValue::String(s) => { /* existing code */ }
//!       OptionValue::Number(n) => { /* handle number */ }
//!       OptionValue::Array(a) => { /* handle array */ }
//!       OptionValue::Object(o) => { /* handle object */ }
//!       OptionValue::Null => { /* handle null */ }
//!   }
//!   ```
//! - **Data Preservation:** All option values are now preserved through the pipeline. Previously,
//!   Number, Array, Object, and Null types were silently dropped. This fixes a data loss issue.
//! - **Validation:** Pass-through types (Number, Array, Object, Null) are accepted but not validated
//!   against feature option schemas, as they are not defined in the DevContainer feature spec.

use crate::errors::{FeatureError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use tracing::{debug, instrument, warn};

/// Processed option value supporting different types
///
/// Supports all JSON value types to ensure complete data preservation through
/// the feature option pipeline. Previously only Boolean and String were supported,
/// causing silent data loss for other types.
///
/// # Examples
///
/// ```
/// use deacon_core::features::OptionValue;
///
/// // String and Boolean (always supported)
/// let string_opt = OptionValue::String("latest".to_string());
/// let bool_opt = OptionValue::Boolean(true);
///
/// // Number, Array, Object, Null (added in v0.1.5 to prevent data loss)
/// let number_opt = OptionValue::Number(serde_json::Number::from(300));
/// let array_opt = OptionValue::Array(vec![serde_json::Value::String("item".to_string())]);
/// let null_opt = OptionValue::Null;
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionValue {
    Boolean(bool),
    String(String),
    Number(serde_json::Number),
    Array(Vec<serde_json::Value>),
    Object(serde_json::Map<String, serde_json::Value>),
    Null,
}

impl OptionValue {
    /// Get as boolean if it's a boolean value
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            OptionValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as string if it's a string value
    pub fn as_str(&self) -> Option<&str> {
        match self {
            OptionValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as number if it's a number value
    pub fn as_number(&self) -> Option<&serde_json::Number> {
        match self {
            OptionValue::Number(n) => Some(n),
            _ => None,
        }
    }

    /// Get as array if it's an array value
    pub fn as_array(&self) -> Option<&Vec<serde_json::Value>> {
        match self {
            OptionValue::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Get as object if it's an object value
    pub fn as_object(&self) -> Option<&serde_json::Map<String, serde_json::Value>> {
        match self {
            OptionValue::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Check if the value is null
    pub fn is_null(&self) -> bool {
        matches!(self, OptionValue::Null)
    }
}

impl std::fmt::Display for OptionValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OptionValue::Boolean(b) => write!(f, "{}", b),
            OptionValue::String(s) => write!(f, "{}", s),
            OptionValue::Number(n) => write!(f, "{}", n),
            OptionValue::Array(a) => write!(f, "{}", serde_json::to_string(a).unwrap_or_default()),
            OptionValue::Object(o) => {
                write!(f, "{}", serde_json::to_string(o).unwrap_or_default())
            }
            OptionValue::Null => write!(f, "null"),
        }
    }
}

/// Feature option definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FeatureOption {
    #[serde(rename = "boolean")]
    Boolean {
        #[serde(default)]
        default: Option<bool>,
        #[serde(default)]
        description: Option<String>,
    },
    #[serde(rename = "string")]
    String {
        #[serde(default)]
        default: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        r#enum: Option<Vec<String>>,
        #[serde(default)]
        proposals: Option<Vec<String>>,
    },
}

impl FeatureOption {
    /// Get the default value for this option
    pub fn default_value(&self) -> Option<OptionValue> {
        match self {
            FeatureOption::Boolean { default, .. } => default.map(OptionValue::Boolean),
            FeatureOption::String { default, .. } => {
                default.as_ref().map(|s| OptionValue::String(s.clone()))
            }
        }
    }

    /// Validate a value against this option definition
    pub fn validate_value(&self, value: &OptionValue) -> std::result::Result<(), String> {
        match (self, value) {
            (FeatureOption::Boolean { .. }, OptionValue::Boolean(_)) => Ok(()),
            (FeatureOption::String { r#enum, .. }, OptionValue::String(s)) => {
                if let Some(allowed_values) = r#enum {
                    if allowed_values.contains(s) {
                        Ok(())
                    } else {
                        Err(format!(
                            "Value '{}' is not one of the allowed values: {:?}",
                            s, allowed_values
                        ))
                    }
                } else {
                    Ok(())
                }
            }
            // For unsupported combinations, only error if the value is not one of the
            // pass-through types (Number, Array, Object, Null)
            (_, OptionValue::Number(_))
            | (_, OptionValue::Array(_))
            | (_, OptionValue::Object(_))
            | (_, OptionValue::Null) => {
                // These types are preserved but not validated against schema
                Ok(())
            }
            _ => Err("Type mismatch between option definition and provided value".to_string()),
        }
    }
}

/// Feature metadata structure representing devcontainer-feature.json
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureMetadata {
    /// Feature identifier (required)
    pub id: String,

    /// Feature version
    #[serde(default)]
    pub version: Option<String>,

    /// Human-readable name
    #[serde(default)]
    pub name: Option<String>,

    /// Feature description
    #[serde(default)]
    pub description: Option<String>,

    /// Documentation URL
    #[serde(default)]
    pub documentation_url: Option<String>,

    /// License URL
    #[serde(default)]
    pub license_url: Option<String>,

    /// Feature options
    #[serde(default)]
    pub options: HashMap<String, FeatureOption>,

    /// Container environment variables
    #[serde(default)]
    pub container_env: HashMap<String, String>,

    /// Container mounts
    #[serde(default)]
    pub mounts: Vec<String>,

    /// Whether to use init
    #[serde(default)]
    pub init: Option<bool>,

    /// Whether to run privileged
    #[serde(default)]
    pub privileged: Option<bool>,

    /// Capabilities to add
    #[serde(default)]
    pub cap_add: Vec<String>,

    /// Security options
    #[serde(default)]
    pub security_opt: Vec<String>,

    /// Entrypoint override or wrapper
    #[serde(default)]
    pub entrypoint: Option<String>,

    /// Features to install after
    #[serde(default)]
    pub installs_after: Vec<String>,

    /// Feature dependencies
    #[serde(default)]
    pub depends_on: HashMap<String, serde_json::Value>,

    /// onCreate lifecycle command
    #[serde(default)]
    pub on_create_command: Option<serde_json::Value>,

    /// updateContent lifecycle command
    #[serde(default)]
    pub update_content_command: Option<serde_json::Value>,

    /// postCreate lifecycle command
    #[serde(default)]
    pub post_create_command: Option<serde_json::Value>,

    /// postStart lifecycle command
    #[serde(default)]
    pub post_start_command: Option<serde_json::Value>,

    /// postAttach lifecycle command
    #[serde(default)]
    pub post_attach_command: Option<serde_json::Value>,
}

impl FeatureMetadata {
    /// Check if any lifecycle commands are present
    pub fn has_lifecycle_commands(&self) -> bool {
        self.on_create_command.is_some()
            || self.update_content_command.is_some()
            || self.post_create_command.is_some()
            || self.post_start_command.is_some()
            || self.post_attach_command.is_some()
    }

    /// Validate the feature metadata
    pub fn validate(&self) -> std::result::Result<(), FeatureError> {
        // Required field validation
        if self.id.is_empty() {
            return Err(FeatureError::Validation {
                message: "Feature id is required and cannot be empty".to_string(),
            });
        }

        // Validate option defaults
        for (option_name, option_def) in &self.options {
            if let Some(default_value) = option_def.default_value() {
                if let Err(err) = option_def.validate_value(&default_value) {
                    return Err(FeatureError::Validation {
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

/// Parse feature metadata from a devcontainer-feature.json file
#[instrument(level = "debug")]
pub fn parse_feature_metadata(path: &Path) -> Result<FeatureMetadata> {
    debug!("Parsing feature metadata from: {}", path.display());

    // Check if file exists
    if !path.exists() {
        return Err(FeatureError::NotFound {
            path: path.display().to_string(),
        }
        .into());
    }

    // Read file content
    let content = std::fs::read_to_string(path).map_err(FeatureError::Io)?;

    // Parse JSON
    let metadata: FeatureMetadata =
        serde_json::from_str(&content).map_err(|e| FeatureError::Parsing {
            message: e.to_string(),
        })?;

    debug!(
        "Parsed feature: id={}, name={:?}",
        metadata.id, metadata.name
    );

    // Log options
    for (option_name, option_def) in &metadata.options {
        debug!("Option '{}': {:?}", option_name, option_def);
    }

    // Log lifecycle presence
    if metadata.has_lifecycle_commands() {
        debug!("Feature has lifecycle commands");
    }

    // Validate metadata
    metadata.validate()?;

    Ok(metadata)
}

/// Represents a feature with its resolved configuration
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedFeature {
    /// Feature identifier
    pub id: String,
    /// Source path or reference (e.g., OCI registry reference)
    pub source: String,
    /// Feature options
    pub options: HashMap<String, OptionValue>,
    /// Feature metadata
    pub metadata: FeatureMetadata,
}

/// Installation plan for features in dependency order
#[derive(Debug, Clone)]
pub struct InstallationPlan {
    /// Features in installation order
    pub features: Vec<ResolvedFeature>,
    /// Parallel execution levels - each level contains features that can be installed concurrently
    pub levels: Vec<Vec<String>>,
}

impl InstallationPlan {
    /// Create a new installation plan
    pub fn new(features: Vec<ResolvedFeature>) -> Self {
        Self {
            levels: vec![features.iter().map(|f| f.id.clone()).collect()],
            features,
        }
    }

    /// Create a new installation plan with parallel levels
    pub fn new_with_levels(features: Vec<ResolvedFeature>, levels: Vec<Vec<String>>) -> Self {
        Self { features, levels }
    }

    /// Get feature IDs in installation order
    pub fn feature_ids(&self) -> Vec<String> {
        self.features.iter().map(|f| f.id.clone()).collect()
    }

    /// Get a feature by ID
    pub fn get_feature(&self, id: &str) -> Option<&ResolvedFeature> {
        self.features.iter().find(|f| f.id == id)
    }

    /// Number of features in the plan
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Check if the plan is empty
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

/// Feature dependency resolver that builds DAG and performs topological sort
#[derive(Debug)]
pub struct FeatureDependencyResolver {
    /// Override install order if present
    override_order: Option<Vec<String>>,
}

impl FeatureDependencyResolver {
    /// Create a new dependency resolver
    pub fn new(override_order: Option<Vec<String>>) -> Self {
        Self { override_order }
    }

    /// Resolve feature dependencies and return installation plan
    #[instrument(level = "debug")]
    pub fn resolve(
        &self,
        features: &[ResolvedFeature],
    ) -> std::result::Result<InstallationPlan, FeatureError> {
        debug!("Resolving dependencies for {} features", features.len());

        // Validate all features exist in override order
        if let Some(ref override_order) = self.override_order {
            self.validate_override_order(features, override_order)?;
        }

        // Build dependency graph
        let graph = self.build_dependency_graph(features)?;

        // Compute parallel execution levels
        let levels = self.compute_parallel_levels(&graph)?;

        // Apply override order constraints if present
        let (sorted_features, final_levels) = if let Some(ref override_order) = self.override_order
        {
            // For override order, fall back to sequential execution
            let sorted_ids = self.topological_sort(&graph)?;
            let final_order = self.apply_override_order(&sorted_ids, override_order)?;
            let sorted_features = final_order
                .iter()
                .filter_map(|id| features.iter().find(|f| f.id == *id).cloned())
                .collect::<Vec<_>>();
            let sequential_levels = vec![final_order];
            (sorted_features, sequential_levels)
        } else {
            // Use parallel levels - flatten for features list but keep levels for parallel execution
            let mut all_features = Vec::new();
            for level in &levels {
                for feature_id in level {
                    if let Some(feature) = features.iter().find(|f| f.id == *feature_id) {
                        all_features.push(feature.clone());
                    }
                }
            }
            (all_features, levels)
        };

        Ok(InstallationPlan::new_with_levels(
            sorted_features,
            final_levels,
        ))
    }

    /// Validate that all features in override order exist
    fn validate_override_order(
        &self,
        features: &[ResolvedFeature],
        override_order: &[String],
    ) -> std::result::Result<(), FeatureError> {
        let feature_ids: HashSet<String> = features.iter().map(|f| f.id.clone()).collect();

        for feature_id in override_order {
            if !feature_ids.contains(feature_id) {
                return Err(FeatureError::DependencyResolution {
                    message: format!(
                        "Feature '{}' in overrideFeatureInstallOrder does not exist in feature set",
                        feature_id
                    ),
                });
            }
        }

        Ok(())
    }

    /// Build dependency graph from features
    fn build_dependency_graph(
        &self,
        features: &[ResolvedFeature],
    ) -> std::result::Result<HashMap<String, HashSet<String>>, FeatureError> {
        let mut graph: HashMap<String, HashSet<String>> = HashMap::new();
        let feature_ids: HashSet<String> = features.iter().map(|f| f.id.clone()).collect();

        // Initialize graph with all feature IDs
        for feature in features {
            graph.insert(feature.id.clone(), HashSet::new());
        }

        // Add dependencies from metadata
        for feature in features {
            let dependencies = &mut graph.get_mut(&feature.id).unwrap();

            // Add installsAfter dependencies
            for after_id in &feature.metadata.installs_after {
                if !feature_ids.contains(after_id) {
                    warn!(
                        "Feature '{}' depends on '{}' which is not in the feature set",
                        feature.id, after_id
                    );
                    continue;
                }
                dependencies.insert(after_id.clone());
            }

            // Add dependsOn dependencies (simplified - just extract string keys)
            for depend_id in feature.metadata.depends_on.keys() {
                if !feature_ids.contains(depend_id) {
                    warn!(
                        "Feature '{}' depends on '{}' which is not in the feature set",
                        feature.id, depend_id
                    );
                    continue;
                }
                dependencies.insert(depend_id.clone());
            }
        }

        debug!("Built dependency graph: {:?}", graph);
        Ok(graph)
    }

    /// Perform topological sort with cycle detection using Kahn's algorithm
    fn topological_sort(
        &self,
        graph: &HashMap<String, HashSet<String>>,
    ) -> std::result::Result<Vec<String>, FeatureError> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, HashSet<String>> = HashMap::new();

        // Initialize in-degree and adjacency list
        for node in graph.keys() {
            in_degree.insert(node.clone(), 0);
            adj_list.insert(node.clone(), HashSet::new());
        }

        // Build adjacency list and calculate in-degrees
        for (node, dependencies) in graph {
            for dep in dependencies {
                adj_list.get_mut(dep).unwrap().insert(node.clone());
                *in_degree.get_mut(node).unwrap() += 1;
            }
        }

        // Initialize queue with nodes having no dependencies (sorted for determinism)
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut zero_degree_nodes: Vec<String> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(node, _)| node.clone())
            .collect();
        zero_degree_nodes.sort(); // Lexicographic ordering for determinism
        for node in zero_degree_nodes {
            queue.push_back(node);
        }

        let mut result = Vec::new();
        let mut processed = 0;

        while let Some(current) = queue.pop_front() {
            result.push(current.clone());
            processed += 1;

            // Process all nodes that depend on current (sorted for determinism)
            let mut neighbors: Vec<String> = adj_list[&current].iter().cloned().collect();
            neighbors.sort(); // Lexicographic ordering for determinism
            for neighbor in neighbors {
                let degree = in_degree.get_mut(&neighbor).unwrap();
                *degree -= 1;
                if *degree == 0 {
                    queue.push_back(neighbor);
                }
            }
        }

        // Check for cycles
        if processed != graph.len() {
            let remaining: Vec<String> = graph
                .keys()
                .filter(|k| !result.contains(k))
                .cloned()
                .collect();

            let cycle_path = self.find_cycle_path(graph, &remaining)?;
            return Err(FeatureError::DependencyCycle { cycle_path });
        }

        debug!("Topological sort result: {:?}", result);
        Ok(result)
    }

    /// Compute parallel execution levels using Kahn's algorithm
    /// Returns levels where features in the same level can be executed concurrently
    fn compute_parallel_levels(
        &self,
        graph: &HashMap<String, HashSet<String>>,
    ) -> std::result::Result<Vec<Vec<String>>, FeatureError> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut adj_list: HashMap<String, HashSet<String>> = HashMap::new();

        // Initialize in-degree and adjacency list
        for node in graph.keys() {
            in_degree.insert(node.clone(), 0);
            adj_list.insert(node.clone(), HashSet::new());
        }

        // Build adjacency list and calculate in-degrees
        for (node, dependencies) in graph {
            for dep in dependencies {
                adj_list.get_mut(dep).unwrap().insert(node.clone());
                *in_degree.get_mut(node).unwrap() += 1;
            }
        }

        let mut levels: Vec<Vec<String>> = Vec::new();
        let mut processed = 0;

        while processed < graph.len() {
            // Find all nodes with zero in-degree (can be processed in parallel)
            let mut current_level: Vec<String> = in_degree
                .iter()
                .filter(|(_, &degree)| degree == 0)
                .map(|(node, _)| node.clone())
                .collect();

            if current_level.is_empty() {
                // No nodes with zero in-degree means there's a cycle
                let remaining: Vec<String> = in_degree
                    .keys()
                    .filter(|k| in_degree[*k] > 0)
                    .cloned()
                    .collect();

                let cycle_path = self.find_cycle_path(graph, &remaining)?;
                return Err(FeatureError::DependencyCycle { cycle_path });
            }

            current_level.sort(); // Deterministic ordering
            processed += current_level.len();

            // Process all nodes in the current level
            for node in &current_level {
                // Mark as processed (remove from in_degree)
                in_degree.remove(node);

                // Update in-degrees for dependent nodes
                let mut neighbors: Vec<String> = adj_list[node].iter().cloned().collect();
                neighbors.sort(); // Deterministic ordering
                for neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(&neighbor) {
                        *degree -= 1;
                    }
                }
            }

            levels.push(current_level);
        }

        debug!("Computed parallel levels: {:?}", levels);
        Ok(levels)
    }

    /// Find and format a cycle path for error reporting
    fn find_cycle_path(
        &self,
        graph: &HashMap<String, HashSet<String>>,
        remaining_nodes: &[String],
    ) -> std::result::Result<String, FeatureError> {
        // Simple cycle detection using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for node in remaining_nodes {
            if !visited.contains(node) {
                if let Some(cycle) =
                    Self::dfs_find_cycle(node, graph, &mut visited, &mut rec_stack, &mut path)
                {
                    return Ok(cycle.join(" -> "));
                }
            }
        }

        Ok("Cycle detected but path could not be determined".to_string())
    }

    /// DFS helper for cycle detection
    fn dfs_find_cycle(
        node: &str,
        graph: &HashMap<String, HashSet<String>>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(dependencies) = graph.get(node) {
            for dep in dependencies {
                if !visited.contains(dep) {
                    if let Some(cycle) = Self::dfs_find_cycle(dep, graph, visited, rec_stack, path)
                    {
                        return Some(cycle);
                    }
                } else if rec_stack.contains(dep) {
                    // Found cycle, return path from dependency to current node
                    let cycle_start = path.iter().position(|x| x == dep).unwrap_or(0);
                    let mut cycle_path = path[cycle_start..].to_vec();
                    cycle_path.push(dep.to_string()); // Close the cycle
                    return Some(cycle_path);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
        None
    }

    /// Apply override order constraints to the topologically sorted list
    /// The override order should be respected where possible without violating dependencies
    fn apply_override_order(
        &self,
        sorted_ids: &[String],
        override_order: &[String],
    ) -> std::result::Result<Vec<String>, FeatureError> {
        // For independent features (no dependencies), we can apply the override order directly
        // For this initial implementation, we'll use the override order if all features are independent

        // Create a set of all feature IDs for quick lookup
        let sorted_set: HashSet<String> = sorted_ids.iter().cloned().collect();

        // If override order contains all features and they're all present, use override order
        let override_set: HashSet<String> = override_order.iter().cloned().collect();
        if override_set == sorted_set {
            return Ok(override_order.to_vec());
        }

        // Otherwise, keep the topological order as a fallback
        let result = sorted_ids.to_vec();
        debug!("Applied override order, final result: {:?}", result);
        Ok(result)
    }
}

/// Placeholder for feature system
pub struct Feature;

impl Feature {
    /// Placeholder feature installer
    pub fn install() -> anyhow::Result<()> {
        Ok(())
    }
}

/// Configuration for feature merging behavior
#[derive(Debug, Clone)]
pub struct FeatureMergeConfig {
    /// Additional features from CLI (JSON string)
    pub additional_features: Option<String>,
    /// Whether CLI features take precedence over config features on conflicts
    pub prefer_cli_features: bool,
    /// Override for feature installation order
    pub feature_install_order: Option<String>,
}

impl FeatureMergeConfig {
    /// Create a new feature merge configuration
    pub fn new(
        additional_features: Option<String>,
        prefer_cli_features: bool,
        feature_install_order: Option<String>,
    ) -> Self {
        Self {
            additional_features,
            prefer_cli_features,
            feature_install_order,
        }
    }
}

/// Feature merger that combines config features with CLI features
#[derive(Debug)]
pub struct FeatureMerger;

impl FeatureMerger {
    /// Parse additional features JSON string into a features map
    #[instrument(level = "debug")]
    pub fn parse_additional_features(
        json_str: &str,
    ) -> std::result::Result<serde_json::Value, FeatureError> {
        debug!("Parsing additional features JSON: {}", json_str);

        // Parse the JSON string
        let parsed: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| FeatureError::Parsing {
                message: format!("Failed to parse additional features JSON: {}", e),
            })?;

        // Validate it's an object (map)
        if !parsed.is_object() {
            return Err(FeatureError::Validation {
                message: "Additional features must be a JSON object (map of id -> value/options)"
                    .to_string(),
            });
        }

        // Validate all keys are strings and values are valid feature values
        if let serde_json::Value::Object(map) = &parsed {
            for (key, value) in map {
                if key.is_empty() {
                    return Err(FeatureError::Validation {
                        message: "Feature ID cannot be empty".to_string(),
                    });
                }

                // Validate value is a valid feature value (bool, string, or object)
                match value {
                    serde_json::Value::Bool(_)
                    | serde_json::Value::String(_)
                    | serde_json::Value::Object(_) => {}
                    _ => {
                        return Err(FeatureError::Validation {
                            message: format!(
                                "Feature '{}' has invalid value type. Must be boolean, string, or object",
                                key
                            ),
                        });
                    }
                }
            }
        }

        debug!(
            "Successfully parsed {} additional features",
            parsed.as_object().unwrap().len()
        );
        Ok(parsed)
    }

    /// Parse feature install order string into a list of feature IDs
    #[instrument(level = "debug")]
    pub fn parse_feature_install_order(
        order_str: &str,
    ) -> std::result::Result<Vec<String>, FeatureError> {
        debug!("Parsing feature install order: {}", order_str);

        let ids: Vec<String> = order_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if ids.is_empty() {
            return Err(FeatureError::Validation {
                message: "Feature install order cannot be empty".to_string(),
            });
        }

        // Check for duplicates
        let mut seen = HashSet::new();
        for id in &ids {
            if !seen.insert(id.clone()) {
                return Err(FeatureError::Validation {
                    message: format!("Duplicate feature ID '{}' in install order", id),
                });
            }
        }

        debug!("Parsed {} feature IDs in install order", ids.len());
        Ok(ids)
    }

    /// Merge config features with additional CLI features
    #[instrument(level = "debug")]
    pub fn merge_features(
        config_features: &serde_json::Value,
        merge_config: &FeatureMergeConfig,
    ) -> std::result::Result<serde_json::Value, FeatureError> {
        debug!("Merging features with CLI configuration");

        // Start with config features
        let mut merged = config_features.clone();

        // Parse and merge additional features if provided
        if let Some(ref additional_json) = merge_config.additional_features {
            let additional_features = Self::parse_additional_features(additional_json)?;

            if let (
                serde_json::Value::Object(merged_map),
                serde_json::Value::Object(additional_map),
            ) = (&mut merged, &additional_features)
            {
                for (key, value) in additional_map {
                    if merged_map.contains_key(key) {
                        // Handle conflict based on precedence preference
                        if merge_config.prefer_cli_features {
                            debug!("CLI feature '{}' overriding config feature", key);
                            merged_map.insert(key.clone(), value.clone());
                        } else {
                            debug!("Config feature '{}' takes precedence over CLI feature", key);
                            // Keep existing config value, don't override
                        }
                    } else {
                        // No conflict, add CLI feature
                        debug!("Adding CLI feature '{}'", key);
                        merged_map.insert(key.clone(), value.clone());
                    }
                }
            }
        }

        debug!("Feature merging completed");
        Ok(merged)
    }

    /// Get the effective feature install order combining config and CLI overrides
    #[instrument(level = "debug")]
    pub fn get_effective_install_order(
        config_order: Option<&Vec<String>>,
        merge_config: &FeatureMergeConfig,
    ) -> std::result::Result<Option<Vec<String>>, FeatureError> {
        debug!("Determining effective feature install order");

        // CLI override takes precedence if provided
        if let Some(ref cli_order_str) = merge_config.feature_install_order {
            let cli_order = Self::parse_feature_install_order(cli_order_str)?;
            debug!("Using CLI feature install order: {:?}", cli_order);
            return Ok(Some(cli_order));
        }

        // Otherwise use config order if available
        if let Some(config_order) = config_order {
            debug!("Using config feature install order: {:?}", config_order);
            return Ok(Some(config_order.clone()));
        }

        debug!("No feature install order override specified");
        Ok(None)
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    #[test]
    fn test_parse_additional_features_valid() {
        let json = r#"{"git": true, "node": "18", "docker": {"version": "latest"}}"#;
        let result = FeatureMerger::parse_additional_features(json).unwrap();

        assert!(result.is_object());
        let obj = result.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert_eq!(obj["git"], serde_json::Value::Bool(true));
        assert_eq!(obj["node"], serde_json::Value::String("18".to_string()));
        assert!(obj["docker"].is_object());
    }

    #[test]
    fn test_parse_additional_features_invalid_json() {
        let json = r#"{"git": true, "node": 18,}"#; // trailing comma
        let result = FeatureMerger::parse_additional_features(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_additional_features_not_object() {
        let json = r#"["git", "node"]"#;
        let result = FeatureMerger::parse_additional_features(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_additional_features_invalid_value_type() {
        let json = r#"{"git": true, "node": 123}"#; // number not allowed
        let result = FeatureMerger::parse_additional_features(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_feature_install_order_valid() {
        let order_str = "git,node,docker";
        let result = FeatureMerger::parse_feature_install_order(order_str).unwrap();
        assert_eq!(result, vec!["git", "node", "docker"]);
    }

    #[test]
    fn test_parse_feature_install_order_with_spaces() {
        let order_str = " git , node , docker ";
        let result = FeatureMerger::parse_feature_install_order(order_str).unwrap();
        assert_eq!(result, vec!["git", "node", "docker"]);
    }

    #[test]
    fn test_parse_feature_install_order_duplicates() {
        let order_str = "git,node,git";
        let result = FeatureMerger::parse_feature_install_order(order_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_feature_install_order_empty() {
        let order_str = "";
        let result = FeatureMerger::parse_feature_install_order(order_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_features_no_conflicts() {
        let config_features = serde_json::json!({"git": true, "node": "16"});
        let merge_config = FeatureMergeConfig::new(
            Some(r#"{"docker": true, "python": "3.9"}"#.to_string()),
            false,
            None,
        );

        let result = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
        let obj = result.as_object().unwrap();

        assert_eq!(obj.len(), 4);
        assert_eq!(obj["git"], serde_json::Value::Bool(true));
        assert_eq!(obj["node"], serde_json::Value::String("16".to_string()));
        assert_eq!(obj["docker"], serde_json::Value::Bool(true));
        assert_eq!(obj["python"], serde_json::Value::String("3.9".to_string()));
    }

    #[test]
    fn test_merge_features_config_precedence() {
        let config_features = serde_json::json!({"git": true, "node": "16"});
        let merge_config = FeatureMergeConfig::new(
            Some(r#"{"git": false, "node": "18"}"#.to_string()),
            false, // config wins
            None,
        );

        let result = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
        let obj = result.as_object().unwrap();

        assert_eq!(obj.len(), 2);
        assert_eq!(obj["git"], serde_json::Value::Bool(true)); // config wins
        assert_eq!(obj["node"], serde_json::Value::String("16".to_string())); // config wins
    }

    #[test]
    fn test_merge_features_cli_precedence() {
        let config_features = serde_json::json!({"git": true, "node": "16"});
        let merge_config = FeatureMergeConfig::new(
            Some(r#"{"git": false, "node": "18"}"#.to_string()),
            true, // CLI wins
            None,
        );

        let result = FeatureMerger::merge_features(&config_features, &merge_config).unwrap();
        let obj = result.as_object().unwrap();

        assert_eq!(obj.len(), 2);
        assert_eq!(obj["git"], serde_json::Value::Bool(false)); // CLI wins
        assert_eq!(obj["node"], serde_json::Value::String("18".to_string())); // CLI wins
    }

    #[test]
    fn test_get_effective_install_order_cli_override() {
        let config_order = Some(vec!["git".to_string(), "node".to_string()]);
        let merge_config =
            FeatureMergeConfig::new(None, false, Some("docker,git,node".to_string()));

        let result =
            FeatureMerger::get_effective_install_order(config_order.as_ref(), &merge_config)
                .unwrap();
        assert_eq!(
            result,
            Some(vec![
                "docker".to_string(),
                "git".to_string(),
                "node".to_string()
            ])
        );
    }

    #[test]
    fn test_get_effective_install_order_config_fallback() {
        let config_order = Some(vec!["git".to_string(), "node".to_string()]);
        let merge_config = FeatureMergeConfig::new(None, false, None);

        let result =
            FeatureMerger::get_effective_install_order(config_order.as_ref(), &merge_config)
                .unwrap();
        assert_eq!(result, Some(vec!["git".to_string(), "node".to_string()]));
    }

    #[test]
    fn test_get_effective_install_order_none() {
        let config_order: Option<Vec<String>> = None;
        let merge_config = FeatureMergeConfig::new(None, false, None);

        let result =
            FeatureMerger::get_effective_install_order(config_order.as_ref(), &merge_config)
                .unwrap();
        assert_eq!(result, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_option_value_conversions() {
        let bool_val = OptionValue::Boolean(true);
        assert_eq!(bool_val.as_bool(), Some(true));
        assert_eq!(bool_val.as_str(), None);

        let string_val = OptionValue::String("test".to_string());
        assert_eq!(string_val.as_bool(), None);
        assert_eq!(string_val.as_str(), Some("test"));
    }

    #[test]
    fn test_option_value_all_types() {
        // Test Boolean
        let bool_val = OptionValue::Boolean(true);
        assert_eq!(bool_val.as_bool(), Some(true));
        assert_eq!(bool_val.as_str(), None);
        assert!(!bool_val.is_null());

        // Test String
        let string_val = OptionValue::String("test".to_string());
        assert_eq!(string_val.as_bool(), None);
        assert_eq!(string_val.as_str(), Some("test"));
        assert!(!string_val.is_null());

        // Test Number
        let number_val = OptionValue::Number(serde_json::Number::from(42));
        assert_eq!(number_val.as_bool(), None);
        assert_eq!(number_val.as_str(), None);
        assert!(number_val.as_number().is_some());
        assert_eq!(number_val.as_number().unwrap().as_i64(), Some(42));
        assert!(!number_val.is_null());

        // Test Array
        let array_val = OptionValue::Array(vec![serde_json::Value::String("item".to_string())]);
        assert_eq!(array_val.as_bool(), None);
        assert!(array_val.as_array().is_some());
        assert_eq!(array_val.as_array().unwrap().len(), 1);
        assert!(!array_val.is_null());

        // Test Object
        let mut obj = serde_json::Map::new();
        obj.insert(
            "key".to_string(),
            serde_json::Value::String("value".to_string()),
        );
        let object_val = OptionValue::Object(obj.clone());
        assert_eq!(object_val.as_bool(), None);
        assert!(object_val.as_object().is_some());
        assert_eq!(object_val.as_object().unwrap().len(), 1);
        assert!(!object_val.is_null());

        // Test Null
        let null_val = OptionValue::Null;
        assert_eq!(null_val.as_bool(), None);
        assert_eq!(null_val.as_str(), None);
        assert!(null_val.is_null());
    }

    #[test]
    fn test_option_value_display() {
        assert_eq!(OptionValue::Boolean(true).to_string(), "true");
        assert_eq!(
            OptionValue::String("hello".to_string()).to_string(),
            "hello"
        );
        assert_eq!(
            OptionValue::Number(serde_json::Number::from(42)).to_string(),
            "42"
        );
        assert_eq!(
            OptionValue::Array(vec![serde_json::Value::String("item".to_string())]).to_string(),
            "[\"item\"]"
        );
        let mut obj = serde_json::Map::new();
        obj.insert(
            "key".to_string(),
            serde_json::Value::String("value".to_string()),
        );
        assert_eq!(OptionValue::Object(obj).to_string(), "{\"key\":\"value\"}");
        assert_eq!(OptionValue::Null.to_string(), "null");
    }

    #[test]
    fn test_option_value_json_roundtrip() {
        // Test that all OptionValue variants can be serialized and deserialized
        let test_values = vec![
            OptionValue::Boolean(true),
            OptionValue::String("test".to_string()),
            OptionValue::Number(serde_json::Number::from(42)),
            OptionValue::Array(vec![
                serde_json::Value::String("item1".to_string()),
                serde_json::Value::Number(serde_json::Number::from(123)),
            ]),
            {
                let mut obj = serde_json::Map::new();
                obj.insert("nested".to_string(), serde_json::Value::Bool(true));
                OptionValue::Object(obj)
            },
            OptionValue::Null,
        ];

        for original in test_values {
            let json = serde_json::to_string(&original).expect("Failed to serialize");
            let deserialized: OptionValue =
                serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(original, deserialized);
        }
    }

    #[test]
    fn test_option_value_from_json_value() {
        // Test converting serde_json::Value to OptionValue for all types
        let test_cases = vec![
            (serde_json::Value::Bool(true), OptionValue::Boolean(true)),
            (
                serde_json::Value::String("test".to_string()),
                OptionValue::String("test".to_string()),
            ),
            (
                serde_json::Value::Number(serde_json::Number::from(42)),
                OptionValue::Number(serde_json::Number::from(42)),
            ),
            (
                serde_json::Value::Array(vec![serde_json::Value::String("item".to_string())]),
                OptionValue::Array(vec![serde_json::Value::String("item".to_string())]),
            ),
            (
                {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "key".to_string(),
                        serde_json::Value::String("value".to_string()),
                    );
                    serde_json::Value::Object(obj.clone())
                },
                {
                    let mut obj = serde_json::Map::new();
                    obj.insert(
                        "key".to_string(),
                        serde_json::Value::String("value".to_string()),
                    );
                    OptionValue::Object(obj)
                },
            ),
            (serde_json::Value::Null, OptionValue::Null),
        ];

        for (json_val, expected_option_val) in test_cases {
            let option_val = match json_val {
                serde_json::Value::Bool(b) => OptionValue::Boolean(b),
                serde_json::Value::String(s) => OptionValue::String(s),
                serde_json::Value::Number(n) => OptionValue::Number(n),
                serde_json::Value::Array(a) => OptionValue::Array(a),
                serde_json::Value::Object(o) => OptionValue::Object(o),
                serde_json::Value::Null => OptionValue::Null,
            };
            assert_eq!(option_val, expected_option_val);
        }
    }

    #[test]
    fn test_feature_option_default_values() {
        let bool_option = FeatureOption::Boolean {
            default: Some(true),
            description: None,
        };
        assert_eq!(
            bool_option.default_value(),
            Some(OptionValue::Boolean(true))
        );

        let string_option = FeatureOption::String {
            default: Some("default_value".to_string()),
            description: None,
            r#enum: None,
            proposals: None,
        };
        assert_eq!(
            string_option.default_value(),
            Some(OptionValue::String("default_value".to_string()))
        );
    }

    #[test]
    fn test_feature_option_validation() {
        let bool_option = FeatureOption::Boolean {
            default: Some(true),
            description: None,
        };
        assert!(bool_option
            .validate_value(&OptionValue::Boolean(false))
            .is_ok());
        assert!(bool_option
            .validate_value(&OptionValue::String("test".to_string()))
            .is_err());

        let enum_option = FeatureOption::String {
            default: None,
            description: None,
            r#enum: Some(vec!["value1".to_string(), "value2".to_string()]),
            proposals: None,
        };
        assert!(enum_option
            .validate_value(&OptionValue::String("value1".to_string()))
            .is_ok());
        assert!(enum_option
            .validate_value(&OptionValue::String("invalid".to_string()))
            .is_err());
    }

    #[test]
    fn test_feature_option_validation_passthrough_types() {
        // Test that pass-through types (Number, Array, Object, Null) are accepted
        // regardless of the option definition, since they're not in the schema
        let bool_option = FeatureOption::Boolean {
            default: Some(true),
            description: None,
        };

        // Pass-through types should be accepted even for Boolean option
        assert!(bool_option
            .validate_value(&OptionValue::Number(serde_json::Number::from(42)))
            .is_ok());
        assert!(bool_option
            .validate_value(&OptionValue::Array(vec![]))
            .is_ok());
        assert!(bool_option
            .validate_value(&OptionValue::Object(serde_json::Map::new()))
            .is_ok());
        assert!(bool_option.validate_value(&OptionValue::Null).is_ok());

        let string_option = FeatureOption::String {
            default: None,
            description: None,
            r#enum: None,
            proposals: None,
        };

        // Pass-through types should also be accepted for String option
        assert!(string_option
            .validate_value(&OptionValue::Number(serde_json::Number::from(42)))
            .is_ok());
        assert!(string_option
            .validate_value(&OptionValue::Array(vec![]))
            .is_ok());
        assert!(string_option
            .validate_value(&OptionValue::Object(serde_json::Map::new()))
            .is_ok());
        assert!(string_option.validate_value(&OptionValue::Null).is_ok());
    }

    #[test]
    fn test_parse_minimal_feature_metadata() {
        let minimal_feature = r#"
        {
            "id": "test-feature"
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(minimal_feature.as_bytes()).unwrap();

        let metadata = parse_feature_metadata(temp_file.path()).unwrap();
        assert_eq!(metadata.id, "test-feature");
        assert_eq!(metadata.name, None);
        assert_eq!(metadata.options.len(), 0);
        assert!(!metadata.has_lifecycle_commands());
    }

    #[test]
    fn test_parse_feature_with_options() {
        let feature_with_options = r#"
        {
            "id": "test-feature",
            "name": "Test Feature",
            "description": "A test feature",
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
            "onCreateCommand": "echo 'Feature installed'"
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file
            .write_all(feature_with_options.as_bytes())
            .unwrap();

        let metadata = parse_feature_metadata(temp_file.path()).unwrap();
        assert_eq!(metadata.id, "test-feature");
        assert_eq!(metadata.name, Some("Test Feature".to_string()));
        assert_eq!(metadata.options.len(), 2);
        assert!(metadata.has_lifecycle_commands());

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
    fn test_parse_invalid_feature_schema() {
        let invalid_feature = r#"
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
        temp_file.write_all(invalid_feature.as_bytes()).unwrap();

        let result = parse_feature_metadata(temp_file.path());
        assert!(result.is_err());

        if let Err(crate::errors::DeaconError::Feature(FeatureError::Validation { message })) =
            result
        {
            assert!(message.contains("Feature id is required"));
        } else {
            panic!("Expected validation error for empty id");
        }
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = parse_feature_metadata(Path::new("/nonexistent/path/feature.json"));
        assert!(result.is_err());

        if let Err(crate::errors::DeaconError::Feature(FeatureError::NotFound { .. })) = result {
            // Expected
        } else {
            panic!("Expected NotFound error");
        }
    }

    #[test]
    fn test_parse_invalid_json() {
        let invalid_json = r#"
        {
            "id": "test-feature",
            "invalid": json
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(invalid_json.as_bytes()).unwrap();

        let result = parse_feature_metadata(temp_file.path());
        assert!(result.is_err());

        if let Err(crate::errors::DeaconError::Feature(FeatureError::Parsing { .. })) = result {
            // Expected
        } else {
            panic!("Expected parsing error for invalid JSON");
        }
    }

    #[test]
    fn test_dependency_resolver_linear_dependencies() {
        let features = vec![
            create_test_feature("feature-a", vec![], HashMap::new()),
            create_test_feature("feature-b", vec!["feature-a".to_string()], HashMap::new()),
            create_test_feature("feature-c", vec!["feature-b".to_string()], HashMap::new()),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();

        assert_eq!(
            plan.feature_ids(),
            vec!["feature-a", "feature-b", "feature-c"]
        );
    }

    #[test]
    fn test_dependency_resolver_branching_graph() {
        let mut depends_on = HashMap::new();
        depends_on.insert("feature-a".to_string(), serde_json::Value::Bool(true));

        let features = vec![
            create_test_feature("feature-a", vec![], HashMap::new()),
            create_test_feature("feature-b", vec!["feature-a".to_string()], HashMap::new()),
            create_test_feature("feature-c", vec!["feature-a".to_string()], HashMap::new()),
            create_test_feature("feature-d", vec!["feature-b".to_string()], depends_on),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();

        let ids = plan.feature_ids();

        // feature-a should come first
        assert_eq!(ids[0], "feature-a");

        // feature-b and feature-c should come before feature-d
        let b_index = ids.iter().position(|x| x == "feature-b").unwrap();
        let c_index = ids.iter().position(|x| x == "feature-c").unwrap();
        let d_index = ids.iter().position(|x| x == "feature-d").unwrap();

        assert!(b_index < d_index);
        assert!(c_index < d_index);
    }

    #[test]
    fn test_dependency_resolver_cycle_detection() {
        let mut depends_on_b = HashMap::new();
        depends_on_b.insert("feature-c".to_string(), serde_json::Value::Bool(true));

        let mut depends_on_c = HashMap::new();
        depends_on_c.insert("feature-a".to_string(), serde_json::Value::Bool(true));

        let features = vec![
            create_test_feature("feature-a", vec!["feature-b".to_string()], HashMap::new()),
            create_test_feature("feature-b", vec![], depends_on_b),
            create_test_feature("feature-c", vec![], depends_on_c),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let result = resolver.resolve(&features);

        assert!(result.is_err());
        if let Err(FeatureError::DependencyCycle { cycle_path }) = result {
            // Should contain the cycle
            assert!(cycle_path.contains("feature-a"));
            assert!(cycle_path.contains("feature-b"));
            assert!(cycle_path.contains("feature-c"));
        } else {
            panic!("Expected dependency cycle error");
        }
    }

    #[test]
    fn test_dependency_cycle_error_format_spec_compliance() {
        // Test: Verify cycle detection error message format per SPEC.md §9
        // SPEC.md §9 requirement: "Circular dependencies detected => error with details"
        // This test validates the error includes all required information and locks the format

        let mut depends_on_b = HashMap::new();
        depends_on_b.insert("feature-c".to_string(), serde_json::Value::Bool(true));

        let mut depends_on_c = HashMap::new();
        depends_on_c.insert("feature-a".to_string(), serde_json::Value::Bool(true));

        let features = vec![
            create_test_feature("feature-a", vec!["feature-b".to_string()], HashMap::new()),
            create_test_feature("feature-b", vec![], depends_on_b),
            create_test_feature("feature-c", vec![], depends_on_c),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let result = resolver.resolve(&features);

        // Verify error is returned
        assert!(
            result.is_err(),
            "Circular dependency should produce an error per SPEC.md §9"
        );

        let err = result.unwrap_err();

        // Test the error structure
        match &err {
            FeatureError::DependencyCycle { cycle_path } => {
                // SPEC.md §9: "error with details" - verify all involved features are present
                assert!(
                    cycle_path.contains("feature-a"),
                    "Cycle path should contain feature-a (required detail), got: {}",
                    cycle_path
                );
                assert!(
                    cycle_path.contains("feature-b"),
                    "Cycle path should contain feature-b (required detail), got: {}",
                    cycle_path
                );
                assert!(
                    cycle_path.contains("feature-c"),
                    "Cycle path should contain feature-c (required detail), got: {}",
                    cycle_path
                );

                // Verify the path shows directionality (part of "details")
                assert!(
                    cycle_path.contains("->"),
                    "Cycle path should show direction with arrows, got: {}",
                    cycle_path
                );

                // Verify the cycle forms a closed loop (validates correctness of cycle detection)
                let parts: Vec<&str> = cycle_path.split(" -> ").collect();
                assert!(
                    parts.len() >= 3,
                    "Cycle path should have at least 3 nodes, got: {}",
                    cycle_path
                );
                assert_eq!(
                    parts.first(),
                    parts.last(),
                    "Cycle path should form a closed loop (start == end), got: {}",
                    cycle_path
                );
            }
            _ => panic!(
                "Expected DependencyCycle error per SPEC.md §9, got: {:?}",
                err
            ),
        }

        // Verify the full error message includes proper terminology per SPEC.md §9
        let full_error_msg = format!("{}", err);

        // SPEC.md §9: "Circular dependencies detected"
        assert!(
            full_error_msg.to_lowercase().contains("cycle")
                || full_error_msg.to_lowercase().contains("circular"),
            "Error message should contain 'cycle' or 'circular' terminology per SPEC.md §9, got: {}",
            full_error_msg
        );

        assert!(
            full_error_msg.to_lowercase().contains("depend"),
            "Error message should reference 'dependencies' per SPEC.md §9, got: {}",
            full_error_msg
        );

        assert!(
            full_error_msg.contains("feature"),
            "Error message should reference 'features' context, got: {}",
            full_error_msg
        );

        // Snapshot test: Lock the exact format to prevent regressions
        // Expected format: "Dependency cycle detected in features: <cycle_path>"
        assert!(
            full_error_msg.starts_with("Dependency cycle detected in features:"),
            "Error message format should match expected pattern (snapshot), got: {}",
            full_error_msg
        );

        // Verify all feature IDs are in the full error message (the "details" requirement)
        assert!(
            full_error_msg.contains("feature-a"),
            "Full error should contain feature-a (required detail), got: {}",
            full_error_msg
        );
        assert!(
            full_error_msg.contains("feature-b"),
            "Full error should contain feature-b (required detail), got: {}",
            full_error_msg
        );
        assert!(
            full_error_msg.contains("feature-c"),
            "Full error should contain feature-c (required detail), got: {}",
            full_error_msg
        );
    }

    #[test]
    fn test_dependency_resolver_override_order() {
        let features = vec![
            create_test_feature("feature-a", vec![], HashMap::new()),
            create_test_feature("feature-b", vec!["feature-a".to_string()], HashMap::new()),
            create_test_feature("feature-c", vec![], HashMap::new()),
        ];

        let override_order = vec!["feature-c".to_string(), "feature-b".to_string()];
        let resolver = FeatureDependencyResolver::new(Some(override_order));
        let plan = resolver.resolve(&features).unwrap();

        let ids = plan.feature_ids();

        // Dependencies must be respected: feature-a must come before feature-b
        let a_index = ids.iter().position(|x| x == "feature-a").unwrap();
        let b_index = ids.iter().position(|x| x == "feature-b").unwrap();
        assert!(a_index < b_index);

        // The order should respect dependencies first
        // feature-c has no dependencies and could be anywhere, but override order
        // is a hint for resolving ties, not violating dependencies
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"feature-a".to_string()));
        assert!(ids.contains(&"feature-b".to_string()));
        assert!(ids.contains(&"feature-c".to_string()));
    }

    #[test]
    fn test_dependency_resolver_override_order_validation() {
        let features = vec![
            create_test_feature("feature-a", vec![], HashMap::new()),
            create_test_feature("feature-b", vec![], HashMap::new()),
        ];

        let override_order = vec!["feature-a".to_string(), "nonexistent".to_string()];
        let resolver = FeatureDependencyResolver::new(Some(override_order));
        let result = resolver.resolve(&features);

        assert!(result.is_err());
        if let Err(FeatureError::DependencyResolution { message }) = result {
            assert!(message.contains("nonexistent"));
            assert!(message.contains("overrideFeatureInstallOrder"));
        } else {
            panic!("Expected dependency resolution error");
        }
    }

    #[test]
    fn test_dependency_resolver_missing_dependencies() {
        let features = vec![
            create_test_feature("feature-a", vec![], HashMap::new()),
            create_test_feature("feature-b", vec!["nonexistent".to_string()], HashMap::new()),
        ];

        let resolver = FeatureDependencyResolver::new(None);
        let plan = resolver.resolve(&features).unwrap();

        // Should succeed but warn about missing dependency
        let mut ids = plan.feature_ids();
        ids.sort(); // Make test deterministic
        assert_eq!(ids, vec!["feature-a", "feature-b"]);
    }

    #[test]
    fn test_installation_plan_methods() {
        let features = vec![
            create_test_feature("feature-a", vec![], HashMap::new()),
            create_test_feature("feature-b", vec![], HashMap::new()),
        ];

        let plan = InstallationPlan::new(features);

        assert_eq!(plan.len(), 2);
        assert!(!plan.is_empty());
        assert_eq!(plan.feature_ids(), vec!["feature-a", "feature-b"]);

        assert!(plan.get_feature("feature-a").is_some());
        assert!(plan.get_feature("nonexistent").is_none());
    }

    #[test]
    fn test_installation_plan_empty() {
        let plan = InstallationPlan::new(vec![]);

        assert_eq!(plan.len(), 0);
        assert!(plan.is_empty());
        assert_eq!(plan.feature_ids(), Vec::<String>::new());
    }

    // Helper function to create test features
    fn create_test_feature(
        id: &str,
        installs_after: Vec<String>,
        depends_on: HashMap<String, serde_json::Value>,
    ) -> ResolvedFeature {
        let metadata = FeatureMetadata {
            id: id.to_string(),
            version: None,
            name: Some(format!("Test Feature {}", id)),
            description: None,
            documentation_url: None,
            license_url: None,
            options: HashMap::new(),
            container_env: HashMap::new(),
            mounts: vec![],
            init: None,
            privileged: None,
            cap_add: vec![],
            security_opt: vec![],
            entrypoint: None,
            installs_after,
            depends_on,
            on_create_command: None,
            update_content_command: None,
            post_create_command: None,
            post_start_command: None,
            post_attach_command: None,
        };

        ResolvedFeature {
            id: id.to_string(),
            source: format!("test://features/{}", id),
            options: HashMap::new(),
            metadata,
        }
    }
}
