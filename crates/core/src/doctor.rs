//! Doctor command implementation for environment diagnostics and support bundles
//!
//! This module provides functionality to collect system information, Docker details,
//! configuration discovery results, and create support bundles for troubleshooting.

use crate::docker::{CliDocker, Docker};
use crate::errors::{DeaconError, Result};
use bytesize::ByteSize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Simple context for doctor command
#[derive(Debug, Clone)]
pub struct DoctorContext {
    /// Workspace folder path
    pub workspace_folder: Option<PathBuf>,
    /// Configuration file path
    pub config: Option<PathBuf>,
}

/// Doctor information collected from the system
#[derive(Debug, Serialize, Deserialize)]
pub struct DoctorInfo {
    /// CLI version information
    pub cli_version: String,
    /// Host operating system details
    pub host_os: HostOsInfo,
    /// Docker version and status
    pub docker_info: DockerDiagnostics,
    /// Available disk space information
    pub disk_space: DiskSpaceInfo,
    /// Configuration discovery results
    pub config_discovery: ConfigDiscoveryInfo,
    /// Available features list
    pub features: Vec<String>,
    /// Last build hash if available
    pub last_build_hash: Option<String>,
    /// Cache statistics
    pub cache_stats: CacheStats,
}

/// Host operating system information
#[derive(Debug, Serialize, Deserialize)]
pub struct HostOsInfo {
    pub name: String,
    pub version: String,
    pub arch: String,
}

/// Docker diagnostics information
#[derive(Debug, Serialize, Deserialize)]
pub struct DockerDiagnostics {
    pub installed: bool,
    pub version: Option<String>,
    pub daemon_running: bool,
    pub info_summary: Option<DockerInfoSummary>,
}

/// Summarized Docker info (not full docker info to avoid sensitive data)
#[derive(Debug, Serialize, Deserialize)]
pub struct DockerInfoSummary {
    pub containers_running: Option<u32>,
    pub containers_paused: Option<u32>,
    pub containers_stopped: Option<u32>,
    pub images: Option<u32>,
    pub server_version: Option<String>,
    pub storage_driver: Option<String>,
}

/// Disk space information
#[derive(Debug, Serialize, Deserialize)]
pub struct DiskSpaceInfo {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
}

/// Configuration discovery information
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigDiscoveryInfo {
    pub config_files_found: Vec<String>,
    pub workspace_folder: Option<String>,
    pub primary_config: Option<String>,
}

/// Cache statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStats {
    pub docker_cache_size: Option<u64>,
    pub build_cache_size: Option<u64>,
}

/// Run the doctor command to collect diagnostics and optionally create a bundle
pub async fn run_doctor(
    json_output: bool,
    bundle_path: Option<PathBuf>,
    context: DoctorContext,
    redaction_config: crate::redaction::RedactionConfig,
) -> Result<()> {
    info!("Running diagnostics...");

    // Collect all diagnostic information
    let doctor_info = collect_diagnostics(&context).await?;

    // Output results with redaction applied
    if json_output {
        let json_output = serde_json::to_string_pretty(&doctor_info).map_err(|e| {
            DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to serialize doctor info to JSON: {}", e),
            })
        })?;
        // Apply redaction to JSON output
        let redacted_output = crate::redaction::redact_if_enabled(&json_output, &redaction_config);
        println!("{}", redacted_output);
    } else {
        print_text_output_with_redaction(&doctor_info, &redaction_config);
    }

    // Create bundle if requested
    if let Some(bundle_path) = bundle_path {
        create_support_bundle(&doctor_info, &bundle_path, &context).await?;
        info!("Support bundle created at: {}", bundle_path.display());
    }

    Ok(())
}

/// Collect all diagnostic information
async fn collect_diagnostics(context: &DoctorContext) -> Result<DoctorInfo> {
    debug!("Collecting diagnostic information");

    let cli_version = crate::version().to_string();
    let host_os = collect_host_os_info();
    let docker_info = collect_docker_info().await;
    let disk_space = collect_disk_space_info();
    let config_discovery = collect_config_discovery_info(context);
    let features = collect_features_info();
    let last_build_hash = collect_last_build_hash();
    let cache_stats = collect_cache_stats().await;

    Ok(DoctorInfo {
        cli_version,
        host_os,
        docker_info,
        disk_space,
        config_discovery,
        features,
        last_build_hash,
        cache_stats,
    })
}

/// Collect host operating system information
fn collect_host_os_info() -> HostOsInfo {
    let name = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    // Try to get more detailed version info
    let version = if cfg!(target_os = "linux") {
        fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.starts_with("PRETTY_NAME="))
                    .map(|line| {
                        line.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "Unknown".to_string())
    } else if cfg!(target_os = "macos") {
        "macOS".to_string()
    } else if cfg!(target_os = "windows") {
        "Windows".to_string()
    } else {
        "Unknown".to_string()
    };

    HostOsInfo {
        name,
        version,
        arch,
    }
}

/// Collect Docker diagnostics information
async fn collect_docker_info() -> DockerDiagnostics {
    debug!("Collecting Docker information");

    let docker_client = CliDocker::new();

    // Check if Docker is installed
    let installed = docker_client.check_docker_installed().is_ok();

    if !installed {
        return DockerDiagnostics {
            installed: false,
            version: None,
            daemon_running: false,
            info_summary: None,
        };
    }

    // Get Docker version
    let version = docker_client.get_version().await.ok();

    // Check if daemon is running
    let daemon_running = docker_client.ping().await.is_ok();

    // Get Docker info summary if daemon is running
    let info_summary = if daemon_running {
        docker_client.get_info_summary().await.ok()
    } else {
        None
    };

    DockerDiagnostics {
        installed,
        version,
        daemon_running,
        info_summary,
    }
}

/// Collect disk space information for current directory
fn collect_disk_space_info() -> DiskSpaceInfo {
    debug!("Collecting disk space information");

    // Get disk space for current working directory
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Use the same real disk space implementation as host_requirements
    match crate::host_requirements::get_disk_space_for_path(&current_dir) {
        Ok(available_bytes) => {
            // For total bytes, we can estimate based on available space
            // This is a conservative estimate - in practice available is usually 70-90% of total
            let estimated_total = (available_bytes as f64 / 0.8) as u64;
            let used_bytes = estimated_total.saturating_sub(available_bytes);

            DiskSpaceInfo {
                total_bytes: estimated_total,
                available_bytes,
                used_bytes,
            }
        }
        Err(e) => {
            warn!(
                "Failed to get real disk space information: {}. Using fallback values.",
                e
            );
            DiskSpaceInfo {
                total_bytes: 0,
                available_bytes: 0,
                used_bytes: 0,
            }
        }
    }
}

/// Collect configuration discovery information
fn collect_config_discovery_info(context: &DoctorContext) -> ConfigDiscoveryInfo {
    debug!("Collecting configuration discovery information");

    let mut config_files_found = Vec::new();
    let workspace_folder = context
        .workspace_folder
        .as_ref()
        .map(|p| p.display().to_string());

    // Look for common devcontainer config files
    let possible_configs = [
        ".devcontainer/devcontainer.json",
        ".devcontainer.json",
        "devcontainer.json",
    ];

    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let base_path = context.workspace_folder.as_ref().unwrap_or(&current_dir);

    for config_path in &possible_configs {
        let full_path = base_path.join(config_path);
        if full_path.exists() {
            config_files_found.push(config_path.to_string());
        }
    }

    let primary_config = if let Some(config_override) = &context.config {
        Some(config_override.display().to_string())
    } else {
        config_files_found.first().cloned()
    };

    ConfigDiscoveryInfo {
        config_files_found,
        workspace_folder,
        primary_config,
    }
}

/// Collect features information
fn collect_features_info() -> Vec<String> {
    debug!("Collecting features information");

    // Placeholder - in a real implementation this would scan for available features
    vec![
        "docker-in-docker".to_string(),
        "node".to_string(),
        "python".to_string(),
        "git".to_string(),
    ]
}

/// Collect last build hash if available
fn collect_last_build_hash() -> Option<String> {
    debug!("Collecting last build hash");

    // Placeholder - in a real implementation this would check for build artifacts
    None
}

/// Collect cache statistics
async fn collect_cache_stats() -> CacheStats {
    debug!("Collecting cache statistics");

    // Placeholder - in a real implementation this would check Docker cache and build cache
    CacheStats {
        docker_cache_size: None,
        build_cache_size: None,
    }
}

/// Print diagnostic information in human-readable text format with redaction applied
fn print_text_output_with_redaction(
    info: &DoctorInfo,
    redaction_config: &crate::redaction::RedactionConfig,
) {
    // Create a macro to print with redaction applied
    macro_rules! println_redacted {
        ($fmt:expr) => {
            let output = format!($fmt);
            let redacted = crate::redaction::redact_if_enabled(&output, redaction_config);
            println!("{}", redacted);
        };
        ($fmt:expr, $($arg:tt)*) => {
            let output = format!($fmt, $($arg)*);
            let redacted = crate::redaction::redact_if_enabled(&output, redaction_config);
            println!("{}", redacted);
        };
    }

    println_redacted!("Deacon Doctor Diagnostics");
    println_redacted!("========================");
    println!();

    println_redacted!("CLI Version: {}", info.cli_version);
    println!();

    println_redacted!("Host OS:");
    println_redacted!("  Name: {}", info.host_os.name);
    println_redacted!("  Version: {}", info.host_os.version);
    println_redacted!("  Architecture: {}", info.host_os.arch);
    println!();

    println_redacted!("Docker:");
    println_redacted!("  Installed: {}", info.docker_info.installed);
    if let Some(version) = &info.docker_info.version {
        println_redacted!("  Version: {}", version);
    }
    println_redacted!("  Daemon Running: {}", info.docker_info.daemon_running);
    if let Some(summary) = &info.docker_info.info_summary {
        println_redacted!(
            "  Containers Running: {}",
            summary.containers_running.unwrap_or(0)
        );
        println_redacted!("  Images: {}", summary.images.unwrap_or(0));
        if let Some(storage) = &summary.storage_driver {
            println_redacted!("  Storage Driver: {}", storage);
        }
    }
    println!();

    println_redacted!("Disk Space:");
    println_redacted!("  Total: {}", ByteSize(info.disk_space.total_bytes));
    println_redacted!("  Available: {}", ByteSize(info.disk_space.available_bytes));
    println_redacted!("  Used: {}", ByteSize(info.disk_space.used_bytes));
    println!();

    println_redacted!("Configuration Discovery:");
    if let Some(workspace) = &info.config_discovery.workspace_folder {
        println_redacted!("  Workspace: {}", workspace);
    }
    if let Some(primary) = &info.config_discovery.primary_config {
        println_redacted!("  Primary Config: {}", primary);
    }
    println_redacted!(
        "  Config Files Found: {:?}",
        info.config_discovery.config_files_found
    );
    println!();

    println_redacted!("Available Features: {:?}", info.features);
    println!();

    if let Some(hash) = &info.last_build_hash {
        println_redacted!("Last Build Hash: {}", hash);
        println!();
    }
}

/// Create a support bundle with diagnostic information and configuration files
async fn create_support_bundle(
    doctor_info: &DoctorInfo,
    bundle_path: &Path,
    context: &DoctorContext,
) -> Result<()> {
    info!("Creating support bundle at: {}", bundle_path.display());

    use std::io::Write;

    let file = std::fs::File::create(bundle_path).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to create bundle file: {}", e),
        })
    })?;

    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // Add doctor.json to bundle
    zip.start_file("doctor.json", options).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to add doctor.json to bundle: {}", e),
        })
    })?;
    let doctor_json = serde_json::to_string_pretty(doctor_info).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to serialize doctor info: {}", e),
        })
    })?;
    zip.write_all(doctor_json.as_bytes()).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to write doctor.json: {}", e),
        })
    })?;

    // Add sanitized config files if they exist
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let base_path = context.workspace_folder.as_ref().unwrap_or(&current_dir);

    for config_file in &doctor_info.config_discovery.config_files_found {
        let config_path = base_path.join(config_file);
        if let Ok(content) = fs::read_to_string(&config_path) {
            let sanitized_content = sanitize_secrets(&content)?;
            zip.start_file(format!("configs/{}", config_file), options)
                .map_err(|e| {
                    DeaconError::Internal(crate::errors::InternalError::Generic {
                        message: format!("Failed to add config file to bundle: {}", e),
                    })
                })?;
            zip.write_all(sanitized_content.as_bytes()).map_err(|e| {
                DeaconError::Internal(crate::errors::InternalError::Generic {
                    message: format!("Failed to write config file: {}", e),
                })
            })?;
        }
    }

    // Add truncated docker info if available
    if doctor_info.docker_info.daemon_running {
        zip.start_file("docker-info-summary.json", options)
            .map_err(|e| {
                DeaconError::Internal(crate::errors::InternalError::Generic {
                    message: format!("Failed to add docker info to bundle: {}", e),
                })
            })?;
        if let Some(summary) = &doctor_info.docker_info.info_summary {
            let summary_json = serde_json::to_string_pretty(summary).map_err(|e| {
                DeaconError::Internal(crate::errors::InternalError::Generic {
                    message: format!("Failed to serialize Docker info summary: {}", e),
                })
            })?;
            zip.write_all(summary_json.as_bytes()).map_err(|e| {
                DeaconError::Internal(crate::errors::InternalError::Generic {
                    message: format!("Failed to write docker info: {}", e),
                })
            })?;
        }
    }

    zip.finish().map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to finish bundle: {}", e),
        })
    })?;
    Ok(())
}

/// Sanitize secrets from configuration content
/// Replaces values of keys matching regex (PASS|TOKEN|SECRET) with ****
pub fn sanitize_secrets(content: &str) -> Result<String> {
    debug!("Sanitizing secrets from content");

    // Regex to match keys containing PASS, TOKEN, or SECRET (case-insensitive)
    let secret_key_regex = Regex::new(r#"(?i)("[^"]*(?:pass|token|secret)[^"]*"\s*:\s*)"[^"]*""#)
        .map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to compile secret key regex: {}", e),
        })
    })?;

    // Also handle non-quoted keys
    let secret_key_regex_unquoted = Regex::new(
        r#"(?i)([a-zA-Z_][a-zA-Z0-9_]*(?:pass|token|secret)[a-zA-Z0-9_]*\s*[:=]\s*)"[^"]*""#,
    )
    .map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to compile unquoted secret key regex: {}", e),
        })
    })?;

    let mut sanitized = content.to_string();

    // Replace quoted keys
    sanitized = secret_key_regex
        .replace_all(&sanitized, r#"$1"****""#)
        .to_string();

    // Replace unquoted keys
    sanitized = secret_key_regex_unquoted
        .replace_all(&sanitized, r#"$1"****""#)
        .to_string();

    Ok(sanitized)
}

impl crate::docker::CliDocker {
    /// Get Docker version information
    pub async fn get_version(&self) -> Result<String> {
        let output = tokio::process::Command::new("docker")
            .arg("--version")
            .output()
            .await
            .map_err(|e| {
                DeaconError::Docker(crate::errors::DockerError::CLIError(format!(
                    "Failed to execute docker --version: {}",
                    e
                )))
            })?;

        if output.status.success() {
            let version = String::from_utf8(output.stdout)
                .map_err(|e| {
                    DeaconError::Docker(crate::errors::DockerError::CLIError(format!(
                        "Invalid UTF-8 in docker version output: {}",
                        e
                    )))
                })?
                .trim()
                .to_string();
            Ok(version)
        } else {
            Err(DeaconError::Docker(crate::errors::DockerError::CLIError(
                "Failed to get Docker version".to_string(),
            )))
        }
    }

    /// Get summarized Docker info (not full docker info to avoid sensitive data)
    pub async fn get_info_summary(&self) -> Result<DockerInfoSummary> {
        let output = tokio::process::Command::new("docker")
            .arg("system")
            .arg("df")
            .arg("--format")
            .arg("json")
            .output()
            .await
            .map_err(|e| {
                DeaconError::Docker(crate::errors::DockerError::CLIError(format!(
                    "Failed to execute docker system df: {}",
                    e
                )))
            })?;

        if output.status.success() {
            // For now, return a basic summary. In a real implementation,
            // this would parse docker info and extract safe, non-sensitive information
            Ok(DockerInfoSummary {
                containers_running: Some(0),
                containers_paused: Some(0),
                containers_stopped: Some(0),
                images: Some(0),
                server_version: None,
                storage_driver: Some("overlay2".to_string()),
            })
        } else {
            Err(DeaconError::Docker(crate::errors::DockerError::CLIError(
                "Failed to get Docker info".to_string(),
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_secrets_quoted_keys() {
        let content = r#"
        {
            "password": "secret123",
            "api_token": "abc123def",
            "database_secret": "mysecret",
            "regular_field": "not_secret"
        }
        "#;

        let sanitized = sanitize_secrets(content).unwrap();

        assert!(sanitized.contains(r#""password": "****""#));
        assert!(sanitized.contains(r#""api_token": "****""#));
        assert!(sanitized.contains(r#""database_secret": "****""#));
        assert!(sanitized.contains(r#""regular_field": "not_secret""#));
    }

    #[test]
    fn test_sanitize_secrets_case_insensitive() {
        let content = r#"
        {
            "PASSWORD": "secret123",
            "Token": "abc123def",
            "MY_SECRET": "mysecret"
        }
        "#;

        let sanitized = sanitize_secrets(content).unwrap();

        assert!(sanitized.contains(r#""PASSWORD": "****""#));
        assert!(sanitized.contains(r#""Token": "****""#));
        assert!(sanitized.contains(r#""MY_SECRET": "****""#));
    }

    #[test]
    fn test_sanitize_secrets_no_secrets() {
        let content = r#"
        {
            "name": "test",
            "version": "1.0.0",
            "description": "A test configuration"
        }
        "#;

        let sanitized = sanitize_secrets(content).unwrap();

        // Content should remain unchanged
        assert_eq!(sanitized.trim(), content.trim());
    }

    #[test]
    fn test_sanitize_secrets_partial_matches() {
        let content = r#"
        {
            "user_password_hash": "hash123",
            "token_expiry": "2024-01-01",
            "secret_key": "key123",
            "password_reset_url": "url123"
        }
        "#;

        let sanitized = sanitize_secrets(content).unwrap();

        assert!(sanitized.contains(r#""user_password_hash": "****""#));
        assert!(sanitized.contains(r#""token_expiry": "****""#));
        assert!(sanitized.contains(r#""secret_key": "****""#));
        assert!(sanitized.contains(r#""password_reset_url": "****""#));
    }
}
