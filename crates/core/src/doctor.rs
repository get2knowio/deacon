//! Doctor command implementation for environment diagnostics and support bundles
//!
//! This module provides functionality to collect system information, Docker details,
//! configuration discovery results, and create support bundles for troubleshooting.

use crate::docker::CliDocker;
use crate::errors::{DeaconError, Result};
use bytesize::ByteSize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tracing::{debug, info, warn};

/// Macro for printing redacted output
macro_rules! println_redacted {
    ($config:expr_2021, $fmt:expr_2021) => {
        let output = format!($fmt);
        let redacted = crate::redaction::redact_if_enabled(&output, $config);
        println!("{}", redacted);
    };
    ($config:expr_2021, $fmt:expr_2021, $($arg:tt)*) => {
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
    /// Probes that produced no value, each with the reason why.
    ///
    /// Omitted when every probe reported — an always-present empty list would
    /// say nothing. See [`SkippedProbe`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped_probes: Vec<SkippedProbe>,
}

/// A diagnostic probe that produced no value, and why.
///
/// `doctor` shells out to the container runtime for several facts, and any of
/// those calls can take unbounded time on a loaded daemon. Every one of them is
/// therefore bounded by [`PROBE_TIMEOUT`]. A probe that is bounded out, fails,
/// or cannot be launched is **reported here** — never silently omitted, and
/// never replaced with a fabricated value.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkippedProbe {
    /// Stable probe identifier, e.g. `docker_info`.
    pub probe: String,
    /// Always `"skipped"`, so an entry read on its own is self-describing.
    pub status: String,
    /// Why the probe produced nothing, e.g.
    /// ``"`docker info --format json` exceeded the 10s probe timeout"``.
    pub reason: String,
}

impl SkippedProbe {
    /// Record a skipped probe. Construction is also the point where the skip is
    /// logged, so a skip can never be reported in the output without also being
    /// audible in the logs.
    fn new(probe: impl Into<String>, reason: impl Into<String>) -> Self {
        let probe = probe.into();
        let reason = reason.into();
        warn!(probe = %probe, reason = %reason, "doctor probe skipped");
        Self {
            probe,
            status: "skipped".to_string(),
            reason,
        }
    }
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

/// Wall-clock bound for a single `doctor` probe that shells out to the
/// container runtime.
///
/// Matches the container environment probe's existing bound
/// ([`crate::container_env_probe`]'s `probe_timeout`), so deacon has one answer
/// to "how long may a diagnostic probe take".
///
/// Sized against measurement rather than taste (#507). On the daemon that
/// motivated the bound — ~4.1k images, ~300 volumes, 81 containers —
/// `docker info` and `docker version` each cost ~0.3s, so 10s leaves better
/// than an order of magnitude of headroom: a far more loaded daemon still
/// reports rather than being skipped. It also keeps `doctor`'s worst case at
/// three bounded probes (~30s) instead of unbounded, comfortably inside the
/// parity suite's 120s per-invocation limit.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for a killed probe to be reaped before handing it to
/// tokio's orphan reaper. SIGKILL is prompt unless the child is stuck in
/// uninterruptible sleep, which is exactly the case this bound exists for.
const REAP_TIMEOUT: Duration = Duration::from_secs(5);

/// Cap on how much of a probe's output is read.
///
/// The probes read small JSON documents; anything beyond this is a runtime
/// misbehaving, and reading it unbounded would put megabytes into a diagnostic
/// report (and, via `reason`, into a support bundle).
const MAX_PROBE_OUTPUT: u64 = 1 << 20;

/// Outcome of a bounded probe process.
enum ProbeOutcome {
    /// The process exited within the bound.
    Completed {
        status: std::process::ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    /// The process exceeded the bound and was killed and reaped.
    TimedOut,
}

/// What a bounded probe produced: a value, or the reason there is none.
enum Probed<T> {
    /// The probe reported.
    Value(T),
    /// The probe ran (or was launched) but produced nothing usable.
    Skipped(SkippedProbe),
    /// The program could not be launched at all — the runtime CLI is absent.
    NotLaunched(String),
}

/// Run `program args…` with a hard wall-clock bound.
///
/// The single place `doctor` shells out to a subprocess. Two properties matter
/// and neither is free:
///
/// * **On timeout the child is killed _and reaped_.** [`tokio::process::Child::kill`]
///   signals and then waits, so a hung daemon call leaves no zombie behind.
/// * **stdout and stderr are drained concurrently with the wait.** Waiting on
///   exit without reading the pipes deadlocks any process that outfills a pipe
///   buffer.
async fn run_bounded_probe(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> std::io::Result<ProbeOutcome> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    // `spawn` with piped stdio always populates these; a missing pipe is an I/O
    // condition to report, not something to unwrap.
    // Capped so a runtime that floods a pipe cannot put megabytes into a
    // diagnostic report (and, via `reason`, into a support bundle).
    let mut stdout_pipe = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("probe stdout pipe missing"))?
        .take(MAX_PROBE_OUTPUT);
    let mut stderr_pipe = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("probe stderr pipe missing"))?
        .take(MAX_PROBE_OUTPUT);

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let waited = tokio::time::timeout(timeout, async {
        let (out, err, status) = tokio::join!(
            stdout_pipe.read_to_end(&mut stdout),
            stderr_pipe.read_to_end(&mut stderr),
            child.wait(),
        );
        out?;
        err?;
        status
    })
    .await;

    match waited {
        Ok(Ok(status)) => Ok(ProbeOutcome::Completed {
            status,
            stdout,
            stderr,
        }),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => {
            // Kill AND reap in one call: `kill` sends SIGKILL then waits. The
            // wait is itself bounded, because it is the last thing in this
            // module that could block forever — a child wedged in uninterruptible
            // sleep (a dead `DOCKER_HOST` over a stuck mount) never reaps. If
            // that bound trips, `kill_on_drop` hands the corpse to tokio's
            // orphan reaper instead, so it is still collected, just not here.
            match tokio::time::timeout(REAP_TIMEOUT, child.kill()).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => debug!("Failed to reap timed-out probe `{}`: {}", program, e),
                Err(_) => debug!(
                    "Timed-out probe `{}` did not reap within {:?}; leaving it to kill_on_drop",
                    program, REAP_TIMEOUT
                ),
            }
            Ok(ProbeOutcome::TimedOut)
        }
    }
}

/// Run a bounded probe and reduce it to its stdout bytes or a recorded skip.
///
/// Every `doctor` runtime probe goes through here, so the skip vocabulary
/// (timeout, non-zero exit, absent binary) is written once.
async fn probe_stdout(
    probe: &str,
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Probed<Vec<u8>> {
    let display = format!(
        "`{}`",
        std::iter::once(program)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    );
    match run_bounded_probe(program, args, timeout).await {
        Ok(ProbeOutcome::Completed { status, stdout, .. }) if status.success() => {
            Probed::Value(stdout)
        }
        Ok(ProbeOutcome::Completed { status, stderr, .. }) => Probed::Skipped(SkippedProbe::new(
            probe,
            format!(
                "{} exited with {}: {}",
                display,
                status,
                summarize_stderr(&stderr)
            ),
        )),
        // `{:?}` on a `Duration` renders "10s" / "250ms" — the bound is stated
        // in the reason so the reader can tell a slow daemon from a broken one.
        Ok(ProbeOutcome::TimedOut) => Probed::Skipped(SkippedProbe::new(
            probe,
            format!("{} exceeded the {:?} probe timeout", display, timeout),
        )),
        // Only an ABSENT (or unexecutable) binary means "the runtime is not
        // installed" — that verdict makes `doctor` report `installed: false`,
        // so it must not absorb every other I/O condition. A fork failure under
        // load or a broken pipe mid-read is a probe that FAILED, and gets
        // reported as such rather than silently reclassified.
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
            ) =>
        {
            Probed::NotLaunched(format!("{} could not be launched: {}", display, e))
        }
        Err(e) => Probed::Skipped(SkippedProbe::new(
            probe,
            format!("{} failed: {}", display, e),
        )),
    }
}

/// Reduce a probe's stderr to a bounded, single-line fragment for its reason.
///
/// The reason string travels into `--json`, the text report and the support
/// bundle, so an unbounded copy of a noisy runtime's stderr would land in all
/// three. Keep the tail short enough to read.
fn summarize_stderr(stderr: &[u8]) -> String {
    const MAX_REASON_STDERR: usize = 512;

    let text = String::from_utf8_lossy(stderr);
    let trimmed = text.trim();
    match trimmed.char_indices().nth(MAX_REASON_STDERR) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed.to_string(),
    }
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
        let bundle_path_clone = bundle_path.clone();
        create_support_bundle(doctor_info, bundle_path, &context).await?;
        info!("Support bundle created at: {}", bundle_path_clone.display());
    }

    Ok(())
}

/// Collect all diagnostic information
async fn collect_diagnostics(context: &DoctorContext) -> Result<DoctorInfo> {
    debug!("Collecting diagnostic information");

    let cli_version = crate::version().to_string();
    let host_os = collect_host_os_info();
    let platform = collect_platform_info();
    let docker_info = collect_docker_info(PROBE_TIMEOUT).await;
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

/// Collect Docker diagnostics information.
///
/// Every call into the container runtime here is bounded by `timeout` (see
/// [`PROBE_TIMEOUT`]), and every bounded-out or failed probe is reported in
/// `skipped_probes` rather than dropped.
async fn collect_docker_info(timeout: Duration) -> DockerDiagnostics {
    debug!("Collecting Docker information");

    let docker_client = CliDocker::new();
    let runtime = docker_client.runtime_path().to_string();
    let mut skipped_probes = Vec::new();

    // Probe 1 — the CLI binary itself. A launch failure here IS the "not
    // installed" signal, so this one bounded async call replaces the previous
    // pairing of a *blocking* `check_docker_installed()` (a `std::process`
    // call inside an async fn) with a second, duplicate `--version` exec.
    let version = match probe_stdout("runtime_version", &runtime, &["--version"], timeout).await {
        Probed::Value(out) => Some(String::from_utf8_lossy(&out).trim().to_string()),
        Probed::Skipped(skip) => {
            skipped_probes.push(skip);
            None
        }
        Probed::NotLaunched(reason) => {
            debug!("Container runtime not installed: {}", reason);
            return DockerDiagnostics {
                installed: false,
                version: None,
                daemon_running: false,
                info_summary: None,
                skipped_probes,
            };
        }
    };

    // Probe 2 — daemon reachability. `<runtime> version` (unlike `--version`)
    // round-trips to the daemon, so it is the ping.
    let daemon_running = match probe_stdout(
        "daemon_ping",
        &runtime,
        &["version", "--format", "json"],
        timeout,
    )
    .await
    {
        Probed::Value(_) => true,
        Probed::Skipped(skip) => {
            skipped_probes.push(skip);
            false
        }
        Probed::NotLaunched(reason) => {
            skipped_probes.push(SkippedProbe::new("daemon_ping", reason));
            false
        }
    };

    // Probe 3 — daemon counters, via `<runtime> info`.
    //
    // NOT `<runtime> system df`, which this probe used to run: `system df`
    // walks the disk usage of every image, container, volume and build-cache
    // record and was measured in *minutes* on a loaded daemon (#507), while
    // `info` returns these very fields in ~0.3s on the same host. The old code
    // also discarded `system df`'s output entirely and returned hardcoded
    // zeros and a hardcoded storage driver, so bounding it alone would have
    // produced a fast fabrication rather than an answer.
    let info_summary = if daemon_running {
        match probe_stdout(
            "docker_info",
            &runtime,
            &["info", "--format", "json"],
            timeout,
        )
        .await
        {
            Probed::Value(out) => match parse_info_summary(&out) {
                Ok(summary) => Some(summary),
                Err(e) => {
                    skipped_probes.push(SkippedProbe::new(
                        "docker_info",
                        format!("could not parse `{} info --format json`: {}", runtime, e),
                    ));
                    None
                }
            },
            Probed::Skipped(skip) => {
                skipped_probes.push(skip);
                None
            }
            Probed::NotLaunched(reason) => {
                skipped_probes.push(SkippedProbe::new("docker_info", reason));
                None
            }
        }
    } else {
        skipped_probes.push(SkippedProbe::new(
            "docker_info",
            "the container runtime daemon is not reachable",
        ));
        None
    };

    DockerDiagnostics {
        installed: true,
        version,
        daemon_running,
        info_summary,
        skipped_probes,
    }
}

/// The subset of `<runtime> info --format json` that [`DockerInfoSummary`]
/// reports. Everything else the daemon returns (registry credentials, proxy
/// settings, plugin paths) is deliberately not read.
#[derive(Deserialize)]
struct DockerInfoRaw {
    #[serde(rename = "ContainersRunning")]
    containers_running: Option<u32>,
    #[serde(rename = "ContainersPaused")]
    containers_paused: Option<u32>,
    #[serde(rename = "ContainersStopped")]
    containers_stopped: Option<u32>,
    #[serde(rename = "Images")]
    images: Option<u32>,
    #[serde(rename = "ServerVersion")]
    server_version: Option<String>,
    #[serde(rename = "Driver")]
    storage_driver: Option<String>,
}

/// Why `<runtime> info --format json` could not be turned into a summary.
#[derive(Debug, thiserror::Error)]
enum InfoParseError {
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// The document parsed but carried none of the fields the summary reports —
    /// e.g. podman's `info`, which nests its counters under `store`/`host`
    /// rather than using Docker's `types.Info` keys. Every field being
    /// `Option`, this would otherwise deserialize "successfully" into a summary
    /// of nothings, which the text report would then render as zeros.
    #[error("no recognized fields (not a Docker `info` document)")]
    NoRecognizedFields,
}

/// Parse `<runtime> info --format json` into the reported summary.
fn parse_info_summary(stdout: &[u8]) -> std::result::Result<DockerInfoSummary, InfoParseError> {
    let raw: DockerInfoRaw = serde_json::from_slice(stdout)?;
    if raw.containers_running.is_none()
        && raw.containers_paused.is_none()
        && raw.containers_stopped.is_none()
        && raw.images.is_none()
        && raw.server_version.is_none()
        && raw.storage_driver.is_none()
    {
        return Err(InfoParseError::NoRecognizedFields);
    }
    Ok(DockerInfoSummary {
        containers_running: raw.containers_running,
        containers_paused: raw.containers_paused,
        containers_stopped: raw.containers_stopped,
        images: raw.images,
        server_version: raw.server_version,
        storage_driver: raw.storage_driver,
    })
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
        "DEACON_CONTAINER_RUNTIME",
        "DEACON_NO_REDACT",
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
        // Each line only where the daemon actually reported the field. An
        // `unwrap_or(0)` here would print "Images: 0" on a host with thousands
        // — the same fabrication this probe was fixed to stop producing.
        if let Some(running) = summary.containers_running {
            println_redacted!(redaction_config, "  Containers Running: {}", running);
        }
        if let Some(images) = summary.images {
            println_redacted!(redaction_config, "  Images: {}", images);
        }
        if let Some(server_version) = &summary.server_version {
            println_redacted!(redaction_config, "  Server Version: {}", server_version);
        }
        if let Some(storage) = &summary.storage_driver {
            println_redacted!(redaction_config, "  Storage Driver: {}", storage);
        }
    }
    // A skipped probe is stated, not silently dropped: without these lines the
    // text report is indistinguishable from a runtime that had nothing to say.
    for line in skipped_probe_lines(&info.docker_info.skipped_probes) {
        println_redacted!(redaction_config, "{}", line);
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
///
/// The whole bundle build (std::fs::File::create, ZipWriter writes, the
/// inner config-file reads, ZipWriter::finish) is synchronous; the `zip`
/// crate has no async surface. We offload the entire block to
/// `spawn_blocking` so we don't block the runtime threadpool on the
/// archive write.
async fn create_support_bundle(
    doctor_info: DoctorInfo,
    bundle_path: PathBuf,
    context: &DoctorContext,
) -> Result<()> {
    info!("Creating support bundle at: {}", bundle_path.display());

    let workspace_folder: Option<PathBuf> = context.workspace_folder.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::Write;

        let file = std::fs::File::create(&bundle_path).map_err(|e| {
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
        let doctor_json = serde_json::to_string_pretty(&doctor_info).map_err(|e| {
            DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to serialize doctor info: {}", e),
            })
        })?;
        // Redact, as `environment.json` below already does. A bundle exists to
        // be sent to someone else, and this document now carries a runtime's
        // verbatim stderr in `skipped_probes[].reason` — which routinely names
        // `DOCKER_HOST`, proxy URLs and registry-auth detail.
        let doctor_json = crate::redaction::redact_if_enabled(
            &doctor_json,
            &crate::redaction::RedactionConfig::default(),
        );
        zip.write_all(doctor_json.as_bytes()).map_err(|e| {
            DeaconError::Internal(crate::errors::InternalError::Generic {
                message: format!("Failed to write doctor.json: {}", e),
            })
        })?;

        // Add sanitized config files if they exist
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let base_path = workspace_folder.as_ref().unwrap_or(&current_dir);

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
        let runtime_json =
            serde_json::to_string_pretty(&doctor_info.runtime_config).map_err(|e| {
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
    })
    .await
    .map_err(|e| {
        DeaconError::Internal(crate::errors::InternalError::Generic {
            message: format!("create_support_bundle join error: {}", e),
        })
    })??;

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

/// Render skipped probes for the human-readable report.
///
/// The text mode's counterpart to the `skipped_probes` array in `--json`: the
/// two modes must agree that a probe was skipped and why, so this is the single
/// definition of the text shape and the printer emits exactly what it returns.
fn skipped_probe_lines(skipped: &[SkippedProbe]) -> Vec<String> {
    skipped
        .iter()
        .map(|skip| format!("  Probe {}: {} ({})", skip.probe, skip.status, skip.reason))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A realistic `docker info --format json` payload, trimmed to the keys the
    /// summary reads plus a few it must ignore.
    const INFO_JSON: &str = r#"{
        "Containers": 81,
        "ContainersRunning": 79,
        "ContainersPaused": 1,
        "ContainersStopped": 1,
        "Images": 4068,
        "ServerVersion": "29.6.2-1",
        "Driver": "overlayfs",
        "RegistryConfig": {"IndexConfigs": {}},
        "HttpProxy": "http://proxy.example:3128"
    }"#;

    #[test]
    fn test_parse_info_summary_reports_what_the_daemon_said() {
        let summary = parse_info_summary(INFO_JSON.as_bytes()).expect("info json should parse");

        // The point of the change behind #507: these are the daemon's numbers,
        // not the hardcoded zeros the `system df` probe used to fabricate.
        assert_eq!(summary.containers_running, Some(79));
        assert_eq!(summary.containers_paused, Some(1));
        assert_eq!(summary.containers_stopped, Some(1));
        assert_eq!(summary.images, Some(4068));
        assert_eq!(summary.server_version.as_deref(), Some("29.6.2-1"));
        assert_eq!(summary.storage_driver.as_deref(), Some("overlayfs"));
    }

    #[test]
    fn test_parse_info_summary_rejects_non_json() {
        assert!(parse_info_summary(b"Cannot connect to the Docker daemon").is_err());
    }

    /// A well-formed JSON document carrying none of the fields — podman's
    /// `info`, which nests its counters under `store`/`host` — must be a
    /// recorded failure, not a summary of nothings that renders as zeros.
    #[test]
    fn test_parse_info_summary_rejects_unrecognized_shape() {
        let podman_shaped = br#"{
            "host": {"arch": "amd64"},
            "store": {"graphDriverName": "overlay",
                      "imageStore": {"number": 12},
                      "containerStore": {"number": 3, "running": 2}},
            "version": {"Version": "5.0.0"}
        }"#;

        let err = parse_info_summary(podman_shaped)
            .expect_err("a document with no recognized fields must not parse to all-None");
        assert!(
            err.to_string().contains("no recognized fields"),
            "got: {err}"
        );
    }

    /// A partially-populated document is still usable — the fields are
    /// genuinely optional, and `None` is reported as absent rather than zero.
    #[test]
    fn test_parse_info_summary_accepts_partial() {
        let summary = parse_info_summary(br#"{"Images": 7}"#).expect("a known field should parse");
        assert_eq!(summary.images, Some(7));
        assert_eq!(summary.containers_running, None);
    }

    /// Only an absent binary means "not installed". Every other I/O condition
    /// is a probe that FAILED, and must be reported rather than reclassified
    /// into `installed: false` with no reason attached.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_unexecutable_binary_is_not_launched() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let script = dir.path().join("wedged-runtime");
        fs::write(&script, "#!/bin/sh\nexit 0\n").expect("write");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).expect("chmod");

        let probed = probe_stdout(
            "runtime_version",
            &script.to_string_lossy(),
            &["--version"],
            Duration::from_secs(5),
        )
        .await;

        assert!(
            matches!(probed, Probed::NotLaunched(_)),
            "a non-executable runtime binary is 'not installed', not a failed probe"
        );
    }

    /// Stderr in a reason is bounded — it travels into `--json`, the text
    /// report and the support bundle.
    #[test]
    fn test_stderr_summary_is_bounded() {
        let noisy = "x".repeat(10_000);
        let summarized = summarize_stderr(noisy.as_bytes());

        assert!(
            summarized.chars().count() <= 513,
            "reason stderr must be capped, got {} chars",
            summarized.chars().count()
        );
        assert!(summarized.ends_with('…'));
    }

    #[test]
    fn test_stderr_summary_passes_short_output_through() {
        assert_eq!(
            summarize_stderr(b"  Cannot connect to the Docker daemon\n"),
            "Cannot connect to the Docker daemon"
        );
    }

    /// The bound must actually bound: a probe that outlives it returns promptly
    /// rather than waiting for the child.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_bounded_probe_times_out_instead_of_hanging() {
        let started = std::time::Instant::now();
        let outcome = run_bounded_probe("sleep", &["30"], Duration::from_millis(200))
            .await
            .expect("spawning `sleep` should succeed");

        assert!(matches!(outcome, ProbeOutcome::TimedOut));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the bound did not take effect: probe took {:?}",
            started.elapsed()
        );
    }

    /// A timed-out probe is killed AND reaped. Without the reap, every slow
    /// `doctor` run would leave a zombie behind.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_timed_out_probe_leaves_no_zombie() {
        let outcome = run_bounded_probe("sleep", &["30"], Duration::from_millis(200))
            .await
            .expect("spawning `sleep` should succeed");
        assert!(matches!(outcome, ProbeOutcome::TimedOut));

        let own_pid = std::process::id();
        let mut zombies = Vec::new();
        for entry in fs::read_dir("/proc")
            .expect("/proc should be readable")
            .flatten()
        {
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            // `stat` is "pid (comm) state ppid …"; comm can contain spaces and
            // parens, so split after the final ')'.
            let Some(rest) = stat.rsplit_once(')').map(|(_, rest)| rest) else {
                continue;
            };
            let fields: Vec<&str> = rest.split_whitespace().collect();
            let (Some(state), Some(ppid)) = (fields.first(), fields.get(1)) else {
                continue;
            };
            if *state == "Z" && ppid.parse::<u32>() == Ok(own_pid) {
                zombies.push(entry.file_name().to_string_lossy().to_string());
            }
        }

        assert!(
            zombies.is_empty(),
            "timed-out probe was not reaped; zombie children: {:?}",
            zombies
        );
    }

    /// A timed-out probe becomes a reported skip naming the probe and the bound.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_timed_out_probe_is_reported_as_skipped() {
        let probed =
            probe_stdout("docker_info", "sleep", &["30"], Duration::from_millis(200)).await;

        let Probed::Skipped(skip) = probed else {
            panic!("a probe that outran its bound must be reported as skipped");
        };
        assert_eq!(skip.probe, "docker_info");
        assert_eq!(skip.status, "skipped");
        assert!(
            skip.reason.contains("exceeded the 200ms probe timeout"),
            "the reason must state the bound, got: {}",
            skip.reason
        );
    }

    /// A non-zero exit is a skip too — reported, not silently swallowed.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_failing_probe_is_reported_as_skipped() {
        let probed = probe_stdout("docker_info", "false", &[], Duration::from_secs(5)).await;

        let Probed::Skipped(skip) = probed else {
            panic!("a probe exiting non-zero must be reported as skipped");
        };
        assert_eq!(skip.probe, "docker_info");
        assert!(skip.reason.contains("exited with"), "got: {}", skip.reason);
    }

    /// An absent binary is distinguishable from a slow one — that distinction is
    /// what lets `collect_docker_info` report `installed: false`.
    #[tokio::test]
    async fn test_absent_binary_is_not_launched() {
        let probed = probe_stdout(
            "runtime_version",
            "deacon-no-such-runtime-binary",
            &["--version"],
            Duration::from_secs(5),
        )
        .await;

        assert!(matches!(probed, Probed::NotLaunched(_)));
    }

    /// Both output modes must carry the skip. JSON mode: the array is present
    /// with the self-describing entry shape.
    #[test]
    fn test_json_mode_carries_skipped_probe_shape() {
        let diagnostics = DockerDiagnostics {
            installed: true,
            version: Some("Docker version 29.6.2".to_string()),
            daemon_running: true,
            info_summary: None,
            skipped_probes: vec![SkippedProbe::new(
                "docker_info",
                "`docker info --format json` exceeded the 10s probe timeout",
            )],
        };

        let json = serde_json::to_value(&diagnostics).expect("diagnostics should serialize");
        let entry = &json["skipped_probes"][0];
        assert_eq!(entry["probe"], "docker_info");
        assert_eq!(entry["status"], "skipped");
        assert_eq!(
            entry["reason"],
            "`docker info --format json` exceeded the 10s probe timeout"
        );
        // A skip is never a fabricated value.
        assert!(json["info_summary"].is_null());
    }

    /// …and nothing is claimed when nothing was skipped.
    #[test]
    fn test_json_mode_omits_empty_skipped_probes() {
        let diagnostics = DockerDiagnostics {
            installed: true,
            version: None,
            daemon_running: true,
            info_summary: None,
            skipped_probes: Vec::new(),
        };

        let json = serde_json::to_value(&diagnostics).expect("diagnostics should serialize");
        assert!(json.get("skipped_probes").is_none());
    }

    /// Text mode: the same fact, in the human report.
    #[test]
    fn test_text_mode_carries_skipped_probe_shape() {
        let lines = skipped_probe_lines(&[SkippedProbe::new(
            "docker_info",
            "`docker info --format json` exceeded the 10s probe timeout",
        )]);

        assert_eq!(
            lines,
            vec![
                "  Probe docker_info: skipped (`docker info --format json` exceeded the 10s probe timeout)"
                    .to_string()
            ]
        );
    }

    #[test]
    fn test_text_mode_says_nothing_when_nothing_skipped() {
        assert!(skipped_probe_lines(&[]).is_empty());
    }

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
