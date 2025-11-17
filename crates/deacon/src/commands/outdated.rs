// Outdated subcommand implementation (Phase 3 MVP)
// This implements configuration discovery, feature extraction (preserving order),
// computing wanted/current/latest using core helpers, and rendering a text table.

use anyhow::Result;
use atty::Stream;
use deacon_core::config::ConfigLoader;
use deacon_core::lockfile as core_lockfile;
use deacon_core::outdated as core_outdated;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

use crate::cli::OutputFormat;

/// Arguments for the outdated subcommand.
///
/// These arguments control how the outdated feature check is performed
/// and how results are presented to the user.
#[derive(Debug, Clone)]
pub struct OutdatedArgs {
    /// Path to the workspace folder containing the dev container configuration.
    /// If empty, the current working directory will be used.
    pub workspace_folder: String,
    /// Output format for the results (text or JSON).
    pub output: OutputFormat,
    /// If true, the command will exit with code 2 when outdated features are detected.
    /// This is useful for CI/CD pipelines to gate on outdated dependencies.
    pub fail_on_outdated: bool,
}

/// Error type used to signal an intended exit code for outdated CI gating
#[derive(Debug)]
pub struct OutdatedExitCode(pub i32);

impl std::fmt::Display for OutdatedExitCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Exit with code {} due to outdated features", self.0)
    }
}

impl std::error::Error for OutdatedExitCode {}

/// Executes the outdated subcommand to check for outdated dev container features.
///
/// This function discovers the dev container configuration, extracts features,
/// queries registries for the latest stable versions, and outputs the results
/// in the specified format (text or JSON).
///
/// # Arguments
///
/// * `args` - Configuration for the outdated check, including workspace path,
///   output format, and whether to fail on outdated features.
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if:
/// - The workspace path cannot be canonicalized
/// - Configuration discovery fails
/// - Configuration parsing fails
/// - Output format is invalid
/// - `fail_on_outdated` is true and outdated features are detected
///
/// # Examples
///
/// ```no_run
/// use deacon::commands::outdated::{run, OutdatedArgs};
/// use deacon::cli::OutputFormat;
///
/// # async fn example() -> anyhow::Result<()> {
/// let args = OutdatedArgs {
///     workspace_folder: ".".to_string(),
///     output: OutputFormat::Text,
///     fail_on_outdated: false,
/// };
///
/// run(args).await?;
/// # Ok(())
/// # }
/// ```
pub async fn run(args: OutdatedArgs) -> Result<()> {
    info!("outdated subcommand invoked");

    // Resolve workspace folder path
    let workspace_folder = if args.workspace_folder.is_empty() {
        std::env::current_dir()?.canonicalize()?
    } else {
        PathBuf::from(&args.workspace_folder)
            .canonicalize()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to canonicalize workspace path '{}': {}",
                    args.workspace_folder,
                    e
                )
            })?
    };

    // Discover configuration
    let config_location = match ConfigLoader::discover_config(&workspace_folder) {
        Ok(loc) => loc,
        Err(e) => {
            // If config not found, surface a clear error message (T044)
            warn!(error = ?e, "Configuration not found for workspace");
            anyhow::bail!(
                "Configuration file not found in workspace: {}",
                workspace_folder.display()
            );
        }
    };

    // Load configuration
    let config = ConfigLoader::load_from_path(config_location.path())?;

    // Extract features map preserving declaration order
    let features_map_opt = config.features.as_object();
    if features_map_opt.is_none() || features_map_opt.unwrap().is_empty() {
        // No features: print header and exit 0 (T016)
        if matches!(args.output, OutputFormat::Json) {
            // Empty JSON result with map shape: { features: {} }
            use serde::Serialize;
            use std::collections::BTreeMap;

            #[derive(Serialize)]
            struct JsonEmptyResult {
                features: BTreeMap<String, serde_json::Value>,
            }

            let empty = JsonEmptyResult {
                features: BTreeMap::new(),
            };

            if atty::is(Stream::Stdout) {
                serde_json::to_writer_pretty(std::io::stdout(), &empty)?;
            } else {
                serde_json::to_writer(std::io::stdout(), &empty)?;
            }
            return Ok(());
        } else {
            println!("Feature | Current | Wanted | Latest");
            return Ok(());
        }
    }
    let features_map = features_map_opt.unwrap();

    // Read lockfile if present
    let lockfile_path = core_lockfile::get_lockfile_path(config_location.path());
    let lockfile_opt = match core_lockfile::read_lockfile(&lockfile_path) {
        Ok(opt) => opt,
        Err(e) => {
            debug!(error = ?e, "Failed to read lockfile - proceeding without it");
            None
        }
    };

    // Prepare results vector
    let mut results: Vec<core_outdated::FeatureVersionInfo> = Vec::new();

    // Collect declared refs preserving declaration order
    let mut declared_refs: Vec<String> = Vec::new();
    for (feature_key, _feature_value) in features_map.iter() {
        declared_refs.push(feature_key.clone());
    }

    // Bounded parallel fetching of latest versions with timeout and deterministic ordering
    let concurrency_limit = std::env::var("DEACON_OUTDATED_CONCURRENCY")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(6usize);
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));

    let mut handles = Vec::new();
    for dr in declared_refs.iter() {
        let sem = semaphore.clone();
        let dr_clone = dr.clone();
        let handle = tokio::spawn(async move {
            // Acquire permit
            let _permit = match sem.acquire().await {
                Ok(permit) => permit,
                Err(_) => {
                    // Semaphore closed (extremely rare); return None for this feature
                    return None;
                }
            };

            // Per-request timeout (sensible default)
            let timeout_secs = std::env::var("DEACON_OUTDATED_FETCH_TIMEOUT_SECONDS")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(5);
            let timeout_dur = Duration::from_secs(timeout_secs);

            // Number of retries for transient failures (default: 2 attempts)
            let max_retries = std::env::var("DEACON_OUTDATED_FETCH_RETRIES")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(2usize);

            let mut attempt: usize = 0;
            loop {
                attempt += 1;
                // Use core helper; enforce timeout per attempt
                let attempt_result = tokio::time::timeout(
                    timeout_dur,
                    core_outdated::fetch_latest_stable_version(&dr_clone),
                )
                .await;

                match attempt_result {
                    Ok(latest_opt) => {
                        // Either Some or None returned by fetcher; treat as final
                        break latest_opt;
                    }
                    Err(_) => {
                        // Timeout during fetch
                        let canonical = core_outdated::canonical_feature_id(&dr_clone);
                        debug!(feature = %canonical, attempt, "Timeout fetching latest version (attempt {})", attempt);

                        if attempt >= max_retries {
                            // Give up after max retries
                            break None;
                        }

                        // Exponential backoff (seconds) capped to 8s
                        let backoff = std::cmp::min(2u64.pow(attempt as u32), 8);
                        tokio::time::sleep(Duration::from_secs(backoff)).await;
                        continue;
                    }
                }
            }
        });
        handles.push(handle);
    }

    // Await handles in order to preserve deterministic output ordering
    let mut latests: Vec<Option<String>> = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(latest_opt) => latests.push(latest_opt),
            Err(e) => {
                debug!(error = ?e, "Task join error when fetching latest");
                latests.push(None);
            }
        }
    }

    // Build results preserving original declaration order
    for (i, declared_ref) in declared_refs.iter().enumerate() {
        let declared_ref = declared_ref.as_str();
        let canonical = core_outdated::canonical_feature_id(declared_ref);
        let wanted = core_outdated::compute_wanted_version(declared_ref);
        let current = core_outdated::derive_current_version(declared_ref, lockfile_opt.as_ref());
        let latest = latests.get(i).cloned().unwrap_or(None);

        let wanted_major = core_outdated::wanted_major(&wanted);
        let latest_major = core_outdated::latest_major(&latest);

        results.push(core_outdated::FeatureVersionInfo {
            id: canonical,
            current: current.clone(),
            wanted: wanted.clone(),
            latest: latest.clone(),
            wanted_major,
            latest_major,
        });
    }

    // If JSON output requested, serialize to stdout and return appropriate exit code
    if matches!(args.output, OutputFormat::Json) {
        // Build a serializable version as a map keyed by canonical id
        use serde::Serialize;
        use std::collections::BTreeMap;

        #[derive(Serialize)]
        struct JsonFeatureFields {
            current: Option<String>,
            wanted: Option<String>,
            #[serde(rename = "wantedMajor")]
            wanted_major: Option<String>,
            latest: Option<String>,
            #[serde(rename = "latestMajor")]
            latest_major: Option<String>,
        }

        #[derive(Serialize)]
        struct JsonResultMap {
            features: BTreeMap<String, JsonFeatureFields>,
        }

        let mut map: BTreeMap<String, JsonFeatureFields> = BTreeMap::new();
        for f in &results {
            map.insert(
                f.id.clone(),
                JsonFeatureFields {
                    current: f.current.clone(),
                    wanted: f.wanted.clone(),
                    wanted_major: f.wanted_major.clone(),
                    latest: f.latest.clone(),
                    latest_major: f.latest_major.clone(),
                },
            );
        }
        let jr = JsonResultMap { features: map };

        if atty::is(Stream::Stdout) {
            serde_json::to_writer_pretty(std::io::stdout(), &jr)?;
        } else {
            serde_json::to_writer(std::io::stdout(), &jr)?;
        }

        // Evaluate outdated condition if fail_on_outdated
        if args.fail_on_outdated {
            use deacon_core::semver_utils::compare_versions;
            let mut any_outdated = false;
            for f in jr.features.values() {
                if let (Some(current), Some(wanted)) = (f.current.as_ref(), f.wanted.as_ref()) {
                    if compare_versions(current, wanted) == std::cmp::Ordering::Less {
                        any_outdated = true;
                        break;
                    }
                }
                if let (Some(wanted), Some(latest)) = (f.wanted.as_ref(), f.latest.as_ref()) {
                    if compare_versions(wanted, latest) == std::cmp::Ordering::Less {
                        any_outdated = true;
                        break;
                    }
                }
            }

            if any_outdated {
                return Err(Box::new(OutdatedExitCode(2)).into());
            }
        }

        return Ok(());
    }

    // Render text table to stdout (T014) - logs use tracing (stderr)
    println!("Feature | Current | Wanted | Latest");
    for f in results {
        println!(
            "{} | {} | {} | {}",
            f.id,
            f.current.as_deref().unwrap_or("-"),
            f.wanted.as_deref().unwrap_or("-"),
            f.latest.as_deref().unwrap_or("-")
        );
    }

    Ok(())
}
