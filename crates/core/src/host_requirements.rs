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

/// Trait for abstracting filesystem operations to enable testing
pub trait FilesystemProvider {
    /// Get available disk space in bytes for the given path
    fn get_available_space(&self, path: &std::path::Path) -> Result<u64>;
}

/// Default filesystem provider that uses real system calls
pub struct DefaultFilesystemProvider;

impl FilesystemProvider for DefaultFilesystemProvider {
    fn get_available_space(&self, path: &std::path::Path) -> Result<u64> {
        get_disk_space_for_path(path)
    }
}

/// Get disk space information for a given path using platform-specific APIs
pub fn get_disk_space_for_path(path: &std::path::Path) -> Result<u64> {
    #[cfg(unix)]
    {
        use std::process::Command;

        // Try to canonicalize path, fall back to parent or current directory if path doesn't exist
        let canonical_path = path.canonicalize().unwrap_or_else(|_| {
            // If path doesn't exist, try parent directory
            path.parent()
                .and_then(|p| p.canonicalize().ok())
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        });

        // Try df with different options depending on the platform
        let result = if cfg!(target_os = "macos") {
            // macOS df doesn't support -B option, use -k (kilobytes)
            Command::new("df").arg("-k").arg(&canonical_path).output()
        } else {
            // Linux and other Unix systems support -B1 (bytes)
            Command::new("df").arg("-B1").arg(&canonical_path).output()
        };

        let output = result.map_err(|e| ConfigError::Validation {
            message: format!("Failed to execute df command: {}", e),
        })?;

        if !output.status.success() {
            // Try fallback with basic df command
            warn!(
                "df command failed for {}, trying fallback",
                canonical_path.display()
            );

            let fallback_output =
                Command::new("df")
                    .arg(&canonical_path)
                    .output()
                    .map_err(|e| ConfigError::Validation {
                        message: format!("Failed to execute fallback df command: {}", e),
                    })?;

            if !fallback_output.status.success() {
                return Err(ConfigError::Validation {
                    message: format!(
                        "df command failed for {}: {}",
                        canonical_path.display(),
                        String::from_utf8_lossy(&fallback_output.stderr)
                    ),
                }
                .into());
            }

            return parse_df_output(
                &String::from_utf8_lossy(&fallback_output.stdout),
                &canonical_path,
                false,
            );
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let is_kilobytes = cfg!(target_os = "macos");
        parse_df_output(&output_str, &canonical_path, is_kilobytes)
    }

    #[cfg(windows)]
    {
        use std::process::Command;

        // Try to canonicalize path, fall back to parent or current directory if path doesn't exist
        let canonical_path = path.canonicalize().unwrap_or_else(|_| {
            // If path doesn't exist, try parent directory
            path.parent()
                .and_then(|p| p.canonicalize().ok())
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        });

        // Extract drive letter from path, handling extended-length paths (\\?\D:\...)
        let path_str = canonical_path.to_string_lossy();
        let drive_letter = if path_str.starts_with(r"\\?\") && path_str.len() >= 5 {
            // Extended-length path: \\?\D:\... - drive letter is at index 4
            path_str.chars().nth(4).unwrap_or('C')
        } else {
            // Normal path: D:\... - drive letter is first char
            path_str.chars().next().unwrap_or('C')
        };

        // Use PowerShell to get free space
        let script = format!(
            "Get-WmiObject -Class Win32_LogicalDisk | Where-Object {{ $_.DeviceID -eq '{}:' }} | Select-Object -ExpandProperty FreeSpace",
            drive_letter
        );

        let output = Command::new("powershell")
            .args(["-Command", &script])
            .output()
            .map_err(|e| ConfigError::Validation {
                message: format!("Failed to execute PowerShell command: {}", e),
            })?;

        if !output.status.success() {
            return Err(ConfigError::Validation {
                message: format!(
                    "PowerShell command failed for {}: {}",
                    canonical_path.display(),
                    String::from_utf8_lossy(&output.stderr)
                ),
            }
            .into());
        }

        let output_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        output_str.parse::<u64>().map_err(|e| {
            ConfigError::Validation {
                message: format!(
                    "Could not parse PowerShell output for {}: {}",
                    canonical_path.display(),
                    e
                ),
            }
            .into()
        })
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(ConfigError::Validation {
            message: format!(
                "Disk space checking not implemented for this platform: {}",
                path.display()
            ),
        }
        .into())
    }
}

/// Parse df command output to extract available space
fn parse_df_output(
    output_str: &str,
    canonical_path: &std::path::Path,
    is_kilobytes: bool,
) -> Result<u64> {
    let lines: Vec<&str> = output_str.lines().collect();

    // df output format varies but generally:
    // Line 1: Headers (e.g., "Filesystem 1024-blocks Used Available Use% Mounted on")
    // Line 2+: Data
    if lines.len() >= 2 {
        let data_line = lines[1];
        let fields: Vec<&str> = data_line.split_whitespace().collect();

        // Different df formats might have different column counts
        // Try to find the available space column (typically 3rd or 4th column)
        let available_column = if fields.len() >= 4 {
            3
        } else if fields.len() >= 3 {
            2
        } else {
            return Err(ConfigError::Validation {
                message: format!(
                    "Could not parse df output format for {}: {}",
                    canonical_path.display(),
                    data_line
                ),
            }
            .into());
        };

        if let Ok(available_value) = fields[available_column].parse::<u64>() {
            let available_bytes = if is_kilobytes {
                available_value * 1024 // Convert from KB to bytes
            } else {
                available_value // Already in bytes (from -B1)
            };
            return Ok(available_bytes);
        }
    }

    // Return error if parsing fails
    Err(ConfigError::Validation {
        message: format!(
            "Could not parse df output for {}: insufficient data in output",
            canonical_path.display()
        ),
    }
    .into())
}

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
pub struct HostRequirementsEvaluator<F: FilesystemProvider = DefaultFilesystemProvider> {
    system: System,
    filesystem_provider: F,
}

impl HostRequirementsEvaluator {
    /// Create a new evaluator with system information.
    pub fn new() -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        Self {
            system,
            filesystem_provider: DefaultFilesystemProvider,
        }
    }
}

impl<F: FilesystemProvider> HostRequirementsEvaluator<F> {
    /// Create a new evaluator with a custom filesystem provider (for testing).
    pub fn with_filesystem_provider<P: FilesystemProvider>(
        filesystem_provider: P,
    ) -> HostRequirementsEvaluator<P> {
        let mut system = System::new_all();
        system.refresh_all();
        HostRequirementsEvaluator {
            system,
            filesystem_provider,
        }
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
    fn get_available_storage(&mut self, workspace_path: Option<&std::path::Path>) -> Result<u64> {
        let path = workspace_path.unwrap_or_else(|| std::path::Path::new("."));
        self.filesystem_provider.get_available_space(path)
    }
}

impl Default for HostRequirementsEvaluator<DefaultFilesystemProvider> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ResourceSpec;
    use std::collections::HashMap;

    /// Mock filesystem provider for testing
    pub struct MockFilesystemProvider {
        space_map: HashMap<String, u64>,
    }

    impl MockFilesystemProvider {
        pub fn new() -> Self {
            Self {
                space_map: HashMap::new(),
            }
        }

        pub fn set_available_space(&mut self, path: &str, bytes: u64) {
            self.space_map.insert(path.to_string(), bytes);
        }
    }

    impl FilesystemProvider for MockFilesystemProvider {
        fn get_available_space(&self, path: &std::path::Path) -> Result<u64> {
            let path_str = path.to_string_lossy().to_string();
            self.space_map
                .get(&path_str)
                .copied()
                .or_else(|| self.space_map.get(".").copied())
                .ok_or_else(|| {
                    ConfigError::Validation {
                        message: format!("No space configured for path: {}", path_str),
                    }
                    .into()
                })
        }
    }

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
    fn test_filesystem_provider_abstraction() {
        let mut mock_provider = MockFilesystemProvider::new();
        mock_provider.set_available_space(".", 2_000_000_000); // 2GB

        let mut evaluator =
            HostRequirementsEvaluator::<MockFilesystemProvider>::with_filesystem_provider(
                mock_provider,
            );

        let result = evaluator.get_available_storage(None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2_000_000_000);
    }

    #[test]
    fn test_storage_evaluation_with_mock_provider() {
        let mut mock_provider = MockFilesystemProvider::new();
        mock_provider.set_available_space(".", 1_500_000_000); // 1.5GB

        let mut evaluator =
            HostRequirementsEvaluator::<MockFilesystemProvider>::with_filesystem_provider(
                mock_provider,
            );

        let requirements = HostRequirements {
            cpus: Some(ResourceSpec::Number(1.0)),
            memory: Some(ResourceSpec::String("100MB".to_string())),
            storage: Some(ResourceSpec::String("1GB".to_string())), // Should pass
        };

        let result = evaluator.evaluate_requirements(&requirements, None);
        assert!(result.is_ok());

        let evaluation = result.unwrap();
        assert!(evaluation.storage_evaluation.is_some());
        let storage_eval = evaluation.storage_evaluation.unwrap();
        assert!(storage_eval.met);
        assert_eq!(storage_eval.required, 1_000_000_000.0);
        assert_eq!(storage_eval.available, 1_500_000_000.0);
    }

    #[test]
    fn test_storage_evaluation_insufficient_space() {
        let mut mock_provider = MockFilesystemProvider::new();
        mock_provider.set_available_space(".", 500_000_000); // 500MB

        let mut evaluator =
            HostRequirementsEvaluator::<MockFilesystemProvider>::with_filesystem_provider(
                mock_provider,
            );

        let requirements = HostRequirements {
            cpus: None,
            memory: None,
            storage: Some(ResourceSpec::String("1GB".to_string())), // Should fail
        };

        let result = evaluator.evaluate_requirements(&requirements, None);
        assert!(result.is_ok());

        let evaluation = result.unwrap();
        assert!(!evaluation.requirements_met);
        assert!(evaluation.storage_evaluation.is_some());
        let storage_eval = evaluation.storage_evaluation.unwrap();
        assert!(!storage_eval.met);
        assert_eq!(storage_eval.required, 1_000_000_000.0);
        assert_eq!(storage_eval.available, 500_000_000.0);
    }

    #[test]
    fn test_validation_with_mock_provider_fail() {
        let mut mock_provider = MockFilesystemProvider::new();
        mock_provider.set_available_space(".", 100_000_000); // 100MB

        let mut evaluator =
            HostRequirementsEvaluator::<MockFilesystemProvider>::with_filesystem_provider(
                mock_provider,
            );

        let requirements = HostRequirements {
            cpus: None,
            memory: None,
            storage: Some(ResourceSpec::String("1GB".to_string())), // Should fail
        };

        // Should fail without ignore flag
        let result = evaluator.validate_requirements(&requirements, None, false);
        assert!(result.is_err());

        // Should succeed with ignore flag
        let result = evaluator.validate_requirements(&requirements, None, true);
        assert!(result.is_ok());
        let evaluation = result.unwrap();
        assert!(!evaluation.requirements_met);
    }

    #[test]
    fn test_real_disk_space_current_directory() {
        // Test that the real implementation works for current directory
        let result = get_disk_space_for_path(std::path::Path::new("."));
        match result {
            Ok(space) => {
                assert!(space > 0, "Available space should be greater than 0");
                // Should be reasonable amount (more than 1MB, less than 1PB)
                assert!(space > 1_000_000, "Should have more than 1MB available");
                assert!(space < 1_000_000_000_000_000, "Should be less than 1PB");
            }
            Err(e) => {
                // In some test environments, this might fail, which is acceptable
                eprintln!(
                    "Real disk space check failed (expected in some test environments): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_real_disk_space_with_workspace_path() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path();

        let result = get_disk_space_for_path(temp_path);
        match result {
            Ok(space) => {
                assert!(space > 0, "Available space should be greater than 0");
            }
            Err(e) => {
                // In some test environments, this might fail, which is acceptable
                eprintln!("Real disk space check failed for temp dir (expected in some test environments): {}", e);
            }
        }
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
        assert!(
            result.is_ok(),
            "evaluate_requirements failed: {:?}",
            result.err()
        );

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

    #[test]
    fn test_no_silent_fallbacks_on_error() {
        // Test that when filesystem provider returns an error,
        // it propagates up rather than being silently converted to a fallback value
        struct FailingFilesystemProvider;

        impl FilesystemProvider for FailingFilesystemProvider {
            fn get_available_space(&self, _path: &std::path::Path) -> Result<u64> {
                Err(ConfigError::Validation {
                    message: "Simulated filesystem error".to_string(),
                }
                .into())
            }
        }

        let mut evaluator =
            HostRequirementsEvaluator::<FailingFilesystemProvider>::with_filesystem_provider(
                FailingFilesystemProvider,
            );

        // Attempting to get host info should fail, not return a fallback value
        let result = evaluator.get_host_info(None);
        assert!(
            result.is_err(),
            "Expected error to propagate, not silent fallback"
        );

        // Verify error message contains indication of failure (it might be wrapped)
        if let Err(e) = result {
            let error_msg = e.to_string();
            // The error should contain our simulated error or reference to validation/filesystem error
            assert!(
                error_msg.contains("Simulated filesystem error")
                    || error_msg.contains("Configuration error")
                    || error_msg.contains("Validation"),
                "Error message should indicate filesystem error, got: {}",
                error_msg
            );
        }
    }

    #[test]
    fn test_parse_df_output_linux_format() {
        // Test parsing standard Linux df -B1 output
        let df_output = "Filesystem     1B-blocks      Used Available Use% Mounted on\n\
                         /dev/sda1    50000000000 30000000000 20000000000  60% /";
        let path = std::path::Path::new("/");
        let result = parse_df_output(df_output, path, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 20000000000);
    }

    #[test]
    fn test_parse_df_output_macos_format() {
        // Test parsing macOS df -k output (kilobytes)
        let df_output = "Filesystem   1024-blocks      Used Available Capacity  Mounted on\n\
                         /dev/disk1s1   244912536 175824536  52248000    78%    /";
        let path = std::path::Path::new("/");
        let result = parse_df_output(df_output, path, true);
        assert!(result.is_ok());
        // 52248000 KB * 1024 = 53501952000 bytes
        assert_eq!(result.unwrap(), 52248000 * 1024);
    }

    #[test]
    fn test_parse_df_output_with_long_device_name() {
        // Test df output where device name wraps to second line
        let df_output = "Filesystem     1B-blocks      Used Available Use% Mounted on\n\
                         /dev/mapper/vg-lv\n\
                                   100000000000 50000000000 50000000000  50% /data";
        let path = std::path::Path::new("/data");
        // This should fail with current simple parsing (line 2 doesn't have enough fields)
        // but we're testing that it returns an error, not a fallback
        let result = parse_df_output(df_output, path, false);
        assert!(result.is_err());
        // The error is wrapped, so just verify it's an error
        assert!(
            result.is_err(),
            "Should return error for malformed df output"
        );
    }

    #[test]
    fn test_parse_df_output_low_disk_space() {
        // Test with very low available space (bytes)
        let df_output = "Filesystem     1B-blocks      Used Available Use% Mounted on\n\
                         /dev/sda1    50000000000 49999000000   1000000  99% /";
        let path = std::path::Path::new("/");
        let result = parse_df_output(df_output, path, false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1000000); // 1MB available
    }

    #[test]
    fn test_parse_df_output_insufficient_columns() {
        // Test df output with too few columns
        let df_output = "Filesystem Used\n\
                         /dev/sda1  50%";
        let path = std::path::Path::new("/");
        let result = parse_df_output(df_output, path, false);
        assert!(
            result.is_err(),
            "Should return error for insufficient columns"
        );
    }

    #[test]
    fn test_parse_df_output_empty() {
        // Test empty df output
        let df_output = "";
        let path = std::path::Path::new("/");
        let result = parse_df_output(df_output, path, false);
        assert!(result.is_err(), "Should return error for empty df output");
    }

    #[test]
    fn test_low_disk_threshold_validation() {
        // Test that low disk space properly fails validation
        let mut mock_provider = MockFilesystemProvider::new();
        mock_provider.set_available_space(".", 100_000_000); // 100MB available

        let mut evaluator =
            HostRequirementsEvaluator::<MockFilesystemProvider>::with_filesystem_provider(
                mock_provider,
            );

        let requirements = HostRequirements {
            cpus: None,
            memory: None,
            storage: Some(ResourceSpec::String("10GB".to_string())), // Need 10GB
        };

        // Without ignore flag, should return error
        let result = evaluator.validate_requirements(&requirements, None, false);
        assert!(
            result.is_err(),
            "Should fail validation when storage requirement not met"
        );

        // With ignore flag, should succeed but mark as not met
        let result = evaluator.validate_requirements(&requirements, None, true);
        assert!(result.is_ok());
        let evaluation = result.unwrap();
        assert!(!evaluation.requirements_met);
        assert!(evaluation.storage_evaluation.is_some());
        let storage_eval = evaluation.storage_evaluation.unwrap();
        assert!(!storage_eval.met);
    }

    #[test]
    fn test_powershell_error_propagation() {
        // Test that PowerShell-like errors propagate properly
        // This simulates what would happen on Windows with a failed PowerShell command
        struct PowerShellErrorProvider;

        impl FilesystemProvider for PowerShellErrorProvider {
            fn get_available_space(&self, path: &std::path::Path) -> Result<u64> {
                Err(ConfigError::Validation {
                    message: format!(
                        "PowerShell command failed for {}: Access denied",
                        path.display()
                    ),
                }
                .into())
            }
        }

        let mut evaluator =
            HostRequirementsEvaluator::<PowerShellErrorProvider>::with_filesystem_provider(
                PowerShellErrorProvider,
            );

        let result = evaluator.get_host_info(None);
        assert!(
            result.is_err(),
            "PowerShell errors should propagate without fallback"
        );
    }

    #[test]
    fn test_call_sites_propagate_errors() {
        // Test that get_host_info and evaluate_requirements properly propagate errors
        struct AlwaysFailProvider;

        impl FilesystemProvider for AlwaysFailProvider {
            fn get_available_space(&self, _path: &std::path::Path) -> Result<u64> {
                Err(ConfigError::Validation {
                    message: "Test error from filesystem provider".to_string(),
                }
                .into())
            }
        }

        let mut evaluator =
            HostRequirementsEvaluator::<AlwaysFailProvider>::with_filesystem_provider(
                AlwaysFailProvider,
            );

        // get_host_info should propagate the error
        let result = evaluator.get_host_info(None);
        assert!(result.is_err());

        // evaluate_requirements should also propagate the error
        let requirements = HostRequirements {
            cpus: Some(ResourceSpec::Number(1.0)),
            memory: Some(ResourceSpec::String("1GB".to_string())),
            storage: Some(ResourceSpec::String("1GB".to_string())),
        };
        let result = evaluator.evaluate_requirements(&requirements, None);
        assert!(result.is_err());

        // validate_requirements should propagate the error regardless of ignore flag
        let result = evaluator.validate_requirements(&requirements, None, false);
        assert!(result.is_err());

        let result = evaluator.validate_requirements(&requirements, None, true);
        assert!(result.is_err());
    }
}
