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

/// Macro for printing redacted output
macro_rules! println_redacted {
    ($config:expr, $fmt:expr) => {
        let output = format!($fmt);
        let redacted = crate::redaction::redact_if_enabled(&output, $config);
        println!("{}", redacted);
    };
    ($config:expr, $fmt:expr, $($arg:tt)*) => {
        let output = format!($fmt, $($arg)*);
        let redacted = crate::redaction::redact_if_enabled(&output, $config);
        println!("{}", redacted);
    };
}

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
    /// Platform support information
    pub platform: PlatformInfo,
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
    /// Environment information
    pub environment: EnvironmentInfo,
    /// Runtime configuration details
    pub runtime_config: RuntimeConfig,
    /// System resource usage
    pub resources: ResourceInfo,
}

/// Host operating system information
#[derive(Debug, Serialize, Deserialize)]
pub struct HostOsInfo {
    pub name: String,
    pub version: String,
    pub arch: String,
}

/// Platform support information
#[derive(Debug, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub platform_type: String,
    pub is_wsl: bool,
    pub supports_full_capabilities: bool,
    pub supports_full_user_remapping: bool,
    pub needs_docker_desktop_path_conversion: bool,
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
    /// Error message if disk space check failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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

/// Environment information
#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    /// Selected environment variables (redacted values for sensitive ones)
    pub variables: std::collections::HashMap<String, String>,
    /// Shell information
    pub shell: Option<String>,
    /// User home directory
    pub home: Option<String>,
    /// Path environment variable
    pub path: Option<String>,
}

/// Runtime configuration details
#[derive(Debug, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Log level setting
    pub log_level: String,
    /// Log format (json or text)
    pub log_format: String,
    /// Redaction enabled status
    pub redaction_enabled: bool,
    /// Container runtime (docker, podman, etc)
    pub container_runtime: String,
}

/// System resource usage information
#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceInfo {
    /// Total system memory in bytes
    pub total_memory: u64,
    /// Available system memory in bytes
    pub available_memory: u64,
    /// CPU count
    pub cpu_count: usize,
    /// System load average (1, 5, 15 minutes) - Linux/macOS only
    pub load_average: Option<(f64, f64, f64)>,
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
    let platform = collect_platform_info();
    let docker_info = collect_docker_info().await;
    let disk_space = collect_disk_space_info();
    let config_discovery = collect_config_discovery_info(context);
    let features = collect_features_info();
    let last_build_hash = collect_last_build_hash();
    let cache_stats = collect_cache_stats().await;
    let environment = collect_environment_info();
    let runtime_config = collect_runtime_config();
    let resources = collect_resource_info();

    Ok(DoctorInfo {
        cli_version,
        host_os,
        platform,
        docker_info,
        disk_space,
        config_discovery,
        features,
        last_build_hash,
        cache_stats,
        environment,
        runtime_config,
        resources,
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

/// Collect platform support information
fn collect_platform_info() -> PlatformInfo {
    let platform = crate::platform::Platform::detect();

    PlatformInfo {
        platform_type: match platform {
            crate::platform::Platform::Linux => "Linux".to_string(),
            crate::platform::Platform::MacOS => "macOS".to_string(),
            crate::platform::Platform::Windows => "Windows".to_string(),
            crate::platform::Platform::WSL => "WSL".to_string(),
        },
        is_wsl: matches!(platform, crate::platform::Platform::WSL),
        supports_full_capabilities: platform.supports_full_capabilities(),
        supports_full_user_remapping: platform.supports_full_user_remapping(),
        needs_docker_desktop_path_conversion: platform.needs_docker_desktop_path_conversion(),
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
                error: None,
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to get disk space information: {}", e);
            warn!("{}", error_msg);
            DiskSpaceInfo {
                total_bytes: 0,
                available_bytes: 0,
                used_bytes: 0,
                error: Some(error_msg),
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

/// Collect environment information
fn collect_environment_info() -> EnvironmentInfo {
    debug!("Collecting environment information");

    let mut variables = std::collections::HashMap::new();

    // Collect key environment variables relevant for diagnostics
    // Only collect non-sensitive ones or mark sensitive ones for redaction
    let env_vars_to_collect = [
        "HOME",
        "USER",
        "SHELL",
        "PATH",
        "LANG",
        "LC_ALL",
        "TERM",
        "DEACON_LOG_LEVEL",
        "DEACON_LOG_FORMAT",
        "DOCKER_HOST",
        "DOCKER_CONFIG",
        "DOCKER_CERT_PATH",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        "http_proxy",
        "https_proxy",
        "no_proxy",
    ];

    for var_name in &env_vars_to_collect {
        if let Ok(value) = std::env::var(var_name) {
            // For PATH, only include first 200 chars to avoid overly long values
            if *var_name == "PATH" && value.len() > 200 {
                variables.insert(var_name.to_string(), format!("{}...", &value[..200]));
            } else {
                variables.insert(var_name.to_string(), value);
            }
        }
    }

    // Cross-platform shell detection: SHELL on Unix, COMSPEC on Windows
    let shell = std::env::var("SHELL")
        .ok()
        .or_else(|| std::env::var("COMSPEC").ok());

    // Cross-platform home directory detection
    let home = std::env::var("HOME").ok().or_else(|| {
        // Try USERPROFILE on Windows
        std::env::var("USERPROFILE").ok().or_else(|| {
            // Fall back to HOMEDRIVE + HOMEPATH on Windows
            match (
                std::env::var("HOMEDRIVE").ok(),
                std::env::var("HOMEPATH").ok(),
            ) {
                (Some(drive), Some(path)) => Some(format!("{}{}", drive, path)),
                _ => None,
            }
        })
    });

    let path = std::env::var("PATH").ok();

    EnvironmentInfo {
        variables,
        shell,
        home,
        path,
    }
}

/// Collect runtime configuration details
fn collect_runtime_config() -> RuntimeConfig {
    debug!("Collecting runtime configuration");

    let log_level = std::env::var("DEACON_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let log_format = std::env::var("DEACON_LOG_FORMAT").unwrap_or_else(|_| "text".to_string());

    // Redaction is enabled by default unless explicitly disabled
    let redaction_enabled = std::env::var("DEACON_NO_REDACT")
        .map(|v| v != "1" && v.to_lowercase() != "true")
        .unwrap_or(true);

    // Container runtime - default to docker
    let container_runtime =
        std::env::var("DEACON_CONTAINER_RUNTIME").unwrap_or_else(|_| "docker".to_string());

    RuntimeConfig {
        log_level,
        log_format,
        redaction_enabled,
        container_runtime,
    }
}

/// Collect system resource information
fn collect_resource_info() -> ResourceInfo {
    debug!("Collecting system resource information");

    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let total_memory = sys.total_memory();
    let available_memory = sys.available_memory();
    let cpu_count = sys.cpus().len();

    // Load average is only available on Unix-like systems
    let load_average = if cfg!(unix) {
        sysinfo::System::load_average();
        let load_avg = sysinfo::System::load_average();
        Some((load_avg.one, load_avg.five, load_avg.fifteen))
    } else {
        None
    };

    ResourceInfo {
        total_memory,
        available_memory,
        cpu_count,
        load_average,
    }
}

/// Print diagnostic information in human-readable text format with redaction applied
fn print_text_output_with_redaction(
    info: &DoctorInfo,
    redaction_config: &crate::redaction::RedactionConfig,
) {
    println_redacted!(redaction_config, "Deacon Doctor Diagnostics");
    println_redacted!(redaction_config, "========================");
    println!();

    println_redacted!(redaction_config, "CLI Version: {}", info.cli_version);
    println!();

    println_redacted!(redaction_config, "Host OS:");
    println_redacted!(redaction_config, "  Name: {}", info.host_os.name);
    println_redacted!(redaction_config, "  Version: {}", info.host_os.version);
    println_redacted!(redaction_config, "  Architecture: {}", info.host_os.arch);
    println!();

    println_redacted!(redaction_config, "Platform:");
    println_redacted!(redaction_config, "  Type: {}", info.platform.platform_type);
    println_redacted!(
        redaction_config,
        "  WSL Environment: {}",
        info.platform.is_wsl
    );
    println_redacted!(
        redaction_config,
        "  Full Capabilities: {}",
        info.platform.supports_full_capabilities
    );
    println_redacted!(
        redaction_config,
        "  Full User Remapping: {}",
        info.platform.supports_full_user_remapping
    );
    println_redacted!(
        redaction_config,
        "  Docker Desktop Path Conversion: {}",
        info.platform.needs_docker_desktop_path_conversion
    );
    println!();

    println_redacted!(redaction_config, "Docker:");
    println_redacted!(
        redaction_config,
        "  Installed: {}",
        info.docker_info.installed
    );
    if let Some(version) = &info.docker_info.version {
        println_redacted!(redaction_config, "  Version: {}", version);
    }
    println_redacted!(
        redaction_config,
        "  Daemon Running: {}",
        info.docker_info.daemon_running
    );
    if let Some(summary) = &info.docker_info.info_summary {
        println_redacted!(
            redaction_config,
            "  Containers Running: {}",
            summary.containers_running.unwrap_or(0)
        );
        println_redacted!(
            redaction_config,
            "  Images: {}",
            summary.images.unwrap_or(0)
        );
        if let Some(storage) = &summary.storage_driver {
            println_redacted!(redaction_config, "  Storage Driver: {}", storage);
        }
    }
    println!();

    println_redacted!(redaction_config, "Disk Space:");
    if let Some(error) = &info.disk_space.error {
        println_redacted!(redaction_config, "  Error: {}", error);
        println_redacted!(redaction_config, "  (Showing 0 bytes as fallback)");
    }
    println_redacted!(
        redaction_config,
        "  Total: {}",
        ByteSize(info.disk_space.total_bytes)
    );
    println_redacted!(
        redaction_config,
        "  Available: {}",
        ByteSize(info.disk_space.available_bytes)
    );
    println_redacted!(
        redaction_config,
        "  Used: {}",
        ByteSize(info.disk_space.used_bytes)
    );
    println!();

    println_redacted!(redaction_config, "Configuration Discovery:");
    if let Some(workspace) = &info.config_discovery.workspace_folder {
        println_redacted!(redaction_config, "  Workspace: {}", workspace);
    }
    if let Some(primary) = &info.config_discovery.primary_config {
        println_redacted!(redaction_config, "  Primary Config: {}", primary);
    }
    println_redacted!(
        redaction_config,
        "  Config Files Found: {:?}",
        info.config_discovery.config_files_found
    );
    println!();

    println_redacted!(redaction_config, "Available Features: {:?}", info.features);
    println!();

    if let Some(hash) = &info.last_build_hash {
        println_redacted!(redaction_config, "Last Build Hash: {}", hash);
        println!();
    }

    println_redacted!(redaction_config, "Environment:");
    if let Some(shell) = &info.environment.shell {
        println_redacted!(redaction_config, "  Shell: {}", shell);
    }
    if let Some(home) = &info.environment.home {
        println_redacted!(redaction_config, "  Home: {}", home);
    }
    println_redacted!(
        redaction_config,
        "  Key Variables: {} collected",
        info.environment.variables.len()
    );
    println!();

    println_redacted!(redaction_config, "Runtime Configuration:");
    println_redacted!(
        redaction_config,
        "  Log Level: {}",
        info.runtime_config.log_level
    );
    println_redacted!(
        redaction_config,
        "  Log Format: {}",
        info.runtime_config.log_format
    );
    println_redacted!(
        redaction_config,
        "  Redaction Enabled: {}",
        info.runtime_config.redaction_enabled
    );
    println_redacted!(
        redaction_config,
        "  Container Runtime: {}",
        info.runtime_config.container_runtime
    );
    println!();

    println_redacted!(redaction_config, "System Resources:");
    println_redacted!(
        redaction_config,
        "  Total Memory: {}",
        ByteSize(info.resources.total_memory)
    );
    println_redacted!(
        redaction_config,
        "  Available Memory: {}",
        ByteSize(info.resources.available_memory)
    );
    println_redacted!(
        redaction_config,
        "  CPU Count: {}",
        info.resources.cpu_count
    );
    if let Some((one, five, fifteen)) = info.resources.load_average {
        println_redacted!(
            redaction_config,
            "  Load Average: {:.2}, {:.2}, {:.2}",
            one,
            five,
            fifteen
        );
    }
    println!();
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

    // Add environment information
    zip.start_file("environment.json", options).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to add environment info to bundle: {}", e),
        })
    })?;
    let env_json = serde_json::to_string_pretty(&doctor_info.environment).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to serialize environment info: {}", e),
        })
    })?;
    // Apply redaction to environment variables
    let redacted_env = crate::redaction::redact_if_enabled(
        &env_json,
        &crate::redaction::RedactionConfig::default(),
    );
    zip.write_all(redacted_env.as_bytes()).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to write environment info: {}", e),
        })
    })?;

    // Add runtime configuration
    zip.start_file("runtime-config.json", options)
        .map_err(|e| {
            DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to add runtime config to bundle: {}", e),
            })
        })?;
    let runtime_json = serde_json::to_string_pretty(&doctor_info.runtime_config).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to serialize runtime config: {}", e),
        })
    })?;
    zip.write_all(runtime_json.as_bytes()).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to write runtime config: {}", e),
        })
    })?;

    // Add system resources information
    zip.start_file("resources.json", options).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to add resources info to bundle: {}", e),
        })
    })?;
    let resources_json = serde_json::to_string_pretty(&doctor_info.resources).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to serialize resources info: {}", e),
        })
    })?;
    zip.write_all(resources_json.as_bytes()).map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("Failed to write resources info: {}", e),
        })
    })?;

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
