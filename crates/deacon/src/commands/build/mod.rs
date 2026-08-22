//! Build command implementation
//!
//! Implements the `deacon build` subcommand for building DevContainer images.
//! Follows the CLI specification for Docker integration.

pub mod result;

use crate::cli::{BuildKitOption, OutputFormat};
use crate::commands::shared::build_resolution::resolve_devcontainer_build_config;
use crate::commands::shared::lockfile::{
    LockfilePolicy, apply_lockfile_policy, ensure_lockfile_usable,
};
use crate::commands::shared::{ConfigLoadArgs, TerminalDimensions, load_config};
use anyhow::{Context, Result, anyhow};
use deacon_core::config::DevContainerConfig;
use deacon_core::errors::{DeaconError, DockerError};
use deacon_core::features::{FeatureMergeConfig, FeatureMerger, ResolvedFeature};
use deacon_core::host_ca::{CorporateCaSet, discover_corporate_set};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info, instrument, warn};

/// Build command arguments
#[derive(Debug, Clone)]
pub struct BuildArgs {
    pub no_cache: bool,
    pub platform: Option<String>,
    pub build_arg: Vec<String>,
    pub force: bool,
    pub output_format: OutputFormat,
    pub cache_from: Vec<String>,
    pub cache_to: Vec<String>,
    pub buildkit: Option<BuildKitOption>,
    /// Resolved build-output presentation (Compact/Inherit/Plain), computed once
    /// at the CLI tier from verbosity + TTY + log-format.
    pub build_output_mode: deacon_core::build::BuildOutputMode,
    pub secret: Vec<String>,
    pub build_secret: Vec<String>,
    pub ssh: Vec<String>,
    pub scan_image: bool,
    pub fail_on_scan: bool,
    pub workspace_folder: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    /// REPLACE base (`--override-config`): replaces the discovered config (#285).
    pub override_config_path: Option<PathBuf>,
    /// Settings-sourced merge fragments from the selected profile, deep-overlaid
    /// on the base (017). Empty ⇒ today's behavior.
    pub settings_merge_paths: Vec<PathBuf>,
    /// CLI `--merge-config` fragments, the highest-precedence merge layer.
    pub cli_merge_paths: Vec<PathBuf>,
    pub secrets_files: Vec<PathBuf>,
    pub additional_features: Option<String>,
    pub prefer_cli_features: bool,
    pub feature_install_order: Option<String>,
    pub ignore_host_requirements: bool,
    pub progress_tracker:
        std::sync::Arc<std::sync::Mutex<Option<deacon_core::progress::ProgressTracker>>>,
    pub redaction_config: deacon_core::redaction::RedactionConfig,
    pub secret_registry: deacon_core::redaction::SecretRegistry,
    /// `--env-file` paths handed to `docker compose` on the compose build paths. Parsed
    /// but unused until #572, which made them load-bearing: they participate in
    /// interpolating an authored `name: ${VAR}`, so `build` and `up` would otherwise
    /// resolve different compose project names from the same configuration.
    pub env_file: Vec<PathBuf>,
    #[allow(dead_code)] // Future: Will be used for custom docker executable path
    pub docker_path: String,
    /// Optional terminal dimension hint for output formatting
    #[allow(dead_code)] // Future: Will be used for terminal output formatting
    pub terminal_dimensions: Option<TerminalDimensions>,
    /// Image names to apply as tags
    pub image_names: Vec<String>,
    /// Metadata labels to apply in key=value format
    pub label: Vec<String>,
    /// Push image to registry after build
    pub push: bool,
    /// Export image to file or directory
    pub output: Option<String>,
    /// Drop `additional_features` entirely, resolving only the Features the configuration
    /// declared. A deacon extension (#498) with no counterpart in the reference CLI,
    /// which honors `--additional-features` unconditionally.
    pub ignore_additional_features: bool,
    /// Skip lockfile generation and verification (graduated 1.0).
    /// Consumed via [`crate::commands::shared::lockfile::LockfilePolicy`],
    /// the same decision `up` makes (#556).
    pub no_lockfile: bool,
    /// Require an up-to-date lockfile; fail if resolution would change it.
    /// Enforced twice, mirroring the reference: a pre-build refusal when the
    /// lockfile is missing (nothing is built and nothing is written), and a
    /// semantic comparison against the freshly-resolved set after the Feature
    /// layering pass (#556).
    pub frozen_lockfile: bool,

    /// Resolved host-CA injection activation (016). Resolved at the CLI tier
    /// from `--inject-host-ca` > `DEACON_INJECT_HOST_CA` > `settings.json`
    /// (never the workspace — FR-015).
    pub host_ca_activation: deacon_core::host_ca::HostCaActivation,

    /// Host user-data folder (global `--user-data-folder`); `None` → `~/.deacon`.
    /// Roots the build cache so it never lands inside the project (#280).
    pub user_data_folder: Option<PathBuf>,
}

impl Default for BuildArgs {
    fn default() -> Self {
        Self {
            no_cache: false,
            platform: None,
            build_arg: Vec::new(),
            force: false,
            output_format: OutputFormat::Text,
            build_output_mode: deacon_core::build::BuildOutputMode::default(),
            cache_from: Vec::new(),
            cache_to: Vec::new(),
            buildkit: None,
            secret: Vec::new(),
            build_secret: Vec::new(),
            ssh: Vec::new(),
            scan_image: false,
            fail_on_scan: false,
            workspace_folder: None,
            config_path: None,
            override_config_path: None,
            settings_merge_paths: Vec::new(),
            cli_merge_paths: Vec::new(),
            secrets_files: Vec::new(),
            additional_features: None,
            prefer_cli_features: false,
            feature_install_order: None,
            ignore_host_requirements: false,
            progress_tracker: std::sync::Arc::new(std::sync::Mutex::new(None)),
            redaction_config: deacon_core::redaction::RedactionConfig::default(),
            secret_registry: deacon_core::redaction::SecretRegistry::new(),
            env_file: Vec::new(),
            docker_path: "docker".to_string(),
            terminal_dimensions: None,
            image_names: Vec::new(),
            label: Vec::new(),
            push: false,
            output: None,
            ignore_additional_features: false,
            no_lockfile: false,
            frozen_lockfile: false,
            host_ca_activation: deacon_core::host_ca::HostCaActivation::Off,
            user_data_folder: None,
        }
    }
}

/// Build secret source type
#[derive(Debug, Clone, PartialEq)]
pub enum BuildSecretSource {
    /// Read secret from file
    File(PathBuf),
    /// Read secret from environment variable
    Env(String),
    /// Read secret from stdin
    Stdin,
}

/// Parsed build secret specification
#[derive(Debug, Clone)]
pub struct BuildSecret {
    /// Secret identifier (required)
    pub id: String,
    /// Secret source
    pub source: BuildSecretSource,
}

impl BuildSecret {
    /// Parse a build secret specification string
    ///
    /// Accepts formats:
    /// - `id=myid,src=/path/to/file`
    /// - `id=myid,env=ENV_VAR`
    /// - `id=myid` (stdin)
    pub fn parse(spec: &str) -> Result<Self> {
        let mut id: Option<String> = None;
        let mut src: Option<PathBuf> = None;
        let mut env: Option<String> = None;
        let mut stdin_flag: bool = false;

        // Parse key=value pairs and standalone flags
        for part in spec.split(',') {
            let part = part.trim();
            let kv: Vec<&str> = part.splitn(2, '=').collect();

            if kv.len() == 1 {
                // Standalone flag (no '=' found)
                match part {
                    "value-stdin" | "stdin" => {
                        stdin_flag = true;
                    }
                    _ => {
                        return Err(anyhow!(
                            "Unknown build secret parameter '{}'. Valid parameters are: id, src, env, value-stdin, stdin",
                            part
                        ));
                    }
                }
            } else {
                // Key=value pair
                let key = kv[0].trim();
                let value = kv[1].trim();

                match key {
                    "id" => {
                        if value.is_empty() {
                            return Err(anyhow!("Build secret id cannot be empty"));
                        }
                        id = Some(value.to_string());
                    }
                    "src" => {
                        if value.is_empty() {
                            return Err(anyhow!("Build secret src cannot be empty"));
                        }
                        src = Some(PathBuf::from(value));
                    }
                    "env" => {
                        if value.is_empty() {
                            return Err(anyhow!("Build secret env cannot be empty"));
                        }
                        env = Some(value.to_string());
                    }
                    _ => {
                        return Err(anyhow!(
                            "Unknown build secret parameter '{}'. Valid parameters are: id, src, env",
                            key
                        ));
                    }
                }
            }
        }

        // Validate required id
        let id = id.ok_or_else(|| anyhow!("Build secret must specify 'id' parameter"))?;

        // Validate that stdin_flag is not mixed with src or env
        if stdin_flag && (src.is_some() || env.is_some()) {
            return Err(anyhow!(
                "Build secret cannot specify 'value-stdin' or 'stdin' flag with 'src' or 'env' parameters"
            ));
        }

        // Determine source - prioritize in order: src, env, stdin (default or explicit)
        let source = if let Some(path) = src {
            if env.is_some() {
                return Err(anyhow!(
                    "Build secret cannot specify both 'src' and 'env' parameters"
                ));
            }
            BuildSecretSource::File(path)
        } else if let Some(env_var) = env {
            BuildSecretSource::Env(env_var)
        } else {
            BuildSecretSource::Stdin
        };

        Ok(Self { id, source })
    }

    /// Validate that the secret source is accessible
    pub fn validate(&self) -> Result<()> {
        match &self.source {
            BuildSecretSource::File(path) => {
                if !path.exists() {
                    return Err(anyhow!(
                        "Build secret file '{}' does not exist",
                        path.display()
                    ));
                }
                if !path.is_file() {
                    return Err(anyhow!(
                        "Build secret path '{}' is not a file",
                        path.display()
                    ));
                }
                // Check if file is readable
                std::fs::metadata(path)
                    .with_context(|| format!("Cannot read secret file '{}'", path.display()))?;
                Ok(())
            }
            BuildSecretSource::Env(env_var) => {
                if std::env::var(env_var).is_err() {
                    return Err(anyhow!(
                        "Build secret environment variable '{}' is not set",
                        env_var
                    ));
                }
                Ok(())
            }
            BuildSecretSource::Stdin => {
                // Stdin validation happens at read time
                Ok(())
            }
        }
    }

    /// Read the secret value from its source
    ///
    /// Returns the secret value as a string. The caller is responsible for
    /// registering the value with the redaction system.
    pub async fn read_value(&self) -> Result<String> {
        match &self.source {
            BuildSecretSource::File(path) => {
                let value = tokio::fs::read_to_string(path)
                    .await
                    .with_context(|| format!("Failed to read secret from '{}'", path.display()))?;
                Ok(value.trim().to_string())
            }
            BuildSecretSource::Env(env_var) => {
                let value = std::env::var(env_var).with_context(|| {
                    format!(
                        "Failed to read secret from environment variable '{}'",
                        env_var
                    )
                })?;
                Ok(value)
            }
            BuildSecretSource::Stdin => {
                use tokio::io::AsyncBufReadExt;
                let stdin = tokio::io::stdin();
                let mut reader = tokio::io::BufReader::new(stdin);
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .await
                    .context("Failed to read secret from stdin")?;
                Ok(line.trim().to_string())
            }
        }
    }

    /// Convert to Docker build argument format
    ///
    /// For file sources, returns the id and file path.
    /// For env/stdin sources, this requires the secret to be written to a temp file first.
    pub fn to_docker_arg(&self, temp_file: Option<&Path>) -> String {
        match &self.source {
            BuildSecretSource::File(path) => {
                format!("id={},src={}", self.id, path.display())
            }
            BuildSecretSource::Env(_) | BuildSecretSource::Stdin => {
                if let Some(temp_path) = temp_file {
                    format!("id={},src={}", self.id, temp_path.display())
                } else {
                    // Fallback - should not happen if properly handled
                    format!("id={}", self.id)
                }
            }
        }
    }
}

/// Build configuration extracted from DevContainer config
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Dockerfile path as specified by config
    pub dockerfile: String,
    /// Resolved Dockerfile path
    pub dockerfile_path: PathBuf,
    /// Build context path
    pub context: String,
    /// Directory containing the active devcontainer config
    pub context_folder: PathBuf,
    /// Build target (optional)
    pub target: Option<String>,
    /// `build.args` — one `--build-arg K=V` pair each.
    #[serde(default)]
    pub build_args: HashMap<String, String>,
    /// `build.options` — Docker CLI build options, forwarded verbatim as
    /// discrete argv elements (#492).
    #[serde(default)]
    pub options: Vec<String>,
}

/// Build result summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    /// Built image ID
    pub image_id: String,
    /// Image tags
    pub tags: Vec<String>,
    /// Build duration in seconds
    pub build_duration: f64,
    /// Image metadata/labels
    pub metadata: HashMap<String, String>,
    /// Configuration hash for caching
    pub config_hash: String,
    /// Subject DNs of corporate CAs injected at build time (016, FR-028).
    /// Additive; omitted (and defaulted on read) when injection was off or
    /// yielded zero certs so the default output stays byte-stable (FR-029).
    #[serde(
        rename = "injectedCaSubjects",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub injected_ca_subjects: Vec<String>,
    /// A tag private to THIS invocation, naming exactly the image this build
    /// produced (#470). See [`run_private_tag`] for why the deterministic
    /// `deacon-build:<hash>` tag is not a safe handle on it.
    ///
    /// Ephemeral: never serialized (a cached run's tag no longer exists) and
    /// never reported — it is dropped by [`drop_run_private_tag`] once the
    /// post-build passes that need the image are done.
    #[serde(skip)]
    pub private_ref: Option<String>,
}

/// Build metadata stored in cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetadata {
    /// Configuration hash
    pub config_hash: String,
    /// Build result
    pub result: BuildResult,
    /// Build inputs summary
    pub inputs: BuildInputs,
    /// When the build was created
    pub created_at: u64,
    /// The `--image-name`s the invocation that wrote this entry passed (#620).
    ///
    /// `--image-name` is an OUTPUT of the build, not an input to its identity, so
    /// it deliberately does NOT participate in `config_hash`. Recording it here is
    /// what lets a later cache hit tell the build's own names (the deterministic
    /// `deacon-build:<hash>` tag, or Compose's derived `<project>-<service>`) apart
    /// from names that were merely requested THEN — see [`reconcile_cached_tags`].
    ///
    /// Absent in entries written before #620; defaults to empty, which degrades to
    /// treating every recorded tag as build-owned.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_image_names: Vec<String>,
}

/// Build inputs tracked for cache invalidation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInputs {
    /// Dockerfile content hash
    pub dockerfile_hash: String,
    /// Build context files that affect the build
    pub context_files: Vec<ContextFile>,
    /// Feature set digest (if applicable)
    pub feature_set_digest: Option<String>,
    /// Build configuration
    pub build_config: BuildConfig,
}

/// A file in the build context that affects the build
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    /// Relative path from workspace root
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Last modified time (seconds since UNIX epoch)
    pub mtime: u64,
}

/// A failure whose `{"outcome": "error", …}` result document has already been
/// written, carrying the message the binary's own top-level handler should print.
///
/// `execute_build` renders one document for every failure (#594), and the
/// pre-flight refusals compose a RICHER one than an error chain can — each names
/// what to do about it in `description`. So they write their own and mark it, and
/// the wrapper leaves them alone rather than printing a second, worse document
/// after the good one. Two documents on stdout would break the output contract as
/// surely as zero did.
///
/// The marker IS the message rather than a context wrapped around one, so that
/// nothing internal reaches the user: an `anyhow` context would make the
/// top-level handler print a `Caused by: result document already written` line
/// under an otherwise clean text-mode diagnostic.
#[derive(Debug)]
struct ReportedFailure(String);

impl std::fmt::Display for ReportedFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReportedFailure {}

/// Write a build failure the way the requested output format asks for.
///
/// JSON mode puts the document on STDOUT (it is the command's result) and text
/// mode puts the diagnostic on stderr — the output-streams contract, unchanged.
fn write_error_document(output_format: &OutputFormat, error: &result::BuildError) {
    if matches!(output_format, OutputFormat::Json) {
        match serde_json::to_string(error) {
            Ok(json) => println!("{}", json),
            // Nothing in `BuildError` can fail to serialize, but swallowing the
            // failure would leave stdout empty, which is the defect this exists to
            // fix. Say so rather than say nothing.
            Err(e) => eprintln!("Error: could not render the result document: {}", e),
        }
    } else {
        eprintln!("Error: {}", error.message());
        if let Some(desc) = error.description() {
            eprintln!("{}", desc);
        }
    }
}

/// Write a pre-flight refusal's document and return the error to propagate.
///
/// One helper rather than four copies: every refusal has to print the same shape
/// in the same two modes and mark it as printed, and the copies had already
/// drifted apart in whether they used `?` on the serialization.
fn report_and_fail(
    output_format: &OutputFormat,
    error: result::BuildError,
    cause: impl Into<String>,
) -> anyhow::Error {
    write_error_document(output_format, &error);
    anyhow!(ReportedFailure(cause.into()))
}

/// Why BuildKit will not run this build's PRIMARY invocation, if it will not.
///
/// Distinct from [`deacon_core::build::buildkit::is_buildkit_available`], which
/// asks whether the host HAS BuildKit. Both questions have to be answered, and
/// answering only the first is what let `--buildkit never --platform <arch>`
/// build for the host architecture and exit 0 ([#592]).
///
/// It is also distinct from [`should_use_buildkit`], which cannot be reused here:
/// that returns `false` both when the user disabled BuildKit and when they simply
/// did not ask for it, and the second case still runs BuildKit on any modern
/// Docker — the primary build sets no `DOCKER_BUILDKIT` at all and the daemon's
/// own default applies. Gating on it would refuse the ordinary
/// `deacon build --platform linux/amd64`, which works.
///
/// So the question this answers is narrower and decidable: did something
/// EXPLICITLY turn BuildKit off?
///
/// [#592]: https://github.com/get2knowio/deacon/issues/592
fn buildkit_disabled(buildkit_option: Option<&BuildKitOption>) -> Option<&'static str> {
    if matches!(buildkit_option, Some(BuildKitOption::Never)) {
        return Some("--buildkit never");
    }
    // `--buildkit auto` and an unset flag both defer to the environment, and the
    // primary build passes that environment through untouched.
    match std::env::var("DOCKER_BUILDKIT") {
        Ok(value) if value == "0" || value.eq_ignore_ascii_case("false") => {
            Some("DOCKER_BUILDKIT=0")
        }
        _ => None,
    }
}

/// Helper function to validate BuildKit availability with consistent error handling.
///
/// `disabled_by` is `Some(cause)` when BuildKit is present but switched off for
/// this build. Only the flags the PRIMARY build carries are gated that way:
/// `--platform`, which the legacy builder ignores while reporting success;
/// `--cache-to`, which it rejects with a bare `unknown flag`; and, since #595,
/// `--output`, which now rides that build rather than a deferred `docker buildx
/// build` pass and is equally unknown to the legacy builder.
///
/// `--push` stays deliberately NOT gated: a single-platform build still defers it
/// ([`defers_publish`]) to a `docker push` over the local image, which works
/// however the build itself ran, so refusing it would break a working invocation
/// to no end. The reference refuses `--push` there; deacon honouring it is a
/// superset in the direction that gives the caller what they asked for.
async fn validate_buildkit_requirement(
    output_format: &OutputFormat,
    feature_name: &str,
    flag_name: &str,
    disabled_by: Option<&str>,
) -> Result<()> {
    if let Some(cause) = disabled_by {
        return Err(report_and_fail(
            output_format,
            result::BuildError::with_description(
                format!("BuildKit is required for {}", flag_name),
                format!(
                    "{} disables BuildKit; remove it or remove {}",
                    cause, flag_name
                ),
            ),
            format!("BuildKit is required for {feature_name} but {cause} disabled it"),
        ));
    }
    match deacon_core::build::buildkit::is_buildkit_available().await {
        Ok(true) => {
            // BuildKit available, proceed
            Ok(())
        }
        Ok(false) => Err(report_and_fail(
            output_format,
            result::BuildError::with_description(
                format!("BuildKit is required for {}", flag_name),
                format!("Enable BuildKit or remove {} flag", flag_name),
            ),
            format!("BuildKit is required for {feature_name}"),
        )),
        Err(e) => {
            // Failed to detect BuildKit
            Err(anyhow!("Failed to detect BuildKit: {}", e))
        }
    }
}

/// Execute the build command.
///
/// Loads the DevContainer configuration (from the provided path or by discovery),
/// validates host requirements, applies feature merges from CLI flags, and
/// derives a build configuration. It computes a deterministic configuration
/// hash, optionally uses a cached build result (unless `force` is set), and
/// performs a Docker build when needed. Progress events (BuildBegin / BuildEnd)
/// are emitted to the configured progress tracker and the build duration is
/// recorded. The final `BuildResult` is cached and printed in the requested
/// output format.
///
/// Errors are returned if configuration loading or validation fails, or if the
/// underlying build (Docker) fails when that feature is enabled.
///
/// # Examples
///
/// ```no_run
/// use deacon::commands::build::execute_build;
/// use deacon::commands::build::BuildArgs;
///
/// // Run the build in an async context (example uses Tokio).
/// #[tokio::main]
/// async fn main() {
///     let args = BuildArgs::default();
///     let _ = execute_build(args).await;
/// }
/// ```
#[instrument(skip(args))]
pub async fn execute_build(args: BuildArgs) -> Result<()> {
    let output_format = args.output_format.clone();
    let outcome = execute_build_inner(args).await;
    if let Err(err) = &outcome {
        // #594: a failure gets a result document too. Before this, `build
        // --output-format json` printed one on success and NOTHING on failure,
        // so a caller doing `| jq -r .outcome` got an empty stdout exactly when
        // it most needed the message — while the diagnostic sat on stderr, and
        // while `deacon up` and the reference CLI both printed one either way.
        //
        // Only in JSON mode. In text mode the diagnostic is already on stderr,
        // and the binary's own top-level handler renders the chain; printing
        // here as well would say everything twice.
        if matches!(output_format, OutputFormat::Json)
            && !err.chain().any(|e| e.is::<ReportedFailure>())
        {
            write_error_document(&output_format, &error_document(err));
        }
    }
    outcome
}

/// Render an arbitrary build failure as the result document.
///
/// `message` is the outermost context — what deacon was doing — and
/// `description` is the chain beneath it, which is where the actionable detail
/// lives (`Configuration file not found: …`, a builder's stderr, a lockfile's
/// parse error). Neither alone is enough: the outermost is too vague to act on
/// and the innermost has lost the operation it belongs to.
fn error_document(err: &anyhow::Error) -> result::BuildError {
    let causes: Vec<String> = err.chain().skip(1).map(|c| c.to_string()).collect();
    if causes.is_empty() {
        result::BuildError::new(err.to_string())
    } else {
        result::BuildError::with_description(err.to_string(), causes.join(": "))
    }
}

async fn execute_build_inner(mut args: BuildArgs) -> Result<()> {
    info!("Starting build command execution");
    debug!("Build args: {:?}", args);

    // Initialize progress tracking
    let emit_progress_event =
        crate::commands::shared::progress::make_progress_callback(&args.progress_tracker);

    // Parse and validate labels from key=value format
    let parsed_labels: Result<Vec<(String, String)>> = args
        .label
        .iter()
        .map(|label_str| {
            let parts: Vec<&str> = label_str.splitn(2, '=').collect();
            if parts.len() != 2 {
                Err(anyhow!(
                    "Invalid label format '{}'. Expected key=value",
                    label_str
                ))
            } else {
                // Validate label name
                deacon_core::docker::validate_label_name(parts[0])
                    .with_context(|| format!("Invalid label name in '{}'", label_str))?;
                Ok((parts[0].to_string(), parts[1].to_string()))
            }
        })
        .collect();
    let labels = parsed_labels?;

    // Validate image names
    for image_name in &args.image_names {
        deacon_core::docker::validate_image_tag(image_name)
            .with_context(|| format!("Invalid image name: {}", image_name))?;
    }

    // Drop duplicate `--image-name` values, preserving first-seen order. Passing
    // the same tag twice is harmless to Docker (which dedups `-t`), but without
    // this the emitted `imageName` array would echo the duplicate back to
    // callers. Normalizing here keeps the result JSON clean for every path.
    {
        let mut seen = std::collections::HashSet::new();
        args.image_names.retain(|name| seen.insert(name.clone()));
    }

    // Validate push/output mutual exclusivity early
    if args.push && args.output.is_some() {
        return Err(report_and_fail(
            &args.output_format,
            result::BuildError::with_description(
                "Cannot use both --push and --output",
                "They are mutually exclusive. Use --push to push to registry or --output to export locally",
            ),
            "Push and output are mutually exclusive",
        ));
    }

    // Whether something explicitly switched BuildKit off for the primary build.
    // Consulted only by the flags the primary invocation carries (#592).
    let buildkit_off = buildkit_disabled(args.buildkit.as_ref());

    // Validate BuildKit requirements for --push
    if args.push {
        validate_buildkit_requirement(&args.output_format, "push", "--push", None).await?;
    }

    // Validate BuildKit requirements for --output. `buildkit_off` applies here
    // since #595: `--output` rides the primary build now, and the legacy builder
    // has no such flag. It used to be deferred to a `docker buildx build` pass
    // that ran whatever the primary build did, which is why it was ungated.
    if args.output.is_some() {
        validate_buildkit_requirement(&args.output_format, "output", "--output", buildkit_off)
            .await?;
    }

    // Validate BuildKit requirements for --platform
    if args.platform.is_some() {
        validate_buildkit_requirement(&args.output_format, "platform", "--platform", buildkit_off)
            .await?;
    }

    // Validate BuildKit requirements for --cache-to
    if !args.cache_to.is_empty() {
        validate_buildkit_requirement(&args.output_format, "cache-to", "--cache-to", buildkit_off)
            .await?;
    }

    // Load configuration using shared helper for consistency with up/exec
    let load_result = load_config(ConfigLoadArgs {
        workspace_folder: args.workspace_folder.as_deref(),
        config_path: args.config_path.as_deref(),
        settings_merge_paths: &args.settings_merge_paths,
        cli_merge_paths: &args.cli_merge_paths,
        override_config_path: args.override_config_path.as_deref(),
        secrets_files: &args.secrets_files,
        resolve_devcontainer_id: true,
    })
    .await?;

    let mut config = load_result.config;
    let workspace_folder = load_result.workspace_folder;
    let config_path = load_result.config_path;

    // Stable per-workspace hash used to key the (host user-data) build cache so it
    // never lands inside the project and never collides across projects (#280).
    let workspace_hash =
        deacon_core::container::ContainerIdentity::new(&workspace_folder, &config).workspace_hash;

    debug!("Loaded configuration: {:?}", config.name);

    // Validate compose mode restrictions
    if config.uses_compose() {
        let unsupported_flags = [
            (args.push, "--push"),
            (args.output.is_some(), "--output"),
            (!args.cache_to.is_empty(), "--cache-to"),
            (args.platform.is_some(), "--platform"),
        ];

        for (flag_active, flag_name) in unsupported_flags {
            if flag_active {
                return Err(report_and_fail(
                    &args.output_format,
                    result::BuildError::with_description(
                        format!(
                            "Cannot use {} with Docker Compose configurations",
                            flag_name
                        ),
                        "Docker Compose does not support this flag during build",
                    ),
                    format!("{flag_name} is not supported with Docker Compose configurations"),
                ));
            }
        }
    }

    // Validate host requirements if specified in configuration
    if let Some(host_requirements) = &config.host_requirements {
        debug!("Validating host requirements");
        let mut evaluator = deacon_core::host_requirements::HostRequirementsEvaluator::new();

        // Advisory per spec: the evaluator warns when unmet and proceeds (never
        // errors); --ignore-host-requirements downgrades the warning to debug.
        match evaluator.validate_requirements(
            host_requirements,
            Some(&workspace_folder),
            args.ignore_host_requirements,
        ) {
            Ok(evaluation) => {
                if evaluation.requirements_met {
                    debug!("Host requirements validation passed");
                }
                debug!("Host evaluation: {:?}", evaluation);
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    } else {
        debug!("No host requirements specified in configuration");
    }

    // Apply feature merging if CLI features are provided
    if args.additional_features.is_some() || args.feature_install_order.is_some() {
        let merge_config = FeatureMergeConfig::new(
            args.additional_features.clone(),
            args.prefer_cli_features,
            args.feature_install_order.clone(),
            args.ignore_additional_features,
        );

        // Merge features
        config.features = Some(FeatureMerger::merge_features(
            config.features(),
            &merge_config,
        )?);
        debug!("Applied feature merging");

        // Update override feature install order if provided
        if let Some(effective_order) = FeatureMerger::get_effective_install_order(
            config.override_feature_install_order.as_ref(),
            &merge_config,
        )? {
            config.override_feature_install_order = Some(effective_order);
            debug!("Updated feature install order");
        }
    }

    // Extract build configuration
    let build_config = extract_build_config(&config, &config_path)?;
    debug!("Build config: {:?}", build_config);

    // Calculate configuration hash for caching
    let config_hash = calculate_config_hash(&build_config, &workspace_folder)?;
    debug!("Configuration hash: {}", config_hash);

    // Feature installation during build.
    //
    // Feature installation is supported for all configuration shapes:
    // - Dockerfile and image-reference builds splice the Feature stages into the
    //   SAME Dockerfile the base is described by and build both in ONE BuildKit
    //   invocation (`execute_single_container_build`). Nothing is handed between
    //   passes through a daemon-local tag, so any buildx driver can run it (#595).
    // - Compose builds resolve the target service's shape and build a
    //   feature-extended image via `execute_compose_build_with_features`
    //   (the same `resolve_compose_feature_image` helper the `up` flow uses).
    let features_present = !config.features().is_null()
        && config
            .features()
            .as_object()
            .is_some_and(|obj| !obj.is_empty());

    // #556: what the two lockfile flags ask for. Both were parsed and then
    // dropped until now — `build` wrote the lockfile unconditionally, which is
    // the precise inverse of what either flag requests.
    let lockfile_policy = LockfilePolicy::from_flags(args.no_lockfile, args.frozen_lockfile);

    // `--frozen-lockfile` with no lockfile on disk cannot be satisfied, so
    // refuse here rather than after a Feature-extended image has been built.
    // The reference refuses from its Feature-resolution pass, before the build,
    // and leaves the workspace clean; measured at oracle 0.87.0: exit 1,
    // `{"outcome":"error","message":"Lockfile does not exist."}`, no lockfile
    // written.
    ensure_lockfile_usable(lockfile_policy, &config_path, config.features()).await?;

    // Check cache if not forced (skip cache if pushing or exporting).
    // When features are present we deliberately skip the cache check: the
    // current `config_hash` does not include feature digests, so a cached
    // hit would point at a base image without the feature layers.
    // Re-running keeps correctness; a future refinement can fold the
    // feature digests into the hash for proper caching.
    if !args.force && !args.push && args.output.is_none() && !features_present {
        if let Some(cached) = check_build_cache(
            &config_hash,
            args.user_data_folder.as_deref(),
            &workspace_hash,
        )
        .await?
        {
            info!("Using cached build result");
            // #620: the image needs no rebuilding, but the TAGS this invocation
            // asked for still have to exist and still have to be what the result
            // document reports. Caching the build is right; caching the tagging is
            // not — the reference CLI tags a cached image with whatever
            // `--image-name` the current invocation passed.
            let reconciled = reconcile_cached_tags(cached, &args.image_names).await?;
            output_result(
                &reconciled,
                &args.output_format,
                &args.redaction_config,
                &args.secret_registry,
                false,
                None,
            )?;
            return Ok(());
        }
    }

    // Execute build
    let build_start_time = Instant::now();

    // Emit build begin event
    emit_progress_event(deacon_core::progress::ProgressEvent::BuildBegin {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        context: build_config.context.clone(),
        dockerfile: Some(build_config.dockerfile.clone()),
    })?;

    // Host-CA discovery for build-time injection (016, T039). Activation is
    // resolved at the CLI tier from machine-owner sources only (CLI flag > env >
    // settings) — never from the workspace config (FR-015); see the guard in
    // `resolve_host_ca_activation_cli`. Discover once and reuse for whichever
    // feature-layering path runs. An empty set means "nothing to inject".
    let host_ca_set: Option<CorporateCaSet> = if args.host_ca_activation.is_enabled() {
        let span = tracing::info_span!("ca.discover", mode = args.host_ca_activation.mode_str());
        let _guard = span.enter();
        let set = discover_corporate_set(&args.host_ca_activation)?;
        if set.is_empty() { None } else { Some(set) }
    } else {
        None
    };
    // FR-018a: build-time injection only happens when deacon generates a
    // feature-layering Dockerfile. Shapes without features (image-only,
    // compose-without-features, plain Dockerfile-without-features) generate no
    // such Dockerfile, so log the skip — runtime injection (`deacon up`) covers
    // them.
    if host_ca_set.is_some() && !features_present {
        info!(
            "Build-time host-CA injection skipped: this config shape generates no \
             feature-layering Dockerfile (FR-018a). Use `deacon up` for runtime injection."
        );
    }

    // Dispatch to appropriate build function based on configuration type
    let mut feature_lockfile: Option<PathBuf> = None;
    let result = if config.uses_compose() {
        if features_present {
            // Compose + features: build the feature-extended image for the target
            // service directly (shape-aware), tag it, and write the lockfile.
            execute_compose_build_with_features(
                &config,
                &load_result.raw_config,
                &args,
                &workspace_folder,
                &config_path,
                &labels,
                &config_hash,
                host_ca_set.as_ref(),
            )
            .await
        } else {
            execute_compose_build(
                &config,
                &args,
                &workspace_folder,
                &config_path,
                &labels,
                &config_hash,
            )
            .await
        }
    } else {
        match execute_single_container_build(
            &config,
            &load_result.raw_config,
            &args,
            &build_config,
            &config_hash,
            &workspace_folder,
            &config_path,
            &labels,
            host_ca_set.as_ref(),
            features_present,
            lockfile_policy,
        )
        .await
        {
            Ok((result, lockfile)) => {
                feature_lockfile = lockfile;
                Ok(result)
            }
            Err(e) => Err(e),
        }
    };
    let build_duration = build_start_time.elapsed();

    // Emit build end event
    let build_success = result.is_ok();
    let image_id = result.as_ref().ok().map(|r| r.image_id.clone());

    emit_progress_event(deacon_core::progress::ProgressEvent::BuildEnd {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        context: build_config.context.clone(),
        duration_ms: build_duration.as_millis() as u64,
        success: build_success,
        image_id,
    })?;

    let result = result?;

    // Record metrics
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            tracker.record_duration("build", build_duration);
        }
    }

    // A deferred `--push` happens here (#440). The image already carries its
    // `devcontainer.metadata` label — that is written by the build itself now,
    // not by a second pass — so all that is left is to publish what the user
    // named. It is held rather than propagated so the run-private tag is dropped
    // on the failure path too, or a push to an unreachable registry leaves a
    // `deacon-build-run:*` tag behind (#470).
    let post_build = async {
        if defers_publish(&args) && args.push {
            // Push what the user named. deacon's own `deacon-build:<hash>` bookkeeping
            // tag has no registry component, so pushing it targets Docker Hub (#438);
            // it is only reached when `--image-name` named nothing else to push, which
            // is what handing the push to BuildKit did.
            let targets: &[String] = if args.image_names.is_empty() {
                &result.tags
            } else {
                &args.image_names
            };
            push_built_image(targets).await?;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;

    // Unconditional: the tag has done its job whether that push succeeded or not.
    if let Some(private) = &result.private_ref {
        drop_run_private_tag(private).await;
    }
    post_build?;

    let final_result = BuildResult {
        image_id: result.image_id,
        metadata: result.metadata,
        tags: result.tags,
        // Ephemeral and just dropped: never reported, never cached.
        private_ref: None,
        build_duration: build_duration.as_secs_f64(),
        config_hash: config_hash.clone(),
        injected_ca_subjects: host_ca_set
            .as_ref()
            .map(|s| s.subjects.clone())
            .unwrap_or_default(),
    };

    if let Some(path) = feature_lockfile {
        debug!("Wrote feature lockfile to '{}'", path.display());
    }

    // Cache the result
    // The requested names travel with the entry so a later hit can tell them apart
    // from the tags this build owns (#620).
    cache_build_result(
        &final_result,
        &args.image_names,
        args.user_data_folder.as_deref(),
        &workspace_hash,
    )
    .await?;

    // Execute vulnerability scan if requested
    if args.scan_image {
        let scan_success =
            execute_vulnerability_scan(&args, &final_result.image_id, &emit_progress_event).await?;
        if !scan_success && args.fail_on_scan {
            return Err(anyhow::anyhow!(
                "Vulnerability scan failed and --fail-on-scan was set"
            ));
        }
    }

    // Output result
    output_result(
        &final_result,
        &args.output_format,
        &args.redaction_config,
        &args.secret_registry,
        args.push,
        args.output.as_deref(),
    )?;

    // Output final summary in debug mode
    if let Ok(tracker_guard) = args.progress_tracker.lock() {
        if let Some(tracker) = tracker_guard.as_ref() {
            if let Some(metrics_summary) = tracker.metrics_summary() {
                debug!("Metrics summary: {:?}", metrics_summary);
            }
        }
    }

    info!("Build command completed successfully");
    Ok(())
}

/// Extract build configuration from DevContainer config
fn extract_build_config(config: &DevContainerConfig, config_path: &Path) -> Result<BuildConfig> {
    let config_folder = config_path.parent().unwrap_or_else(|| Path::new("."));

    // Check if this is a compose-based configuration
    if config.uses_compose() {
        // For compose mode, we use the service name as a placeholder
        // Actual compose build will be handled by execute_compose_build
        let service = config.service.as_ref().ok_or_else(|| {
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "Docker Compose configuration must specify a service".to_string(),
            })
        })?;

        return Ok(BuildConfig {
            dockerfile: format!("compose-service-{}", service),
            dockerfile_path: config_folder.join(format!("compose-service-{}", service)),
            context: ".".to_string(),
            context_folder: config_folder.to_path_buf(),
            target: None,
            build_args: HashMap::new(),
            options: Vec::new(),
        });
    }

    if let Some(resolved) = resolve_devcontainer_build_config(config, config_path)? {
        return Ok(BuildConfig {
            dockerfile: resolved.dockerfile,
            dockerfile_path: resolved.dockerfile_path,
            context: resolved.context,
            context_folder: resolved.context_folder,
            target: resolved.target,
            build_args: resolved.build_args,
            options: resolved.options,
        });
    }

    if let Some(image) = &config.image {
        // For image-reference mode, create a build config that will generate a Dockerfile
        // Actual image-reference build will be handled by execute_image_reference_build
        Ok(BuildConfig {
            dockerfile: format!("image-reference-{}", image.replace([':', '/'], "-")),
            dockerfile_path: config_folder.join(format!(
                "image-reference-{}",
                image.replace([':', '/'], "-")
            )),
            context: ".".to_string(),
            context_folder: config_folder.to_path_buf(),
            target: None,
            build_args: HashMap::new(),
            options: Vec::new(),
        })
    } else {
        // No dockerfile or image specified
        Err(
            DeaconError::Config(deacon_core::errors::ConfigError::Validation {
                message: "No 'dockerFile' or 'image' specified in configuration".to_string(),
            })
            .into(),
        )
    }
}

/// Calculate configuration hash for caching
fn calculate_config_hash(build_config: &BuildConfig, _workspace_folder: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    // Hash the build config
    hasher.update(build_config.dockerfile.as_bytes());
    hasher.update(build_config.context.as_bytes());
    if let Some(target) = &build_config.target {
        hasher.update(target.as_bytes());
    }

    // Hash the build args in a deterministic order
    let mut build_args: Vec<_> = build_config.build_args.iter().collect();
    build_args.sort_by_key(|(k, _)| *k);
    for (key, value) in build_args {
        hasher.update(key.as_bytes());
        hasher.update(value.as_bytes());
    }

    // Hash `build.options` in AUTHORED order — they are positional argv
    // elements, so reordering them is a different build.
    for option in &build_config.options {
        hasher.update(option.as_bytes());
    }

    // Hash dockerfile content
    let dockerfile_path = build_config.dockerfile_path.clone();
    if dockerfile_path.exists() {
        let dockerfile_content = std::fs::read_to_string(&dockerfile_path)?;
        hasher.update(dockerfile_content.as_bytes());
    }

    // Hash selected build context files (limit count for performance)
    let context_path = build_config.context_folder.join(&build_config.context);
    if context_path.exists() {
        let mut build_affecting_files = Vec::new();

        // Collect files that affect the build, excluding non-affecting ones like README
        // Use a breadth-first, deterministic traversal
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(context_path.clone());

        while let Some(dir) = queue.pop_front() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut entries: Vec<_> = entries.flatten().collect();
                entries.sort_by_key(|e| e.path());

                // Process files first for this directory level
                for entry in &entries {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            if !is_non_build_affecting_file(file_name) {
                                if let Ok(metadata) = std::fs::metadata(&path) {
                                    build_affecting_files.push((
                                        path.strip_prefix(&context_path)
                                            .unwrap_or(&path)
                                            .to_string_lossy()
                                            .to_string(),
                                        metadata.len(),
                                        metadata
                                            .modified()
                                            .unwrap_or(std::time::UNIX_EPOCH)
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap_or_default()
                                            .as_secs(),
                                    ));
                                }
                            }
                        }
                    }
                    if build_affecting_files.len() >= 50 {
                        break;
                    }
                }

                // Then add directories to queue for next level processing
                if build_affecting_files.len() < 50 {
                    for entry in &entries {
                        let path = entry.path();
                        if path.is_dir() {
                            // Skip cache directories and other non-build-affecting directories
                            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                                if !is_non_build_affecting_directory(dir_name) {
                                    queue.push_back(path);
                                }
                            }
                        }
                    }
                }
            }
            if build_affecting_files.len() >= 50 {
                break;
            }
        }

        // Sort for deterministic hashing
        build_affecting_files.sort();
        for (path, size, mtime) in build_affecting_files {
            hasher.update(path.as_bytes());
            hasher.update(size.to_le_bytes());
            hasher.update(mtime.to_le_bytes());
        }
    }

    let hash = hasher.finalize();
    // Use first 16 hex chars for consistency with previous format
    Ok(format!(
        "{:016x}",
        u64::from_be_bytes(hash[0..8].try_into().unwrap())
    ))
}

/// Check if a file is unlikely to affect the build
fn is_non_build_affecting_file(filename: &str) -> bool {
    let filename_lower = filename.to_lowercase();
    matches!(
        filename_lower.as_str(),
        "readme"
            | "readme.md"
            | "readme.txt"
            | "readme.rst"
            | "changelog"
            | "changelog.md"
            | "changelog.txt"
            | "license"
            | "license.md"
            | "license.txt"
            | "authors"
            | "authors.md"
            | "authors.txt"
            | "contributors"
            | "contributors.md"
            | "contributors.txt"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
            | ".vscode"
            | ".idea"
            | ".git"
    ) || filename_lower.ends_with(".md") && !filename_lower.contains("dockerfile")
}

/// Check if a directory is unlikely to affect the build
fn is_non_build_affecting_directory(dirname: &str) -> bool {
    let dirname_lower = dirname.to_lowercase();
    matches!(
        dirname_lower.as_str(),
        ".git"
            | ".vscode" 
            | ".idea"
            | ".devcontainer"  // DevContainer config and cache directory
            | "node_modules"
            | ".pytest_cache"
            | "__pycache__"
            | ".mypy_cache"
            | "build-cache"  // Our own build cache directory
            | ".next"
            | ".nuxt"
            | "target"  // Rust build directory
            | "dist"
            | "coverage"
    )
}

/// A cache hit: the recorded build result plus the `--image-name`s the invocation
/// that recorded it asked for (#620).
struct CachedBuild {
    result: BuildResult,
    requested_image_names: Vec<String>,
}

/// Check for cached build result
async fn check_build_cache(
    config_hash: &str,
    user_data_folder: Option<&Path>,
    workspace_hash: &str,
) -> Result<Option<CachedBuild>> {
    let cache_file = match get_build_cache_path(user_data_folder, workspace_hash, config_hash) {
        Ok(p) => p,
        Err(e) => {
            // A missing user-data folder is a cache miss, never a build failure.
            debug!("Build cache directory unavailable: {}", e);
            return Ok(None);
        }
    };

    // Read cache file. NotFound is a normal cache-miss; other errors fall through too.
    let contents = match tokio::fs::read_to_string(&cache_file).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("No cache file found at {}", cache_file.display());
            return Ok(None);
        }
        Err(e) => {
            debug!("Failed to read cache file: {}", e);
            return Ok(None);
        }
    };

    match serde_json::from_str::<BuildMetadata>(&contents) {
        Ok(metadata) => {
            // Validate that the image still exists
            if is_image_available(&metadata.result.image_id).await? {
                debug!("Cache hit for config hash {}", config_hash);
                Ok(Some(CachedBuild {
                    result: metadata.result,
                    requested_image_names: metadata.requested_image_names,
                }))
            } else {
                debug!(
                    "Cached image {} no longer available, invalidating cache",
                    metadata.result.image_id
                );
                let _ = tokio::fs::remove_file(&cache_file).await;
                Ok(None)
            }
        }
        Err(e) => {
            debug!("Failed to deserialize cache metadata: {}", e);
            let _ = tokio::fs::remove_file(&cache_file).await;
            Ok(None)
        }
    }
}

/// Apply the `--image-name`s of the CURRENT invocation to a cached image, and
/// return the result document that invocation should report (#620).
///
/// A cache hit skips the build, which is correct — `--image-name` is an OUTPUT of
/// the build, not an input to its identity, so it never changes `config_hash`.
/// What is NOT correct is skipping the *tagging*: before this, a second `build`
/// with a different `--image-name` reported the name the FIRST build was given and
/// created no tag for the new one, so `docker inspect --type=image <new name>`
/// failed against an `"outcome": "success"` document that named it. The reference
/// CLI 0.87.0 tags a cached image with whatever the current invocation asked for.
///
/// Reconciliation keeps the tags the cached build OWNS — deacon's deterministic
/// `deacon-build:<hash>` tag on the single-container path, Compose's derived
/// `<project>-<service>` — and replaces the names that were merely *requested* by
/// the earlier run with the ones requested now. Previously requested tags are left
/// on the daemon rather than removed; the reference leaves them too.
async fn reconcile_cached_tags(cached: CachedBuild, requested: &[String]) -> Result<BuildResult> {
    let CachedBuild {
        mut result,
        requested_image_names: previously_requested,
    } = cached;

    // Retag unconditionally rather than only for names the cache does not already
    // carry: a name repeated across runs must still resolve, and the tag may have
    // been removed from the daemon since. `is_image_available` has just confirmed
    // `image_id` resolves.
    for name in requested {
        retag_image(&result.image_id, name).await?;
    }

    result.tags = reconciled_tags(&result.tags, &previously_requested, requested);
    Ok(result)
}

/// The tag list a cache hit should report: the tags the cached build OWNS, then
/// every `--image-name` this invocation passed, in the order it passed them.
///
/// Split out from [`reconcile_cached_tags`] so the ordering rules — which decide
/// what the result document says — are testable without a daemon.
fn reconciled_tags(
    cached_tags: &[String],
    previously_requested: &[String],
    requested: &[String],
) -> Vec<String> {
    let mut tags: Vec<String> = cached_tags
        .iter()
        .filter(|t| !previously_requested.contains(t) && !requested.contains(t))
        .cloned()
        .collect();
    tags.extend(requested.iter().cloned());

    // The one shape with no build-owned tag to fall back on: a Compose build whose
    // first run supplied `--image-name` (which suppresses the derived
    // `<project>-<service>` name) and whose second run supplied none. Report what
    // the cache recorded rather than an `imageName`-less document — those tags do
    // still name this image.
    if tags.is_empty() {
        return cached_tags.to_vec();
    }
    tags
}

/// Cache build result
async fn cache_build_result(
    result: &BuildResult,
    requested_image_names: &[String],
    user_data_folder: Option<&Path>,
    workspace_hash: &str,
) -> Result<()> {
    let cache_dir = match get_build_cache_dir(user_data_folder, workspace_hash) {
        Ok(d) => d,
        Err(e) => {
            debug!("Build cache directory unavailable, skipping cache: {}", e);
            return Ok(()); // Don't fail the build if caching is unavailable
        }
    };

    // Ensure cache directory exists
    if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
        debug!("Failed to create cache directory: {}", e);
        return Ok(()); // Don't fail the build if caching fails
    }

    // Create build inputs for metadata
    let inputs = create_build_inputs(result)?;

    let metadata = BuildMetadata {
        config_hash: result.config_hash.clone(),
        result: result.clone(),
        inputs,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        requested_image_names: requested_image_names.to_vec(),
    };

    let cache_file = cache_dir.join(format!("{}.json", result.config_hash));

    match serde_json::to_string_pretty(&metadata) {
        Ok(json) => {
            if let Err(e) = tokio::fs::write(&cache_file, json).await {
                debug!("Failed to write cache file: {}", e);
            } else {
                debug!("Cached build result to {}", cache_file.display());
            }
        }
        Err(e) => {
            debug!("Failed to serialize cache metadata: {}", e);
        }
    }

    Ok(())
}

/// Get the cache directory for builds.
///
/// Lives under the host user-data folder (`~/.deacon/build-cache/<workspace_hash>/`)
/// rather than inside the project (`<workspace>/.devcontainer/build-cache/`), so a
/// `deacon build` never leaves stray files in the user's repository (issue #280).
/// The `config_hash` is content-based and workspace-agnostic, so the cache is keyed
/// by an additional per-workspace subdir to avoid collisions across projects that
/// happen to share an identical Dockerfile/context.
fn get_build_cache_dir(user_data_folder: Option<&Path>, workspace_hash: &str) -> Result<PathBuf> {
    Ok(deacon_core::trust::user_data_root(user_data_folder)?
        .join("build-cache")
        .join(workspace_hash))
}

/// Get the cache file path for a specific config hash
fn get_build_cache_path(
    user_data_folder: Option<&Path>,
    workspace_hash: &str,
    config_hash: &str,
) -> Result<PathBuf> {
    Ok(
        get_build_cache_dir(user_data_folder, workspace_hash)?
            .join(format!("{}.json", config_hash)),
    )
}

/// Create build inputs for cache metadata
fn create_build_inputs(result: &BuildResult) -> Result<BuildInputs> {
    // For now, create a simplified version - full implementation would track more details
    let dockerfile_hash = result.config_hash.clone(); // Simplified
    let context_files = Vec::new(); // Would be populated from actual context scanning

    Ok(BuildInputs {
        dockerfile_hash,
        context_files,
        feature_set_digest: None, // TODO: Implement when features are integrated
        build_config: BuildConfig {
            dockerfile: "Dockerfile".to_string(), // Would be extracted from actual config
            dockerfile_path: PathBuf::from("Dockerfile"),
            context: ".".to_string(),
            context_folder: PathBuf::from("."),
            target: None,
            build_args: HashMap::new(),
            options: Vec::new(),
        },
    })
}

/// Check if a Docker image is available locally
async fn is_image_available(image_id: &str) -> Result<bool> {
    // Use docker inspect to check if image exists
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "--type=image", image_id])
        .output()
        .await;

    match output {
        Ok(output) => Ok(output.status.success()),
        Err(e) => {
            // If docker command fails, assume image is not available
            debug!("Failed to check image availability for {}: {}", image_id, e);
            Ok(false)
        }
    }
}

/// Whether the primary build runs through `docker buildx build` — which honours
/// the buildx builder the user selected — rather than `docker build`, which always
/// runs on the daemon's own "default" instance and ignores that selection (#595).
///
/// The gate is [`buildkit_disabled`], deliberately, and NOT
/// [`should_use_buildkit`]. The latter is false when nothing ASKED for BuildKit,
/// and that case still runs BuildKit on any modern Docker — deciding the
/// invocation form on it would put the ordinary `deacon build` back on the legacy
/// builder, which is the very defect #595 reports, reached by another route. Only
/// an explicit off-switch (`--buildkit never`, `DOCKER_BUILDKIT=0`) reaches
/// `docker build`, which is the sole route to the legacy builder.
fn uses_buildx(buildkit_option: Option<&BuildKitOption>) -> bool {
    buildkit_disabled(buildkit_option).is_none()
}

/// Detect if BuildKit should be used based on CLI flag and environment.
///
/// Narrower than [`uses_buildx`]: this answers whether the caller ASKED for
/// BuildKit, which decides whether to hand the child an explicit
/// `DOCKER_BUILDKIT` and whether the BuildKit-only input flags (`--secret`,
/// `--ssh`) are allowed. It is false when nothing asked and nothing refused, so it
/// must not be used to decide how the build is invoked.
fn should_use_buildkit(buildkit_option: Option<&BuildKitOption>) -> bool {
    match buildkit_option {
        Some(BuildKitOption::Auto) => {
            // Check DOCKER_BUILDKIT environment variable
            match std::env::var("DOCKER_BUILDKIT") {
                Ok(value) => value == "1" || value.to_lowercase() == "true",
                Err(_) => {
                    // Default to true for auto mode if no env var is set
                    // Modern Docker versions enable BuildKit by default
                    debug!("DOCKER_BUILDKIT not set, defaulting to BuildKit enabled for auto mode");
                    true
                }
            }
        }
        Some(BuildKitOption::Never) => false,
        None => {
            // Default behavior: respect DOCKER_BUILDKIT environment variable
            match std::env::var("DOCKER_BUILDKIT") {
                Ok(value) => value == "1" || value.to_lowercase() == "true",
                Err(_) => false, // Default to legacy build if no explicit setting
            }
        }
    }
}

/// Execute Compose build
#[instrument(skip(config, args, workspace_folder, labels))]
async fn execute_compose_build(
    config: &DevContainerConfig,
    args: &BuildArgs,
    workspace_folder: &Path,
    config_path: &Path,
    labels: &[(String, String)],
    config_hash: &str,
) -> Result<BuildResult> {
    use deacon_core::compose::ComposeManager;
    use std::time::Instant;

    let service = config
        .service
        .as_ref()
        .ok_or_else(|| anyhow!("Docker Compose configuration must specify a service"))?;

    info!("Building Docker Compose service: {}", service);

    let build_start = Instant::now();

    // Create compose project. Compose files resolve relative to the directory
    // containing devcontainer.json (spec parity), not the workspace folder.
    // Use the *resolved* config path (discovery may place it under
    // `.devcontainer/`); `args.config_path` is only the explicit `--config` flag.
    // `--env-file` is threaded through for the reason #572 gave it teeth: env files
    // participate in interpolating an authored `name: ${VAR}`, so a `build` that ignored
    // them would derive a DIFFERENT compose project name than `up --env-file` on the same
    // configuration. The flag documents itself as "passed to docker compose" and was
    // parsed-but-unused until now.
    let compose_manager = ComposeManager::new();
    let config_dir = config_path.parent().unwrap_or(workspace_folder);
    let project = compose_manager
        .create_project(config, workspace_folder, config_dir, &args.env_file)
        .await?;

    // Validate service exists
    if !compose_manager
        .validate_service_exists(&project, service)
        .await?
    {
        return Err(anyhow!(
            "Service '{}' not found in Docker Compose configuration",
            service
        ));
    }

    // Build the service, rendering its BuildKit output per the resolved mode.
    // This is the base compose-service build (no feature-install steps — feature
    // layering renders separately), so there are no feature ids to register.
    // Pause the spinner so the build's streaming renderer owns stderr.
    let _pause = crate::commands::shared::progress::SpinnerPause::new(&args.progress_tracker);
    let renderer = crate::ui::build_render::BuildRenderer::for_mode(
        args.build_output_mode,
        Vec::<&str>::new(),
    );
    let sink = renderer
        .as_ref()
        .map(|r| r as &dyn deacon_core::docker_retry::BuildLineSink);
    let build_result = compose_manager.build_service(&project, service, sink).await;
    if let Some(r) = &renderer {
        r.finish(build_result.is_ok());
    }
    let _build_output = build_result?;

    let build_duration = build_start.elapsed().as_secs_f64();

    info!("Docker Compose service built successfully: {}", service);

    // Generate image names - compose services typically use project-service naming
    let mut image_names = args.image_names.clone();
    if image_names.is_empty() {
        // Use default naming: project_service
        image_names.push(format!("{}-{}", project.name, service));
    }

    // Create metadata with labels
    let mut metadata = HashMap::new();
    for (key, value) in labels {
        metadata.insert(key.clone(), value.clone());
    }

    Ok(BuildResult {
        image_id: format!("{}-{}", project.name, service),
        tags: image_names,
        // Compose owns its own image naming (workspace-namespaced, so never
        // shared with a concurrent build); no run-private tag is minted (#470).
        private_ref: None,
        build_duration,
        metadata,
        // Non-features compose build generates no feature-layering Dockerfile,
        // so build-time host-CA injection does not apply here (FR-018a).
        injected_ca_subjects: Vec::new(),
        config_hash: config_hash.to_string(),
    })
}

/// Execute a Compose build that also installs declared features.
///
/// Unlike `execute_compose_build` (a plain `docker compose build`), this resolves
/// the target service's shape (`image:` vs `build:`) and builds a
/// feature-extended image via the same helper `up` uses
/// (`resolve_compose_feature_image`), then tags that image with the
/// deterministic `deacon-build:<hash>` tag plus any `--image-name`s and writes
/// the feature lockfile next to the config.
// `config` is the substituted configuration the build reads; `raw_config` is the
// same document as authored, which is what travels with the image (#373).
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, raw_config, args, workspace_folder, config_path, labels))]
async fn execute_compose_build_with_features(
    config: &DevContainerConfig,
    raw_config: &DevContainerConfig,
    args: &BuildArgs,
    workspace_folder: &Path,
    config_path: &Path,
    labels: &[(String, String)],
    config_hash: &str,
    host_ca_set: Option<&CorporateCaSet>,
) -> Result<BuildResult> {
    use crate::commands::up::compose::resolve_compose_feature_image;
    use deacon_core::compose::ComposeManager;
    use deacon_core::container::ContainerIdentity;

    let service = config
        .service
        .as_ref()
        .ok_or_else(|| anyhow!("Docker Compose configuration must specify a service"))?;

    let compose_manager = ComposeManager::new();
    // Compose files resolve relative to the config dir (spec parity). `--env-file` is
    // threaded for the same reason as `execute_compose_build` above: it participates in
    // resolving an authored `name: ${VAR}`, so ignoring it here would put `build` on a
    // different compose project than `up`.
    let config_dir = config_path.parent().unwrap_or(workspace_folder);
    let project = compose_manager
        .create_project(config, workspace_folder, config_dir, &args.env_file)
        .await?;
    if !compose_manager
        .validate_service_exists(&project, service)
        .await?
    {
        return Err(anyhow!(
            "Service '{}' not found in Docker Compose configuration",
            service
        ));
    }

    // Namespace the produced image by workspace (+ `-build`) so it does not
    // collide with `up`'s compose feature image on the same host.
    let identity = ContainerIdentity::new(workspace_folder, config);
    let workspace_hash = format!("{}-build", identity.workspace_hash);

    // Carry the resolved build-output mode into the feature build so it renders
    // like the other build paths (cache/builder options stay default here).
    let build_options = deacon_core::build::BuildOptions {
        output_mode: args.build_output_mode,
        ..Default::default()
    };
    let feature_build = resolve_compose_feature_image(
        config,
        &compose_manager,
        &project,
        workspace_folder,
        config_path,
        &workspace_hash,
        Some(&build_options),
        host_ca_set,
        // `deacon build` is docker-only today; podman parity for the build
        // command is tracked separately (issue #30 deferred items).
        &deacon_core::docker::CliDocker::new(),
        LockfilePolicy::from_flags(args.no_lockfile, args.frozen_lockfile),
        // #436: record `devcontainer.metadata` on the image this build produces.
        // The label rides the Feature build itself rather than a second build that
        // would have to `FROM` a daemon-local tag — the chain that made deacon pin
        // `--builder default` and override the user's builder (#595). `raw_config`
        // is the configuration as authored, because the label travels with the
        // image (#373).
        Some(raw_config),
    )
    .await?
    .ok_or_else(|| anyhow!("Compose feature build produced no image (no features declared?)"))?;

    // Tag the feature-extended image with the deterministic tag + user image names
    // so `--image-name` resolves to the image with features installed.
    let deterministic_tag = format!("deacon-build:{}", &config_hash[..12.min(config_hash.len())]);
    let mut all_tags = vec![deterministic_tag];
    all_tags.extend(args.image_names.clone());
    for tag in &all_tags {
        retag_image(&feature_build.image_tag, tag).await?;
    }

    // Apply the lockfile policy the flags selected (#556): skip under
    // `--no-lockfile`, compare-and-fail under `--frozen-lockfile`, otherwise
    // write next to the config (best-effort on a read-only FS).
    if let Some(path) = apply_lockfile_policy(
        LockfilePolicy::from_flags(args.no_lockfile, args.frozen_lockfile),
        config_path,
        &feature_build.lockfile,
    )
    .await?
    {
        debug!("Wrote feature lockfile to '{}'", path.display());
    }

    let mut metadata = HashMap::new();
    for (key, value) in labels {
        metadata.insert(key.clone(), value.clone());
    }
    if let Some(label) = &feature_build.metadata_label {
        metadata.insert("devcontainer.metadata".to_string(), label.clone());
    }

    Ok(BuildResult {
        image_id: feature_build.image_tag,
        tags: all_tags,
        // See the sibling compose result above: no run-private tag here (#470).
        private_ref: None,
        build_duration: 0.0,
        metadata,
        config_hash: config_hash.to_string(),
        injected_ca_subjects: host_ca_set.map(|s| s.subjects.clone()).unwrap_or_default(),
    })
}

/// RAII guard that removes a temporary directory on drop.
///
/// Covers the error/early-return (`?`), panic, and unwind paths that an explicit
/// end-of-function cleanup would miss, so deacon does not leave a
/// `.deacon-temp-build/` directory behind in the user's workspace when a build
/// fails partway through (issue #280).
struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        // Best-effort synchronous cleanup: the directory holds a single small
        // Dockerfile, so the blocking remove is negligible, and it is a no-op
        // when the happy-path async cleanup already removed it.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// What a single-container build adds on top of the base Dockerfile the
/// configuration describes: Feature layers, and the `devcontainer.metadata` the
/// produced image must carry.
///
/// All of it rides the ONE BuildKit invocation that produces the image. deacon
/// used to run three chained builds — base, then Features `FROM` the base's local
/// tag, then a metadata stamp `FROM` that — and every link but the first named an
/// image that exists only in the local daemon store. A `docker-container` driver
/// builder cannot read that store, so deacon pinned `--builder default` to make
/// the chain work, silently overriding the builder the user selected and taking
/// OCI export, local cache export and multi-platform output with it (#595); the
/// same chain is why a foreign `--platform` could not resolve its own base (#593).
/// With one build there is nothing to hand over and nothing to pin.
#[derive(Debug, Default)]
struct BuildOverlay {
    /// Replaces `-f`: the merged document holding the base Dockerfile's own
    /// stages followed by the Feature-install stage.
    dockerfile_path: Option<PathBuf>,
    /// Replaces `build.target`: the build must stop at the Feature stage.
    target: Option<String>,
    /// `--build-context` pairs the Feature RUN-mounts resolve against.
    extra_args: Vec<String>,
    /// The `devcontainer.metadata` value, computed BEFORE the build so BuildKit
    /// writes it in the pass that produces the image.
    metadata_label: Option<String>,
    /// Feature ids, for the build renderer's step labels.
    feature_ids: Vec<String>,
}

/// Build a single-container configuration — a user-authored Dockerfile or a bare
/// image reference — in one BuildKit invocation, Features and metadata included.
///
/// Returns the build result and the lockfile path the Feature resolution wrote,
/// if any.
///
/// Shape handling mirrors the reference CLI: an image-reference configuration is
/// a one-line `FROM <image>` Dockerfile in a temp context, a Dockerfile
/// configuration is the user's own document, and either way the Feature stages
/// are appended to that document rather than layered by a second build.
#[allow(clippy::too_many_arguments)]
#[instrument(skip(config, raw_config, args, build_config, labels, host_ca_set))]
async fn execute_single_container_build(
    config: &DevContainerConfig,
    raw_config: &DevContainerConfig,
    args: &BuildArgs,
    build_config: &BuildConfig,
    config_hash: &str,
    workspace_folder: &Path,
    config_path: &Path,
    labels: &[(String, String)],
    host_ca_set: Option<&CorporateCaSet>,
    features_present: bool,
    lockfile_policy: LockfilePolicy,
) -> Result<(BuildResult, Option<PathBuf>)> {
    use crate::commands::up::features_build::{
        FEATURE_TARGET_STAGE, base_stage_for_features, merge_dockerfile_with_feature_stage,
        prepare_feature_layer,
    };
    use deacon_core::container::ContainerIdentity;
    use deacon_core::docker::Docker;
    use deacon_core::dockerfile_utils::{find_user_statement, resolve_base_image};

    let cli = deacon_core::docker::CliDocker::new();

    // The base Dockerfile this build starts from, plus the hash that names its
    // deterministic tag. An image-reference configuration has no Dockerfile of its
    // own, so synthesize one into a temp context — NOT into the project, which a
    // `deacon build` must leave untouched (#280).
    let _temp_guard;
    let (effective, inner_hash) = match &config.image {
        Some(image) => {
            info!("Building from image reference: {}", image);
            let workspace_hash = ContainerIdentity::new(workspace_folder, config).workspace_hash;
            let temp_dir =
                std::env::temp_dir().join(format!("deacon-temp-build-{}", workspace_hash));
            tokio::fs::create_dir_all(&temp_dir).await?;
            // Guard cleanup against early `?` returns, panics, and unwinds. SIGKILL
            // can't be handled in-process; the next run's `create_dir_all` is idempotent.
            _temp_guard = Some(TempDirGuard::new(temp_dir.clone()));
            let dockerfile_path = temp_dir.join("Dockerfile");
            // No labels and no `devcontainer.metadata` written here (#436): the
            // user's `--label`s ride the build invocation, and the metadata label
            // is computed below from the entries this base image already carries.
            tokio::fs::write(&dockerfile_path, format!("FROM {}\n", image)).await?;
            (
                BuildConfig {
                    dockerfile: "Dockerfile".to_string(),
                    dockerfile_path,
                    context: ".".to_string(),
                    context_folder: temp_dir,
                    target: None,
                    build_args: HashMap::new(),
                    options: Vec::new(),
                },
                format!("image-ref-{}", image.replace([':', '/'], "-")),
            )
        }
        None => {
            _temp_guard = None;
            (build_config.clone(), config_hash.to_string())
        }
    };

    let base_content = tokio::fs::read_to_string(&effective.dockerfile_path)
        .await
        .with_context(|| {
            format!(
                "Failed to read Dockerfile '{}'",
                effective.dockerfile_path.display()
            )
        })?;

    // The EXTERNAL image this build derives from — what contributes inherited
    // `devcontainer.metadata` and the baked-in `USER` that `_CONTAINER_USER`
    // defaults to. For an image-reference configuration it is the reference
    // itself; for a Dockerfile it is whatever its target stage ultimately
    // `FROM`s, which is exactly what the reference resolves (`findBaseImage`).
    let base_image_ref = match &config.image {
        Some(image) => Some(image.clone()),
        None => resolve_base_image(
            &base_content,
            &effective.build_args,
            effective.target.as_deref(),
        ),
    };

    let mut overlay = BuildOverlay::default();
    let mut resolved_features: Vec<ResolvedFeature> = Vec::new();
    let mut lockfile_written = None;

    if features_present {
        // Refuse before resolving anything. Installing Features needs buildx's
        // named build contexts, so a disabled BuildKit cannot do it — and finding
        // that out after downloading every Feature and writing a lockfile would be
        // work done for a build that was never going to run.
        if !uses_buildx(args.buildkit.as_ref()) {
            return Err(DockerError::CLIError(
                "Installing Features requires BuildKit, which the current settings disable"
                    .to_string(),
            )
            .into());
        }

        let (staged_content, base_stage) =
            base_stage_for_features(&base_content, effective.target.as_deref()).with_context(
                || {
                    format!(
                        "Failed to locate a base stage in Dockerfile '{}' to install Features on",
                        effective.dockerfile_path.display()
                    )
                },
            )?;

        // Spec parity (#89): the four env vars every `install.sh` is guaranteed.
        // `remoteUser` is commonly declared by the base image's metadata rather
        // than the config, and the Dockerfile may `USER` its way somewhere else
        // again, so consult both before falling back to the config alone.
        let dockerfile_user = find_user_statement(
            &base_content,
            &effective.build_args,
            effective.target.as_deref(),
        );
        let feature_install_env = match &base_image_ref {
            Some(image) => {
                crate::commands::up::merged_config::resolve_feature_install_env(
                    &cli,
                    image,
                    config,
                    dockerfile_user.as_deref(),
                )
                .await
            }
            // Nothing inspectable to derive from (`FROM scratch`, an unset `ARG`):
            // the config and whatever the Dockerfile itself declares are all there is.
            None => deacon_core::dockerfile_generator::FeatureInstallEnv::resolve(
                config.remote_user.as_deref(),
                config.container_user.as_deref(),
                dockerfile_user.as_deref(),
            ),
        };

        // Namespace the staging directory by workspace+config so it does not
        // collide with `up`'s feature staging on the same host.
        let mut identity = ContainerIdentity::new(workspace_folder, config);
        identity.workspace_hash = format!("{}-build", identity.workspace_hash);

        let prepared = prepare_feature_layer(
            config,
            &identity,
            config_path,
            &base_stage,
            feature_install_env,
            host_ca_set,
            lockfile_policy,
        )
        .await?;

        let merged = merge_dockerfile_with_feature_stage(&staged_content, &prepared);
        let staging_root = crate::commands::shared::feature_resolver::feature_staging_root(
            &identity.workspace_hash,
        );
        tokio::fs::create_dir_all(&staging_root).await?;
        let merged_path = staging_root.join("Dockerfile.extended");
        tokio::fs::write(&merged_path, merged.as_bytes()).await?;
        debug!(
            dockerfile = %merged_path.display(),
            base_stage = %base_stage,
            "Wrote the merged base + Feature Dockerfile for a single build"
        );

        // Apply the lockfile policy the flags selected (#556). The default writes
        // next to the config file (spec §6 naming rule); `--no-lockfile` writes
        // nothing; `--frozen-lockfile` compares and fails on any difference.
        lockfile_written =
            apply_lockfile_policy(lockfile_policy, config_path, &prepared.lockfile).await?;

        overlay.feature_ids = prepared
            .resolved_features
            .iter()
            .map(|f| f.id.clone())
            .collect();
        resolved_features = prepared.resolved_features;
        overlay.extra_args = prepared.build_contexts;
        overlay.dockerfile_path = Some(merged_path);
        overlay.target = Some(FEATURE_TARGET_STAGE.to_string());
    }

    // #436: `devcontainer.metadata` — the entries a later `up` from this image, or
    // VS Code / Zed / envbuilder, read to learn what it carries. Computed here,
    // BEFORE the build, from the base image's own entries plus one per Feature
    // plus the config pick, and written by the build itself. Reading it off the
    // base rather than off the finished image is what makes the single-invocation
    // shape possible, and is what the reference does.
    if let Some(image) = &base_image_ref {
        // Best-effort: the entries are additive, so an image that cannot be
        // pulled contributes none rather than failing the build.
        if let Err(e) = cli.ensure_image_available(image).await {
            debug!(
                image = %image,
                error = %e,
                "Could not make the base image available for metadata inspection; \
                 proceeding with no inherited devcontainer.metadata entries"
            );
        }
    }
    let entries = crate::commands::up::merged_config::container_metadata_entries(
        &cli,
        base_image_ref.as_deref(),
        raw_config,
        &resolved_features,
    )
    .await;
    if entries.is_empty() {
        debug!("No devcontainer.metadata entries to record; leaving the label unset");
    } else {
        overlay.metadata_label = Some(
            serde_json::to_string(&serde_json::Value::Array(entries))
                .context("Failed to serialize the devcontainer.metadata label")?,
        );
    }

    let mut result = execute_docker_build(
        &effective,
        args,
        &inner_hash,
        workspace_folder,
        labels,
        &overlay,
    )
    .await?;

    // Keep the reported labels describing what the build actually wrote. When the
    // image is not local (a multi-platform export) there is nothing to inspect, so
    // report the label deacon asked BuildKit for.
    if let Some(label) = &overlay.metadata_label {
        result
            .metadata
            .entry("devcontainer.metadata".to_string())
            .or_insert_with(|| label.clone());
    }
    result.injected_ca_subjects = host_ca_set.map(|s| s.subjects.clone()).unwrap_or_default();

    Ok((result, lockfile_written))
}

/// Monotonic counter making [`run_private_tag`] unique across concurrent builds
/// inside ONE process, where the pid alone does not separate them.
static RUN_PRIVATE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Mint a tag private to this build invocation.
///
/// The deterministic `deacon-build:<hash>` tag is derived from the build's
/// CONTENT — the Dockerfile bytes plus each context file's relative path, size
/// and mtime — and deliberately not from the workspace path, so two builds of
/// identical content in different workspaces name the SAME tag. deacon then
/// resolved its own just-built image through references it did not own: the raw
/// `--iidfile` digest, and that shared tag.
///
/// Measured (#470): with concurrent builds of identical content, each
/// invocation's image carries a DISTINCT digest (BuildKit emits a per-build
/// attestation manifest, hence a distinct index digest) while all of them name
/// the one shared tag. Whichever build names it last leaves the others' images
/// unreferenced, and the containerd image store drops them — after which
/// `docker inspect <our-own-digest>` fails with `no such object` and the build
/// dies AFTER BuildKit reported success. The parity case
/// `case-build-output-export-tar` was the reliable victim precisely because it
/// passes no `--image-name`: its three concurrent fixture-sharing siblings each
/// hold a unique second tag that keeps their image alive, and it does not.
///
/// Adding this tag to the build invocation itself (not afterwards) closes the
/// window completely: BuildKit applies every `-t` in the same naming step, so
/// the image is referenced by a name no other process can take from the instant
/// it exists.
fn run_private_tag() -> String {
    let seq = RUN_PRIVATE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("deacon-build-run:{}-{}", std::process::id(), seq)
}

/// Drop a [`run_private_tag`] once the post-build passes that needed a stable
/// handle on the image are done.
///
/// Best-effort by design: the tag is bookkeeping, and failing a build that has
/// already produced (and possibly exported or pushed) its image because an
/// untag failed would be a worse outcome than leaving one dangling tag behind.
async fn drop_run_private_tag(tag: &str) {
    let removed = tokio::process::Command::new("docker")
        .args(["image", "rm", tag])
        .output()
        .await;
    match removed {
        Ok(out) if out.status.success() => debug!(tag = %tag, "Dropped the run-private build tag"),
        Ok(out) => debug!(
            tag = %tag,
            stderr = %String::from_utf8_lossy(&out.stderr).trim(),
            "Could not drop the run-private build tag; continuing"
        ),
        Err(e) => debug!(tag = %tag, error = %e, "Could not run 'docker image rm'; continuing"),
    }
}

/// Apply `target` as an additional tag on the local image `source`
/// (`docker tag source target`).
///
/// Used after the post-build feature pass to re-point the base build's tags at
/// the feature-extended image, so `--image-name` resolves to the image that
/// actually contains the installed features — and by [`reconcile_cached_tags`],
/// to apply a changed `--image-name` to an image served from the build cache (#620).
async fn retag_image(source: &str, target: &str) -> Result<()> {
    let output = tokio::process::Command::new("docker")
        .args(["tag", source, target])
        .output()
        .await
        .map_err(|e| {
            DeaconError::Docker(DockerError::CLIError(format!(
                "Failed to run 'docker tag {} {}': {}",
                source, target, e
            )))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DeaconError::Docker(DockerError::CLIError(format!(
            "Failed to tag '{}' as '{}': {}",
            source,
            target,
            stderr.trim()
        )))
        .into());
    }

    debug!("Re-tagged feature image '{}' as '{}'", source, target);
    Ok(())
}

/// Whether this build's `--push` happens AFTER the build, from the local daemon,
/// rather than being handed to BuildKit (#440).
///
/// Every single-platform build loads its result into the daemon, so pushing from
/// there costs nothing and keeps the pushed reference identical to the local one.
/// The one shape it cannot cover is a multi-platform build: BuildKit refuses to
/// `--load` a manifest list, so there is nothing local to push and the push stays
/// on the build invocation.
///
/// `--output` is never deferred: it rides the build itself, which is what lets an
/// exporter only a non-docker driver can serve work at all (#595).
fn defers_publish(args: &BuildArgs) -> bool {
    args.push
        && !args
            .platform
            .as_deref()
            .is_some_and(|platform| platform.contains(','))
}

/// Whether a buildx `--output` spec loads the result into the local daemon
/// (`type=docker` with no destination) rather than writing it somewhere else.
///
/// Used to avoid naming the `docker` exporter twice in one invocation when the
/// build wants to load as well as export (#440).
fn exports_to_daemon(spec: &str) -> bool {
    let mut kind = None;
    let mut has_destination = false;
    for field in spec.split(',') {
        match field.split_once('=') {
            Some(("type", value)) => kind = Some(value.trim()),
            Some(("dest", _)) | Some(("output", _)) => has_destination = true,
            _ => {}
        }
    }
    kind == Some("docker") && !has_destination
}

/// Push the tags a deferred `--push` build produced (#440).
///
/// The primary build no longer hands the push to BuildKit (see
/// [`defers_publish`]), so each target is pushed from the local daemon once the
/// image carries its `devcontainer.metadata` label. Progress streams as it
/// arrives, with `docker push`'s own stdout relayed to stderr so the
/// `--output-format json` contract (a single JSON document on stdout) holds.
async fn push_built_image(targets: &[String]) -> Result<()> {
    use tokio::io::AsyncBufReadExt;

    for target in targets {
        info!("Pushing '{}'", target);
        let mut child = tokio::process::Command::new("docker")
            .args(["push", target])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .with_context(|| format!("Failed to run 'docker push {}'", target))?;

        if let Some(stdout) = child.stdout.take() {
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            while let Some(line) = lines
                .next_line()
                .await
                .with_context(|| format!("Failed to read 'docker push {}' output", target))?
            {
                eprintln!("{}", line);
            }
        }

        let status = child
            .wait()
            .await
            .with_context(|| format!("Failed to wait for 'docker push {}'", target))?;

        if !status.success() {
            // The daemon's own diagnostic already went to stderr above; name the
            // step and the target so the failure is attributable.
            return Err(DeaconError::Docker(DockerError::CLIError(format!(
                "failed to push '{}': docker push exited with {}",
                target, status
            )))
            .into());
        }
        debug!(target = %target, "Pushed the built image");
    }
    Ok(())
}

/// Execute Docker build
#[instrument(skip(build_config, args, workspace_folder, labels))]
async fn execute_docker_build(
    build_config: &BuildConfig,
    args: &BuildArgs,
    config_hash: &str,
    workspace_folder: &Path,
    labels: &[(String, String)],
    overlay: &BuildOverlay,
) -> Result<BuildResult> {
    {
        use deacon_core::docker::{CliDocker, Docker};

        let docker = CliDocker::new();

        // Check Docker availability
        docker.check_docker_installed()?;
        docker.ping().await?;

        debug!("Building Docker image");

        // Prepare build context
        let context_path = build_config.context_folder.join(&build_config.context);
        // The overlay's merged document replaces the configuration's Dockerfile
        // when Feature layers were spliced into it.
        let dockerfile_path = overlay
            .dockerfile_path
            .clone()
            .unwrap_or_else(|| build_config.dockerfile_path.clone());

        // WHICH BUILDER runs this. `docker build` always runs on the "default"
        // instance — the daemon's own builder — so a builder the user selected with
        // `docker buildx use` is silently ignored and every capability only another
        // driver can serve (OCI export, local cache export, multi-platform output)
        // is out of reach (#595). `docker buildx build` honours the selection, and
        // is the reference CLI's own invocation.
        //
        // The gate is `buildkit_disabled`, NOT `should_use_buildkit`: the latter is
        // false when nothing ASKED for BuildKit, and that case still runs BuildKit
        // on any modern Docker (the daemon's own default). Deciding the invocation
        // form on it would put the ordinary `deacon build` back on the legacy
        // builder — which is exactly the defect, just reached by a different route.
        // Only an explicit off-switch (`--buildkit never`, `DOCKER_BUILDKIT=0`)
        // sends this to `docker build`, the sole route to the legacy builder.
        let use_buildx = uses_buildx(args.buildkit.as_ref());

        // Narrower question, unchanged: whether to hand the child an explicit
        // `DOCKER_BUILDKIT`, and whether the BuildKit-only INPUT flags
        // (`--secret`, `--ssh`) are allowed.
        let use_buildkit = should_use_buildkit(args.buildkit.as_ref());
        debug!(use_buildx, use_buildkit, "Resolved the build invocation");

        let mut build_args = if use_buildx {
            vec!["buildx".to_string(), "build".to_string()]
        } else {
            vec!["build".to_string()]
        };

        // Defer adding context until after all flags (Docker expects PATH last)

        // Add dockerfile
        build_args.push("-f".to_string());
        build_args.push(
            dockerfile_path
                .to_str()
                .ok_or_else(|| {
                    DeaconError::Docker(DockerError::CLIError(
                        "Invalid dockerfile path".to_string(),
                    ))
                })?
                .to_string(),
        );

        // Add no-cache flag
        if args.no_cache {
            build_args.push("--no-cache".to_string());
        }

        // Add platform
        if let Some(platform) = &args.platform {
            build_args.push("--platform".to_string());
            build_args.push(platform.clone());
        }

        // Add target. The Feature stage supersedes the configuration's own
        // `build.target`, which the Feature layers were stacked on top of.
        if let Some(target) = overlay.target.as_ref().or(build_config.target.as_ref()) {
            build_args.push("--target".to_string());
            build_args.push(target.clone());
        }

        // Add build args from config
        for (key, value) in &build_config.build_args {
            let build_arg_str = format!("{}={}", key, value);
            build_args.push("--build-arg".to_string());
            build_args.push(build_arg_str);
        }

        // Add `build.options` verbatim, right after the build args — the
        // position and pass-through semantics the reference CLI uses (measured
        // at 0.87.0: `options?.length && argv.push(...options)`; no filtering).
        // Each entry is its own argv element, never concatenated into a shell
        // line (#492).
        if !build_config.options.is_empty() {
            debug!("Adding config build.options: {:?}", build_config.options);
            build_args.extend(build_config.options.iter().cloned());
        }

        // Add build args from CLI
        for build_arg in &args.build_arg {
            build_args.push("--build-arg".to_string());
            build_args.push(build_arg.clone());
        }

        // Add cache-from options
        for cache_from in &args.cache_from {
            build_args.push("--cache-from".to_string());
            build_args.push(cache_from.clone());
        }

        // Add cache-to options
        for cache_to in &args.cache_to {
            build_args.push("--cache-to".to_string());
            build_args.push(cache_to.clone());
        }

        // Add secret forwarding
        for secret in &args.secret {
            build_args.push("--secret".to_string());
            build_args.push(secret.clone());
        }

        // Process and add build secrets
        let mut temp_secret_files = Vec::new();
        if !args.build_secret.is_empty() {
            debug!("Processing {} build secrets", args.build_secret.len());

            // Parse all build secrets
            let mut parsed_secrets = Vec::new();
            let mut seen_ids = HashSet::new();

            for spec in &args.build_secret {
                let secret = BuildSecret::parse(spec)
                    .with_context(|| format!("Failed to parse build secret spec: {}", spec))?;

                // Check for duplicate IDs
                if !seen_ids.insert(secret.id.clone()) {
                    return Err(anyhow!(
                        "Duplicate build secret id '{}'. Each secret must have a unique id.",
                        secret.id
                    ));
                }

                // Validate the secret source is accessible
                secret
                    .validate()
                    .with_context(|| format!("Build secret '{}' validation failed", secret.id))?;

                parsed_secrets.push(secret);
            }

            // Read all secret values first (before creating any temp files)
            // This allows early returns on errors without leaving temp files behind
            let mut secret_values = Vec::new();
            for secret in &parsed_secrets {
                let value = secret
                    .read_value()
                    .await
                    .with_context(|| format!("Failed to read build secret '{}'", secret.id))?;

                // Register the secret value for redaction
                if args.redaction_config.enabled {
                    debug!(
                        "Registering build secret '{}' for redaction (length: {})",
                        secret.id,
                        value.len()
                    );
                    args.secret_registry.add_secret(&value);
                }

                secret_values.push(value);
            }

            // Now create temp files and build args (after all validation succeeds)
            for (secret, value) in parsed_secrets.iter().zip(secret_values.iter()) {
                // For env and stdin sources, we need to write to a temp file
                let temp_file = match &secret.source {
                    BuildSecretSource::File(_) => None,
                    BuildSecretSource::Env(_) | BuildSecretSource::Stdin => {
                        let temp_file = tempfile::NamedTempFile::new()
                            .context("Failed to create temporary file for build secret")?;
                        tokio::fs::write(temp_file.path(), value)
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to write build secret '{}' to temporary file",
                                    secret.id
                                )
                            })?;
                        debug!(
                            "Wrote build secret '{}' to temp file: {}",
                            secret.id,
                            temp_file.path().display()
                        );
                        Some(temp_file)
                    }
                };

                // Generate the Docker argument
                let docker_arg = if let Some(ref temp) = temp_file {
                    secret.to_docker_arg(Some(temp.path()))
                } else {
                    secret.to_docker_arg(None)
                };

                build_args.push("--secret".to_string());
                build_args.push(docker_arg);

                // Store temp file to keep it alive during the build
                if let Some(temp) = temp_file {
                    temp_secret_files.push(temp);
                }
            }
        }

        // Add SSH forwarding
        for ssh in &args.ssh {
            build_args.push("--ssh".to_string());
            build_args.push(ssh.clone());
        }

        // The Feature RUN-mounts' named build contexts. A buildx-only flag, so the
        // legacy builder cannot install Features at all — say so rather than let it
        // fail on an `unknown flag`.
        if !overlay.extra_args.is_empty() {
            if !use_buildx {
                return Err(DockerError::CLIError(
                    "Installing Features requires BuildKit, which the current settings disable"
                        .to_string(),
                )
                .into());
            }
            build_args.extend(overlay.extra_args.iter().cloned());
        }

        // Secrets/SSH require BuildKit; provide a clear error early.
        if !use_buildkit
            && (!args.secret.is_empty() || !args.build_secret.is_empty() || !args.ssh.is_empty())
        {
            if args.buildkit == Some(BuildKitOption::Never) {
                return Err(DockerError::CLIError(
                    "The --secret/--build-secret/--ssh options require BuildKit but --buildkit never was specified"
                        .to_string(),
                )
                .into());
            }
            return Err(DockerError::CLIError(
                "The --secret/--build-secret/--ssh options require BuildKit. Re-run with --buildkit auto or set DOCKER_BUILDKIT=1"
                    .to_string(),
            )
            .into());
        }

        // Set DOCKER_BUILDKIT environment variable if needed
        let mut cmd = tokio::process::Command::new("docker");
        if use_buildkit {
            cmd.env("DOCKER_BUILDKIT", "1");
        } else if args.buildkit == Some(BuildKitOption::Never) {
            cmd.env("DOCKER_BUILDKIT", "0");
        }

        // Add deterministic tag with config hash
        let tag = format!("deacon-build:{}", &config_hash[..12]);
        build_args.push("-t".to_string());
        build_args.push(tag.clone());

        // Add user-specified image names as additional tags
        for image_name in &args.image_names {
            build_args.push("-t".to_string());
            build_args.push(image_name.clone());
        }

        // Add label with config hash
        let label = format!("org.deacon.configHash={}", config_hash);
        build_args.push("--label".to_string());
        build_args.push(label);

        // #436: `devcontainer.metadata`. The caller computed it from the base
        // image's own entries plus one per Feature plus the config pick, so it is
        // written by the build that produces the image rather than by a second
        // build that would have to `FROM` a daemon-local tag (#595).
        if let Some(metadata) = &overlay.metadata_label {
            build_args.push("--label".to_string());
            build_args.push(format!("devcontainer.metadata={}", metadata));
        }

        // Add user-specified labels
        for (key, value) in labels {
            build_args.push("--label".to_string());
            build_args.push(format!("{}={}", key, value));
        }

        // A multi-platform result cannot be loaded into the local daemon, so it is
        // the one shape that leaves nothing local behind. Everything else does,
        // which is what lets `--image-name` and the deterministic tag resolve and
        // what lets a `--push` be issued from the daemon after the fact (#440).
        let multi_platform = args
            .platform
            .as_deref()
            .is_some_and(|platform| platform.contains(','));
        let produces_local_image = !multi_platform;
        let defer_publish = args.push && produces_local_image;

        // #470: name this build's image with a tag no concurrent build can take,
        // so every post-build pass has a handle that survives a sibling
        // re-pointing the content-derived `deacon-build:<hash>` tag. It rides the
        // build invocation itself, not a later `docker tag`, so there is no window
        // in which our image is unreferenced. See `run_private_tag`.
        let private_ref = if produces_local_image {
            let private = run_private_tag();
            build_args.push("-t".to_string());
            build_args.push(private.clone());
            Some(private)
        } else {
            None
        };

        // Add --push flag if requested. A single-platform push is deferred to
        // `execute_build`, which pushes the loaded image from the daemon (#440).
        if args.push && !defer_publish {
            build_args.push("--push".to_string());
        }

        // `--output` rides THIS invocation — the only one there is. It used to be
        // deferred to a second build so the image could be stamped first, and that
        // second build could only run on the docker driver, which is why every
        // exporter the docker driver cannot serve was unreachable (#595).
        if let Some(output) = &args.output {
            build_args.push("--output".to_string());
            build_args.push(output.clone());
            // Also load, so the local tags (`--image-name` and the deterministic
            // one) name the image this build produced. `--load` alone is dropped as
            // a duplicate of an explicit `type=docker` spec, so name the exporter
            // directly — and only when the user's own spec is not already it.
            if produces_local_image && !exports_to_daemon(output) {
                build_args.push("--output".to_string());
                build_args.push("type=docker".to_string());
            }
        } else if use_buildx && produces_local_image {
            // With no exporter named, `--load` is what puts the result in the local
            // Docker daemon: buildx does not do this by default, and the legacy
            // builder (which always does) has no such flag.
            build_args.push("--load".to_string());
        }

        // Retrieve the image ID via `--iidfile` instead of `docker build -q`
        // stdout scraping. Dropping `-q` lets BuildKit progress stream to stderr
        // for the build-output UI while the digest still arrives reliably. Skipped
        // whenever an exporter is named — the `local` and `tar` exporters reject
        // the flag outright — and whenever no local image is produced at all. The
        // temp file must outlive the build invocation below.
        let iidfile = if produces_local_image && args.output.is_none() {
            let f =
                tempfile::NamedTempFile::new().context("Failed to create image ID temp file")?;
            build_args.push("--iidfile".to_string());
            build_args.push(f.path().display().to_string());
            Some(f)
        } else {
            None
        };

        // Finally add build context (must be last)
        build_args.push(
            context_path
                .to_str()
                .ok_or_else(|| {
                    DeaconError::Docker(DockerError::CLIError("Invalid context path".to_string()))
                })?
                .to_string(),
        );

        debug!("Docker build command: docker {}", build_args.join(" "));

        // Execute the build through the streaming executor so its output honors the
        // resolved mode (Compact/Inherit/Plain). The Feature-install steps are part
        // of THIS invocation, so their ids are registered with the renderer here.
        // `run_build_once` runs a single attempt (no retry) on our pre-configured
        // command (env vars + working dir).
        cmd.args(&build_args) // Pass all args including "build" subcommand
            .current_dir(workspace_folder);
        // Pause the spinner so the build's streaming renderer owns stderr.
        let _pause = crate::commands::shared::progress::SpinnerPause::new(&args.progress_tracker);
        let renderer = crate::ui::build_render::BuildRenderer::for_mode(
            args.build_output_mode,
            overlay.feature_ids.iter().map(String::as_str),
        );
        let output = deacon_core::docker_retry::run_build_once(
            cmd,
            crate::ui::build_render::io_for(&renderer),
        )
        .await?;
        if let Some(r) = &renderer {
            r.finish(output.status.success());
        }

        if !output.status.success() {
            // In Inherit mode stderr wasn't captured (it went to the terminal);
            // fall back to a generic message so we never print an empty error.
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = if stderr.trim().is_empty() {
                format!("exited with {}", output.status)
            } else {
                stderr.to_string()
            };
            return Err(DockerError::CLIError(format!("Docker build failed: {}", detail)).into());
        }

        // When using --push or --output, we may not get a local image ID; use a
        // tag reference. Otherwise read the digest from the `--iidfile` the
        // daemon wrote (replaces the former `-q` stdout scrape).
        let image_id = match iidfile {
            None => {
                // push/export path — the image may not be available locally
                if !args.image_names.is_empty() {
                    args.image_names[0].clone()
                } else {
                    tag.clone()
                }
            }
            Some(f) => {
                let id = tokio::fs::read_to_string(f.path())
                    .await
                    .context("Build succeeded but the image ID file could not be read")?
                    .trim()
                    .to_string();
                if id.is_empty() {
                    return Err(DockerError::CLIError(
                        "Build succeeded but wrote an empty image ID file".to_string(),
                    )
                    .into());
                }
                id
            }
        };

        // Extract image metadata (skip when this invocation pushed/exported
        // directly, as the image may not be local).
        //
        // Read through the run-private tag, never the raw digest: the digest is
        // unique per invocation, so a concurrent build of identical content
        // orphans it the moment it re-points the shared `deacon-build:<hash>`
        // tag, and the store then drops it (#470).
        let metadata = match &private_ref {
            Some(private) => extract_image_metadata(private).await?,
            None => HashMap::new(),
        };

        // Collect all tags: deterministic tag plus user-specified tags
        let mut all_tags = vec![tag];
        all_tags.extend(args.image_names.clone());

        let result = BuildResult {
            image_id,
            tags: all_tags,
            private_ref,
            build_duration: 0.0, // Will be set by caller
            metadata,
            config_hash: config_hash.to_string(),
            // Filled in by the caller, which knows the CA set this build was
            // handed (the RUN step is part of the Feature stage above).
            injected_ca_subjects: Vec::new(),
        };

        debug!("Docker build completed successfully");
        Ok(result)
    }
}

/// Extract image metadata using docker inspect
#[allow(dead_code)]
async fn extract_image_metadata(image_id: &str) -> Result<HashMap<String, String>> {
    debug!("Extracting metadata for image: {}", image_id);

    let output = tokio::process::Command::new("docker")
        .args(["inspect", "--format={{json .Config.Labels}}", image_id])
        .output()
        .await
        .map_err(|e| DockerError::CLIError(format!("Failed to inspect image: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DockerError::CLIError(format!("Docker inspect failed: {}", stderr)).into());
    }

    let labels_json = String::from_utf8_lossy(&output.stdout);
    let labels: HashMap<String, String> = if labels_json.trim() == "null" {
        HashMap::new()
    } else {
        serde_json::from_str(&labels_json)
            .map_err(|e| DockerError::CLIError(format!("Failed to parse image labels: {}", e)))?
    };

    debug!("Extracted {} labels from image", labels.len());
    Ok(labels)
}

/// Output build result in the specified format with redaction
fn output_result(
    result: &BuildResult,
    format: &OutputFormat,
    redaction_config: &deacon_core::redaction::RedactionConfig,
    registry: &deacon_core::redaction::SecretRegistry,
    pushed: bool,
    export_path: Option<&str>,
) -> Result<()> {
    use deacon_core::redaction::RedactingWriter;
    use std::io::Write;

    let stdout = std::io::stdout();
    let mut writer = RedactingWriter::new(stdout, redaction_config.clone(), registry);

    match format {
        OutputFormat::Json => {
            // Build spec-compliant JSON output
            // Deterministic fallback tag (first) should NOT be included when user supplied tags.
            // If user provided image names, they appear after the fallback tag.
            let display_tags: Vec<String> = if result.tags.len() > 1 {
                // Skip the first deterministic tag
                result.tags[1..].to_vec()
            } else {
                result.tags.clone()
            };

            // The reference CLI always emits `imageName` as an array, regardless of
            // tag count (issue #310). Emit a (possibly one-element) array for any
            // non-empty tag list rather than collapsing the single-tag case to a
            // bare string.
            let mut success_result = if display_tags.is_empty() {
                result::BuildSuccess::default()
            } else {
                result::BuildSuccess::new_multiple(display_tags)
            };

            // Add push status if --push was used
            if pushed {
                success_result = success_result.with_pushed(true);
            }

            // Add export path if --output was used
            if let Some(path) = export_path {
                success_result = success_result.with_export_path(path.to_string());
            }

            let json = serde_json::to_string(&success_result).map_err(|e| {
                DeaconError::Internal(deacon_core::errors::InternalError::Generic {
                    message: format!("Failed to serialize result to JSON: {}", e),
                })
            })?;
            writer.write_line(&json)?;
        }
        OutputFormat::Text => {
            writer.write_line("Build completed successfully!")?;
            if !result.image_id.is_empty() {
                writer.write_line(&format!("Image ID: {}", result.image_id))?;
            }
            writer.write_line(&format!("Tags: {}", result.tags.join(", ")))?;
            writer.write_line(&format!("Build duration: {:.2}s", result.build_duration))?;
            writer.write_line(&format!("Config hash: {}", result.config_hash))?;

            if pushed {
                writer.write_line("Image pushed to registry successfully")?;
            }

            if let Some(path) = export_path {
                writer.write_line(&format!("Image exported to: {}", path))?;
            }

            if !result.metadata.is_empty() {
                writer.write_line("Labels:")?;
                for (key, value) in &result.metadata {
                    writer.write_line(&format!("  {}: {}", key, value))?;
                }
            }
        }
    }

    writer.flush()?;
    Ok(())
}

/// Execute vulnerability scan on the built image
#[instrument(skip(args, emit_progress_event))]
async fn execute_vulnerability_scan<F>(
    args: &BuildArgs,
    image_id: &str,
    emit_progress_event: F,
) -> Result<bool>
where
    F: Fn(deacon_core::progress::ProgressEvent) -> Result<()>,
{
    // Get scan command from environment variable
    let scan_cmd_template = match std::env::var("DEACON_SCAN_CMD") {
        Ok(template) => template,
        Err(_) => {
            warn!("DEACON_SCAN_CMD environment variable not set, skipping vulnerability scan");
            return Ok(true); // Consider no scan command as success
        }
    };

    // Perform token substitution
    let scan_command = substitute_tokens(&scan_cmd_template, image_id)?;

    info!("Executing vulnerability scan: {}", scan_command);

    let scan_start_time = std::time::Instant::now();

    // Emit scan begin event
    emit_progress_event(deacon_core::progress::ProgressEvent::ScanBegin {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        image_id: image_id.to_string(),
        command: scan_command.clone(),
    })?;

    // Parse and execute the scan command
    let scan_result = execute_scan_command(&scan_command, args).await;
    let scan_duration = scan_start_time.elapsed();

    let (success, exit_code) = match scan_result {
        Ok(exit_code) => {
            let success = exit_code == 0;
            if success {
                info!("Vulnerability scan completed successfully");
            } else if args.fail_on_scan {
                warn!(
                    "Vulnerability scan failed with exit code {} (will fail build)",
                    exit_code
                );
            } else {
                warn!(
                    "Vulnerability scan failed with exit code {} (continuing build)",
                    exit_code
                );
            }
            (success, Some(exit_code))
        }
        Err(e) => {
            warn!("Failed to execute vulnerability scan: {}", e);
            (false, None)
        }
    };

    // Emit scan end event
    emit_progress_event(deacon_core::progress::ProgressEvent::ScanEnd {
        id: deacon_core::progress::ProgressTracker::next_event_id(),
        timestamp: deacon_core::progress::ProgressTracker::current_timestamp(),
        image_id: image_id.to_string(),
        duration_ms: scan_duration.as_millis() as u64,
        success,
        exit_code,
    })?;

    Ok(success)
}

/// Substitute tokens in the scan command template
pub fn substitute_tokens(template: &str, image_id: &str) -> Result<String> {
    let substituted = template.replace("{image}", image_id);
    debug!("Substituted scan command: {} -> {}", template, substituted);
    Ok(substituted)
}

/// Execute the scan command and return exit code
async fn execute_scan_command(command: &str, args: &BuildArgs) -> Result<i32> {
    use std::process::Stdio;

    // Parse command into program and arguments using shell-aware splitting
    let parts = shell_words::split(command)
        .map_err(|e| anyhow::anyhow!("Failed to parse scan command '{}': {}", command, e))?;
    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty scan command"));
    }

    let program = &parts[0];
    let command_args = &parts[1..];

    debug!(
        "Executing scan command: {} with args: {:?}",
        program, command_args
    );

    // Create redacting writer for scan output
    use deacon_core::redaction::RedactingWriter;
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut writer =
        RedactingWriter::new(stdout, args.redaction_config.clone(), &args.secret_registry);

    let mut cmd = tokio::process::Command::new(program);
    cmd.args(command_args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn scan command '{}': {}", program, e))?;

    // Read stdout and stderr in parallel
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout from scan command"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stderr from scan command"))?;

    let stdout_task = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        let mut output = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            output.push(line);
        }
        Ok::<Vec<String>, anyhow::Error>(output)
    });

    let stderr_task = tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        let mut output = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            output.push(line);
        }
        Ok::<Vec<String>, anyhow::Error>(output)
    });

    // Wait for command to complete
    let status = child
        .wait()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to wait for scan command: {}", e))?;

    // Collect output
    let stdout_lines = stdout_task
        .await
        .map_err(|e| anyhow::anyhow!("Failed to join stdout task: {}", e))?
        .context("Failed to read stdout from scan command")?;
    let stderr_lines = stderr_task
        .await
        .map_err(|e| anyhow::anyhow!("Failed to join stderr task: {}", e))?
        .context("Failed to read stderr from scan command")?;

    // Write output through redacting writer
    if !stdout_lines.is_empty() {
        writer.write_line("Scan stdout:")?;
        for line in &stdout_lines {
            writer.write_line(&format!("  {}", line))?;
        }
    }

    if !stderr_lines.is_empty() {
        writer.write_line("Scan stderr:")?;
        for line in &stderr_lines {
            writer.write_line(&format!("  {}", line))?;
        }
    }

    writer.flush()?;

    let exit_code = status.code().unwrap_or(-1);
    debug!("Scan command completed with exit code: {}", exit_code);

    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_defers_publish_covers_push_but_never_output() {
        // #440: a single-platform `--push` is issued from the daemon after the
        // build, which loaded the image. A multi-platform build cannot be
        // `--load`ed, so its push rides the build invocation.
        let local = BuildArgs::default();
        assert!(!defers_publish(&local), "a plain build publishes nothing");

        let push = BuildArgs {
            push: true,
            ..BuildArgs::default()
        };
        assert!(defers_publish(&push));

        // `--output` is NEVER deferred: it rides the build itself, which is what
        // lets an exporter only a non-docker driver can serve work at all (#595).
        let export = BuildArgs {
            output: Some("type=docker,dest=/tmp/out.tar".to_string()),
            ..BuildArgs::default()
        };
        assert!(
            !defers_publish(&export),
            "an export belongs to the build that produces the image"
        );

        let single_platform_push = BuildArgs {
            push: true,
            platform: Some("linux/amd64".to_string()),
            ..BuildArgs::default()
        };
        assert!(defers_publish(&single_platform_push));

        let multi_platform_push = BuildArgs {
            push: true,
            platform: Some("linux/amd64,linux/arm64".to_string()),
            ..BuildArgs::default()
        };
        assert!(
            !defers_publish(&multi_platform_push),
            "BuildKit will not --load a manifest list"
        );
    }

    #[test]
    fn test_exports_to_daemon_only_for_a_destinationless_docker_exporter() {
        // The deferred export pass names `type=docker` a second time so the local
        // tags follow the stamped image — but only when the user's own spec is
        // not already that exporter, which buildx would reject as a duplicate.
        assert!(exports_to_daemon("type=docker"));
        assert!(!exports_to_daemon("type=docker,dest=/tmp/out.tar"));
        assert!(!exports_to_daemon("type=oci,dest=/tmp/out.tar"));
        assert!(!exports_to_daemon("type=local,dest=/tmp/rootfs"));
        assert!(!exports_to_daemon("type=registry"));
        assert!(!exports_to_daemon("/tmp/rootfs"));
    }

    #[test]
    fn test_run_private_tag_is_unique_per_invocation() {
        // #470: the whole point of this tag is that no other build — in another
        // process or another task of this one — can be holding the same name.
        let a = run_private_tag();
        let b = run_private_tag();
        assert_ne!(a, b, "two mints must not collide within one process");
        for tag in [&a, &b] {
            assert!(
                tag.starts_with("deacon-build-run:"),
                "the private tag must be namespaced away from `deacon-build:<hash>`, got {tag}"
            );
            assert!(
                tag.contains(&std::process::id().to_string()),
                "the private tag must carry this pid so concurrent processes differ, got {tag}"
            );
        }
    }

    #[test]
    fn test_run_private_tag_is_never_serialized_into_the_build_cache() {
        // #470: a cached result replays a tag that was dropped when its build
        // ended, so the field must not round-trip through the cache file.
        let result = BuildResult {
            image_id: "sha256:abc".to_string(),
            tags: vec!["deacon-build:abc123456789".to_string()],
            private_ref: Some("deacon-build-run:1-0".to_string()),
            build_duration: 1.0,
            metadata: HashMap::new(),
            config_hash: "abc123456789".to_string(),
            injected_ca_subjects: Vec::new(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(
            !json.contains("deacon-build-run"),
            "the run-private tag must not be serialized; got {json}"
        );
        let round_tripped: BuildResult = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.private_ref, None);
    }

    #[test]
    fn test_temp_dir_guard_removes_dir_on_drop() {
        // Regression for #280: an image-reference build that fails partway (early
        // `?` / panic) must not leave `.deacon-temp-build/` behind. The RAII guard
        // removes the directory whenever it drops, not just on the happy path.
        let parent = tempfile::tempdir().unwrap();
        let temp_dir = parent.path().join(".deacon-temp-build");
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("Dockerfile"), "FROM alpine:3.19\n").unwrap();
        assert!(temp_dir.exists());

        {
            let _guard = TempDirGuard::new(temp_dir.clone());
            // guard drops at end of this scope, simulating an early return / unwind
        }

        assert!(
            !temp_dir.exists(),
            "TempDirGuard must remove the temp build dir on drop"
        );
    }

    #[test]
    fn test_temp_dir_guard_drop_is_noop_when_already_removed() {
        // The happy path removes the dir explicitly (async) before the guard drops;
        // the guard's drop must then be a harmless no-op.
        let parent = tempfile::tempdir().unwrap();
        let temp_dir = parent.path().join(".deacon-temp-build");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let guard = TempDirGuard::new(temp_dir.clone());
        std::fs::remove_dir_all(&temp_dir).unwrap();
        drop(guard); // must not panic even though the dir is already gone
        assert!(!temp_dir.exists());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_build_config_dockerfile_parsing() {
        let mut config = DevContainerConfig::default();
        config.name = Some("test".to_string());
        config.dockerfile = Some("Dockerfile".to_string());

        // Test with simple dockerfile
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\nLABEL test=1\n").unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let result = extract_build_config(&config, &config_path);
        assert!(result.is_ok());
        let build_config = result.unwrap();
        assert_eq!(build_config.dockerfile, "Dockerfile");
        assert_eq!(build_config.context, ".");

        // Test with build configuration. `build.options` is an ARRAY of Docker
        // CLI build options (#492) — kept in authored order, distinct from the
        // `build.args` map.
        config.build = Some(serde_json::json!({
            "context": "docker",
            "target": "development",
            "options": [ "--label", "test_build_options=success" ]
        }));

        let result = extract_build_config(&config, &config_path);
        assert!(result.is_ok());
        let build_config = result.unwrap();
        assert_eq!(build_config.context, "docker");
        assert_eq!(build_config.target, Some("development".to_string()));
        assert_eq!(
            build_config.options,
            vec!["--label", "test_build_options=success"]
        );
        assert!(build_config.build_args.is_empty());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_build_config_nested_build_dockerfile() {
        // The canonical containers.dev form nests the Dockerfile under `build`:
        //   "build": { "dockerfile": "Dockerfile" }
        // `deacon build` must honor it just like the legacy top-level `dockerFile`
        // field (parity with `up`'s image_build path).
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\nLABEL test=1\n").unwrap();
        let config_path = temp_dir.path().join("devcontainer.json");

        let mut config = DevContainerConfig::default();
        config.name = Some("test".to_string());
        // No top-level `dockerFile` — only the nested `build.dockerfile`.
        config.build = Some(serde_json::json!({
            "dockerfile": "Dockerfile",
            "context": ".",
            "args": { "FOO": "bar" }
        }));

        let result = extract_build_config(&config, &config_path);
        assert!(
            result.is_ok(),
            "build.dockerfile should be recognized: {:?}",
            result.err()
        );
        let build_config = result.unwrap();
        assert_eq!(build_config.dockerfile, "Dockerfile");
        assert_eq!(build_config.context, ".");
        assert_eq!(build_config.build_args.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_build_config_resolves_dockerfile_relative_to_config_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config_dir = temp_dir.path().join(".devcontainer");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("Dockerfile"), "FROM alpine:3.19\n").unwrap();
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM busybox:1.36\n").unwrap();
        let config_path = config_dir.join("devcontainer.json");

        let mut config = DevContainerConfig::default();
        config.name = Some("test".to_string());
        config.build = Some(serde_json::json!({
            "dockerfile": "Dockerfile",
            "context": ".."
        }));

        let build_config = extract_build_config(&config, &config_path).unwrap();
        assert_eq!(build_config.dockerfile_path, config_dir.join("Dockerfile"));
        assert_eq!(build_config.context_folder, config_dir);
        assert_eq!(build_config.context, "..");
    }

    #[test]
    fn test_image_name_dedup_preserves_first_seen_order() {
        // Mirror the normalization in execute_build: duplicate `--image-name`
        // values collapse to first-seen order.
        let mut image_names = vec![
            "myorg/dups:latest".to_string(),
            "myorg/dups:latest".to_string(),
            "myorg/other:v1".to_string(),
            "myorg/dups:latest".to_string(),
        ];
        let mut seen = std::collections::HashSet::new();
        image_names.retain(|name| seen.insert(name.clone()));
        assert_eq!(
            image_names,
            vec![
                "myorg/dups:latest".to_string(),
                "myorg/other:v1".to_string()
            ]
        );
    }

    #[test]
    fn test_config_hash_calculation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: Some("dev".to_string()),
            build_args: {
                let mut map = HashMap::new();
                map.insert("ARG1".to_string(), "value1".to_string());
                map.insert("ARG2".to_string(), "value2".to_string());
                map
            },
            options: vec!["--label".to_string(), "x=y".to_string()],
        };

        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\n").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Same config should produce same hash
        assert_eq!(hash1, hash2);

        // Different config should produce different hash
        let mut build_config2 = build_config.clone();
        build_config2.dockerfile = "Dockerfile.dev".to_string();

        let hash3 = calculate_config_hash(&build_config2, temp_dir.path()).unwrap();
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_build_args_assembly() {
        let args = BuildArgs {
            cache_from: vec![
                "registry://example.com/cache".to_string(),
                "type=local,src=/tmp/cache".to_string(),
            ],
            cache_to: vec!["registry://example.com/cache:latest".to_string()],
            buildkit: Some(BuildKitOption::Auto),
            secret: vec![
                "id=mypassword,src=./password.txt".to_string(),
                "id=mytoken".to_string(),
            ],
            ssh: vec!["default".to_string(), "mykey=/path/to/key".to_string()],
            ..Default::default()
        };

        // Verify args are structured correctly
        // Defaults currently retain cache and have no platform/build args set
        assert!(!args.no_cache);
        assert_eq!(args.platform, None);
        assert!(args.build_arg.is_empty());
    }

    #[test]
    fn test_advanced_build_args_assembly() {
        let args = BuildArgs {
            cache_from: vec![
                "registry://example.com/cache".to_string(),
                "type=local,src=/tmp/cache".to_string(),
            ],
            cache_to: vec!["registry://example.com/cache:latest".to_string()],
            buildkit: Some(BuildKitOption::Auto),
            secret: vec![
                "id=mypassword,src=./password.txt".to_string(),
                "id=mykey,env=SSH_KEY".to_string(),
            ],
            build_secret: vec!["id=mysecret,src=./secret.txt".to_string()],
            ssh: vec!["default".to_string()],
            ..Default::default()
        };

        // Verify advanced args are structured correctly
        assert_eq!(args.cache_from.len(), 2);
        assert!(
            args.cache_from
                .contains(&"registry://example.com/cache".to_string())
        );
        assert!(
            args.cache_from
                .contains(&"type=local,src=/tmp/cache".to_string())
        );

        assert_eq!(args.cache_to.len(), 1);
        assert!(
            args.cache_to
                .contains(&"registry://example.com/cache:latest".to_string())
        );

        assert_eq!(args.buildkit, Some(BuildKitOption::Auto));

        assert_eq!(args.secret.len(), 2);
        assert!(
            args.secret
                .contains(&"id=mypassword,src=./password.txt".to_string())
        );
        assert!(args.secret.contains(&"id=mykey,env=SSH_KEY".to_string()));

        // SSH defaults currently only contain explicitly provided entries
        assert_eq!(args.ssh.len(), 1);
        assert!(args.ssh.contains(&"default".to_string()));
    }

    #[test]
    fn test_buildkit_detection() {
        // Test BuildKit Auto mode with DOCKER_BUILDKIT=1
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=true
        temp_env::with_var("DOCKER_BUILDKIT", Some("true"), || {
            assert!(should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=0
        temp_env::with_var("DOCKER_BUILDKIT", Some("0"), || {
            assert!(!should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Auto mode with DOCKER_BUILDKIT=false
        temp_env::with_var("DOCKER_BUILDKIT", Some("false"), || {
            assert!(!should_use_buildkit(Some(&BuildKitOption::Auto)));
        });

        // Test BuildKit Never mode (should always be false)
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(!should_use_buildkit(Some(&BuildKitOption::Never)));
        });

        // Test None (default) mode - should respect env var
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(should_use_buildkit(None));
        });

        temp_env::with_var("DOCKER_BUILDKIT", Some("0"), || {
            assert!(!should_use_buildkit(None));
        });

        // Test None with no env var (should default to false)
        temp_env::with_var_unset("DOCKER_BUILDKIT", || {
            assert!(!should_use_buildkit(None));
        });
    }

    /// #595: the ORDINARY invocation — no `--buildkit`, no `DOCKER_BUILDKIT` —
    /// must reach `docker buildx build`, because `docker build` runs on the
    /// daemon's own "default" instance and ignores the builder the user selected.
    ///
    /// This is the trap the fix nearly fell into: `should_use_buildkit` is FALSE
    /// in exactly that case, so deciding the invocation form on it would have left
    /// the defect in place for every user who has not set `DOCKER_BUILDKIT=1` —
    /// invisible on a dev machine that does set it.
    #[test]
    fn the_invocation_form_follows_the_off_switch_not_the_request() {
        temp_env::with_var_unset("DOCKER_BUILDKIT", || {
            assert!(
                uses_buildx(None),
                "an ordinary `deacon build` must run on the selected buildx builder"
            );
            assert!(
                !should_use_buildkit(None),
                "the narrower question is false here — which is why it cannot decide this"
            );
            assert!(uses_buildx(Some(&BuildKitOption::Auto)));
            assert!(
                !uses_buildx(Some(&BuildKitOption::Never)),
                "`--buildkit never` is the one route to the legacy builder"
            );
        });

        // An explicit off-switch in the environment reaches the legacy builder too.
        temp_env::with_var("DOCKER_BUILDKIT", Some("0"), || {
            assert!(!uses_buildx(None));
            assert!(!uses_buildx(Some(&BuildKitOption::Auto)));
        });

        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(uses_buildx(None));
            assert!(
                !uses_buildx(Some(&BuildKitOption::Never)),
                "the flag wins over the environment"
            );
        });
    }

    /// `buildkit_disabled` answers "was BuildKit switched OFF", which is not the
    /// question `should_use_buildkit` answers (#592).
    #[test]
    fn buildkit_disabled_reports_only_an_explicit_off_switch() {
        // `--buildkit never` wins over anything the environment says.
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert_eq!(
                buildkit_disabled(Some(&BuildKitOption::Never)),
                Some("--buildkit never")
            );
        });

        // An explicitly falsey environment disables it for `auto` and for no flag
        // at all, because the primary build passes that environment through.
        for value in ["0", "false", "FALSE"] {
            temp_env::with_var("DOCKER_BUILDKIT", Some(value), || {
                assert_eq!(
                    buildkit_disabled(Some(&BuildKitOption::Auto)),
                    Some("DOCKER_BUILDKIT=0"),
                    "DOCKER_BUILDKIT={value} should read as disabled"
                );
                assert_eq!(buildkit_disabled(None), Some("DOCKER_BUILDKIT=0"));
            });
        }

        // The case that separates this from `should_use_buildkit`, and the reason
        // the gate cannot reuse it: nothing asked for BuildKit and nothing turned
        // it off, so the daemon's own default applies — which on any modern Docker
        // is BuildKit. `should_use_buildkit(None)` is FALSE here; refusing
        // `--platform` on that basis would break the ordinary `deacon build
        // --platform linux/amd64`, which works.
        temp_env::with_var_unset("DOCKER_BUILDKIT", || {
            assert!(!should_use_buildkit(None));
            assert_eq!(buildkit_disabled(None), None);
            assert_eq!(buildkit_disabled(Some(&BuildKitOption::Auto)), None);
        });

        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert_eq!(buildkit_disabled(None), None);
            assert_eq!(buildkit_disabled(Some(&BuildKitOption::Auto)), None);
        });
    }

    #[test]
    fn test_build_output_redaction() {
        use deacon_core::redaction::{RedactionConfig, SecretRegistry};
        use std::collections::HashMap;

        // Create a test BuildResult with potentially sensitive information
        let mut metadata = HashMap::new();
        metadata.insert("secret-key".to_string(), "password123".to_string());
        metadata.insert("public-key".to_string(), "public-value".to_string());

        let result = BuildResult {
            image_id: "sha256:secret123abc".to_string(),
            tags: vec!["myapp:latest".to_string()],
            private_ref: None,
            metadata,
            config_hash: "hash123secret".to_string(),
            injected_ca_subjects: Vec::new(),
            build_duration: 1.5,
        };

        // Set up redaction
        let registry = SecretRegistry::new();
        registry.add_secret("password123");
        registry.add_secret("secret123");
        let config = RedactionConfig::with_custom_registry(registry.clone());

        // Test that calling output_result doesn't panic and applies redaction
        // Note: In a real test we'd capture stdout, but for now we just ensure it compiles and runs
        let result_call = output_result(
            &result,
            &OutputFormat::Text,
            &config,
            &registry,
            false,
            None,
        );
        assert!(result_call.is_ok(), "Output should not fail");
    }

    #[test]
    fn test_docker_cli_arg_ordering() {
        // Test that Docker build args are assembled in correct order
        // This simulates the argument building logic from execute_docker_build
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\n").unwrap();

        let config_hash = "abcd1234567890";
        let context_path = temp_dir.path();

        // Simulate the build_args construction from execute_docker_build
        let mut build_args = vec!["build".to_string()];

        // Defer adding context until after all flags (Docker expects PATH last)

        // Add dockerfile
        build_args.push("-f".to_string());
        build_args.push(dockerfile_path.to_str().unwrap().to_string());

        // Add no-cache flag
        build_args.push("--no-cache".to_string());

        // Add platform
        build_args.push("--platform".to_string());
        build_args.push("linux/amd64".to_string());

        // Add build args
        build_args.push("--build-arg".to_string());
        build_args.push("ENV=test".to_string());

        // Add tag
        let tag = format!("deacon-build:{}", &config_hash[..12]);
        build_args.push("-t".to_string());
        build_args.push(tag.clone());

        // Add label
        let label = format!("org.deacon.configHash={}", config_hash);
        build_args.push("--label".to_string());
        build_args.push(label);

        // Capture the image ID via --iidfile (replaces the former `-q` scrape)
        build_args.push("--iidfile".to_string());
        build_args.push("/tmp/deacon-iid".to_string());

        // Finally add context (PATH last)
        build_args.push(context_path.to_str().unwrap().to_string());

        // Verify the ordering: should start with "build" subcommand
        assert_eq!(build_args[0], "build");
        assert_eq!(build_args[1], "-f");
        assert_eq!(build_args[2], dockerfile_path.to_str().unwrap());
        assert_eq!(build_args[3], "--no-cache");
        assert_eq!(build_args[4], "--platform");
        assert_eq!(build_args[5], "linux/amd64");
        assert_eq!(build_args[6], "--build-arg");
        assert_eq!(build_args[7], "ENV=test");
        assert_eq!(build_args[8], "-t");
        assert_eq!(build_args[9], "deacon-build:abcd12345678");
        assert_eq!(build_args[10], "--label");
        assert_eq!(build_args[11], "org.deacon.configHash=abcd1234567890");
        assert_eq!(build_args[12], "--iidfile");
        assert_eq!(build_args[13], "/tmp/deacon-iid");
        assert_eq!(build_args[14], context_path.to_str().unwrap());

        // Verify that when passed to Command::new("docker").args(&build_args),
        // it will correctly execute "docker build ..." not "docker -f ..."
        assert!(
            build_args[0] == "build",
            "First argument must be 'build' subcommand"
        );
        assert!(
            build_args.iter().position(|arg| arg == "-f").unwrap() > 0,
            "-f flag must come after build subcommand"
        );
    }

    #[test]
    fn test_docker_cli_arg_ordering_with_advanced_options() {
        // Test that Docker build args are assembled in correct order with advanced options
        // This simulates the argument building logic from execute_docker_build with all advanced options
        let temp_dir = tempfile::tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine:3.19\n").unwrap();

        let config_hash = "abcd1234567890";
        let context_path = temp_dir.path();

        // Simulate the build_args construction from execute_docker_build with advanced options
        let mut build_args = vec!["build".to_string()];

        // Add dockerfile
        build_args.push("-f".to_string());
        build_args.push(dockerfile_path.to_str().unwrap().to_string());

        // Add no-cache flag
        build_args.push("--no-cache".to_string());

        // Add platform
        build_args.push("--platform".to_string());
        build_args.push("linux/amd64".to_string());

        // Add build args
        build_args.push("--build-arg".to_string());
        build_args.push("ENV=test".to_string());

        // Add advanced build options
        // Add cache-from options
        build_args.push("--cache-from".to_string());
        build_args.push("registry://example.com/cache".to_string());
        build_args.push("--cache-from".to_string());
        build_args.push("type=local,src=/tmp/cache".to_string());

        // Add cache-to options
        build_args.push("--cache-to".to_string());
        build_args.push("registry://example.com/cache:latest".to_string());

        // Add secret forwarding
        build_args.push("--secret".to_string());
        build_args.push("id=mypassword,src=./password.txt".to_string());

        // Add SSH forwarding
        build_args.push("--ssh".to_string());
        build_args.push("default".to_string());

        // Add tag
        let tag = format!("deacon-build:{}", &config_hash[..12]);
        build_args.push("-t".to_string());
        build_args.push(tag.clone());

        // Add label
        let label = format!("org.deacon.configHash={}", config_hash);
        build_args.push("--label".to_string());
        build_args.push(label);

        // Capture the image ID via --iidfile (replaces the former `-q` scrape)
        build_args.push("--iidfile".to_string());
        build_args.push("/tmp/deacon-iid".to_string());

        // Finally add context (PATH last)
        build_args.push(context_path.to_str().unwrap().to_string());

        // Verify advanced options are in the correct positions
        let cache_from_idx = build_args
            .iter()
            .position(|arg| arg == "--cache-from")
            .unwrap();
        let cache_to_idx = build_args
            .iter()
            .position(|arg| arg == "--cache-to")
            .unwrap();
        let secret_idx = build_args.iter().position(|arg| arg == "--secret").unwrap();
        let ssh_idx = build_args.iter().position(|arg| arg == "--ssh").unwrap();

        // Verify advanced options come after basic build args but before context
        let context_idx = build_args.len() - 1; // Context is last
        assert!(cache_from_idx < context_idx);
        assert!(cache_to_idx < context_idx);
        assert!(secret_idx < context_idx);
        assert!(ssh_idx < context_idx);

        // Verify specific advanced option values
        assert_eq!(
            build_args[cache_from_idx + 1],
            "registry://example.com/cache"
        );
        assert_eq!(
            build_args[cache_to_idx + 1],
            "registry://example.com/cache:latest"
        );
        assert_eq!(
            build_args[secret_idx + 1],
            "id=mypassword,src=./password.txt"
        );
        assert_eq!(build_args[ssh_idx + 1], "default");

        // Verify that context is still last
        assert_eq!(build_args[context_idx], context_path.to_str().unwrap());

        // Verify that the command still starts with "build"
        assert!(
            build_args[0] == "build",
            "First argument must be 'build' subcommand"
        );
    }

    #[test]
    fn test_secret_ssh_require_buildkit_validation() {
        // Test that BuildKitOption::Never always returns false
        let args_with_secret = BuildArgs {
            secret: vec!["id=test".to_string()],
            buildkit: Some(BuildKitOption::Never),
            ..BuildArgs::default()
        };

        let use_buildkit = should_use_buildkit(args_with_secret.buildkit.as_ref());
        assert!(
            !use_buildkit,
            "BuildKitOption::Never should always return false"
        );
        assert!(!args_with_secret.secret.is_empty());
        assert_eq!(args_with_secret.buildkit, Some(BuildKitOption::Never));

        // Test that None respects DOCKER_BUILDKIT environment variable
        let args_with_ssh = BuildArgs {
            ssh: vec!["default".to_string()],
            buildkit: None,
            ..BuildArgs::default()
        };

        assert!(!args_with_ssh.ssh.is_empty());
        assert_eq!(args_with_ssh.buildkit, None);

        // Test behavior with DOCKER_BUILDKIT unset (should default to false)
        temp_env::with_var_unset("DOCKER_BUILDKIT", || {
            assert!(
                !should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return false when DOCKER_BUILDKIT is unset and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=1 (should return true)
        temp_env::with_var("DOCKER_BUILDKIT", Some("1"), || {
            assert!(
                should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return true when DOCKER_BUILDKIT=1 and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=true (should return true)
        temp_env::with_var("DOCKER_BUILDKIT", Some("true"), || {
            assert!(
                should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return true when DOCKER_BUILDKIT=true and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=0 (should return false)
        temp_env::with_var("DOCKER_BUILDKIT", Some("0"), || {
            assert!(
                !should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return false when DOCKER_BUILDKIT=0 and buildkit is None"
            );
        });

        // Test behavior with DOCKER_BUILDKIT=false (should return false)
        temp_env::with_var("DOCKER_BUILDKIT", Some("false"), || {
            assert!(
                !should_use_buildkit(args_with_ssh.buildkit.as_ref()),
                "should_use_buildkit should return false when DOCKER_BUILDKIT=false and buildkit is None"
            );
        });

        // Test explicit Never option with SSH
        let args_ssh_never = BuildArgs {
            ssh: vec!["default".to_string()],
            buildkit: Some(BuildKitOption::Never),
            ..BuildArgs::default()
        };
        let use_buildkit_never = should_use_buildkit(args_ssh_never.buildkit.as_ref());
        assert!(
            !use_buildkit_never,
            "BuildKitOption::Never should return false even with SSH"
        );
    }

    #[test]
    fn test_is_non_build_affecting_file() {
        // Files that should not affect builds
        assert!(is_non_build_affecting_file("README.md"));
        assert!(is_non_build_affecting_file("readme"));
        assert!(is_non_build_affecting_file("CHANGELOG.md"));
        assert!(is_non_build_affecting_file("LICENSE"));
        assert!(is_non_build_affecting_file(".gitignore"));
        assert!(is_non_build_affecting_file("docs.md"));

        // Files that should affect builds
        assert!(!is_non_build_affecting_file("Dockerfile"));
        assert!(!is_non_build_affecting_file("main.py"));
        assert!(!is_non_build_affecting_file("package.json"));
        assert!(!is_non_build_affecting_file("requirements.txt"));
        assert!(!is_non_build_affecting_file("docker-compose.yml"));
        assert!(!is_non_build_affecting_file("dockerfile.dev"));
    }

    #[test]
    fn test_config_hash_with_context_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: None,
            build_args: HashMap::new(),
            options: Vec::new(),
        };

        // Create Dockerfile
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM alpine:3.19\n").unwrap();

        // Create files that affect build
        std::fs::write(temp_dir.path().join("main.py"), "print('hello')").unwrap();
        std::fs::write(temp_dir.path().join("requirements.txt"), "flask==2.0.0").unwrap();

        // Create files that don't affect build
        std::fs::write(temp_dir.path().join("README.md"), "# Project").unwrap();
        std::fs::write(temp_dir.path().join(".gitignore"), "*.pyc").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Modifying non-build-affecting file should not change hash
        std::fs::write(temp_dir.path().join("README.md"), "# Updated Project").unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_eq!(
            hash1, hash2,
            "Hash should not change when non-build-affecting files change"
        );

        // Modifying build-affecting file should change hash
        std::fs::write(temp_dir.path().join("main.py"), "print('updated')").unwrap();
        let hash3 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_ne!(
            hash1, hash3,
            "Hash should change when build-affecting files change"
        );
    }

    #[test]
    fn test_config_hash_recursive_directory_traversal() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: None,
            build_args: HashMap::new(),
            options: Vec::new(),
        };

        // Create Dockerfile
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM alpine:3.19\n").unwrap();

        // Create nested directory structure
        let src_dir = temp_dir.path().join("src");
        let utils_dir = src_dir.join("utils");
        std::fs::create_dir_all(&utils_dir).unwrap();

        // Create files in nested directories
        std::fs::write(src_dir.join("main.py"), "print('hello')").unwrap();
        std::fs::write(utils_dir.join("helper.py"), "def help(): pass").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Modify nested file should change hash
        std::fs::write(utils_dir.join("helper.py"), "def help(): return 'updated'").unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_ne!(hash1, hash2, "Hash should change when nested file changes");

        // Add non-affecting file in nested directory should not change hash
        std::fs::write(utils_dir.join("README.md"), "# Utils module").unwrap();
        let hash3 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_eq!(
            hash2, hash3,
            "Hash should not change when non-affecting nested file is added"
        );
    }

    #[test]
    fn test_config_hash_excludes_devcontainer_directory() {
        let temp_dir = tempfile::tempdir().unwrap();
        let build_config = BuildConfig {
            dockerfile: "Dockerfile".to_string(),
            dockerfile_path: temp_dir.path().join("Dockerfile"),
            context: ".".to_string(),
            context_folder: temp_dir.path().to_path_buf(),
            target: None,
            build_args: HashMap::new(),
            options: Vec::new(),
        };

        // Create Dockerfile
        std::fs::write(temp_dir.path().join("Dockerfile"), "FROM alpine:3.19\n").unwrap();

        // Create .devcontainer directory with cache
        let devcontainer_dir = temp_dir.path().join(".devcontainer");
        let cache_dir = devcontainer_dir.join("build-cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(devcontainer_dir.join("devcontainer.json"), "{}").unwrap();

        let hash1 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();

        // Add/modify files in .devcontainer should not change hash
        std::fs::write(cache_dir.join("somecache.json"), "{}").unwrap();
        std::fs::write(devcontainer_dir.join("another_file.json"), "{}").unwrap();
        let hash2 = calculate_config_hash(&build_config, temp_dir.path()).unwrap();
        assert_eq!(
            hash1, hash2,
            "Hash should not change when .devcontainer directory contents change"
        );
    }

    #[test]
    fn test_cache_paths() {
        // The build cache lives under the host user-data folder (never the
        // project), keyed by a per-workspace hash subdir (#280).
        let udf = tempfile::tempdir().unwrap();
        let user_data = udf.path();
        let workspace_hash = "ws01hash";
        let config_hash = "abcd1234efgh5678";

        let cache_dir = get_build_cache_dir(Some(user_data), workspace_hash).unwrap();
        let expected_cache_dir = user_data.join("build-cache").join(workspace_hash);
        assert_eq!(cache_dir, expected_cache_dir);
        // Must NOT be inside the project.
        assert!(!cache_dir.to_string_lossy().contains(".devcontainer"));

        let cache_file =
            get_build_cache_path(Some(user_data), workspace_hash, config_hash).unwrap();
        let expected_cache_file = expected_cache_dir.join("abcd1234efgh5678.json");
        assert_eq!(cache_file, expected_cache_file);
    }

    #[test]
    fn test_build_metadata_serialization() {
        let build_result = BuildResult {
            image_id: "sha256:abcd1234".to_string(),
            tags: vec!["myapp:latest".to_string()],
            private_ref: None,
            build_duration: 123.45,
            metadata: {
                let mut map = HashMap::new();
                map.insert("test".to_string(), "value".to_string());
                map
            },
            config_hash: "hash123".to_string(),
            injected_ca_subjects: Vec::new(),
        };

        let inputs = BuildInputs {
            dockerfile_hash: "dockerfile_hash".to_string(),
            context_files: vec![ContextFile {
                path: "main.py".to_string(),
                size: 100,
                mtime: 1234567890,
            }],
            feature_set_digest: Some("features_hash".to_string()),
            build_config: BuildConfig {
                dockerfile: "Dockerfile".to_string(),
                dockerfile_path: PathBuf::from("Dockerfile"),
                context: ".".to_string(),
                context_folder: PathBuf::from("."),
                target: None,
                build_args: HashMap::new(),
                options: Vec::new(),
            },
        };

        let metadata = BuildMetadata {
            config_hash: "hash123".to_string(),
            result: build_result,
            inputs,
            created_at: 1234567890,
            requested_image_names: vec!["myimage:v1".to_string()],
        };

        // Test serialization
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(!json.is_empty());

        // Test deserialization
        let deserialized: BuildMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.config_hash, metadata.config_hash);
        assert_eq!(deserialized.result.image_id, metadata.result.image_id);
        assert_eq!(
            deserialized.inputs.dockerfile_hash,
            metadata.inputs.dockerfile_hash
        );
        assert_eq!(
            deserialized.requested_image_names,
            metadata.requested_image_names
        );
    }

    /// A cache entry written before #620 carries no `requested_image_names`; it must
    /// still deserialize, defaulting to "every recorded tag is build-owned".
    #[test]
    fn test_build_metadata_without_requested_image_names_deserializes() {
        let json = r#"{
            "config_hash": "hash123",
            "result": {
                "image_id": "sha256:abc",
                "tags": ["deacon-build:abc123456789", "old:v1"],
                "build_duration": 1.0,
                "metadata": {},
                "config_hash": "hash123"
            },
            "inputs": {
                "dockerfile_hash": "dh",
                "context_files": [],
                "feature_set_digest": null,
                "build_config": {
                    "dockerfile": "Dockerfile",
                    "dockerfile_path": "Dockerfile",
                    "context": ".",
                    "context_folder": ".",
                    "target": null,
                    "build_args": {},
                    "options": []
                }
            },
            "created_at": 1234567890
        }"#;
        let parsed: BuildMetadata = serde_json::from_str(json).unwrap();
        assert!(parsed.requested_image_names.is_empty());
    }

    // #620: `--image-name` is an OUTPUT of the build, so a cache hit reports the
    // names THIS invocation asked for, not the ones that first populated the entry.
    #[test]
    fn test_reconciled_tags_replaces_a_changed_image_name() {
        let cached = vec![
            "deacon-build:abc123456789".to_string(),
            "old:v1".to_string(),
        ];
        let tags = reconciled_tags(&cached, &["old:v1".to_string()], &["new:v1".to_string()]);
        // `output_result` drops the leading deterministic tag, so the document reads
        // exactly `["new:v1"]`.
        assert_eq!(tags, vec!["deacon-build:abc123456789", "new:v1"]);
    }

    #[test]
    fn test_reconciled_tags_preserves_requested_order() {
        let cached = vec![
            "deacon-build:abc123456789".to_string(),
            "old:v1".to_string(),
        ];
        let tags = reconciled_tags(
            &cached,
            &["old:v1".to_string()],
            &["first:v1".to_string(), "second:v1".to_string()],
        );
        assert_eq!(
            tags,
            vec!["deacon-build:abc123456789", "first:v1", "second:v1"]
        );
    }

    #[test]
    fn test_reconciled_tags_drops_a_stale_name_when_none_is_requested() {
        let cached = vec![
            "deacon-build:abc123456789".to_string(),
            "old:v1".to_string(),
        ];
        let tags = reconciled_tags(&cached, &["old:v1".to_string()], &[]);
        assert_eq!(tags, vec!["deacon-build:abc123456789"]);
    }

    #[test]
    fn test_reconciled_tags_keeps_a_build_owned_name() {
        // Compose derives `<project>-<service>` when no `--image-name` is given; it
        // belongs to the build and survives alongside a newly requested name.
        let cached = vec!["proj-app".to_string()];
        let tags = reconciled_tags(&cached, &[], &["new:v1".to_string()]);
        assert_eq!(tags, vec!["proj-app", "new:v1"]);
    }

    #[test]
    fn test_reconciled_tags_does_not_duplicate_a_repeated_name() {
        let cached = vec![
            "deacon-build:abc123456789".to_string(),
            "same:v1".to_string(),
        ];
        let tags = reconciled_tags(&cached, &["same:v1".to_string()], &["same:v1".to_string()]);
        assert_eq!(tags, vec!["deacon-build:abc123456789", "same:v1"]);
    }

    #[test]
    fn test_reconciled_tags_falls_back_when_nothing_would_be_reported() {
        // Compose, first run named, second run unnamed: no build-owned tag exists to
        // report, so the recorded names stand rather than an `imageName`-less document.
        let cached = vec!["old:v1".to_string()];
        let tags = reconciled_tags(&cached, &["old:v1".to_string()], &[]);
        assert_eq!(tags, vec!["old:v1"]);
    }

    /// A pre-#620 entry has no record of what was requested, so every recorded tag
    /// counts as build-owned; a newly requested name is still applied and reported.
    #[test]
    fn test_reconciled_tags_legacy_entry_appends() {
        let cached = vec![
            "deacon-build:abc123456789".to_string(),
            "old:v1".to_string(),
        ];
        let tags = reconciled_tags(&cached, &[], &["new:v1".to_string()]);
        assert_eq!(tags, vec!["deacon-build:abc123456789", "old:v1", "new:v1"]);
    }

    #[test]
    fn test_token_substitution() {
        let template = "trivy image {image}";
        let image_id = "sha256:abc123def456";
        let result = substitute_tokens(template, image_id).unwrap();
        assert_eq!(result, "trivy image sha256:abc123def456");

        // Test with multiple occurrences
        let template = "scanner --image {image} --output /tmp/{image}.json";
        let result = substitute_tokens(template, image_id).unwrap();
        assert_eq!(
            result,
            "scanner --image sha256:abc123def456 --output /tmp/sha256:abc123def456.json"
        );

        // Test with no tokens
        let template = "trivy image latest";
        let result = substitute_tokens(template, image_id).unwrap();
        assert_eq!(result, "trivy image latest");
    }

    #[test]
    fn test_build_args_with_scan_options() {
        let args = BuildArgs {
            scan_image: true,
            fail_on_scan: true,
            ..BuildArgs::default()
        };

        assert!(args.scan_image);
        assert!(args.fail_on_scan);
    }

    #[test]
    fn test_build_args_default_scan_options() {
        let args = BuildArgs::default();
        assert!(!args.scan_image);
        assert!(!args.fail_on_scan);
    }

    #[test]
    fn test_shell_command_parsing() {
        // Test that shell command parsing handles quoted arguments correctly
        let command_simple = "trivy image my-image";
        let parts_simple = shell_words::split(command_simple).unwrap();
        assert_eq!(parts_simple, vec!["trivy", "image", "my-image"]);

        // Test with quoted arguments
        let command_quoted = r#"sh -c "trivy image --severity 'CRITICAL,HIGH' my-image""#;
        let parts_quoted = shell_words::split(command_quoted).unwrap();
        assert_eq!(
            parts_quoted,
            vec![
                "sh",
                "-c",
                "trivy image --severity 'CRITICAL,HIGH' my-image"
            ]
        );

        // Test with spaces in arguments
        let command_spaces = r#"scanner --output "/path with spaces/scan.json" my-image"#;
        let parts_spaces = shell_words::split(command_spaces).unwrap();
        assert_eq!(
            parts_spaces,
            vec![
                "scanner",
                "--output",
                "/path with spaces/scan.json",
                "my-image"
            ]
        );
    }

    #[test]
    fn test_build_secret_parse_file_source() {
        let spec = "id=mytoken,src=/path/to/secret.txt";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "mytoken");
        assert_eq!(
            secret.source,
            BuildSecretSource::File(PathBuf::from("/path/to/secret.txt"))
        );
    }

    #[test]
    fn test_build_secret_parse_env_source() {
        let spec = "id=apikey,env=API_TOKEN";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "apikey");
        assert_eq!(
            secret.source,
            BuildSecretSource::Env("API_TOKEN".to_string())
        );
    }

    #[test]
    fn test_build_secret_parse_stdin_default() {
        let spec = "id=password";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "password");
        assert_eq!(secret.source, BuildSecretSource::Stdin);
    }

    #[test]
    fn test_build_secret_parse_stdin_explicit_value_stdin() {
        let spec = "id=password,value-stdin";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "password");
        assert_eq!(secret.source, BuildSecretSource::Stdin);
    }

    #[test]
    fn test_build_secret_parse_stdin_explicit_stdin() {
        let spec = "id=password,stdin";
        let secret = BuildSecret::parse(spec).unwrap();
        assert_eq!(secret.id, "password");
        assert_eq!(secret.source, BuildSecretSource::Stdin);
    }

    #[test]
    fn test_build_secret_parse_stdin_flag_with_src_error() {
        let spec = "id=test,stdin,src=/path/to/file";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot specify 'value-stdin' or 'stdin' flag with 'src' or 'env'")
        );
    }

    #[test]
    fn test_build_secret_parse_stdin_flag_with_env_error() {
        let spec = "id=test,value-stdin,env=MY_VAR";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot specify 'value-stdin' or 'stdin' flag with 'src' or 'env'")
        );
    }

    #[test]
    fn test_build_secret_parse_missing_id() {
        let spec = "src=/path/to/file";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must specify 'id'")
        );
    }

    #[test]
    fn test_build_secret_parse_empty_id() {
        let spec = "id=,src=/path/to/file";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_build_secret_parse_both_src_and_env() {
        let spec = "id=test,src=/path,env=VAR";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot specify both")
        );
    }

    #[test]
    fn test_build_secret_parse_unknown_parameter() {
        let spec = "id=test,unknown=value";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown build secret parameter")
        );
    }

    #[test]
    fn test_build_secret_parse_unknown_flag() {
        let spec = "id=test,invalid";
        let result = BuildSecret::parse(spec);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown build secret parameter")
        );
    }

    #[test]
    fn test_build_secret_validate_missing_file() {
        let secret = BuildSecret {
            id: "test".to_string(),
            source: BuildSecretSource::File(PathBuf::from("/nonexistent/path")),
        };
        let result = secret.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn test_build_secret_validate_missing_env() {
        // Make sure this env var doesn't exist
        temp_env::with_var_unset("NONEXISTENT_SECRET_VAR_12345", || {
            let secret = BuildSecret {
                id: "test".to_string(),
                source: BuildSecretSource::Env("NONEXISTENT_SECRET_VAR_12345".to_string()),
            };
            let result = secret.validate();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("is not set"));
        });
    }

    #[test]
    fn test_build_secret_to_docker_arg_file() {
        let secret = BuildSecret {
            id: "mytoken".to_string(),
            source: BuildSecretSource::File(PathBuf::from("/secrets/token.txt")),
        };
        let docker_arg = secret.to_docker_arg(None);
        assert_eq!(docker_arg, "id=mytoken,src=/secrets/token.txt");
    }

    #[test]
    fn test_build_secret_to_docker_arg_with_temp() {
        let secret = BuildSecret {
            id: "apikey".to_string(),
            source: BuildSecretSource::Env("API_KEY".to_string()),
        };
        let temp_path = PathBuf::from("/tmp/secret123");
        let docker_arg = secret.to_docker_arg(Some(&temp_path));
        assert_eq!(docker_arg, "id=apikey,src=/tmp/secret123");
    }

    #[tokio::test]
    async fn test_build_secret_read_from_env() {
        temp_env::async_with_vars(
            [("TEST_BUILD_SECRET_12345", Some("secret_value_here"))],
            async {
                let secret = BuildSecret {
                    id: "test".to_string(),
                    source: BuildSecretSource::Env("TEST_BUILD_SECRET_12345".to_string()),
                };
                let value = secret.read_value().await.unwrap();
                assert_eq!(value, "secret_value_here");
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_build_secret_read_from_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let secret_file = temp_dir.path().join("secret.txt");
        std::fs::write(&secret_file, "my_secret_token\n").unwrap();

        let secret = BuildSecret {
            id: "test".to_string(),
            source: BuildSecretSource::File(secret_file),
        };
        let value = secret.read_value().await.unwrap();
        assert_eq!(value, "my_secret_token");
    }

    // =========================================================================
    // PR-4c: features-during-build (helpers tested in isolation)
    // =========================================================================
}
