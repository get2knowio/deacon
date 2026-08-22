//! Docker Compose integration
//!
//! This module handles Docker Compose-based development containers,
//! including service management, project detection, and container lifecycle.

use crate::config::DevContainerConfig;
use crate::errors::{ConfigError, DockerError, Result};
use crate::security::SecurityOptions;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, instrument, warn};

/// Docker Compose project information
#[derive(Debug, Clone)]
pub struct ComposeProject {
    /// Project name (derived from directory name or compose project name)
    pub name: String,
    /// Base directory containing compose files
    pub base_path: PathBuf,
    /// Compose files in order
    pub compose_files: Vec<PathBuf>,
    /// Primary service name
    pub service: String,
    /// Additional services to run
    pub run_services: Vec<String>,
    /// Environment files to pass to docker compose
    pub env_files: Vec<PathBuf>,
    /// Additional mounts to apply to the primary service
    /// Includes workspace mounts (with optional consistency) and CLI --mount flags
    pub additional_mounts: Vec<ComposeMount>,
    /// Profiles to activate for this project
    /// Automatically derived from runServices profiles
    pub profiles: Vec<String>,
    /// Additional environment variables to inject into primary service
    pub additional_env: IndexMap<String, String>,
    /// External volume names that must remain referenced (not replaced by injection)
    /// Per spec: these volumes should surface compose errors if missing, not bind fallback
    pub external_volumes: Vec<String>,
    /// Whether the primary service's command should be overridden with a keep-alive
    /// command so the container stays running through the full lifecycle.
    /// `None` follows the spec default (treated as true). `Some(false)` runs the
    /// service's natural command.
    pub override_command: Option<bool>,
    /// When `Some`, replaces the primary service's `image:` in the injection override.
    /// Set by the compose-features pipeline (bead 14a) after building a feature-extended
    /// image, so the subsequent `docker compose up` runs the extended image instead of
    /// the original `image:` declared in the compose file.
    pub service_image_override: Option<String>,
    /// Deacon-applied container labels — emitted into the injection
    /// overlay under each service's `labels:` block so every container
    /// in the project (primary + runServices) carries the spec-mandated
    /// `devcontainer.local_folder`, `devcontainer.config_file`, etc.
    /// Per #100. Populated by the up flow from
    /// `ContainerIdentity::labels()`.
    pub deacon_labels: IndexMap<String, String>,
}

/// Mount specification for Docker Compose volumes
///
/// Used to inject additional volume mounts into Compose services during
/// container startup. Supports workspace mounts with consistency options.
#[derive(Debug, Clone)]
pub struct ComposeMount {
    /// Mount type (bind, volume or tmpfs)
    pub mount_type: String,
    /// Source path or volume name. Empty when the mount has no source: a tmpfs,
    /// or a `volume` ANONYMOUS volume (#617).
    pub source: String,
    /// Target path in container
    pub target: String,
    /// Whether the mount is read-only (adds `:ro` suffix to the volume)
    pub read_only: bool,
    /// Mount consistency option (cached, consistent, delegated)
    /// Only applicable to bind mounts on macOS for performance tuning
    pub consistency: Option<String>,
}

/// Docker Compose service information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeService {
    /// Service name
    pub name: String,
    /// Container ID (if running)
    pub container_id: Option<String>,
    /// Service image
    pub image: Option<String>,
    /// Service state
    pub state: String,
    /// Service status
    pub status: String,
}

/// Docker Compose command builder
#[derive(Debug)]
pub struct ComposeCommand {
    /// Docker binary path
    docker_path: String,
    /// Compose files
    compose_files: Vec<PathBuf>,
    /// Project name
    project_name: Option<String>,
    /// Base directory
    base_path: PathBuf,
    /// Environment files
    env_files: Vec<PathBuf>,
    /// Profiles to activate
    profiles: Vec<String>,
}

impl ComposeCommand {
    /// Create a new compose command builder
    pub fn new(base_path: PathBuf, compose_files: Vec<PathBuf>) -> Self {
        Self {
            docker_path: "docker".to_string(),
            compose_files,
            project_name: None,
            base_path,
            env_files: Vec::new(),
            profiles: Vec::new(),
        }
    }

    /// Set custom docker binary path
    pub fn with_docker_path(mut self, docker_path: String) -> Self {
        self.docker_path = docker_path;
        self
    }

    /// Set project name
    pub fn with_project_name(mut self, project_name: String) -> Self {
        self.project_name = Some(project_name);
        self
    }

    /// Set environment files
    pub fn with_env_files(mut self, env_files: Vec<PathBuf>) -> Self {
        self.env_files = env_files;
        self
    }

    /// Set profiles to activate
    ///
    /// Per FR-005: The up workflow must respect compose profiles
    pub fn with_profiles(mut self, profiles: Vec<String>) -> Self {
        self.profiles = profiles;
        self
    }

    /// Build docker compose command with given arguments.
    ///
    /// Returns a `tokio::process::Command` for async-safe process spawning.
    /// For test inspection (e.g., `get_args()`), use `.as_std().get_args()`.
    pub fn build_command(&self, args: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.docker_path);
        command.arg("compose");

        // Add compose files
        for file in &self.compose_files {
            command.arg("-f").arg(file);
        }

        // Add environment files
        for file in &self.env_files {
            command.arg("--env-file").arg(file);
        }

        // Add project name if specified
        if let Some(ref project_name) = self.project_name {
            command.arg("-p").arg(project_name);
        }

        // Add profiles if specified (per FR-005)
        for profile in &self.profiles {
            command.arg("--profile").arg(profile);
        }

        // Add arguments
        command.args(args);

        // Set working directory
        command.current_dir(&self.base_path);

        command
    }

    /// Execute compose command and return output
    #[instrument(skip(self))]
    pub async fn execute(&self, args: &[&str]) -> Result<String> {
        self.execute_with_stdin(args, None).await
    }

    /// Execute compose command with optional stdin input (e.g., for inline override YAML)
    ///
    /// When `stdin_input` is Some, the command will:
    /// 1. Add `-f -` to read an additional compose file from stdin
    /// 2. Pipe the stdin_input content to the command
    ///
    /// This allows injecting mounts/env without creating temporary override files.
    ///
    /// Uses `tokio::process::Command` for async-safe process spawning per CLAUDE.md:
    /// "Async code MUST avoid blocking calls (std::process::Command::output, blocking file IO)."
    #[instrument(skip(self, stdin_input))]
    pub async fn execute_with_stdin(
        &self,
        args: &[&str],
        stdin_input: Option<&str>,
    ) -> Result<String> {
        use std::process::Stdio;
        use tokio::io::AsyncWriteExt;

        let mut command = self.build_command(args);

        // Add stdin file source if we have input
        if stdin_input.is_some() {
            // Insert -f - before the subcommand args to read from stdin
            // Note: We need to rebuild command to insert at the right position
            let mut new_command = tokio::process::Command::new(&self.docker_path);
            new_command.arg("compose");

            // Add compose files
            for file in &self.compose_files {
                new_command.arg("-f").arg(file);
            }

            // Add stdin as additional compose file
            new_command.arg("-f").arg("-");

            // Add environment files
            for file in &self.env_files {
                new_command.arg("--env-file").arg(file);
            }

            // Add project name if specified
            if let Some(ref project_name) = self.project_name {
                new_command.arg("-p").arg(project_name);
            }

            // Add profiles if specified
            for profile in &self.profiles {
                new_command.arg("--profile").arg(profile);
            }

            // Add arguments
            new_command.args(args);

            // Set working directory
            new_command.current_dir(&self.base_path);

            command = new_command;
        }

        debug!(
            "Executing docker compose command: {} compose {} {} {}",
            self.docker_path,
            self.compose_files
                .iter()
                .map(|f| format!("-f {}", f.display()))
                .collect::<Vec<_>>()
                .join(" "),
            if stdin_input.is_some() {
                "-f - (stdin)"
            } else {
                ""
            },
            args.join(" ")
        );

        // Set up stdin/stdout/stderr
        if stdin_input.is_some() {
            command.stdin(Stdio::piped());
        }
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| {
            DockerError::CLIError(format!("Failed to execute docker compose command: {}", e))
        })?;

        // Write stdin input if provided
        if let Some(input) = stdin_input {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes()).await.map_err(|e| {
                    DockerError::CLIError(format!("Failed to write stdin to docker compose: {}", e))
                })?;
                // Drop stdin to signal EOF
            }
        }

        let output = child.wait_with_output().await.map_err(|e| {
            DockerError::CLIError(format!("Failed to wait for docker compose command: {}", e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerError::CLIError(format!(
                "Docker compose command failed: {}",
                stderr
            ))
            .into());
        }

        let stdout = String::from_utf8(output.stdout).map_err(|e| {
            DockerError::CLIError(format!("Invalid UTF-8 in docker compose output: {}", e))
        })?;

        Ok(stdout)
    }

    /// Execute a compose command, streaming its stderr (where `docker compose
    /// build` writes BuildKit progress) line-by-line to `sink` as it arrives.
    ///
    /// Mirrors [`Self::execute`] for the no-stdin case but reuses the shared
    /// build-output streaming path ([`crate::docker_retry::stream_captured_child`])
    /// so `deacon build`'s compose-service build renders like the other build
    /// paths. Returns captured stdout on success.
    #[instrument(skip(self, sink))]
    pub async fn execute_streamed(
        &self,
        args: &[&str],
        sink: Option<&dyn crate::docker_retry::BuildLineSink>,
    ) -> Result<String> {
        use std::process::Stdio;

        if let Some(sink) = sink {
            sink.reset();
        }

        let mut command = self.build_command(args);
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let child = command.spawn().map_err(|e| {
            DockerError::CLIError(format!("Failed to execute docker compose command: {}", e))
        })?;

        let output = crate::docker_retry::stream_captured_child(child, sink)
            .await
            .map_err(|e| DockerError::CLIError(format!("docker compose I/O error: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DockerError::CLIError(format!(
                "Docker compose command failed: {}",
                stderr
            ))
            .into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Start services
    #[instrument(skip(self))]
    pub async fn up(
        &self,
        services: &[String],
        detached: bool,
        gpu_mode: crate::gpu::GpuMode,
    ) -> Result<String> {
        self.up_with_injection(services, detached, gpu_mode, None)
            .await
    }

    /// Start services with optional inline injection override.
    ///
    /// Per FR-001/FR-002: This method allows injecting mounts and environment
    /// variables into the primary service without creating temporary override files.
    ///
    /// The injection_override YAML is piped to docker compose via stdin using `-f -`.
    #[instrument(skip(self, injection_override))]
    pub async fn up_with_injection(
        &self,
        services: &[String],
        detached: bool,
        gpu_mode: crate::gpu::GpuMode,
        injection_override: Option<&str>,
    ) -> Result<String> {
        let mut args = vec!["up"];
        if detached {
            args.push("-d");
        }

        // Add GPU flags based on GPU mode
        // Note: GpuMode::Detect is resolved to All or None by the caller (e.g., in up.rs)
        match gpu_mode {
            crate::gpu::GpuMode::All => {
                args.push("--gpus");
                args.push("all");
                debug!("Added --gpus all flag for compose up (GpuMode::All)");
            }
            crate::gpu::GpuMode::None => {
                // Silent no-op per FR-006: no GPU requests, no GPU-related logs
            }
            crate::gpu::GpuMode::Detect => {
                // This should never happen - Detect mode should be resolved upstream
                warn!(
                    "GpuMode::Detect passed to compose.rs - this indicates a bug. Skipping GPU flags."
                );
            }
        }

        args.extend(services.iter().map(|s| s.as_str()));
        self.execute_with_stdin(&args, injection_override).await
    }

    /// Warn about security options that cannot be applied dynamically in Docker Compose
    pub fn warn_security_options_for_compose(config: &DevContainerConfig) {
        // TODO: In the future, this should accept features parameter to check feature-derived options too

        // For now, only check config options. Features would require access to resolved features.
        let security = SecurityOptions {
            privileged: config.privileged.unwrap_or(false),
            cap_add: SecurityOptions::normalize_capabilities(config.cap_add()),
            security_opt: SecurityOptions::normalize_security_opts(config.security_opt()),
            conflicts: Vec::new(),
        };

        if security.has_security_options() {
            warn!("Security options detected in configuration for Docker Compose:");

            if security.privileged {
                warn!("  - privileged mode must be defined in docker-compose.yml file");
            }

            if !security.cap_add.is_empty() {
                warn!(
                    "  - capabilities ({:?}) must be defined in docker-compose.yml file",
                    security.cap_add
                );
            }

            if !security.security_opt.is_empty() {
                warn!(
                    "  - security options ({:?}) must be defined in docker-compose.yml file",
                    security.security_opt
                );
            }

            warn!(
                "Security options from devcontainer.json cannot be applied dynamically to Docker Compose services."
            );
            warn!("Please add these options to your docker-compose.yml service definition.");
        }

        if !config.run_args().is_empty() {
            warn!(
                "runArgs ({:?}) are ignored in Docker Compose mode. These flags only apply to single-container (docker run) workflows.",
                config.run_args()
            );
        }
    }

    /// Stop and remove containers
    #[instrument(skip(self))]
    pub async fn down(&self) -> Result<String> {
        self.execute(&["down"]).await
    }

    /// Stop containers without removing them (`docker compose stop`).
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<String> {
        self.execute(&["stop"]).await
    }

    /// Stop and remove containers with additional flags
    #[instrument(skip(self))]
    pub async fn down_with_flags(&self, flags: &[&str]) -> Result<String> {
        let mut args = vec!["down"];
        args.extend(flags);
        self.execute(&args).await
    }

    /// List services with their status
    #[instrument(skip(self))]
    pub async fn ps(&self) -> Result<Vec<ComposeService>> {
        let output = self.execute(&["ps", "--format", "json"]).await?;
        self.parse_ps_output(&output)
    }

    /// Parse docker compose ps JSON output
    fn parse_ps_output(&self, json_output: &str) -> Result<Vec<ComposeService>> {
        if json_output.trim().is_empty() {
            return Ok(Vec::new());
        }

        let services: Vec<serde_json::Value> = json_output
            .trim()
            .lines()
            .map(serde_json::from_str)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| {
                DockerError::CLIError(format!("Failed to parse compose ps JSON: {}", e))
            })?;

        let mut result = Vec::new();
        for service in services {
            let compose_service = ComposeService {
                name: service
                    .get("Service")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                container_id: service
                    .get("ID")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                image: service
                    .get("Image")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                state: service
                    .get("State")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                status: service
                    .get("Status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
            };
            result.push(compose_service);
        }

        Ok(result)
    }

    /// Extract external volumes from compose configuration.
    ///
    /// Uses `docker compose config --format json` to get the merged configuration
    /// and extracts volume names that are marked as external.
    ///
    /// Per the spec (data-model.md): External volumes are those with `external: true`
    /// or `external: { name: "..." }` in the compose configuration. These must remain
    /// intact and not be replaced by injection logic.
    ///
    /// # Returns
    ///
    /// A list of external volume names. Returns an empty list if no volumes are defined
    /// or if none are marked as external.
    ///
    /// # Errors
    ///
    /// Returns an error if the docker compose config command fails to execute or
    /// produces invalid JSON output.
    #[instrument(skip(self))]
    pub async fn extract_external_volumes(&self) -> Result<Vec<String>> {
        let output = self.execute(&["config", "--format", "json"]).await?;
        parse_external_volumes_from_config(&output)
    }

    /// Ask Compose which project name it resolves this file set to.
    ///
    /// A compose document's top-level `name:` is a TEMPLATE — Compose interpolates it
    /// from the process environment and the project's `.env` before using it, and then
    /// normalizes the result (lowercase, illegal characters dropped). `name: ${CUSTOM_NAME}`
    /// is therefore not a project name at all until Compose has evaluated it, and no
    /// reader of the raw document can produce the right answer without reimplementing
    /// Compose's interpolation grammar (`${VAR:-default}`, `${VAR:?err}`, `$VAR`, `$$`).
    ///
    /// So we ask Compose, exactly as the reference CLI does (its `Rp` project resolver
    /// reads `name` off `docker compose config`'s output). This is a CLIENT-SIDE call:
    /// `docker compose config` needs the `docker` binary and its compose plugin, both of
    /// which every compose flow already requires, but NOT a reachable daemon — measured
    /// with `DOCKER_HOST` pointed at a nonexistent socket, where `config` still exits 0.
    ///
    /// The returned name is whatever Compose resolves, which is an authored `name:` when
    /// the document has one and the project-directory basename when it does not. This
    /// method cannot tell those apart, so callers MUST establish authorship first (see
    /// `derive_project_name`) — adopting Compose's directory default would silently
    /// overrule deacon's own namespaced derivation (#265/#564).
    ///
    /// # Errors
    ///
    /// Returns an error when `docker compose config` fails (no `docker` binary, an
    /// unparseable document, or a `name:` that interpolates to empty — Compose itself
    /// rejects that with "project name must not be empty") or when its JSON carries no
    /// usable `name`.
    #[instrument(skip(self))]
    pub async fn extract_project_name(&self) -> Result<String> {
        let output = self.execute(&["config", "--format", "json"]).await?;
        parse_project_name_from_config(&output)
    }

    /// Extract profiles for target services from compose configuration.
    ///
    /// Uses `docker compose config --format json` to get the merged configuration
    /// and extracts the `profiles` arrays for the specified target services.
    ///
    /// Per spec §7: `docker compose config` returns the full resolved config
    /// including all services regardless of active profiles.
    #[instrument(skip(self))]
    pub async fn extract_service_profiles(
        &self,
        target_services: &[String],
    ) -> Result<Vec<String>> {
        let output = self.execute(&["config", "--format", "json"]).await?;
        parse_service_profiles_from_config(&output, target_services)
    }

    /// Inspect a compose service's "shape" — whether it provides an `image:`,
    /// a `build:` block, or neither — to decide how features should be installed.
    ///
    /// Per bead 14a, only the `image:` shape is supported today; `build:` returns
    /// `ServiceShape::Build` so callers can emit a clear "deferred to bead 14b"
    /// error without crashing.
    #[instrument(skip(self))]
    pub async fn extract_service_shape(&self, service_name: &str) -> Result<ServiceShape> {
        let output = self.execute(&["config", "--format", "json"]).await?;
        parse_service_shape_from_config(&output, service_name)
    }
}

/// The image name Compose gives a built service when the service authors no
/// `image:` of its own.
///
/// Compose v2 joins the project and service names with a hyphen (v1 used an
/// underscore); deacon requires v2, so the hyphen form is the only one produced.
pub fn default_service_image_name(project_name: &str, service: &str) -> String {
    format!("{}-{}", project_name, service)
}

/// Shape of a compose service relevant to the features-install pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceShape {
    /// Service declares an `image:` to pull. The String is the resolved image
    /// reference from `docker compose config`.
    Image(String),
    /// Service declares a `build:` block. Field values are best-effort; future
    /// bead 14b will parse the referenced Dockerfile.
    Build {
        context: Option<String>,
        dockerfile: Option<String>,
        target: Option<String>,
    },
    /// Service exists but has neither `image:` nor `build:` — this is invalid for
    /// our purposes (compose itself would also reject it for `up`).
    Neither,
    /// The named service was not found in the resolved compose config.
    NotFound,
}

/// Parse external volumes from docker compose config JSON output.
///
/// This function extracts volume names that are marked as external from the
/// compose configuration. It handles both formats:
/// - `external: true` - Simple boolean form
/// - `external: { name: "actual-volume-name" }` - Object form with explicit name
///
/// # Arguments
///
/// * `json_output` - The JSON output from `docker compose config --format json`
///
/// # Returns
///
/// A list of external volume names. For the object form with `name`, the actual
/// external volume name is used. For the simple boolean form, the key name from
/// the volumes section is used.
fn parse_external_volumes_from_config(json_output: &str) -> Result<Vec<String>> {
    if json_output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let config: serde_json::Value = serde_json::from_str(json_output).map_err(|e| {
        DockerError::CLIError(format!("Failed to parse compose config JSON: {}", e))
    })?;

    let mut external_volumes = Vec::new();

    if let Some(volumes) = config.get("volumes").and_then(|v| v.as_object()) {
        for (volume_name, volume_config) in volumes {
            // Check if the volume is marked as external
            if let Some(external) = volume_config.get("external") {
                if external.as_bool() == Some(true) {
                    // Simple form: external: true
                    external_volumes.push(volume_name.clone());
                    debug!("Found external volume (simple form): {}", volume_name);
                } else if external.is_object() {
                    // Object form: external: { name: "..." }
                    // In this case, the external volume name might differ from the key
                    if let Some(external_name) = external.get("name").and_then(|n| n.as_str()) {
                        external_volumes.push(external_name.to_string());
                        debug!(
                            "Found external volume (object form): {} -> {}",
                            volume_name, external_name
                        );
                    } else {
                        // external is an object but no name specified, use the key name
                        external_volumes.push(volume_name.clone());
                        debug!(
                            "Found external volume (object form, no name): {}",
                            volume_name
                        );
                    }
                }
            }
        }
    }

    debug!(
        "Extracted {} external volumes from compose config",
        external_volumes.len()
    );
    Ok(external_volumes)
}

/// Read the resolved project name out of `docker compose config --format json` output.
///
/// Pure counterpart of [`ComposeCommand::extract_project_name`]; the subprocess lives
/// there and the parsing lives here so the shape of Compose's answer is unit-testable
/// without a `docker` binary.
///
/// Compose always emits a `name` on a successful `config`, so an absent or empty one is
/// an error rather than a "no name authored" signal — a `name:` that interpolates to
/// empty makes `config` itself fail with "project name must not be empty" long before
/// we get here.
fn parse_project_name_from_config(json_output: &str) -> Result<String> {
    let config: serde_json::Value = serde_json::from_str(json_output).map_err(|e| {
        DockerError::CLIError(format!("Failed to parse compose config JSON: {}", e))
    })?;

    config
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            DockerError::CLIError("docker compose config reported no project name".to_string())
                .into()
        })
}

/// Parse service profiles from docker compose config JSON output.
///
/// Extracts the `profiles` arrays from services that match the target
/// service names. Returns a deduplicated list of profile names preserving
/// first-seen order.
///
/// Uses the same `docker compose config --format json` output pattern as
/// `parse_external_volumes_from_config`, avoiding a separate YAML parsing
/// dependency.
///
/// # Arguments
///
/// * `json_output` - The JSON output from `docker compose config --format json`
/// * `target_services` - Service names to collect profiles for (primary + runServices)
fn parse_service_profiles_from_config(
    json_output: &str,
    target_services: &[String],
) -> Result<Vec<String>> {
    if json_output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let config: serde_json::Value = serde_json::from_str(json_output).map_err(|e| {
        DockerError::CLIError(format!("Failed to parse compose config JSON: {}", e))
    })?;

    let mut seen = std::collections::HashSet::new();
    let mut profiles = Vec::new();

    if let Some(services) = config.get("services").and_then(|s| s.as_object()) {
        for target in target_services {
            if let Some(service_def) = services.get(target) {
                if let Some(service_profiles) =
                    service_def.get("profiles").and_then(|p| p.as_array())
                {
                    for profile_val in service_profiles {
                        if let Some(profile_name) = profile_val.as_str() {
                            if seen.insert(profile_name.to_string()) {
                                profiles.push(profile_name.to_string());
                                debug!("Found profile '{}' for service '{}'", profile_name, target);
                            }
                        }
                    }
                }
            }
        }
    }

    debug!(
        "Extracted {} profiles from compose config for services {:?}",
        profiles.len(),
        target_services
    );
    Ok(profiles)
}

/// Parse a single service's shape (image: vs build:) from the resolved compose config JSON.
///
/// Decision rule:
/// - `image:` present (string) → `ServiceShape::Image`. Per compose semantics, even when both
///   `image:` and `build:` are set, `image:` is what `compose pull` would use as the published
///   tag; for feature installation we always extend the image side.
/// - `build:` present (object or string) → `ServiceShape::Build` with best-effort field extraction.
///   `build:` may be a shorthand string (the build context) per compose schema, in which case
///   we capture it as context and leave dockerfile/target unset.
/// - Neither → `ServiceShape::Neither`.
/// - Service key missing → `ServiceShape::NotFound`.
fn parse_service_shape_from_config(json_output: &str, service_name: &str) -> Result<ServiceShape> {
    if json_output.trim().is_empty() {
        return Ok(ServiceShape::NotFound);
    }

    let config: serde_json::Value = serde_json::from_str(json_output).map_err(|e| {
        DockerError::CLIError(format!("Failed to parse compose config JSON: {}", e))
    })?;

    let Some(service) = config
        .get("services")
        .and_then(|s| s.as_object())
        .and_then(|s| s.get(service_name))
    else {
        return Ok(ServiceShape::NotFound);
    };

    if let Some(image) = service.get("image").and_then(|v| v.as_str()) {
        return Ok(ServiceShape::Image(image.to_string()));
    }

    if let Some(build) = service.get("build") {
        match build {
            serde_json::Value::String(context) => {
                return Ok(ServiceShape::Build {
                    context: Some(context.clone()),
                    dockerfile: None,
                    target: None,
                });
            }
            serde_json::Value::Object(obj) => {
                return Ok(ServiceShape::Build {
                    context: obj
                        .get("context")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    dockerfile: obj
                        .get("dockerfile")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    target: obj.get("target").and_then(|v| v.as_str()).map(String::from),
                });
            }
            _ => {}
        }
    }

    Ok(ServiceShape::Neither)
}

/// Docker Compose manager
pub struct ComposeManager {
    /// Docker binary path
    docker_path: String,
}

impl ComposeManager {
    /// Create a new compose manager
    pub fn new() -> Self {
        Self {
            docker_path: "docker".to_string(),
        }
    }

    /// Create a new compose manager with custom docker path
    pub fn with_docker_path(docker_path: String) -> Self {
        Self { docker_path }
    }

    /// Create a compose project from configuration.
    ///
    /// `env_files` are the `--env-file` paths the caller will run Compose with. They are
    /// stored on the project AND used while resolving the project name, because a
    /// `--env-file` replaces Compose's default `.env` discovery and therefore changes
    /// what an authored `name: ${VAR}` interpolates to. Taking them as a parameter
    /// rather than letting callers assign the field afterwards is deliberate: `up`,
    /// `exec` and `down` must all land on the SAME project name, and a name resolved
    /// before the env files were attached would not.
    ///
    /// This is `async` because resolving an authored project name asks Compose to
    /// interpolate it (see [`derive_project_name`]). Configurations that author no
    /// `name:` never spawn a subprocess.
    #[instrument(skip(self))]
    pub async fn create_project(
        &self,
        config: &DevContainerConfig,
        base_path: &Path,
        config_dir: &Path,
        env_files: &[PathBuf],
    ) -> Result<ComposeProject> {
        // Check if docker_compose_file is specified
        if config.docker_compose_file.is_none() {
            return Err(ConfigError::Validation {
                message: "Configuration does not specify Docker Compose setup".to_string(),
            }
            .into());
        }

        let compose_files = config.get_compose_files();
        if compose_files.is_empty() {
            return Err(ConfigError::Validation {
                message: "No Docker Compose files specified".to_string(),
            }
            .into());
        }

        let service = config
            .service
            .as_ref()
            .ok_or_else(|| ConfigError::Validation {
                message: "No service specified for compose project".to_string(),
            })?;

        // Resolve compose file paths relative to the directory containing
        // devcontainer.json (`config_dir`), per the containers.dev spec
        // ("relative to the devcontainer.json file") and the reference CLI.
        // For the standard `.devcontainer/docker-compose.yml` layout this is the
        // `.devcontainer` dir, NOT the workspace folder. The project name and
        // working dir still derive from `base_path` (the workspace folder).
        let mut resolved_files = Vec::new();
        for file in &compose_files {
            let file_path = if Path::new(file).is_absolute() {
                PathBuf::from(file)
            } else {
                config_dir.join(file)
            };

            if !file_path.exists() {
                warn!("Compose file does not exist: {}", file_path.display());
            }

            resolved_files.push(file_path);
        }

        // Deacon-namespaced project name, unique per devcontainer (workspace +
        // config hash), so sibling devcontainers in one repo never collide — unless the
        // user authored a name, in which case Compose resolves it.
        // `COMPOSE_PROJECT_NAME` is COMPOSE's environment variable, not a deacon flag, so
        // the CLAUDE.md rule that flag-backed vars must be declared with clap's `env =`
        // does not reach it — there is no `--project-name` flag on deacon to back it, and
        // inventing one would add surface the reference does not have. It is read here,
        // at the single seam where a project name is resolved, and threaded down as a
        // parameter so `derive_project_name` stays testable: `unsafe_code = "deny"` rules
        // out `set_var` in a unit test, and a global read would make the precedence rules
        // in `explicit_compose_project_name` unassertable.
        let process_env_project_name = std::env::var("COMPOSE_PROJECT_NAME").ok();

        let project_name = derive_project_name(
            &self.docker_path,
            base_path,
            config,
            &resolved_files,
            env_files,
            process_env_project_name.as_deref(),
        )
        .await?;

        Ok(ComposeProject {
            name: project_name,
            base_path: base_path.to_path_buf(),
            compose_files: resolved_files,
            service: service.clone(),
            run_services: config.run_services().to_vec(),
            env_files: env_files.to_vec(),
            additional_mounts: Vec::new(), // Will be populated from CLI --mount flags
            profiles: Vec::new(),          // Will be populated from service profiles
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(), // Will be populated via populate_external_volumes()
            override_command: config.override_command,
            service_image_override: None,
            deacon_labels: IndexMap::new(), // Populated by the up flow from ContainerIdentity (#100)
        })
    }

    /// Populate external volumes for a compose project.
    ///
    /// This method uses `docker compose config --format json` to extract external
    /// volume declarations from the compose configuration files. The extracted
    /// volume names are stored in the project's `external_volumes` field.
    ///
    /// Per the spec (data-model.md): External volumes must remain intact and
    /// not be replaced or mutated by injection logic. This method enables
    /// tracking which volumes are external for validation and preservation.
    ///
    /// # Arguments
    ///
    /// * `project` - The compose project to populate external volumes for
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success. The project's `external_volumes` field
    /// is modified in-place.
    ///
    /// # Errors
    ///
    /// Returns an error if the docker compose config command fails to execute.
    /// This may happen if:
    /// - Docker is not available
    /// - The compose files are invalid
    /// - Variable substitution fails
    ///
    /// # Note
    ///
    /// This operation requires Docker to be available and may fail in
    /// environments without Docker. Callers should handle errors gracefully
    /// and potentially continue without external volume information if
    /// Docker is unavailable.
    #[instrument(skip(self))]
    pub async fn populate_external_volumes(&self, project: &mut ComposeProject) -> Result<()> {
        let command = self.get_command(project);
        let external_volumes = command.extract_external_volumes().await?;
        project.external_volumes = external_volumes;
        debug!(
            "Populated {} external volumes for project {}",
            project.external_volumes.len(),
            project.name
        );
        Ok(())
    }

    /// Populate profiles for a compose project from compose configuration.
    ///
    /// Uses `docker compose config --format json` to find which profiles are
    /// associated with the services that need to run (primary service + runServices).
    /// The collected profiles are then set on the project so that all
    /// subsequent compose commands include the appropriate `--profile` flags.
    ///
    /// Per spec §7: The up workflow must resolve compose config including profiles,
    /// and pass `--profile *` for each required profile to `docker compose up -d`.
    ///
    /// # Errors
    ///
    /// Returns an error if the docker compose config command fails.
    /// This requires Docker to be available.
    #[instrument(skip(self))]
    pub async fn populate_profiles(&self, project: &mut ComposeProject) -> Result<()> {
        let target_services = project.get_all_services();
        let command = self.get_command(project);
        let profiles = command.extract_service_profiles(&target_services).await?;
        project.profiles = profiles;
        debug!(
            "Populated {} profiles for project {}: {:?}",
            project.profiles.len(),
            project.name,
            project.profiles
        );
        Ok(())
    }

    /// Get compose command for a project
    ///
    /// Per T005: Threads profiles, env-files, and project naming through all compose invocations
    pub fn get_command(&self, project: &ComposeProject) -> ComposeCommand {
        ComposeCommand::new(project.base_path.clone(), project.compose_files.clone())
            .with_docker_path(self.docker_path.clone())
            .with_project_name(project.name.clone())
            .with_env_files(project.env_files.clone())
            .with_profiles(project.profiles.clone())
    }

    /// Find Compose projects that still hold NAMED VOLUMES for this workspace under a
    /// project name deacon no longer derives (#564).
    ///
    /// Volumes, not containers, are the subject: `stop_superseded_containers` already
    /// stops the container side of a superseded generation, and nothing touches volumes
    /// (they hold data). The query reads Compose's own `com.docker.compose.project` label
    /// off every volume rather than parsing volume NAMES, so it does not care how either
    /// tool spells a resource; [`classify_superseded_projects`] then decides which of
    /// those project names belong to this workspace.
    ///
    /// Best-effort and infallible by design — a daemon that cannot be queried costs the
    /// user a diagnostic, never an `up`. Returns an empty vector in that case.
    #[instrument(skip(self))]
    pub async fn detect_superseded_volume_projects(
        &self,
        current_project: &str,
        workspace_hash: &str,
        workspace_folder: &Path,
    ) -> Vec<SupersededProject> {
        let Some(basename) = workspace_folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
        else {
            return Vec::new();
        };

        let output = tokio::process::Command::new(&self.docker_path)
            .args([
                "volume",
                "ls",
                "--format",
                "{{.Label \"com.docker.compose.project\"}}",
            ])
            .output()
            .await;

        let stdout = match output {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
            Ok(out) => {
                debug!(
                    "Could not list volumes for the superseded-project check: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
                return Vec::new();
            }
            Err(e) => {
                debug!("Could not run `{} volume ls`: {e}", self.docker_path);
                return Vec::new();
            }
        };

        classify_superseded_projects(
            stdout.lines().map(str::trim),
            current_project,
            workspace_hash,
            &basename,
        )
    }

    /// Check if project containers are running
    #[instrument(skip(self))]
    pub async fn is_project_running(&self, project: &ComposeProject) -> Result<bool> {
        let command = self.get_command(project);
        let services = command.ps().await?;

        // Get all services that should be running (primary + run_services)
        let all_services = project.get_all_services();

        // Check if all required services are running
        let running_services: Vec<String> = services
            .iter()
            .filter(|s| s.state == "running")
            .map(|s| s.name.clone())
            .collect();

        let all_running = all_services
            .iter()
            .all(|service| running_services.contains(service));

        debug!(
            "Project {} all services {:?} running: {} (running services: {:?})",
            project.name, all_services, all_running, running_services
        );

        Ok(all_running)
    }

    /// Start compose project
    ///
    /// Per FR-001/FR-002: Injects mounts and env into the primary service
    /// using inline YAML via stdin (no temp files).
    #[instrument(skip(self))]
    pub async fn start_project(
        &self,
        project: &ComposeProject,
        gpu_mode: crate::gpu::GpuMode,
    ) -> Result<()> {
        let command = self.get_command(project);
        // Per spec, `runServices` defaults to all services: an empty selection
        // means we pass no service names to `compose up`, so Compose starts
        // every service. An explicit `runServices` scopes the up to
        // primary ∪ runServices.
        let services = project.up_service_selection();

        debug!(
            "Starting compose project {} with services: {:?} (empty = all), gpu_mode: {:?}",
            project.name, services, gpu_mode
        );

        // Generate injection override if we have mounts or env to inject
        let injection_override = project.generate_injection_override();

        command
            .up_with_injection(&services, true, gpu_mode, injection_override.as_deref())
            .await?;

        debug!("Compose project {} started successfully", project.name);
        Ok(())
    }

    /// Stop compose project
    #[instrument(skip(self))]
    pub async fn stop_project(&self, project: &ComposeProject) -> Result<()> {
        let command = self.get_command(project);

        debug!("Stopping compose project {}", project.name);

        // `docker compose stop` — stop the services but keep the containers so
        // `down` (the default `stopCompose` action, without `--remove`) mirrors
        // single-container `down`: stopped-but-present, resumable on next `up`.
        // Removal is reserved for `down --remove` (-> down_project).
        command.stop().await?;

        debug!("Compose project {} stopped successfully", project.name);
        Ok(())
    }

    /// Stop and remove compose project containers
    #[instrument(skip(self))]
    pub async fn down_project(&self, project: &ComposeProject) -> Result<()> {
        let command = self.get_command(project);

        debug!("Stopping and removing compose project {}", project.name);

        // Use down without --volumes to preserve volumes
        command.down_with_flags(&[]).await?;

        debug!(
            "Compose project {} stopped and removed successfully",
            project.name
        );
        Ok(())
    }

    /// Stop and remove compose project containers including volumes
    #[instrument(skip(self))]
    pub async fn down_project_with_volumes(&self, project: &ComposeProject) -> Result<()> {
        let command = self.get_command(project);

        debug!(
            "Stopping and removing compose project {} with volumes",
            project.name
        );

        // Use down with --volumes to remove named volumes as well
        command.down_with_flags(&["--volumes"]).await?;

        debug!(
            "Compose project {} stopped and removed with volumes successfully",
            project.name
        );
        Ok(())
    }

    /// Build a specific service in a Docker Compose project.
    ///
    /// This method executes `docker compose build <service>` to build the specified
    /// service defined in the project's compose configuration.
    ///
    /// # Arguments
    ///
    /// * `project` - The compose project containing the service
    /// * `service` - Name of the service to build
    ///
    /// # Returns
    ///
    /// Returns the command output on success.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The docker compose command fails to execute
    /// - The service does not exist in the project
    /// - The build process fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deacon_core::compose::{ComposeManager, ComposeProject};
    /// # use indexmap::IndexMap;
    /// # use std::path::PathBuf;
    /// # async fn example() -> anyhow::Result<()> {
    /// let manager = ComposeManager::new();
    /// let project = ComposeProject {
    ///     name: "my-project".to_string(),
    ///     base_path: PathBuf::from("/path/to/project"),
    ///     compose_files: vec![PathBuf::from("docker-compose.yml")],
    ///     service: "web".to_string(),
    ///     run_services: Vec::new(),
    ///     env_files: Vec::new(),
    ///     additional_mounts: Vec::new(),
    ///     profiles: Vec::new(),
    ///     additional_env: IndexMap::new(),
    ///     external_volumes: Vec::new(),
    ///     override_command: Some(false),
    ///     service_image_override: None,
    ///     deacon_labels: IndexMap::new(),
    /// };
    ///
    /// let output = manager.build_service(&project, "web", None).await?;
    /// println!("Build output: {}", output);
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self, sink))]
    pub async fn build_service(
        &self,
        project: &ComposeProject,
        service: &str,
        sink: Option<&dyn crate::docker_retry::BuildLineSink>,
    ) -> Result<String> {
        let command = self.get_command(project);

        debug!(
            "Building compose project {} service {}",
            project.name, service
        );

        let output = command.execute_streamed(&["build", service], sink).await?;

        debug!(
            "Compose project {} service {} built successfully",
            project.name, service
        );
        Ok(output)
    }

    /// Resolve the image reference `docker compose build` tags a service's build
    /// result with.
    ///
    /// Compose names the built image after the service's own `image:` when one is
    /// authored, and otherwise after its `<project>-<service>` default. Callers
    /// that need to hang additional tags off the produced image (`deacon build
    /// --image-name`, #619) must resolve that reference rather than assume the
    /// default, which is why the `image:` case is read back out of
    /// `docker compose config` instead of guessed.
    #[instrument(skip(self))]
    pub async fn resolve_service_image_name(
        &self,
        project: &ComposeProject,
        service: &str,
    ) -> Result<String> {
        let shape = self
            .get_command(project)
            .extract_service_shape(service)
            .await?;
        Ok(match shape {
            ServiceShape::Image(image) => image,
            _ => default_service_image_name(&project.name, service),
        })
    }

    /// Validate that a service exists in a Docker Compose project configuration.
    ///
    /// This method queries the compose configuration to determine if a service
    /// with the given name is defined in the project.
    ///
    /// # Arguments
    ///
    /// * `project` - The compose project to check
    /// * `service` - Name of the service to validate
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if the service exists, `Ok(false)` if it doesn't.
    ///
    /// # Errors
    ///
    /// Returns an error if the docker compose command fails to execute.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use deacon_core::compose::{ComposeManager, ComposeProject};
    /// # use indexmap::IndexMap;
    /// # use std::path::PathBuf;
    /// # async fn example() -> anyhow::Result<()> {
    /// let manager = ComposeManager::new();
    /// let project = ComposeProject {
    ///     name: "my-project".to_string(),
    ///     base_path: PathBuf::from("/path/to/project"),
    ///     compose_files: vec![PathBuf::from("docker-compose.yml")],
    ///     service: "web".to_string(),
    ///     run_services: Vec::new(),
    ///     env_files: Vec::new(),
    ///     additional_mounts: Vec::new(),
    ///     profiles: Vec::new(),
    ///     additional_env: IndexMap::new(),
    ///     external_volumes: Vec::new(),
    ///     override_command: Some(false),
    ///     service_image_override: None,
    ///     deacon_labels: IndexMap::new(),
    /// };
    ///
    /// if manager.validate_service_exists(&project, "web").await? {
    ///     println!("Service 'web' exists in the project");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[instrument(skip(self))]
    pub async fn validate_service_exists(
        &self,
        project: &ComposeProject,
        service: &str,
    ) -> Result<bool> {
        let command = self.get_command(project);

        // Use docker compose config --services to list all available services
        let output = command.execute(&["config", "--services"]).await?;

        let services: Vec<String> = output
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        debug!(
            "Found services in compose project {}: {:?}",
            project.name, services
        );

        Ok(services.contains(&service.to_string()))
    }

    /// Get primary service container ID
    #[instrument(skip(self))]
    pub async fn get_primary_container_id(
        &self,
        project: &ComposeProject,
    ) -> Result<Option<String>> {
        let command = self.get_command(project);
        let services = command.ps().await?;

        let primary_service = services.iter().find(|s| s.name == project.service);

        match primary_service {
            Some(service) => {
                debug!(
                    "Found primary service container: {} -> {:?}",
                    service.name, service.container_id
                );
                Ok(service.container_id.clone())
            }
            None => {
                debug!(
                    "Primary service {} not found in running services",
                    project.service
                );
                Ok(None)
            }
        }
    }

    /// Get container IDs for all services in the project
    #[instrument(skip(self))]
    pub async fn get_all_container_ids(
        &self,
        project: &ComposeProject,
    ) -> Result<std::collections::HashMap<String, String>> {
        let command = self.get_command(project);
        let services = command.ps().await?;

        let mut container_ids = std::collections::HashMap::new();

        for service in services.iter() {
            if let Some(ref container_id) = service.container_id {
                container_ids.insert(service.name.clone(), container_id.clone());
                debug!(
                    "Found service container: {} -> {}",
                    service.name, container_id
                );
            }
        }

        debug!("Found {} service containers total", container_ids.len());
        Ok(container_ids)
    }
}

impl Default for ComposeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ComposeProject {
    /// Get all services to start (primary + run services)
    pub fn get_all_services(&self) -> Vec<String> {
        let mut services = vec![self.service.clone()];
        services.extend(self.run_services.clone());
        services
    }

    /// Service names to pass to `docker compose up`.
    ///
    /// Per the containers.dev spec, `runServices` *"defaults to all services"*.
    /// When `run_services` is empty (unset) we return an empty list so that
    /// Compose brings up **all** services in the project — this mirrors the
    /// reference `@devcontainers/cli`, which omits service arguments to `up`
    /// when `runServices` is undefined. When `run_services` is set we scope to
    /// the explicit selection (primary service ∪ `runServices`) via
    /// [`Self::get_all_services`].
    pub fn up_service_selection(&self) -> Vec<String> {
        if self.run_services.is_empty() {
            Vec::new()
        } else {
            self.get_all_services()
        }
    }

    /// Generate inline compose override YAML for mount/env injection.
    ///
    /// Per FR-001/FR-002: Apply CLI mounts and remote env to the primary service
    /// without creating temporary override files that users need to manage.
    ///
    /// **External Volume Preservation (FR-004, T010)**:
    /// This override only adds volumes to the service definition - it does NOT
    /// define or modify top-level volume declarations. External volumes declared
    /// in the original compose files remain intact, and missing external volumes
    /// will surface as compose errors (not silently replaced with bind mounts).
    ///
    /// Returns None if no mounts, env, or command override are needed.
    #[must_use = "injection override should be passed to compose up"]
    pub fn generate_injection_override(&self) -> Option<String> {
        // Spec default for overrideCommand is true: keep the container alive
        // through the lifecycle so exec and post-create hooks can attach.
        let override_cmd = self.override_command.unwrap_or(true);

        if self.additional_mounts.is_empty()
            && self.additional_env.is_empty()
            && !override_cmd
            && self.service_image_override.is_none()
            && self.deacon_labels.is_empty()
        {
            return None;
        }

        let mut yaml = String::from("services:\n");
        yaml.push_str(&format!("  {}:\n", self.service));

        if let Some(ref image) = self.service_image_override {
            // Quote the image tag so reserved characters (':' is the separator
            // for tags) don't confuse the YAML parser.
            // Same rule as every other scalar here: a hand-rolled `"…"` that
            // escapes only `"` leaves a backslash to be reinterpreted by the YAML
            // parser (#609). Route it through the one escaper. NOT
            // `escape_compose_value` — an image reference is not `$`-doubled.
            yaml.push_str(&format!("    image: {}\n", escape_yaml_value(image)));
        }

        if override_cmd {
            // Mirror the single-container keep-alive used in docker.rs:
            // sleep infinity (GNU coreutils), fall back to tail -f /dev/null
            // for BusyBox/Alpine. Only command is overridden; the image's
            // entrypoint is preserved so multi-stage entrypoints (e.g. tini)
            // still receive our command as args.
            //
            // The `trap` + background + `wait` shape must MATCH docker.rs: without it a
            // foreground `sleep` as PID 1 cannot service SIGTERM, so `docker stop` /
            // `compose down` waits the full 10s grace period and then SIGKILLs (measured
            // at 10,258 ms vs the reference CLI's 215 ms before the fix). Keeping the two
            // paths symmetric is the point — a compose container that took 10s to stop
            // while the single-container path took 245ms would be a silent asymmetry.
            yaml.push_str(
                "    command: [\"/bin/sh\", \"-c\", \"trap \\\"exit 0\\\" TERM INT; \
                 (sleep infinity || tail -f /dev/null) & wait $$!\"]\n",
            );
        }

        if !self.additional_env.is_empty() {
            yaml.push_str("    environment:\n");
            // IndexMap preserves insertion order - no sorting needed
            for (key, value) in &self.additional_env {
                let escaped = escape_compose_value(value);
                yaml.push_str(&format!("      {}: {}\n", key, escaped));
            }
        }

        if !self.additional_mounts.is_empty() {
            yaml.push_str("    volumes:\n");
            for mount in &self.additional_mounts {
                if mount.mount_type == "tmpfs" {
                    yaml.push_str("      - type: tmpfs\n");
                    yaml.push_str(&format!("        target: {}\n", mount.target));
                    if mount.read_only {
                        yaml.push_str("        read_only: true\n");
                    }
                    continue;
                }

                // A `type=volume` mount with no source is an ANONYMOUS volume
                // (#617). The compose SHORT form cannot express it — `- /target`
                // reads as a target-only entry, and adding any option turns the
                // first field back into a source (`- /target:ro` means
                // source=/target, target=ro) — so emit the long form, which says
                // exactly this and nothing else.
                if mount.mount_type == "volume" && mount.source.is_empty() {
                    yaml.push_str("      - type: volume\n");
                    yaml.push_str(&format!("        target: {}\n", mount.target));
                    if mount.read_only {
                        yaml.push_str("        read_only: true\n");
                    }
                    continue;
                }

                let mut mount_str = format!("{}:{}", mount.source, mount.target);
                // Build options suffix: ro and/or consistency
                // Docker Compose short-form: source:target:options
                // Options can be comma-separated: :ro,cached or just :cached
                let mut options = Vec::new();
                if mount.read_only {
                    options.push("ro");
                }
                if let Some(ref consistency) = mount.consistency {
                    options.push(consistency);
                }
                if !options.is_empty() {
                    mount_str.push(':');
                    mount_str.push_str(&options.join(","));
                }
                yaml.push_str(&format!("      - {}\n", mount_str));
            }
        }

        // Per #100: emit deacon labels on the primary service plus every
        // run_services entry, so external tooling (VS Code Dev
        // Containers reconnect, `docker ps --filter`, `deacon exec
        // --id-label`) finds compose-managed containers via the
        // standard `devcontainer.*` keys.
        if !self.deacon_labels.is_empty() {
            yaml.push_str("    labels:\n");
            for (key, value) in &self.deacon_labels {
                let escaped = escape_compose_value(value);
                yaml.push_str(&format!("      {}: {}\n", key, escaped));
            }
            for svc in &self.run_services {
                // Skip the primary service: it already has a full block above
                // (with image/command/labels). `runServices` commonly lists the
                // primary service alongside the others, and emitting it again
                // here produces a duplicate YAML mapping key ("mapping key
                // <svc> already defined") that docker compose rejects.
                if svc == &self.service {
                    continue;
                }
                yaml.push_str(&format!("  {}:\n", svc));
                yaml.push_str("    labels:\n");
                for (key, value) in &self.deacon_labels {
                    let escaped = escape_compose_value(value);
                    yaml.push_str(&format!("      {}: {}\n", key, escaped));
                }
            }
        }

        // Named-volume mounts (e.g. a feature-contributed `type=volume`
        // mount, #272) must be declared under the compose file's top-level
        // `volumes:` key, or `docker compose up` rejects the project with
        // "refers to undefined volume …: invalid compose project". Compose
        // deep-merges `volumes:` across override files, so declaring an
        // already-known volume here as an empty mapping is a harmless no-op
        // (the base file's `external`/driver settings win); for a genuinely
        // new volume, this is what makes the reference valid. This is a
        // top-level (not per-service) YAML key, so it must sit outside the
        // `services:` block built above.
        //
        // An ANONYMOUS volume (`type=volume` with no source, #617) has no name to
        // declare — Compose creates it per-container — so it is skipped here; a
        // blank key would produce an invalid `volumes:` mapping.
        let mut new_volume_names: Vec<&str> = Vec::new();
        for mount in &self.additional_mounts {
            if mount.mount_type == "volume"
                && !mount.source.is_empty()
                && !new_volume_names.contains(&mount.source.as_str())
            {
                new_volume_names.push(&mount.source);
            }
        }
        if !new_volume_names.is_empty() {
            yaml.push_str("volumes:\n");
            for name in &new_volume_names {
                yaml.push_str(&format!("  {}: {{}}\n", name));
            }
        }

        debug!(
            "Generated compose injection override for service '{}': {} env vars, {} mounts, {} labels, {} volumes, override_command={}",
            self.service,
            self.additional_env.len(),
            self.additional_mounts.len(),
            self.deacon_labels.len(),
            new_volume_names.len(),
            override_cmd,
        );

        Some(yaml)
    }

    /// Merge CLI remote environment with existing environment entries.
    ///
    /// Per the spec (FR-002, research.md Decision 3):
    /// - CLI/remote env entries override duplicate keys from env-files/service defaults
    /// - Non-conflicting keys remain untouched
    /// - Returns merged IndexMap with CLI values taking precedence
    ///
    /// # Arguments
    /// * `service_env` - Environment variables from compose service definition
    /// * `env_file_env` - Environment variables from env-files
    /// * `cli_env` - CLI-provided remote environment entries (highest precedence)
    ///
    /// # Returns
    /// Merged IndexMap with CLI precedence: CLI > env-files > service defaults
    pub fn merge_env_with_cli_precedence(
        service_env: &IndexMap<String, String>,
        env_file_env: &IndexMap<String, String>,
        cli_env: &IndexMap<String, String>,
    ) -> IndexMap<String, String> {
        let mut merged = IndexMap::new();

        // Layer 1: Service defaults (lowest precedence)
        for (key, value) in service_env {
            merged.insert(key.clone(), value.clone());
        }

        // Layer 2: Env-file values (override service defaults)
        for (key, value) in env_file_env {
            merged.insert(key.clone(), value.clone());
        }

        // Layer 3: CLI/remote env (highest precedence)
        for (key, value) in cli_env {
            merged.insert(key.clone(), value.clone());
        }

        debug!(
            "Merged env: {} service defaults + {} env-file + {} CLI = {} total",
            service_env.len(),
            env_file_env.len(),
            cli_env.len(),
            merged.len()
        );

        merged
    }

    /// Apply additional mounts and environment to this project.
    ///
    /// This method prepares the project for compose up by:
    /// 1. Setting additional mounts for the primary service
    /// 2. Merging CLI environment with precedence over defaults
    ///
    /// Per the spec, injection targets only the primary service.
    pub fn with_injection(
        mut self,
        additional_mounts: Vec<ComposeMount>,
        cli_env: IndexMap<String, String>,
    ) -> Self {
        self.additional_mounts = additional_mounts;
        self.additional_env = cli_env;
        self
    }
}

/// Escape a value for the generated compose override.
///
/// Docker Compose interpolates `${…}` in every file it reads, so a `$` that must
/// reach Docker verbatim has to be doubled (`$$`) — the same escape the keep-alive
/// `wait $$!` above already spells out by hand. Without it Compose expands an unset
/// variable to the empty string, which is how the `devcontainer.metadata` label
/// recorded `source=/sib` for an authored `source=${localWorkspaceFolder}/sib`
/// (#437, a compose-path-only regression of #373: the label must carry the
/// configuration AS AUTHORED, templates intact, because it travels with the image).
fn escape_compose_value(value: &str) -> String {
    escape_yaml_value(&value.replace('$', "$$"))
}

/// Escape a value into a double-quoted YAML scalar.
///
/// Every value is emitted double-quoted, so every value is escaped. Those two
/// halves must travel together: a double-quoted YAML scalar PROCESSES escape
/// sequences, so quoting a value without escaping it is the one combination that
/// is wrong, and it is what #609 reported. `value with \back slash` tripped no
/// clause of the old `needs_quoting` predicate — no newline, colon, hash, quote
/// or edge space — so it took the branch that quoted but did not escape, and the
/// YAML parser read its `\b` back as U+0008 BACKSPACE. A lone trailing backslash
/// was worse still: it escaped the closing quote and produced a document Compose
/// could not parse at all.
///
/// Escaping unconditionally is a no-op for any value that needs no escaping, so
/// this narrows strictly to the mis-escaped cases and changes nothing else. In
/// particular a value carrying REAL newlines is unaffected — it always tripped
/// the old predicate and was always emitted as `\n`, which the parser turns back
/// into a real newline (deliberately out of scope here; see #480 batch 7).
///
/// Handled:
/// - `\` and `"` — doubled / escaped, or the parser reinterprets them.
/// - Newline, carriage return, tab — the named escapes.
/// - Every other C0 control and DEL — as `\xNN`. YAML forbids these raw inside a
///   double-quoted scalar, so passing them through produces an invalid document.
///
/// Everything else, non-ASCII included, is passed through verbatim: the override
/// file is written as UTF-8.
fn escape_yaml_value(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                escaped.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped.push('"');
    escaped
}

/// Parse .env file and extract COMPOSE_PROJECT_NAME if present.
///
/// Reads a .env file line by line, looking for COMPOSE_PROJECT_NAME=value.
/// Returns the value if found, otherwise None.
///
/// Per Task T020: Support .env project name propagation for compose workflows.
fn parse_env_file_for_project_name(env_file_path: &Path) -> Option<String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    if !env_file_path.exists() {
        return None;
    }

    let file = File::open(env_file_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.ok()?;
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Look for COMPOSE_PROJECT_NAME=value
        if let Some(value) = line.strip_prefix("COMPOSE_PROJECT_NAME=") {
            let value = value.trim();
            // Remove quotes if present
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            let value = value
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
                .unwrap_or(value);

            if !value.is_empty() {
                debug!("Found COMPOSE_PROJECT_NAME in .env: {}", value);
                return Some(value.to_string());
            }
        }
    }

    None
}

/// Detect whether a compose file AUTHORS a top-level `name`, returning the raw line's
/// value.
///
/// This is an AUTHORSHIP detector, not a project-name resolver. The value it returns is
/// the raw document text, which may well be a template (`name: ${CUSTOM_NAME}`) rather
/// than a usable name — `derive_project_name` hands the resolution to Compose itself via
/// [`ComposeCommand::extract_project_name`] and uses this only to decide whether to ask.
///
/// The split exists because Compose's own answer cannot tell the two apart: `docker
/// compose config` reports a `name` whether or not the document authored one, falling
/// back to the project-directory basename. The reference CLI has the same problem and
/// solves it the same way — its `Rp` resolver re-reads the compose files whenever
/// Compose's answer is the literal `devcontainer` (its own directory default) to decide
/// whether a human wrote it. deacon's derived default is namespaced rather than
/// directory-derived (#265/#564), so it has to ask the question for EVERY document
/// instead of one special-cased name.
fn parse_compose_file_for_project_name(compose_file_path: &Path) -> Option<String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    if !compose_file_path.exists() {
        return None;
    }

    let file = File::open(compose_file_path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let line = line.ok()?;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Top-level `name:` is unindented in compose files.
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("name:") {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            let value = value
                .strip_prefix('\'')
                .and_then(|v| v.strip_suffix('\''))
                .unwrap_or(value);

            if !value.is_empty() {
                debug!(
                    "Found compose top-level project name in {}: {}",
                    compose_file_path.display(),
                    value
                );
                return Some(value.to_string());
            }
        }
    }

    None
}

/// The most characters of the workspace-folder stem an auto-derived project name keeps.
///
/// The stem is readability, not identity — the two hashes after it are what make the name
/// unique — so truncating a very long folder name costs nothing but keeps the derived
/// Compose resource names (`<project>-<service>-1`, `<project>_<volume>`) comfortably
/// inside Docker's 255-character object-name limit.
const PROJECT_STEM_MAX_LEN: usize = 32;

/// Reduce a workspace-folder basename to a segment that is legal inside a Docker Compose
/// project name.
///
/// Compose requires a project name to match `[a-z0-9][a-z0-9_-]*`, and the value goes
/// straight into a `--project-name` argument, so this is the ingress filter for a string
/// the user controls (a directory name). Everything outside the allowed set is replaced
/// with `-` rather than deleted — deleting would silently glue unrelated words together —
/// runs of separators collapse, and leading characters that are not alphanumeric are
/// trimmed because the first character has the stricter rule.
///
/// Returns an EMPTY string when nothing survives (`/`, `..`, `---`, a purely non-ASCII
/// name). The caller must then fall back to the hash-only form: a malformed
/// `--project-name` is rejected by `docker compose` outright, which is the failure
/// `bhv-compose-project-name-robust` exists to prevent.
pub fn sanitize_project_stem(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(PROJECT_STEM_MAX_LEN));
    let mut last_was_separator = false;

    for ch in raw.chars() {
        let lowered = ch.to_ascii_lowercase();
        // Anything outside `[a-z0-9_-]` — whitespace, `.`, `/`, `:`, a non-ASCII grapheme
        // — becomes a single separator. The `is_ascii_*` tests are deliberate: a
        // multi-byte char is never legal in a Compose project name, so it is replaced
        // rather than lowercased and passed through.
        let mapped = match lowered {
            c if c.is_ascii_alphanumeric() || c == '_' || c == '-' => c,
            _ => '-',
        };

        let is_separator = mapped == '-' || mapped == '_';
        if is_separator {
            // Collapse runs, and never let a separator open the stem (the first character
            // must be `[a-z0-9]`).
            if last_was_separator || out.is_empty() {
                continue;
            }
        }
        if out.len() >= PROJECT_STEM_MAX_LEN {
            break;
        }
        out.push(mapped);
        last_was_separator = is_separator;
    }

    // A trailing separator is legal but reads badly against the `_<hash>` that follows.
    while out.ends_with('-') || out.ends_with('_') {
        out.pop();
    }
    out
}

/// Find an explicit `COMPOSE_PROJECT_NAME`, consulting the sources in the order the
/// reference CLI does (#580).
///
/// deacon used to look in exactly ONE place — a `.env` in the workspace folder — which is
/// the middle of three. MEASURED at oracle 0.87.0 on a document authoring no `name:`,
/// setting the variable in one place at a time:
///
/// | set in | reference | deacon before |
/// |---|---|---|
/// | the process environment | `env-wins` | derived `deacon_*` |
/// | the workspace folder's `.env` | `from-workspace` | `from-workspace` |
/// | `.devcontainer/.env`, beside the compose file | `from-configdir` | derived `deacon_*` |
/// | both `.env` files | `from-workspace` | `from-workspace` |
/// | the process environment AND the workspace `.env` | `env-wins` | `from-workspace` |
///
/// The last row is why this is an ORDERED search rather than three independent reads: the
/// two sources deacon was missing sit on either side of the one it had, so adding them
/// without the order would trade a missing-source bug for a precedence bug.
///
/// The third source is Compose's own default `.env` discovery. Compose reads a `.env` from
/// the PROJECT DIRECTORY, which — absent `--project-directory`, which deacon never passes —
/// is the directory of the first `-f` file. For the standard layout that is `.devcontainer`,
/// which is also why `fx-upstream-compose-with-name-env-var` can interpolate `${CUSTOM_NAME}`
/// from a `.env` deacon itself never reads. A `--env-file` REPLACES that discovery, so when
/// the caller has env files they are searched instead, last-wins (Compose merges them in
/// order and a later file overrides an earlier one).
///
/// Values are taken verbatim, matching the reference and matching what deacon already did
/// with the workspace `.env`. Compose would normalize a name it resolved itself, but neither
/// CLI normalizes an explicitly declared `COMPOSE_PROJECT_NAME` — it is a user's deliberate
/// override, and Compose rejects an illegal one loudly rather than silently rewriting it.
fn explicit_compose_project_name(
    base_path: &Path,
    compose_files: &[PathBuf],
    env_files: &[PathBuf],
    process_env: Option<&str>,
) -> Option<String> {
    if let Some(name) = process_env.map(str::trim).filter(|name| !name.is_empty()) {
        debug!("Using project name from the COMPOSE_PROJECT_NAME environment variable");
        return Some(name.to_string());
    }

    if let Some(name) = parse_env_file_for_project_name(&base_path.join(".env")) {
        debug!("Using project name from the workspace folder's .env: {name}");
        return Some(name);
    }

    if env_files.is_empty() {
        let project_dir = compose_files.first().and_then(|file| file.parent())?;
        let name = parse_env_file_for_project_name(&project_dir.join(".env"))?;
        debug!("Using project name from the compose project directory's .env: {name}");
        Some(name)
    } else {
        let name = env_files
            .iter()
            .rev()
            .find_map(|file| parse_env_file_for_project_name(file))?;
        debug!("Using project name from an --env-file: {name}");
        Some(name)
    }
}

async fn derive_project_name(
    docker_path: &str,
    base_path: &Path,
    config: &DevContainerConfig,
    compose_files: &[PathBuf],
    env_files: &[PathBuf],
    process_env_project_name: Option<&str>,
) -> Result<String> {
    // An explicit COMPOSE_PROJECT_NAME is used verbatim (no suffix) — this is a deliberate
    // user override, so honor it exactly as docker compose and the reference CLI would.
    // See `explicit_compose_project_name` for where it is looked for and why in that order.
    if let Some(project_name) = explicit_compose_project_name(
        base_path,
        compose_files,
        env_files,
        process_env_project_name,
    ) {
        return Ok(project_name);
    }

    // An AUTHORED top-level `name:` wins over any derivation — but the authored text is
    // a TEMPLATE, so the value that goes on `--project-name` is Compose's, not ours
    // (#572). Reading the line ourselves and passing it through produced
    // `invalid project name "${CUSTOM_NAME}"` and no container at all, while the
    // reference — which takes the name off `docker compose config` — brought the project
    // up as `custom-name-with-env-var`. Asking Compose also gets its normalization for
    // free (`Custom.Name Upper` → `customnameupper`), which is what the reference's own
    // `Rg` sanitizer reproduces by hand.
    if let Some(authored) = compose_files
        .iter()
        .find_map(|file| parse_compose_file_for_project_name(file))
    {
        let command = ComposeCommand::new(base_path.to_path_buf(), compose_files.to_vec())
            .with_docker_path(docker_path.to_string())
            .with_env_files(env_files.to_vec());

        // No fallback to the raw line on failure. A `name:` we cannot evaluate has no
        // safe reading: passing the template through is the defect this replaced, and
        // quietly deriving a name instead would ignore what the author wrote and strand
        // the project under a name they never asked for. The reference fails the same
        // way — a failing `docker compose config` aborts its `up` with "An error
        // occurred retrieving the Docker Compose configuration."
        let resolved = command.extract_project_name().await.map_err(|err| {
            DockerError::CLIError(format!(
                "Compose file declares a top-level project name (`{authored}`) that Docker \
                 Compose could not resolve. A compose `name:` is a template — Compose \
                 interpolates it from the environment and the project's `.env` — so deacon \
                 asks `docker compose config` for the resolved value instead of guessing. \
                 Underlying failure: {err}"
            ))
        })?;

        debug!(
            "Using compose-resolved project name: {} (authored as `{}`)",
            resolved, authored
        );
        return Ok(resolved);
    }

    // Auto-derived default: deacon-namespaced (#265) and unique per devcontainer.
    //
    // The reference CLI's own default is `<folder>_devcontainer` — identical
    // to what deacon used to produce here. That collision meant `devcontainer
    // up` would *discover* a compose project deacon brought up (same name,
    // same directory), then fail looking for its own `vsc-*`-tagged image,
    // which deacon never creates (deacon's compose service images aren't
    // named that way). The `deacon_` prefix keeps deacon's compose project
    // cleanly out of the reference CLI's naming convention.
    //
    // The name combines BOTH hashes `ContainerIdentity` uses — `workspace_hash`
    // (the `devcontainer.workspaceHash` label) plus `config_hash` — mirroring
    // `ContainerIdentity::container_name`. `workspace_hash` walks to the git
    // root, so `workspace_hash` alone would be IDENTICAL for two devcontainer
    // folders in the same repo (e.g. a monorepo's `services/api` and
    // `services/web`), collapsing them onto one compose project and letting a
    // `deacon up`/`down` in one silently reconcile or tear down the other.
    // Folding in `config_hash` disambiguates siblings exactly as the
    // single-container path does. This is a one-way migration from any prior
    // naming: projects created by older deacon versions become orphaned (not
    // deleted) — `docker compose -p <old-name> down` clears them, and `up`
    // says so once via `superseded_project_advice`.
    //
    // The SANITIZED WORKSPACE-FOLDER STEM leads (#564, maintainer ruling
    // 2026-08-11). `deacon_6fb1205c_532a7bdd` identifies nothing to someone
    // reading `docker compose ps` or pointing an agent at container state, and
    // that terminal-first audience is deacon's primary one: there is no VS Code
    // integration path for deacon (no extension; the Dev Containers extension
    // bundles and drives the reference CLI), so the two-tools-provisioning-one-
    // workspace collision #265 defends against is a migration period, not a
    // steady state. `deacon_site_6fb1205c_532a7bdd` is readable AND still
    // outside the reference's `<folder>_devcontainer` convention by
    // construction, so #265's isolation holds unchanged.
    //
    // A stem that sanitizes to empty (a folder like `-myproj`, `..`, a
    // workspace at `/`, a purely non-ASCII basename) falls back to the
    // hash-only form rather than emitting a malformed `--project-name`
    // (`bhv-compose-project-name-robust`).
    let workspace_hash = crate::container::ContainerIdentity::hash_workspace_path(base_path);
    let config_hash = crate::container::ContainerIdentity::hash_config(config);
    let stem = base_path
        .file_name()
        .map(|n| sanitize_project_stem(&n.to_string_lossy()))
        .unwrap_or_default();
    if stem.is_empty() {
        return Ok(format!("deacon_{workspace_hash}_{config_hash}"));
    }
    Ok(format!("deacon_{stem}_{workspace_hash}_{config_hash}"))
}

/// Which tool/generation named a Compose project deacon no longer derives for this
/// workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupersededProjectOrigin {
    /// `deacon_<workspaceHash>_<configHash>` — the auto-derived name deacon used before
    /// #564 put the workspace-folder stem in front of the hashes.
    DeaconLegacy,
    /// `<folder>_devcontainer` — the reference CLI's own derivation, i.e. the user came
    /// from `devcontainer up` or the VS Code Dev Containers extension.
    ReferenceCli,
}

/// A Compose project that holds this workspace's named volumes under a project name
/// deacon will not use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SupersededProject {
    /// The Compose project name, exactly as `com.docker.compose.project` reports it.
    pub name: String,
    /// What named it.
    pub origin: SupersededProjectOrigin,
}

/// Pick out the projects in `projects` that named THIS workspace's volumes under a
/// superseded naming scheme. Pure — the Docker call that produces `projects` is
/// [`ComposeManager::detect_superseded_volume_projects`].
///
/// Two schemes qualify, and only two:
///
/// * **`deacon_<workspaceHash>_<configHash>`** — this workspace's `workspace_hash`
///   followed by exactly one more field. The trailing field is the *config* hash, which
///   is unknown for a configuration the user has since edited, so it is matched
///   structurally (no further `_`) rather than by value. The post-#564 form always has
///   the stem between `deacon_` and the workspace hash, so it can never be mistaken for
///   the legacy form unless the stem IS the workspace hash — and in that case the project
///   is this workspace's anyway.
/// * **`<folder>_devcontainer`** — the reference CLI's derivation, matched against both
///   the raw lowercased basename (what the reference passes to `--project-name`) and the
///   sanitized stem (what Compose may normalize it to).
///
/// `current` is never reported: it is the project this `up` is about to use.
pub fn classify_superseded_projects<I, S>(
    projects: I,
    current: &str,
    workspace_hash: &str,
    workspace_basename: &str,
) -> Vec<SupersededProject>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let legacy_prefix = format!("deacon_{workspace_hash}_");
    let reference_forms = {
        let mut forms = vec![format!(
            "{}_devcontainer",
            workspace_basename.to_ascii_lowercase()
        )];
        let sanitized = sanitize_project_stem(workspace_basename);
        if !sanitized.is_empty() {
            let form = format!("{sanitized}_devcontainer");
            if !forms.contains(&form) {
                forms.push(form);
            }
        }
        forms
    };

    let mut found: Vec<SupersededProject> = Vec::new();
    for project in projects {
        let name = project.as_ref();
        if name.is_empty() || name == current {
            continue;
        }
        let origin = if name
            .strip_prefix(&legacy_prefix)
            .is_some_and(|rest| !rest.is_empty() && !rest.contains('_'))
        {
            SupersededProjectOrigin::DeaconLegacy
        } else if reference_forms.iter().any(|f| f == name) {
            SupersededProjectOrigin::ReferenceCli
        } else {
            continue;
        };
        let entry = SupersededProject {
            name: name.to_string(),
            origin,
        };
        if !found.contains(&entry) {
            found.push(entry);
        }
    }
    found.sort();
    found
}

/// The one-time `up` diagnostic for a detected project-name transition (#564).
///
/// Compose prefixes every named volume with the project name, so a project rename leaves
/// the previous project's volumes intact but INVISIBLE to the new project — an unexplained
/// empty database is the worst outcome here, and it is silent. `stop_superseded_containers`
/// already handles the container side; nothing touches volumes, deliberately (they hold
/// data), which is exactly why this has to be said out loud.
///
/// Returns `None` when there is nothing to report, so the caller emits nothing.
pub fn superseded_project_advice(
    superseded: &[SupersededProject],
    current_project: &str,
) -> Option<String> {
    if superseded.is_empty() {
        return None;
    }

    let mut message = format!(
        "This workspace has Docker Compose volumes under {} other project name(s); deacon will use `{current_project}`.",
        superseded.len()
    );
    for project in superseded {
        let origin = match project.origin {
            SupersededProjectOrigin::DeaconLegacy => "created by an older deacon",
            SupersededProjectOrigin::ReferenceCli => "created by the devcontainer CLI",
        };
        message.push_str(&format!("\n  - `{}` ({origin})", project.name));
    }
    message.push_str(
        "\nDocker Compose prefixes named volumes with the project name, so those volumes belong to the previous project and are NOT visible to the new one. \
No data was deleted — it is still there under the old project. \
To inspect or remove a previous project: `docker compose -p <project> ps` / `docker compose -p <project> down -v`. \
To migrate a volume's contents: `docker run --rm -v <old-volume>:/from -v <new-volume>:/to alpine sh -c 'cp -a /from/. /to/'`.",
    );
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_compose_command_build() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let compose_files = vec![
            base_path.join("docker-compose.yml"),
            base_path.join("docker-compose.override.yml"),
        ];

        let cmd = ComposeCommand::new(base_path.clone(), compose_files.clone())
            .with_project_name("test-project".to_string());

        let command = cmd.build_command(&["up", "-d"]);

        let args: Vec<String> = command
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"compose".to_string()));
        assert!(args.contains(&"-f".to_string()));
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"test-project".to_string()));
        assert!(args.contains(&"up".to_string()));
        assert!(args.contains(&"-d".to_string()));
    }

    #[tokio::test]
    async fn test_create_project_resolves_compose_files_against_config_dir() {
        // Compose files must resolve relative to the directory containing
        // devcontainer.json (the `.devcontainer` dir for the standard layout),
        // NOT the workspace folder — matching the spec and the reference CLI.
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().to_path_buf();
        let config_dir = workspace.join(".devcontainer");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("docker-compose.yml"), "services: {}\n").unwrap();
        std::fs::write(
            config_dir.join("docker-compose.override.yml"),
            "services: {}\n",
        )
        .unwrap();

        let config = crate::config::DevContainerConfig {
            docker_compose_file: Some(serde_json::json!([
                "docker-compose.yml",
                "docker-compose.override.yml"
            ])),
            service: Some("app".to_string()),
            ..Default::default()
        };

        let manager = ComposeManager::new();
        let project = manager
            .create_project(&config, &workspace, &config_dir, &[])
            .await
            .unwrap();

        // Files resolved under `.devcontainer`, not the workspace root.
        assert_eq!(
            project.compose_files,
            vec![
                config_dir.join("docker-compose.yml"),
                config_dir.join("docker-compose.override.yml"),
            ]
        );
        // Project base_path stays the workspace folder (naming / working dir).
        assert_eq!(project.base_path, workspace);
    }

    #[tokio::test]
    async fn test_create_project_absolute_compose_path_is_not_rebased() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().to_path_buf();
        let config_dir = workspace.join(".devcontainer");
        let abs = workspace.join("custom-compose.yml");
        std::fs::write(&abs, "services: {}\n").unwrap();

        let config = crate::config::DevContainerConfig {
            docker_compose_file: Some(serde_json::json!(abs.to_string_lossy())),
            service: Some("app".to_string()),
            ..Default::default()
        };

        let project = ComposeManager::new()
            .create_project(&config, &workspace, &config_dir, &[])
            .await
            .unwrap();
        assert_eq!(project.compose_files, vec![abs]);
    }

    #[test]
    fn test_compose_command_with_env_file() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let compose_files = vec![base_path.join("docker-compose.yml")];
        let env_files = vec![base_path.join(".env"), base_path.join(".env.local")];

        let cmd = ComposeCommand::new(base_path.clone(), compose_files.clone())
            .with_project_name("test-project".to_string())
            .with_env_files(env_files.clone());

        let command = cmd.build_command(&["up", "-d"]);

        let args: Vec<String> = command
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"compose".to_string()));
        assert!(args.contains(&"--env-file".to_string()));

        // Verify that both env files are included
        let env_file_count = args.iter().filter(|&arg| arg == "--env-file").count();
        assert_eq!(env_file_count, 2, "Should have two --env-file flags");
    }

    #[test]
    fn test_compose_command_with_profiles() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let compose_files = vec![base_path.join("docker-compose.yml")];
        let profiles = vec!["dev".to_string(), "debug".to_string()];

        let cmd = ComposeCommand::new(base_path.clone(), compose_files.clone())
            .with_project_name("test-project".to_string())
            .with_profiles(profiles);

        let command = cmd.build_command(&["up", "-d"]);

        let args: Vec<String> = command
            .as_std()
            .get_args()
            .map(|s| s.to_string_lossy().to_string())
            .collect();

        assert!(args.contains(&"compose".to_string()));
        assert!(args.contains(&"--profile".to_string()));
        assert!(args.contains(&"dev".to_string()));
        assert!(args.contains(&"debug".to_string()));

        // Verify that both profiles are included
        let profile_count = args.iter().filter(|&arg| arg == "--profile").count();
        assert_eq!(profile_count, 2, "Should have two --profile flags");
    }

    #[test]
    fn test_compose_project_get_all_services() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec!["db".to_string(), "redis".to_string()],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let services = project.get_all_services();
        assert_eq!(services, vec!["app", "db", "redis"]);
    }

    #[test]
    fn test_config_compose_methods() {
        use serde_json::json;

        // Test single compose file
        let mut config = DevContainerConfig {
            docker_compose_file: Some(json!("docker-compose.yml")),
            service: Some("app".to_string()),
            ..Default::default()
        };

        assert!(config.uses_compose());
        assert_eq!(config.get_compose_files(), vec!["docker-compose.yml"]);
        assert_eq!(config.get_all_services(), vec!["app"]);

        // Test multiple compose files
        config.docker_compose_file =
            Some(json!(["docker-compose.yml", "docker-compose.override.yml"]));
        config.run_services = Some(vec!["db".to_string(), "redis".to_string()]);

        assert_eq!(
            config.get_compose_files(),
            vec!["docker-compose.yml", "docker-compose.override.yml"]
        );
        assert_eq!(config.get_all_services(), vec!["app", "db", "redis"]);

        // Test stopCompose shutdown action
        config.shutdown_action = Some("stopCompose".to_string());
        assert!(config.has_stop_compose_shutdown());
    }

    #[test]
    fn test_security_options_warning_for_compose() {
        // Test config with security options
        let config = DevContainerConfig {
            privileged: Some(true),
            cap_add: Some(vec!["SYS_PTRACE".to_string(), "NET_ADMIN".to_string()]),
            security_opt: Some(vec!["seccomp=unconfined".to_string()]),
            ..Default::default()
        };

        // This should log warnings - in a real test we'd capture logs
        ComposeCommand::warn_security_options_for_compose(&config);

        // Test config without security options
        let empty_config = DevContainerConfig::default();

        // This should not log any warnings
        ComposeCommand::warn_security_options_for_compose(&empty_config);
    }

    #[test]
    fn test_compose_project_all_services_coverage() {
        // Test that get_all_services includes primary service and run_services
        let project = ComposeProject {
            name: "multi-service".to_string(),
            base_path: PathBuf::from("/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "web".to_string(),
            run_services: vec![
                "database".to_string(),
                "cache".to_string(),
                "queue".to_string(),
            ],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let all_services = project.get_all_services();
        assert_eq!(all_services.len(), 4);
        assert_eq!(all_services[0], "web"); // Primary service first
        assert!(all_services.contains(&"database".to_string()));
        assert!(all_services.contains(&"cache".to_string()));
        assert!(all_services.contains(&"queue".to_string()));
    }

    #[test]
    fn test_compose_project_single_service_only() {
        // Test project with only primary service, no run_services
        let project = ComposeProject {
            name: "single-service".to_string(),
            base_path: PathBuf::from("/workspace"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec![],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let all_services = project.get_all_services();
        assert_eq!(all_services, vec!["app"]);
    }

    /// Test shim for the HERMETIC half of [`derive_project_name`] — the `.env` override
    /// and the auto-derived default, neither of which touches Compose.
    ///
    /// Every caller of this helper either passes no compose files or short-circuits on a
    /// `.env`, so no `docker compose config` subprocess is ever spawned. A caller that
    /// authored a `name:` would need a real `docker` binary and belongs in
    /// `crates/core/tests/integration_compose.rs` behind the docker group instead.
    async fn derived_name(
        base_path: &Path,
        config: &DevContainerConfig,
        compose_files: &[PathBuf],
    ) -> String {
        derive_project_name("docker", base_path, config, compose_files, &[], None)
            .await
            .expect("hermetic derivation must not fail")
    }

    /// #265 + #564: the auto-derived project name is
    /// `deacon_<stem>_<workspace_hash>_<config_hash>` — the sanitized workspace-folder
    /// stem in front of the SAME two hashes `ContainerIdentity` computes — never the
    /// reference CLI's `<folder>_devcontainer` convention, so `devcontainer up` can
    /// still not mistake a deacon-owned compose project for its own.
    #[tokio::test]
    async fn test_derive_project_name_is_deacon_namespaced_and_hash_based() {
        let path = Path::new("/tmp/my-workspace");
        let config = DevContainerConfig::default();
        let name = derived_name(path, &config, &[]).await;
        assert!(
            name.starts_with("deacon_"),
            "expected deacon_<stem>_<hash>_<hash>, got {name}"
        );
        let expected = format!(
            "deacon_my-workspace_{}_{}",
            crate::container::ContainerIdentity::hash_workspace_path(path),
            crate::container::ContainerIdentity::hash_config(&config),
        );
        assert_eq!(name, expected);
    }

    /// #564's whole point: the derived name has to be READABLE, and readable means the
    /// folder someone is working in appears in `docker compose ps` without them computing
    /// a hash. Both hashes stay — `workspace_hash` disambiguates two checkouts sharing a
    /// basename, `config_hash` is what makes an edited configuration a new generation
    /// (#371, #551 are built on it).
    #[tokio::test]
    async fn test_derive_project_name_leads_with_the_workspace_stem() {
        let config = DevContainerConfig::default();
        let name = derived_name(Path::new("/home/dev/site"), &config, &[]).await;
        assert!(
            name.starts_with("deacon_site_"),
            "the workspace stem must lead the hashes, got {name}"
        );
        let fields: Vec<&str> = name.split('_').collect();
        assert_eq!(
            fields.len(),
            4,
            "expected deacon_<stem>_<wsHash>_<cfgHash>, got {name}"
        );
        assert_eq!(fields[2].len(), 8, "workspace hash must survive: {name}");
        assert_eq!(fields[3].len(), 8, "config hash must survive: {name}");
    }

    /// Two checkouts of the same project under different parents share a BASENAME. The
    /// stem alone would collapse them onto one Compose project; `workspace_hash` is what
    /// keeps them apart, and it is still there.
    #[tokio::test]
    async fn test_derive_project_name_same_stem_different_parents_still_differ() {
        let config = DevContainerConfig::default();
        let a = derived_name(Path::new("/home/dev/work/api"), &config, &[]).await;
        let b = derived_name(Path::new("/home/dev/tmp/api"), &config, &[]).await;
        assert!(a.starts_with("deacon_api_") && b.starts_with("deacon_api_"));
        assert_ne!(a, b, "same basename, different checkouts must not collide");
    }

    /// A folder whose basename sanitizes to nothing falls back to the hash-only form
    /// rather than emitting a `--project-name` `docker compose` rejects — the robustness
    /// claim `bhv-compose-project-name-robust` records, preserved verbatim through #564.
    #[tokio::test]
    async fn test_derive_project_name_empty_stem_falls_back_to_hash_only() {
        let config = DevContainerConfig::default();
        for path in ["/tmp/-", "/tmp/...", "/tmp/---"] {
            let path = Path::new(path);
            let name = derived_name(path, &config, &[]).await;
            let expected = format!(
                "deacon_{}_{}",
                crate::container::ContainerIdentity::hash_workspace_path(path),
                crate::container::ContainerIdentity::hash_config(&config),
            );
            assert_eq!(name, expected, "{} must fall back", path.display());
        }
        // A workspace at the filesystem root has no basename at all.
        let root = Path::new("/");
        assert_eq!(
            derived_name(root, &config, &[]).await,
            format!(
                "deacon_{}_{}",
                crate::container::ContainerIdentity::hash_workspace_path(root),
                crate::container::ContainerIdentity::hash_config(&config),
            )
        );
    }

    /// Every derived name must satisfy Compose's `[a-z0-9][a-z0-9_-]*`, including for the
    /// hostile basenames a user can create. This is the security-adjacent half: the stem
    /// is user-controlled input that ends up in a `--project-name` argument.
    #[test]
    fn test_sanitize_project_stem_covers_hostile_inputs() {
        // (raw basename, expected stem — empty means "fall back to hash-only")
        let cases = [
            ("site", "site"),
            ("My-Project", "my-project"),
            ("my project", "my-project"),
            ("My  Weird   Name", "my-weird-name"),
            ("café", "caf"),
            ("日本語", ""),
            ("..", ""),
            ("-myproj", "myproj"),
            ("---", ""),
            ("", ""),
            ("_leading", "leading"),
            ("trailing-", "trailing"),
            ("a/b:c;d", "a-b-c-d"),
            ("under_score", "under_score"),
            ("$(whoami)", "whoami"),
            ("--project-name=evil", "project-name-evil"),
            ("9lives", "9lives"),
            (
                "a-very-long-workspace-folder-name-that-keeps-going-and-going",
                "a-very-long-workspace-folder-nam",
            ),
        ];
        for (raw, expected) in cases {
            let got = sanitize_project_stem(raw);
            assert_eq!(got, expected, "sanitizing {raw:?}");
            assert!(
                got.len() <= PROJECT_STEM_MAX_LEN,
                "{raw:?} produced an over-long stem: {got}"
            );
            if !got.is_empty() {
                let mut chars = got.chars();
                let first = chars.next().expect("non-empty");
                assert!(
                    first.is_ascii_alphanumeric(),
                    "{raw:?} produced a stem starting with {first:?}"
                );
                assert!(
                    got.chars().all(|c| c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == '-'
                        || c == '_'),
                    "{raw:?} produced an illegal stem: {got}"
                );
            }
        }
    }

    /// The classifier that drives the #564 transition diagnostic. Both superseded shapes
    /// are recognized, and nothing else is — a sibling workspace's project, another
    /// deacon workspace's project, and the current project are all left alone.
    #[test]
    fn test_classify_superseded_projects_recognizes_both_transitions() {
        let current = "deacon_site_742b6f14_b5e9edc0";
        let projects = [
            current,
            "deacon_742b6f14_b5e9edc0", // this workspace, older deacon format
            "deacon_742b6f14_0bad0bad", // same, a different (edited) config
            "site_devcontainer",        // this workspace, reference CLI
            "deacon_99999999_b5e9edc0", // a DIFFERENT workspace, older format
            "deacon_other_742b6f14_b5e9ed", // a stem that merely looks similar
            "other_devcontainer",       // a different folder's reference project
            "",
        ];
        let found = classify_superseded_projects(projects, current, "742b6f14", "site");
        assert_eq!(
            found,
            vec![
                SupersededProject {
                    name: "deacon_742b6f14_0bad0bad".to_string(),
                    origin: SupersededProjectOrigin::DeaconLegacy,
                },
                SupersededProject {
                    name: "deacon_742b6f14_b5e9edc0".to_string(),
                    origin: SupersededProjectOrigin::DeaconLegacy,
                },
                SupersededProject {
                    name: "site_devcontainer".to_string(),
                    origin: SupersededProjectOrigin::ReferenceCli,
                },
            ]
        );
    }

    /// The reference CLI passes the workspace basename to `--project-name` verbatim and
    /// Compose lowercases it, so a mixed-case folder must still be recognized — the same
    /// case-insensitivity rule every name-marker sweep here relies on (#442).
    #[test]
    fn test_classify_superseded_projects_matches_lowercased_reference_project() {
        let found = classify_superseded_projects(
            ["mysite_devcontainer", "my-site_devcontainer"],
            "deacon_mysite_1111_2222",
            "1111",
            "MySite",
        );
        assert_eq!(
            found,
            vec![SupersededProject {
                name: "mysite_devcontainer".to_string(),
                origin: SupersededProjectOrigin::ReferenceCli,
            }]
        );
    }

    /// Nothing detected means nothing said: the diagnostic never fires on a clean machine
    /// or on the second `up` after the old volumes are gone.
    #[test]
    fn test_superseded_project_advice_is_silent_when_nothing_is_superseded() {
        assert!(superseded_project_advice(&[], "deacon_site_1111_2222").is_none());
    }

    /// The message has to carry three things or it is not worth emitting: the project
    /// deacon WILL use, the project(s) whose volumes are now separate, and the fact that
    /// Compose prefixes named volumes with the project name (which is why the data looks
    /// gone without being gone).
    #[test]
    fn test_superseded_project_advice_names_project_and_explains_volumes() {
        let advice = superseded_project_advice(
            &[
                SupersededProject {
                    name: "deacon_742b6f14_b5e9edc0".to_string(),
                    origin: SupersededProjectOrigin::DeaconLegacy,
                },
                SupersededProject {
                    name: "site_devcontainer".to_string(),
                    origin: SupersededProjectOrigin::ReferenceCli,
                },
            ],
            "deacon_site_742b6f14_b5e9edc0",
        )
        .expect("advice");
        assert!(advice.contains("deacon_site_742b6f14_b5e9edc0"));
        assert!(advice.contains("deacon_742b6f14_b5e9edc0"));
        assert!(advice.contains("site_devcontainer"));
        assert!(advice.contains("older deacon"));
        assert!(advice.contains("devcontainer CLI"));
        assert!(advice.contains("prefixes named volumes with the project name"));
        assert!(advice.contains("No data was deleted"));
        assert!(advice.contains("docker compose -p"));
    }

    /// Same input path + config -> same project name (deterministic), and it
    /// must NOT collide with the reference CLI's `<folder>_devcontainer` form
    /// for any folder name.
    #[tokio::test]
    async fn test_derive_project_name_deterministic_and_not_reference_form() {
        let path = Path::new("/home/user/myapp");
        let config = DevContainerConfig::default();
        let first = derived_name(path, &config, &[]).await;
        let second = derived_name(path, &config, &[]).await;
        assert_eq!(first, second, "derivation must be deterministic");
        assert_ne!(
            first, "myapp_devcontainer",
            "must not collide with the reference CLI's own default naming"
        );
    }

    /// Distinct workspace paths must not collide on the same project name.
    #[tokio::test]
    async fn test_derive_project_name_differs_per_workspace() {
        let config = DevContainerConfig::default();
        let a = derived_name(Path::new("/tmp/workspace-a"), &config, &[]).await;
        let b = derived_name(Path::new("/tmp/workspace-b"), &config, &[]).await;
        assert_ne!(a, b);
    }

    /// #265 regression guard: two devcontainers that resolve to the SAME
    /// workspace path (e.g. sibling folders under one git root in a monorepo,
    /// both hashing to the git-root `workspace_hash`) but carry DIFFERENT
    /// configs must derive DIFFERENT compose project names. Otherwise a
    /// `deacon up`/`down` in one would silently reconcile or tear down the
    /// other's compose project. The `config_hash` component disambiguates
    /// them, exactly as it does for the single-container `container_name`.
    #[tokio::test]
    async fn test_derive_project_name_differs_by_config_for_same_path() {
        let path = Path::new("/tmp/shared-workspace");
        let config_a = DevContainerConfig {
            name: Some("api".to_string()),
            ..DevContainerConfig::default()
        };
        let config_b = DevContainerConfig {
            name: Some("web".to_string()),
            ..DevContainerConfig::default()
        };
        let a = derived_name(path, &config_a, &[]).await;
        let b = derived_name(path, &config_b, &[]).await;
        assert_ne!(
            a, b,
            "sibling devcontainers under one workspace path must not share a compose project name"
        );
    }

    /// An explicit `COMPOSE_PROJECT_NAME` in a sibling `.env` is honored
    /// verbatim — only the auto-derived branch changed under #265.
    #[tokio::test]
    async fn test_derive_project_name_env_override_used_verbatim() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(
            temp_dir.path().join(".env"),
            "COMPOSE_PROJECT_NAME=my-custom-project\n",
        )
        .unwrap();
        let config = DevContainerConfig::default();
        assert_eq!(
            derived_name(temp_dir.path(), &config, &[]).await,
            "my-custom-project"
        );
    }

    /// The `.env` override short-circuits BEFORE the compose document is consulted, so
    /// an authored `name:` never reaches Compose here. That ordering is what keeps this
    /// test hermetic — `derived_name`'s docker path is never invoked — and it is also
    /// the reference CLI's ordering (`Rp` returns on `COMPOSE_PROJECT_NAME` before it
    /// ever looks at `composeConfig.name`).
    #[tokio::test]
    async fn test_derive_project_name_env_override_wins_over_compose_name() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(
            temp_dir.path().join(".env"),
            "COMPOSE_PROJECT_NAME=my-custom-project\n",
        )
        .unwrap();
        let compose_file = temp_dir.path().join("docker-compose.yml");
        std::fs::write(
            &compose_file,
            "name: r4-explicit-project\nservices:\n  app:\n    image: alpine:3.18\n",
        )
        .unwrap();
        let config = DevContainerConfig::default();

        assert_eq!(
            derived_name(temp_dir.path(), &config, &[compose_file]).await,
            "my-custom-project"
        );
    }

    /// Build a workspace with a compose file in a `.devcontainer` child, plus whichever
    /// `.env` files the caller names, and return `(workspace, compose_file)`.
    ///
    /// The nesting is the point: Compose's project directory is the directory of the first
    /// `-f` file, so `.devcontainer/.env` and `<workspace>/.env` are two DIFFERENT sources
    /// and a flat fixture could not tell them apart.
    fn compose_workspace(env_files: &[(&str, &str)]) -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join(".devcontainer");
        std::fs::create_dir_all(&config_dir).unwrap();
        let compose_file = config_dir.join("docker-compose.yml");
        std::fs::write(&compose_file, "services:\n  app:\n    image: alpine:3.18\n").unwrap();
        for (relative, name) in env_files {
            std::fs::write(
                temp_dir.path().join(relative),
                format!("COMPOSE_PROJECT_NAME={name}\n"),
            )
            .unwrap();
        }
        (temp_dir, compose_file)
    }

    /// #580: the process environment is the FIRST source, ahead of the workspace `.env`
    /// deacon used to treat as the only one.
    ///
    /// This is the row of the measured matrix that the parity lane cannot assert — an
    /// `Operation` has no field for environment variables — so it is pinned here instead.
    /// The value arrives as a parameter rather than through `std::env::var` precisely so
    /// this test can exist: `unsafe_code = "deny"` rules out `set_var`, and a test that
    /// mutated the process environment would race every other test in the binary anyway.
    #[test]
    fn test_explicit_project_name_prefers_the_process_environment() {
        let (temp_dir, compose_file) = compose_workspace(&[
            (".env", "from-workspace"),
            (".devcontainer/.env", "from-configdir"),
        ]);

        assert_eq!(
            explicit_compose_project_name(temp_dir.path(), &[compose_file], &[], Some("env-wins")),
            Some("env-wins".to_string())
        );
    }

    /// A blank or whitespace-only `COMPOSE_PROJECT_NAME` is not an override. Compose
    /// rejects an empty project name outright ("project name must not be empty"), so
    /// treating an exported-but-empty variable as a declaration would turn a stray
    /// `export COMPOSE_PROJECT_NAME=` into a hard failure with no name to report.
    #[test]
    fn test_explicit_project_name_ignores_a_blank_process_environment_value() {
        let (temp_dir, compose_file) = compose_workspace(&[(".env", "from-workspace")]);

        assert_eq!(
            explicit_compose_project_name(temp_dir.path(), &[compose_file], &[], Some("   ")),
            Some("from-workspace".to_string())
        );
    }

    /// #580: a `.env` BESIDE THE COMPOSE FILE is the third source — Compose's own default
    /// discovery, which deacon never consulted. This is the case where deacon fell all the
    /// way through to its derived `deacon_*` default while the reference answered
    /// `from-configdir`.
    #[test]
    fn test_explicit_project_name_reads_the_compose_project_directory_env() {
        let (temp_dir, compose_file) =
            compose_workspace(&[(".devcontainer/.env", "from-configdir")]);

        assert_eq!(
            explicit_compose_project_name(temp_dir.path(), &[compose_file], &[], None),
            Some("from-configdir".to_string())
        );
    }

    /// #580, the ordering half: with BOTH `.env` files declaring a name the workspace
    /// folder's wins. Measured against the reference, which answers `from-workspace` here.
    ///
    /// This is what rules out the simpler fix of handing the whole question to `docker
    /// compose config` — Compose would answer `from-configdir`, because the workspace
    /// `.env` is not in its project directory at all.
    #[test]
    fn test_explicit_project_name_workspace_env_outranks_the_compose_directory() {
        let (temp_dir, compose_file) = compose_workspace(&[
            (".env", "from-workspace"),
            (".devcontainer/.env", "from-configdir"),
        ]);

        assert_eq!(
            explicit_compose_project_name(temp_dir.path(), &[compose_file], &[], None),
            Some("from-workspace".to_string())
        );
    }

    /// `--env-file` REPLACES Compose's default `.env` discovery rather than adding to it,
    /// so the compose directory's `.env` must not be read when the caller supplied env
    /// files — and when several are supplied the LAST one wins, which is how Compose
    /// merges them.
    #[test]
    fn test_explicit_project_name_env_files_replace_the_default_discovery() {
        let (temp_dir, compose_file) =
            compose_workspace(&[(".devcontainer/.env", "from-configdir")]);
        let first = temp_dir.path().join("first.env");
        let second = temp_dir.path().join("second.env");
        std::fs::write(&first, "COMPOSE_PROJECT_NAME=from-first\n").unwrap();
        std::fs::write(&second, "COMPOSE_PROJECT_NAME=from-second\n").unwrap();

        assert_eq!(
            explicit_compose_project_name(
                temp_dir.path(),
                std::slice::from_ref(&compose_file),
                &[first, second],
                None
            ),
            Some("from-second".to_string())
        );

        // With env files that declare nothing, the compose directory's `.env` stays
        // unread — Compose would not read it either — and the caller derives.
        let silent = temp_dir.path().join("silent.env");
        std::fs::write(&silent, "UNRELATED=1\n").unwrap();
        assert_eq!(
            explicit_compose_project_name(temp_dir.path(), &[compose_file], &[silent], None),
            None
        );
    }

    /// The explicit override short-circuits ahead of the authored-`name:` branch, so a
    /// process-environment declaration never spawns `docker compose config`. Asserted with
    /// a docker path that cannot exist: if the branch were reached, the derivation would
    /// fail rather than answer.
    #[tokio::test]
    async fn test_derive_project_name_process_env_short_circuits_before_compose() {
        let (temp_dir, compose_file) = compose_workspace(&[]);
        std::fs::write(
            &compose_file,
            "name: authored-literal\nservices:\n  app:\n    image: alpine:3.18\n",
        )
        .unwrap();
        let config = DevContainerConfig::default();

        let name = derive_project_name(
            "deacon-no-such-docker-binary",
            temp_dir.path(),
            &config,
            &[compose_file],
            &[],
            Some("env-wins"),
        )
        .await
        .expect("an explicit COMPOSE_PROJECT_NAME must not depend on Compose");
        assert_eq!(name, "env-wins");
    }

    /// #572: an authored `name:` is resolved BY COMPOSE, and when Compose cannot be
    /// reached there is no second-best answer — deacon errors instead of guessing.
    ///
    /// Both wrong guesses are worse than the error. Passing the raw line through is the
    /// defect this replaced (`invalid project name "${CUSTOM_NAME}"`), and falling back
    /// to the derived `deacon_*` default would ignore what the author wrote and strand
    /// the project under a name they never asked for — a `down` computed the same way
    /// would then match, but any `docker compose -p <authored>` they run by hand would
    /// not. The reference fails here too: a failing `docker compose config` aborts its
    /// `up` outright.
    ///
    /// Hermetic: the docker path is a name that cannot exist, so the spawn fails without
    /// a daemon, a registry or a `docker` binary being involved.
    #[tokio::test]
    async fn test_derive_project_name_authored_name_errors_when_compose_unavailable() {
        let temp_dir = TempDir::new().unwrap();
        let compose_file = temp_dir.path().join("docker-compose.yml");
        std::fs::write(
            &compose_file,
            "name: ${CUSTOM_NAME}\nservices:\n  app:\n    image: alpine:3.18\n",
        )
        .unwrap();
        let config = DevContainerConfig::default();

        let err = derive_project_name(
            "deacon-no-such-docker-binary",
            temp_dir.path(),
            &config,
            &[compose_file],
            &[],
            None,
        )
        .await
        .expect_err("an authored name deacon cannot resolve must not be guessed at");

        let message = err.to_string();
        assert!(
            message.contains("${CUSTOM_NAME}"),
            "the error must quote what the author wrote: {message}"
        );
        assert!(
            message.contains("docker compose config"),
            "the error must name the mechanism that failed: {message}"
        );
        assert!(
            !message.contains("deacon_"),
            "no silent fall back to the derived default: {message}"
        );
    }

    /// A document that authors NO `name:` never spawns a subprocess, so the derived
    /// default stays reachable with no `docker` binary at all — the property that keeps
    /// `bhv-compose-project-name-robust` independent of #572's Compose call.
    #[tokio::test]
    async fn test_derive_project_name_without_authored_name_needs_no_compose() {
        let temp_dir = TempDir::new().unwrap();
        let compose_file = temp_dir.path().join("docker-compose.yml");
        std::fs::write(&compose_file, "services:\n  app:\n    image: alpine:3.18\n").unwrap();
        let config = DevContainerConfig::default();

        let name = derive_project_name(
            "deacon-no-such-docker-binary",
            temp_dir.path(),
            &config,
            &[compose_file],
            &[],
            None,
        )
        .await
        .expect("derivation must not depend on Compose when nobody authored a name");
        assert!(name.starts_with("deacon_"), "got {name}");
    }

    /// The pure half of the resolution: Compose's answer is a JSON `name`.
    #[test]
    fn test_parse_project_name_from_config() {
        assert_eq!(
            parse_project_name_from_config(r#"{"name":"custom-name-with-env-var"}"#).unwrap(),
            "custom-name-with-env-var"
        );
        // Compose always emits a name on success; an absent or blank one is a broken
        // answer rather than "nobody authored one", so it must not be silently accepted
        // (an empty `--project-name` is rejected by Compose anyway).
        assert!(parse_project_name_from_config(r#"{"services":{}}"#).is_err());
        assert!(parse_project_name_from_config(r#"{"name":"   "}"#).is_err());
        assert!(parse_project_name_from_config("not json").is_err());
    }

    #[test]
    fn test_merge_env_with_cli_precedence() {
        let mut service_env: IndexMap<String, String> = IndexMap::new();
        service_env.insert("DB_HOST".to_string(), "localhost".to_string());
        service_env.insert("DB_PORT".to_string(), "5432".to_string());
        service_env.insert("SERVICE_ONLY".to_string(), "from_service".to_string());

        let mut env_file_env: IndexMap<String, String> = IndexMap::new();
        env_file_env.insert("DB_HOST".to_string(), "db.example.com".to_string());
        env_file_env.insert("ENV_FILE_ONLY".to_string(), "from_env_file".to_string());

        let mut cli_env: IndexMap<String, String> = IndexMap::new();
        cli_env.insert(
            "DB_HOST".to_string(),
            "cli-override.example.com".to_string(),
        );
        cli_env.insert("CLI_ONLY".to_string(), "from_cli".to_string());

        let merged =
            ComposeProject::merge_env_with_cli_precedence(&service_env, &env_file_env, &cli_env);

        // CLI takes precedence over both env-file and service defaults
        assert_eq!(
            merged.get("DB_HOST"),
            Some(&"cli-override.example.com".to_string())
        );

        // Service default preserved when not overridden
        assert_eq!(merged.get("DB_PORT"), Some(&"5432".to_string()));
        assert_eq!(
            merged.get("SERVICE_ONLY"),
            Some(&"from_service".to_string())
        );

        // Env-file value preserved when not overridden by CLI
        assert_eq!(
            merged.get("ENV_FILE_ONLY"),
            Some(&"from_env_file".to_string())
        );

        // CLI-only value present
        assert_eq!(merged.get("CLI_ONLY"), Some(&"from_cli".to_string()));

        // Total should be 5 unique keys
        assert_eq!(merged.len(), 5);
    }

    #[test]
    fn test_merge_env_empty_inputs() {
        let service_env: IndexMap<String, String> = IndexMap::new();
        let env_file_env: IndexMap<String, String> = IndexMap::new();
        let cli_env: IndexMap<String, String> = IndexMap::new();

        let merged =
            ComposeProject::merge_env_with_cli_precedence(&service_env, &env_file_env, &cli_env);

        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_env_cli_only() {
        let service_env: IndexMap<String, String> = IndexMap::new();
        let env_file_env: IndexMap<String, String> = IndexMap::new();
        let mut cli_env: IndexMap<String, String> = IndexMap::new();
        cli_env.insert("MY_VAR".to_string(), "my_value".to_string());

        let merged =
            ComposeProject::merge_env_with_cli_precedence(&service_env, &env_file_env, &cli_env);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged.get("MY_VAR"), Some(&"my_value".to_string()));
    }

    #[test]
    fn test_generate_injection_override_empty() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        // No mounts or env, should return None
        assert!(project.generate_injection_override().is_none());
    }

    #[test]
    fn test_generate_injection_override_with_env() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("FOO".to_string(), "bar".to_string());
        additional_env.insert("BAZ".to_string(), "qux".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "myservice".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();
        assert!(override_yaml.contains("services:"));
        assert!(override_yaml.contains("myservice:"));
        assert!(override_yaml.contains("environment:"));
        assert!(override_yaml.contains("FOO:"));
        assert!(override_yaml.contains("BAZ:"));
    }

    #[test]
    fn test_generate_injection_override_with_mounts() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "myservice".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![
                ComposeMount {
                    mount_type: "bind".to_string(),
                    source: "/host/path".to_string(),
                    target: "/container/path".to_string(),
                    read_only: false,
                    consistency: None,
                },
                ComposeMount {
                    mount_type: "bind".to_string(),
                    source: "/another/host".to_string(),
                    target: "/another/container".to_string(),
                    read_only: true,
                    consistency: None,
                },
            ],
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();
        assert!(override_yaml.contains("services:"));
        assert!(override_yaml.contains("myservice:"));
        assert!(override_yaml.contains("volumes:"));
        assert!(override_yaml.contains("/host/path:/container/path"));
        assert!(override_yaml.contains("/another/host:/another/container:ro"));
    }

    #[test]
    fn test_generate_injection_override_with_tmpfs_mount() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "myservice".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "tmpfs".to_string(),
                source: String::new(),
                target: "/mnt/config-tmp".to_string(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();
        assert!(override_yaml.contains("volumes:"));
        assert!(override_yaml.contains("- type: tmpfs"));
        assert!(override_yaml.contains("target: /mnt/config-tmp"));
    }

    #[test]
    fn test_generate_injection_override_declares_new_named_volumes() {
        // #272 follow-up: a `type=volume` additional mount (e.g. a
        // feature-contributed mount) must be declared under a top-level
        // `volumes:` key, or `docker compose up` rejects the project with
        // "refers to undefined volume …: invalid compose project".
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![
                ComposeMount {
                    mount_type: "volume".to_string(),
                    source: "feat-probe-vol".to_string(),
                    target: "/feat-mnt".to_string(),
                    read_only: false,
                    consistency: None,
                },
                ComposeMount {
                    mount_type: "bind".to_string(),
                    source: "/host/ws".to_string(),
                    target: "/workspace".to_string(),
                    read_only: false,
                    consistency: None,
                },
            ],
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // The per-service `volumes:` list still gets the short-form mount…
        assert!(override_yaml.contains("feat-probe-vol:/feat-mnt"));
        // …and a SIBLING top-level `volumes:` key declares the named volume
        // (not nested under `services:`), so compose accepts the reference.
        let top_level_volumes_idx = override_yaml
            .find("\nvolumes:\n")
            .expect("expected a top-level `volumes:` key");
        let declaration = &override_yaml[top_level_volumes_idx..];
        assert!(declaration.contains("  feat-probe-vol: {}"));
        // The bind mount's source must NOT be declared as a named volume.
        assert!(!declaration.contains("/host/ws"));
    }

    /// #617: a `type=volume` mount with NO source is an anonymous volume. The
    /// compose short form cannot express it — `- /target` reads as target-only
    /// and `- /target:ro` re-reads the first field as a source — so the override
    /// must emit the LONG form, and must not declare a nameless top-level volume.
    #[test]
    fn test_generate_injection_override_with_anonymous_volume() {
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![
                ComposeMount {
                    mount_type: "volume".to_string(),
                    source: String::new(),
                    target: "/home/anon".to_string(),
                    read_only: false,
                    consistency: None,
                },
                ComposeMount {
                    mount_type: "volume".to_string(),
                    source: "named-vol".to_string(),
                    target: "/home/named".to_string(),
                    read_only: false,
                    consistency: None,
                },
            ],
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Long form for the anonymous volume…
        assert!(
            override_yaml.contains("      - type: volume\n        target: /home/anon\n"),
            "expected long-form anonymous volume, got:\n{override_yaml}"
        );
        // …never the malformed short form with an empty source.
        assert!(
            !override_yaml.contains("- :/home/anon"),
            "empty source must not reach the short form:\n{override_yaml}"
        );
        // The NAMED sibling still uses the short form and is still declared.
        assert!(override_yaml.contains("named-vol:/home/named"));

        let top_level_volumes_idx = override_yaml
            .find("\nvolumes:\n")
            .expect("expected a top-level `volumes:` key for the named volume");
        let declaration = &override_yaml[top_level_volumes_idx..];
        assert!(declaration.contains("  named-vol: {}"));
        // An anonymous volume has no name to declare; a blank key would be
        // an invalid mapping.
        assert!(
            !declaration.contains("  : {}"),
            "anonymous volume must not be declared:\n{declaration}"
        );
    }

    #[test]
    fn test_generate_injection_override_with_both() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("MY_VAR".to_string(), "my_value".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: vec![ComposeMount {
                mount_type: "bind".to_string(),
                source: "/src".to_string(),
                target: "/dst".to_string(),
                read_only: false,
                consistency: None,
            }],
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Should have both environment and volumes sections
        assert!(override_yaml.contains("environment:"));
        assert!(override_yaml.contains("volumes:"));
        assert!(override_yaml.contains("MY_VAR:"));
        assert!(override_yaml.contains("/src:/dst"));
    }

    #[test]
    fn test_generate_injection_override_with_special_chars() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("MULTILINE".to_string(), "line1\nline2".to_string());
        additional_env.insert("QUOTED".to_string(), "value with \"quotes\"".to_string());
        additional_env.insert("COLON".to_string(), "key:value".to_string());
        additional_env.insert("HASH".to_string(), "before#after".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // Verify proper escaping
        assert!(override_yaml.contains("MULTILINE: \"line1\\nline2\""));
        assert!(override_yaml.contains("QUOTED: \"value with \\\"quotes\\\"\""));
        assert!(override_yaml.contains("COLON: \"key:value\""));
        assert!(override_yaml.contains("HASH: \"before#after\""));
    }

    #[test]
    fn test_generate_injection_override_preserves_insertion_order() {
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        // Insert in this specific order: ZZZ, AAA, MMM
        additional_env.insert("ZZZ".to_string(), "last".to_string());
        additional_env.insert("AAA".to_string(), "first".to_string());
        additional_env.insert("MMM".to_string(), "middle".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // IndexMap preserves insertion order: ZZZ, AAA, MMM (not sorted alphabetically)
        let zzz_pos = override_yaml.find("ZZZ:").unwrap();
        let aaa_pos = override_yaml.find("AAA:").unwrap();
        let mmm_pos = override_yaml.find("MMM:").unwrap();

        assert!(
            zzz_pos < aaa_pos && aaa_pos < mmm_pos,
            "Keys should be in insertion order: ZZZ < AAA < MMM, but got ZZZ={}, AAA={}, MMM={}",
            zzz_pos,
            aaa_pos,
            mmm_pos
        );
    }

    #[test]
    fn test_generate_injection_override_run_services_includes_primary() {
        // `runServices` commonly lists the primary service alongside others.
        // The override must emit the primary service exactly once (it already
        // has a full block); a second top-level `app:` mapping is invalid YAML
        // ("mapping key app already defined") and docker compose rejects it.
        let mut labels: IndexMap<String, String> = IndexMap::new();
        labels.insert("devcontainer.local_folder".to_string(), "/ws".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec!["app".to_string(), "db".to_string()],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(true),
            service_image_override: Some("deacon-features:abc123".to_string()),
            deacon_labels: labels,
        };

        let yaml = project.generate_injection_override().unwrap();

        // Exactly one `app:` mapping, exactly one `db:` mapping.
        assert_eq!(
            yaml.matches("  app:\n").count(),
            1,
            "primary service must appear once:\n{yaml}"
        );
        assert_eq!(
            yaml.matches("  db:\n").count(),
            1,
            "secondary run service must appear once:\n{yaml}"
        );
    }

    #[test]
    fn test_generate_injection_override_command_default_on() {
        // Spec default: override_command unset (None) is treated as true,
        // so an otherwise-empty override still injects the keep-alive command.
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: None,
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project
            .generate_injection_override()
            .expect("override command should produce override yaml even with no env/mounts");

        assert!(override_yaml.contains("services:\n  app:\n"));
        // The keep-alive must carry the SIGTERM trap and the background+`wait` shape,
        // not a bare foreground `sleep`: without them PID 1 cannot service SIGTERM and
        // `docker stop` burns the full 10s grace period before SIGKILL (024).
        assert!(
            override_yaml.contains("trap \\\"exit 0\\\" TERM INT;"),
            "keep-alive must trap SIGTERM: {override_yaml}"
        );
        assert!(
            override_yaml.contains("(sleep infinity || tail -f /dev/null) & wait $$!"),
            "keep-alive must background the sleep and wait on it: {override_yaml}"
        );
    }

    #[test]
    fn test_generate_injection_override_command_explicit_false() {
        // overrideCommand=false must run the service's natural command;
        // with no env/mounts the override yaml is None.
        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env: IndexMap::new(),
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        assert!(project.generate_injection_override().is_none());
    }

    #[test]
    fn test_generate_injection_override_command_with_env() {
        // override_command=true alongside env: both should appear in the yaml.
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("FOO".to_string(), "bar".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
            override_command: Some(true),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let override_yaml = project.generate_injection_override().unwrap();

        // The keep-alive must carry the SIGTERM trap and the background+`wait` shape,
        // not a bare foreground `sleep`: without them PID 1 cannot service SIGTERM and
        // `docker stop` burns the full 10s grace period before SIGKILL (024).
        assert!(
            override_yaml.contains("trap \\\"exit 0\\\" TERM INT;"),
            "keep-alive must trap SIGTERM: {override_yaml}"
        );
        assert!(
            override_yaml.contains("(sleep infinity || tail -f /dev/null) & wait $$!"),
            "keep-alive must background the sleep and wait on it: {override_yaml}"
        );
        assert!(override_yaml.contains("environment:"));
        assert!(override_yaml.contains("FOO: \"bar\""));
        // command must appear before environment (matches struct field order in yaml)
        let cmd_pos = override_yaml.find("command:").unwrap();
        let env_pos = override_yaml.find("environment:").unwrap();
        assert!(cmd_pos < env_pos);
    }

    /// #437: the `devcontainer.metadata` label records the configuration AS
    /// AUTHORED, so its `${localWorkspaceFolder}` templates must survive the
    /// generated override. Compose interpolates `${…}` in every file it reads and
    /// expands an unset variable to the empty string, so an unescaped `$` turned
    /// `source=${localWorkspaceFolder}/sib` into `source=/sib` on the container —
    /// a path that resolves nowhere. The single-container path never had the
    /// problem, which is why the six fixtures verifying #373 missed it.
    #[test]
    fn test_generate_injection_override_escapes_compose_interpolation() {
        let mut deacon_labels: IndexMap<String, String> = IndexMap::new();
        deacon_labels.insert(
            "devcontainer.metadata".to_string(),
            r#"[{"id":"./mountprobe","mounts":[{"source":"${localWorkspaceFolder}/featmnt","target":"/feat-mnt","type":"bind"}]},{"mounts":["source=${localWorkspaceFolder}/sib,target=/workspaces/sib,type=bind"]}]"#
                .to_string(),
        );
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("LITERAL".to_string(), "a${NOT_A_VAR}b".to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "app".to_string(),
            run_services: vec!["app".to_string(), "db".to_string()],
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
            override_command: Some(true),
            service_image_override: None,
            deacon_labels,
        };

        let yaml = project.generate_injection_override().unwrap();

        // Every `$` a value carries is doubled, on the primary service and on the
        // extra `runServices` block that repeats the same labels.
        assert_eq!(
            yaml.matches("source=$${localWorkspaceFolder}/sib").count(),
            2,
            "metadata label must reach Compose escaped on both services: {yaml}"
        );
        assert_eq!(
            yaml.matches(r#"\"source\":\"$${localWorkspaceFolder}/featmnt\""#)
                .count(),
            2,
            "feature-contributed mount source must be escaped too: {yaml}"
        );
        assert!(
            yaml.contains(r#"LITERAL: "a$${NOT_A_VAR}b""#),
            "env values are literals too — Compose must not expand them: {yaml}"
        );
        assert!(
            !yaml.contains("source=${localWorkspaceFolder}"),
            "an unescaped template would be interpolated away: {yaml}"
        );
    }

    #[test]
    fn test_escape_compose_value_doubles_dollars() {
        assert_eq!(escape_compose_value("plain"), "\"plain\"");
        assert_eq!(escape_compose_value("$"), "\"$$\"");
        assert_eq!(escape_compose_value("a${B}c"), "\"a$${B}c\"");
        // Already-doubled input is data, not an escape — it doubles again.
        assert_eq!(escape_compose_value("$$"), "\"$$$$\"");
    }

    /// Decode a double-quoted YAML scalar the way a YAML parser does.
    ///
    /// This is the independent inverse of [`escape_yaml_value`], written against
    /// the YAML 1.2 escape table (§7.3.1) rather than against the encoder, so a
    /// round-trip assertion is a real property check and not a restatement of the
    /// encoder's own rules. It exists so the round trip can be proven without
    /// adding a YAML dependency to `deacon-core`.
    ///
    /// It deliberately understands the escapes the ENCODER never emits — `\b`,
    /// `\0`, `\e`, `\u….` — because those are exactly what a mis-escaped value is
    /// silently reinterpreted as. `"value with \back slash"` decodes to a value
    /// carrying U+0008, which is the corruption #609 reported.
    fn decode_double_quoted_yaml_scalar(scalar: &str) -> String {
        let inner = scalar
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or_else(|| panic!("not a double-quoted scalar: {scalar}"));

        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }
            let esc = chars.next().expect("dangling escape");
            match esc {
                '0' => out.push('\0'),
                'a' => out.push('\u{7}'),
                'b' => out.push('\u{8}'),
                't' => out.push('\t'),
                'n' => out.push('\n'),
                'v' => out.push('\u{b}'),
                'f' => out.push('\u{c}'),
                'r' => out.push('\r'),
                'e' => out.push('\u{1b}'),
                '"' => out.push('"'),
                '/' => out.push('/'),
                '\\' => out.push('\\'),
                ' ' => out.push(' '),
                'x' | 'u' | 'U' => {
                    let width = match esc {
                        'x' => 2,
                        'u' => 4,
                        _ => 8,
                    };
                    let hex: String = (0..width)
                        .map(|_| chars.next().expect("truncated hex escape"))
                        .collect();
                    let code = u32::from_str_radix(&hex, 16).expect("bad hex escape");
                    out.push(char::from_u32(code).expect("bad scalar value"));
                }
                other => panic!("unrecognized YAML escape: \\{other}"),
            }
        }
        out
    }

    /// Values a quoting layer is liable to mangle. Reused by the exact-output and
    /// round-trip tests so neither can drift away from the other.
    const RISKY_YAML_SCALARS: &[&str] = &[
        // #609: the reported corruption. No newline, colon, hash, quote or edge
        // space, so it used to take the branch that quoted WITHOUT escaping, and
        // `\b` was read back as a backspace.
        r"value with \back slash",
        r"\",
        r"\\",
        // The two-character sequence backslash-n, NOT a newline.
        r"\n",
        r"C:\Users\dev",
        r"trailing\\",
        "say \"hi\"",
        // Compose interpolation escape: `$$` is data by the time it gets here.
        "[$${severity}] [$${node}]",
        "$",
        "plain",
        "",
        " leading",
        "trailing ",
        "key:value",
        "has # hash",
        "line1\nline2",
        "tab\there",
        "carriage\rreturn",
        "unicode: café 日本語 🦀",
        "bell\u{7}and\u{1b}escape",
        "nul\u{0}byte",
        "del\u{7f}",
    ];

    #[test]
    fn test_escape_yaml_value_exact_encoding() {
        // Every value is emitted as a double-quoted scalar, and a double-quoted
        // scalar processes escapes — so every value is escaped. Both halves of
        // that sentence must hold together; quoting without escaping (the #609
        // bug) is the one combination that is wrong.
        assert_eq!(escape_yaml_value("hello"), "\"hello\"");
        assert_eq!(escape_yaml_value(""), "\"\"");
        assert_eq!(escape_yaml_value("key:value"), "\"key:value\"");
        assert_eq!(escape_yaml_value(" leading"), "\" leading\"");
        assert_eq!(escape_yaml_value("line1\nline2"), "\"line1\\nline2\"");
        assert_eq!(escape_yaml_value("say \"hi\""), "\"say \\\"hi\\\"\"");

        // #609: a lone backslash is doubled, whatever else the value contains.
        assert_eq!(
            escape_yaml_value(r"value with \back slash"),
            r#""value with \\back slash""#
        );
        assert_eq!(escape_yaml_value(r"path\to\file"), r#""path\\to\\file""#);
        assert_eq!(escape_yaml_value(r"\"), r#""\\""#);

        // A control character has no raw spelling inside a double-quoted scalar.
        assert_eq!(escape_yaml_value("nul\u{0}byte"), r#""nul\x00byte""#);
        assert_eq!(escape_yaml_value("\u{1b}"), r#""\x1b""#);
        assert_eq!(escape_yaml_value("\u{7f}"), r#""\x7f""#);

        // Non-ASCII is passed through as-is: the override file is UTF-8.
        assert_eq!(escape_yaml_value("café 🦀"), "\"café 🦀\"");
    }

    #[test]
    fn test_escape_yaml_value_round_trips() {
        for value in RISKY_YAML_SCALARS {
            let encoded = escape_yaml_value(value);
            let decoded = decode_double_quoted_yaml_scalar(&encoded);
            assert_eq!(
                decoded, *value,
                "value did not survive the YAML round trip: {value:?} -> {encoded} -> {decoded:?}"
            );
        }
    }

    #[test]
    fn test_escape_compose_value_round_trips_through_yaml_then_interpolation() {
        // The full compose pipeline is two layers: the YAML parser reads the
        // scalar, then Compose's interpolation collapses `$$` to `$`. A value is
        // only correct if it survives BOTH.
        for value in RISKY_YAML_SCALARS {
            let encoded = escape_compose_value(value);
            let after_yaml = decode_double_quoted_yaml_scalar(&encoded);
            let after_interpolation = after_yaml.replace("$$", "$");
            assert_eq!(
                after_interpolation, *value,
                "value did not survive YAML + interpolation: {value:?} -> {encoded}"
            );
        }
    }

    #[test]
    fn test_compose_override_delivers_backslash_env_verbatim() {
        // The #609 reproduction at the level that actually ships: the generated
        // override text. Reading the emitted scalar back must yield the authored
        // value, not a backspace.
        let authored = r"value with \back slash";
        let mut additional_env: IndexMap<String, String> = IndexMap::new();
        additional_env.insert("VAR_WITH_BACK_SLASH".to_string(), authored.to_string());

        let project = ComposeProject {
            name: "test".to_string(),
            base_path: PathBuf::from("/test"),
            compose_files: vec![PathBuf::from("docker-compose.yml")],
            service: "myservice".to_string(),
            run_services: Vec::new(),
            env_files: Vec::new(),
            additional_mounts: Vec::new(),
            profiles: Vec::new(),
            additional_env,
            external_volumes: Vec::new(),
            override_command: Some(false),
            service_image_override: None,
            deacon_labels: IndexMap::new(),
        };

        let yaml = project.generate_injection_override().unwrap();
        let line = yaml
            .lines()
            .find(|l| l.trim_start().starts_with("VAR_WITH_BACK_SLASH:"))
            .expect("env line present");
        let scalar = line
            .split_once(':')
            .expect("key: value")
            .1
            .trim()
            .to_string();

        let delivered = decode_double_quoted_yaml_scalar(&scalar).replace("$$", "$");
        assert_eq!(
            delivered, authored,
            "the override must carry the backslash verbatim; got {delivered:?} from {scalar}"
        );
        assert!(
            !delivered.contains('\u{8}'),
            "a backspace means the backslash was reinterpreted (#609): {delivered:?}"
        );
    }

    // Tests for parse_external_volumes_from_config

    #[test]
    fn test_parse_external_volumes_empty_config() {
        // Empty JSON
        let result = parse_external_volumes_from_config("").unwrap();
        assert!(result.is_empty());

        // JSON with no volumes section
        let config = r#"{"services": {"app": {"image": "nginx"}}}"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert!(result.is_empty());

        // JSON with empty volumes section
        let config = r#"{"volumes": {}}"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_external_volumes_simple_form() {
        // external: true (simple boolean form)
        let config = r#"{
            "volumes": {
                "my_data": {
                    "external": true
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["my_data"]);
    }

    #[test]
    fn test_parse_external_volumes_object_form_with_name() {
        // external: { name: "actual-name" } (object form with explicit name)
        let config = r#"{
            "volumes": {
                "local_name": {
                    "external": {
                        "name": "actual-external-volume"
                    }
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["actual-external-volume"]);
    }

    #[test]
    fn test_parse_external_volumes_object_form_without_name() {
        // external: {} (object form without name, uses key name)
        let config = r#"{
            "volumes": {
                "my_volume": {
                    "external": {}
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["my_volume"]);
    }

    #[test]
    fn test_parse_external_volumes_multiple_volumes() {
        // Mix of external and non-external volumes
        let config = r#"{
            "volumes": {
                "external_vol1": {
                    "external": true
                },
                "local_vol": {
                    "driver": "local"
                },
                "external_vol2": {
                    "external": {
                        "name": "shared-data"
                    }
                },
                "another_local": {}
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"external_vol1".to_string()));
        assert!(result.contains(&"shared-data".to_string()));
    }

    #[test]
    fn test_parse_external_volumes_external_false() {
        // external: false should not be included
        let config = r#"{
            "volumes": {
                "not_external": {
                    "external": false
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_external_volumes_invalid_json() {
        // Invalid JSON should return an error
        let result = parse_external_volumes_from_config("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_external_volumes_realistic_config() {
        // A more realistic compose config output
        let config = r#"{
            "name": "myproject",
            "services": {
                "app": {
                    "image": "myapp:latest",
                    "volumes": [
                        {
                            "type": "volume",
                            "source": "app_data",
                            "target": "/data"
                        },
                        {
                            "type": "volume",
                            "source": "shared_cache",
                            "target": "/cache"
                        }
                    ]
                }
            },
            "volumes": {
                "app_data": {
                    "driver": "local"
                },
                "shared_cache": {
                    "external": true
                }
            }
        }"#;
        let result = parse_external_volumes_from_config(config).unwrap();
        assert_eq!(result, vec!["shared_cache"]);
    }

    #[test]
    fn test_parse_service_profiles_single_service() {
        let config = r#"{
            "services": {
                "app": {
                    "image": "ubuntu",
                    "profiles": ["dev"]
                },
                "db": {
                    "image": "postgres",
                    "profiles": ["dev", "test"]
                },
                "cache": {
                    "image": "redis",
                    "profiles": ["full"]
                }
            }
        }"#;
        let target_services = vec!["app".to_string(), "db".to_string()];
        let profiles = parse_service_profiles_from_config(config, &target_services).unwrap();
        assert_eq!(profiles, vec!["dev", "test"]);
    }

    #[test]
    fn test_parse_service_profiles_no_profiles() {
        let config = r#"{
            "services": {
                "app": {
                    "image": "ubuntu"
                },
                "db": {
                    "image": "postgres"
                }
            }
        }"#;
        let target_services = vec!["app".to_string(), "db".to_string()];
        let profiles = parse_service_profiles_from_config(config, &target_services).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_parse_service_profiles_deduplicates() {
        let config = r#"{
            "services": {
                "app": {
                    "image": "ubuntu",
                    "profiles": ["dev"]
                },
                "db": {
                    "image": "postgres",
                    "profiles": ["dev"]
                }
            }
        }"#;
        let target_services = vec!["app".to_string(), "db".to_string()];
        let profiles = parse_service_profiles_from_config(config, &target_services).unwrap();
        assert_eq!(profiles, vec!["dev"]);
    }

    #[test]
    fn test_parse_service_profiles_empty_input() {
        let profiles = parse_service_profiles_from_config("", &["app".to_string()]).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_parse_service_profiles_service_not_found() {
        let config = r#"{
            "services": {
                "web": {
                    "image": "nginx",
                    "profiles": ["prod"]
                }
            }
        }"#;
        let target_services = vec!["app".to_string()];
        let profiles = parse_service_profiles_from_config(config, &target_services).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_parse_service_profiles_no_services_section() {
        let config = r#"{ "volumes": {} }"#;
        let target_services = vec!["app".to_string()];
        let profiles = parse_service_profiles_from_config(config, &target_services).unwrap();
        assert!(profiles.is_empty());
    }

    #[test]
    fn test_parse_service_profiles_preserves_order() {
        // Profiles should appear in first-seen order across services
        let config = r#"{
            "services": {
                "app": {
                    "profiles": ["beta", "alpha"]
                },
                "db": {
                    "profiles": ["gamma", "alpha"]
                }
            }
        }"#;
        let target_services = vec!["app".to_string(), "db".to_string()];
        let profiles = parse_service_profiles_from_config(config, &target_services).unwrap();
        // "beta" and "alpha" from app, then "gamma" from db ("alpha" deduplicated)
        assert_eq!(profiles, vec!["beta", "alpha", "gamma"]);
    }

    /// BEAD-14a-T01: parser extracts `image:` shape from compose config.
    #[test]
    fn parse_service_shape_image_only() {
        let cfg = r#"{ "services": { "dev": { "image": "ubuntu:22.04" } } }"#;
        assert_eq!(
            parse_service_shape_from_config(cfg, "dev").unwrap(),
            ServiceShape::Image("ubuntu:22.04".to_string())
        );
    }

    /// BEAD-14a-T02: parser extracts `build:` object with context/dockerfile/target.
    #[test]
    fn parse_service_shape_build_object() {
        let cfg = r#"{
            "services": {
                "dev": {
                    "build": {
                        "context": "./svc",
                        "dockerfile": "Dockerfile",
                        "target": "runtime"
                    }
                }
            }
        }"#;
        assert_eq!(
            parse_service_shape_from_config(cfg, "dev").unwrap(),
            ServiceShape::Build {
                context: Some("./svc".into()),
                dockerfile: Some("Dockerfile".into()),
                target: Some("runtime".into()),
            }
        );
    }

    /// BEAD-14a-T03: parser handles `build:` as a bare string (shorthand context).
    #[test]
    fn parse_service_shape_build_shorthand_string() {
        let cfg = r#"{ "services": { "dev": { "build": "./ctx" } } }"#;
        assert_eq!(
            parse_service_shape_from_config(cfg, "dev").unwrap(),
            ServiceShape::Build {
                context: Some("./ctx".into()),
                dockerfile: None,
                target: None,
            }
        );
    }

    /// BEAD-14a-T04: `image:` wins when both `image:` and `build:` are present
    /// — compose semantics use `image:` as the published tag, which is what we
    /// extend with feature layers.
    #[test]
    fn parse_service_shape_image_wins_over_build() {
        let cfg = r#"{
            "services": {
                "dev": {
                    "image": "node:20-alpine",
                    "build": { "context": "." }
                }
            }
        }"#;
        assert_eq!(
            parse_service_shape_from_config(cfg, "dev").unwrap(),
            ServiceShape::Image("node:20-alpine".to_string())
        );
    }

    /// #619: the fallback `deacon build --image-name` tags off when a compose
    /// service authors no `image:` of its own must be Compose v2's default
    /// `<project>-<service>` (hyphen, not v1's underscore) — a wrong separator
    /// names an image the daemon does not have and `docker tag` fails.
    #[test]
    fn default_service_image_name_uses_compose_v2_separator() {
        assert_eq!(
            default_service_image_name("deacon_ws_abc123_def456", "app"),
            "deacon_ws_abc123_def456-app"
        );
    }

    /// BEAD-14a-T05: missing service → NotFound (not an error).
    #[test]
    fn parse_service_shape_missing_service() {
        let cfg = r#"{ "services": { "db": { "image": "postgres" } } }"#;
        assert_eq!(
            parse_service_shape_from_config(cfg, "dev").unwrap(),
            ServiceShape::NotFound
        );
    }

    /// BEAD-14a-T06: service exists but neither image nor build → Neither.
    #[test]
    fn parse_service_shape_neither_image_nor_build() {
        let cfg = r#"{ "services": { "dev": { "command": "sleep infinity" } } }"#;
        assert_eq!(
            parse_service_shape_from_config(cfg, "dev").unwrap(),
            ServiceShape::Neither
        );
    }

    /// BEAD-14a-T07: empty / whitespace-only JSON → NotFound rather than parse error,
    /// matching how the other parsers treat empty `docker compose config` output.
    #[test]
    fn parse_service_shape_empty_input() {
        assert_eq!(
            parse_service_shape_from_config("", "dev").unwrap(),
            ServiceShape::NotFound
        );
        assert_eq!(
            parse_service_shape_from_config("   \n  ", "dev").unwrap(),
            ServiceShape::NotFound
        );
    }

    /// BEAD-14a-T08: the injection override emits `image: <override>` when
    /// `service_image_override` is set, so `docker compose up` runs the feature-
    /// extended image instead of the original `image:` in the compose file.
    #[test]
    fn injection_override_includes_image_override_when_set() {
        let project = ComposeProject {
            name: "test".into(),
            base_path: PathBuf::from("/tmp"),
            compose_files: vec![PathBuf::from("compose.yml")],
            service: "dev".into(),
            run_services: vec![],
            env_files: vec![],
            additional_mounts: vec![],
            profiles: vec![],
            additional_env: IndexMap::new(),
            external_volumes: vec![],
            override_command: Some(false),
            service_image_override: Some("deacon-features:abc123".into()),
            deacon_labels: IndexMap::new(),
        };
        let yaml = project
            .generate_injection_override()
            .expect("override emitted");
        assert!(
            yaml.contains("image: \"deacon-features:abc123\""),
            "override YAML should set image; got:\n{}",
            yaml
        );
        assert!(yaml.contains("services:"));
        assert!(yaml.contains("  dev:"));
        // overrideCommand was false → no command line.
        assert!(!yaml.contains("command:"));
    }

    /// BEAD-14a-T09: setting `service_image_override` is sufficient to emit an
    /// override even when there are no mounts/env and overrideCommand=false.
    #[test]
    fn injection_override_emitted_for_image_only() {
        let project = ComposeProject {
            name: "test".into(),
            base_path: PathBuf::from("/tmp"),
            compose_files: vec![PathBuf::from("compose.yml")],
            service: "dev".into(),
            run_services: vec![],
            env_files: vec![],
            additional_mounts: vec![],
            profiles: vec![],
            additional_env: IndexMap::new(),
            external_volumes: vec![],
            override_command: Some(false),
            service_image_override: Some("img:tag".into()),
            deacon_labels: IndexMap::new(),
        };
        assert!(project.generate_injection_override().is_some());
    }

    /// BEAD-14a-T10: image tags with embedded double-quotes are escaped so they
    /// don't break the YAML.
    #[test]
    fn injection_override_escapes_quotes_in_image_tag() {
        let project = ComposeProject {
            name: "test".into(),
            base_path: PathBuf::from("/tmp"),
            compose_files: vec![PathBuf::from("compose.yml")],
            service: "dev".into(),
            run_services: vec![],
            env_files: vec![],
            additional_mounts: vec![],
            profiles: vec![],
            additional_env: IndexMap::new(),
            external_volumes: vec![],
            override_command: Some(false),
            service_image_override: Some(r#"weird"tag"#.into()),
            deacon_labels: IndexMap::new(),
        };
        let yaml = project.generate_injection_override().unwrap();
        assert!(yaml.contains(r#"image: "weird\"tag""#));
    }
}
