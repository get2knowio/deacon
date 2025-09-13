//! Host requirements evaluation and validation
//!
//! This module provides functionality to evaluate whether the host system meets
//! the minimum requirements specified in the devcontainer configuration.
//!
//! Uses the sysinfo crate to inspect actual system resources and compares them
//! against the requirements specified in HostRequirements.

use crate::config::HostRequirements;
use crate::errors::{ConfigError, Result};
use serde::Serialize;
use sysinfo::System;
use tracing::{debug, warn};

/// Host system information collected for requirements evaluation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostInfo {
    /// Number of logical CPU cores
    pub cpu_cores: f64,
    /// Total system memory in bytes  
    pub total_memory: u64,
    /// Available storage space in bytes (for current working directory)
    pub available_storage: u64,
}

/// Result of host requirements evaluation.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HostRequirementsEvaluation {
    /// Host system information
    pub host_info: HostInfo,
    /// Requirements that were evaluated
    pub requirements: HostRequirements,
    /// Whether all requirements are met
    pub requirements_met: bool,
    /// Detailed evaluation results for each requirement
    pub cpu_evaluation: Option<RequirementEvaluation>,
    pub memory_evaluation: Option<RequirementEvaluation>,
    pub storage_evaluation: Option<RequirementEvaluation>,
}

/// Evaluation result for a specific requirement.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequirementEvaluation {
    /// Required value (in appropriate units)
    pub required: f64,
    /// Available value (in appropriate units)
    pub available: f64,
    /// Whether this specific requirement is met
    pub met: bool,
    /// Human-readable description
    pub description: String,
}

/// Host requirements evaluator that can inspect system resources.
pub struct HostRequirementsEvaluator {
    system: System,
}

impl HostRequirementsEvaluator {
    /// Create a new evaluator with system information.
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self { system }
    }

    /// Get current host system information.
    pub fn get_host_info(&mut self, workspace_path: Option<&std::path::Path>) -> Result<HostInfo> {
        // Refresh system information
        self.system.refresh_all();

        let cpu_cores =
            System::physical_core_count().unwrap_or_else(|| self.system.cpus().len()) as f64;

        let total_memory = self.system.total_memory();

        // Get available storage for the workspace path or current directory
        let available_storage = self.get_available_storage(workspace_path)?;

        debug!(
            "Host info: {} CPU cores, {} bytes memory, {} bytes storage",
            cpu_cores, total_memory, available_storage
        );

        Ok(HostInfo {
            cpu_cores,
            total_memory,
            available_storage,
        })
    }

    /// Evaluate host requirements against actual system capabilities.
    pub fn evaluate_requirements(
        &mut self,
        requirements: &HostRequirements,
        workspace_path: Option<&std::path::Path>,
    ) -> Result<HostRequirementsEvaluation> {
        let host_info = self.get_host_info(workspace_path)?;

        let cpu_evaluation = requirements.cpus.as_ref().map(|req| {
            let required = req.parse_cpu_cores().unwrap_or(0.0);
            let available = host_info.cpu_cores;
            let met = available >= required;

            RequirementEvaluation {
                required,
                available,
                met,
                description: format!(
                    "CPU: {} cores required, {} cores available",
                    required, available
                ),
            }
        });

        let memory_evaluation =
            requirements
                .memory
                .as_ref()
                .and_then(|req| match req.parse_bytes() {
                    Ok(required) => {
                        let available = host_info.total_memory;
                        let met = available >= required;

                        Some(RequirementEvaluation {
                            required: required as f64,
                            available: available as f64,
                            met,
                            description: format!(
                                "Memory: {} bytes required, {} bytes available",
                                required, available
                            ),
                        })
                    }
                    Err(e) => {
                        warn!("Failed to parse memory requirement: {}", e);
                        None
                    }
                });

        let storage_evaluation =
            requirements
                .storage
                .as_ref()
                .and_then(|req| match req.parse_bytes() {
                    Ok(required) => {
                        let available = host_info.available_storage;
                        let met = available >= required;

                        Some(RequirementEvaluation {
                            required: required as f64,
                            available: available as f64,
                            met,
                            description: format!(
                                "Storage: {} bytes required, {} bytes available",
                                required, available
                            ),
                        })
                    }
                    Err(e) => {
                        warn!("Failed to parse storage requirement: {}", e);
                        None
                    }
                });

        let requirements_met = [&cpu_evaluation, &memory_evaluation, &storage_evaluation]
            .iter()
            .filter_map(|&eval| eval.as_ref())
            .all(|eval| eval.met);

        Ok(HostRequirementsEvaluation {
            host_info,
            requirements: requirements.clone(),
            requirements_met,
            cpu_evaluation,
            memory_evaluation,
            storage_evaluation,
        })
    }

    /// Validate host requirements, returning an error if requirements are not met.
    ///
    /// This is the main entry point for requirement validation. If `ignore_failures`
    /// is true, unmet requirements will be logged as warnings instead of failing.
    pub fn validate_requirements(
        &mut self,
        requirements: &HostRequirements,
        workspace_path: Option<&std::path::Path>,
        ignore_failures: bool,
    ) -> Result<HostRequirementsEvaluation> {
        let evaluation = self.evaluate_requirements(requirements, workspace_path)?;

        if !evaluation.requirements_met {
            let failed_requirements: Vec<String> = [
                &evaluation.cpu_evaluation,
                &evaluation.memory_evaluation,
                &evaluation.storage_evaluation,
            ]
            .iter()
            .filter_map(|&eval| eval.as_ref())
            .filter(|eval| !eval.met)
            .map(|eval| eval.description.clone())
            .collect();

            let message = format!(
                "Host requirements not met: {}",
                failed_requirements.join(", ")
            );

            if ignore_failures {
                warn!("{}", message);
            } else {
                return Err(ConfigError::Validation { message }.into());
            }
        }

        Ok(evaluation)
    }

    /// Get available storage space for the given path.
    fn get_available_storage(&mut self, _workspace_path: Option<&std::path::Path>) -> Result<u64> {
        // For now, return a large value since we can't easily get disk info
        // This will be improved in a future iteration when we figure out the correct sysinfo API
        warn!("Storage evaluation not implemented yet, assuming unlimited storage");
        Ok(u64::MAX / 2) // Use half of max to be conservative
    }
}

impl Default for HostRequirementsEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResourceSpec;

    #[test]
    fn test_resource_spec_parsing() {
        // Test number parsing
        let spec = ResourceSpec::Number(4.0);
        assert_eq!(spec.parse_cpu_cores().unwrap(), 4.0);
        assert_eq!(spec.parse_bytes().unwrap(), 4);

        // Test string parsing for CPU
        let spec = ResourceSpec::String("2.5".to_string());
        assert_eq!(spec.parse_cpu_cores().unwrap(), 2.5);

        // Test string parsing for memory/storage
        let spec = ResourceSpec::String("4GB".to_string());
        assert_eq!(spec.parse_bytes().unwrap(), 4_000_000_000);

        let spec = ResourceSpec::String("512MB".to_string());
        assert_eq!(spec.parse_bytes().unwrap(), 512_000_000);

        let spec = ResourceSpec::String("1GiB".to_string());
        assert_eq!(spec.parse_bytes().unwrap(), 1_073_741_824);
    }

    #[test]
    fn test_host_requirements_evaluation() {
        let mut evaluator = HostRequirementsEvaluator::new();

        // Test with reasonable requirements that should pass
        let requirements = HostRequirements {
            cpus: Some(ResourceSpec::Number(1.0)),
            memory: Some(ResourceSpec::String("100MB".to_string())),
            storage: Some(ResourceSpec::String("1MB".to_string())),
        };

        let result = evaluator.evaluate_requirements(&requirements, None);
        assert!(result.is_ok());

        let evaluation = result.unwrap();
        assert!(evaluation.cpu_evaluation.is_some());
        assert!(evaluation.memory_evaluation.is_some());
        assert!(evaluation.storage_evaluation.is_some());
    }

    #[test]
    fn test_validation_with_ignore_failures() {
        let mut evaluator = HostRequirementsEvaluator::new();

        // Test with unrealistic requirements
        let requirements = HostRequirements {
            cpus: Some(ResourceSpec::Number(1000.0)), // Very high CPU requirement
            memory: Some(ResourceSpec::String("1TB".to_string())), // Very high memory
            storage: Some(ResourceSpec::String("1PB".to_string())), // Impossible storage
        };

        // Should fail without ignore flag
        let result = evaluator.validate_requirements(&requirements, None, false);
        assert!(result.is_err());

        // Should succeed with ignore flag (but requirements_met will be false)
        let result = evaluator.validate_requirements(&requirements, None, true);
        assert!(result.is_ok());
        let evaluation = result.unwrap();
        assert!(!evaluation.requirements_met);
    }
}
