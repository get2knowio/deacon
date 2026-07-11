//! State management for tracking running containers and compose projects
//!
//! This module provides state persistence to track which containers and compose projects
//! are running, enabling the down command to stop them according to shutdown actions.
//!
//! ## Lifecycle Phase Markers
//!
//! This module also provides centralized helpers for reading and writing lifecycle phase
//! markers. These markers track which phases have completed, enabling resume and prebuild
//! scenarios:
//!
//! - Normal markers: `.devcontainer-state/<phase>.json`
//! - Prebuild markers: `.devcontainer-state/prebuild/<phase>.json` (isolated per Decision 1)
//!
//! Prebuild uses isolated markers so a subsequent normal `up` reruns onCreate and
//! updateContent before proceeding to postCreate/postStart/postAttach.

use crate::cache::{Cache, DiskCache};
use crate::errors::StateError;
use crate::lifecycle::{LifecyclePhase, LifecyclePhaseState, PhaseStatus};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument, warn};

/// Convenience `Result` alias for state operations
pub type Result<T, E = StateError> = std::result::Result<T, E>;

fn state_io<P: Into<PathBuf>>(path: P) -> impl FnOnce(std::io::Error) -> StateError {
    let path = path.into();
    move |source| StateError::Io { path, source }
}

/// State information for a running container
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContainerState {
    /// Container ID
    pub container_id: String,
    /// Container name (if any)
    pub container_name: Option<String>,
    /// Image ID used
    pub image_id: String,
    /// Shutdown action from config
    pub shutdown_action: Option<String>,
}

/// State information for a running compose project
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComposeState {
    /// Compose project name
    pub project_name: String,
    /// Service name (primary service)
    pub service_name: String,
    /// Base directory containing compose files
    pub base_path: String,
    /// Compose file paths (relative to base_path)
    pub compose_files: Vec<String>,
    /// Shutdown action from config
    pub shutdown_action: Option<String>,
}

/// Overall state for a workspace
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkspaceState {
    /// Single container workspace
    Container(ContainerState),
    /// Docker Compose workspace
    Compose(ComposeState),
}

/// State manager for tracking workspace states
pub struct StateManager {
    cache: DiskCache<String, WorkspaceState>,
}

impl StateManager {
    /// Create a new state manager with default cache directory
    pub fn new() -> Result<Self> {
        let cache_dir = Self::default_cache_dir()?;
        Self::new_with_cache_dir(cache_dir)
    }

    /// Create a new state manager with custom cache directory
    pub fn new_with_cache_dir<P: AsRef<Path>>(cache_dir: P) -> Result<Self> {
        let state_cache_dir = cache_dir.as_ref().join("state");
        let cache = DiskCache::new(&state_cache_dir)?;

        Ok(Self { cache })
    }

    /// Get the default cache directory for state management
    fn default_cache_dir() -> Result<PathBuf> {
        // Use the same pattern as features cache
        let cache_dir = std::env::temp_dir().join("deacon-state");
        if !cache_dir.exists() {
            std::fs::create_dir_all(&cache_dir).map_err(state_io(cache_dir.clone()))?;
        }
        Ok(cache_dir)
    }

    /// Save container state for a workspace
    #[instrument(skip(self))]
    pub fn save_container_state(
        &mut self,
        workspace_hash: &str,
        container_state: ContainerState,
    ) -> Result<()> {
        debug!(
            workspace_hash = %workspace_hash,
            container_id = %container_state.container_id,
            "Saving container state"
        );

        let state = WorkspaceState::Container(container_state);
        self.cache.set(workspace_hash.to_string(), state)?;

        info!(
            workspace_hash = %workspace_hash,
            "Container state saved successfully"
        );

        Ok(())
    }

    /// Save compose state for a workspace
    #[instrument(skip(self))]
    pub fn save_compose_state(
        &mut self,
        workspace_hash: &str,
        compose_state: ComposeState,
    ) -> Result<()> {
        debug!(
            workspace_hash = %workspace_hash,
            project_name = %compose_state.project_name,
            "Saving compose state"
        );

        let state = WorkspaceState::Compose(compose_state);
        self.cache.set(workspace_hash.to_string(), state)?;

        info!(
            workspace_hash = %workspace_hash,
            "Compose state saved successfully"
        );

        Ok(())
    }

    /// Get workspace state by workspace hash
    #[instrument(skip(self))]
    pub fn get_workspace_state(&mut self, workspace_hash: &str) -> Option<WorkspaceState> {
        debug!(workspace_hash = %workspace_hash, "Getting workspace state");

        let state = self.cache.get(&workspace_hash.to_string());

        if state.is_some() {
            debug!(workspace_hash = %workspace_hash, "Found workspace state");
        } else {
            debug!(workspace_hash = %workspace_hash, "No workspace state found");
        }

        state
    }

    /// Remove workspace state (called after successful shutdown)
    #[instrument(skip(self))]
    pub fn remove_workspace_state(&mut self, workspace_hash: &str) -> Option<WorkspaceState> {
        debug!(workspace_hash = %workspace_hash, "Removing workspace state");

        let removed = self.cache.remove(&workspace_hash.to_string());

        if removed.is_some() {
            info!(workspace_hash = %workspace_hash, "Workspace state removed");
        } else {
            debug!(workspace_hash = %workspace_hash, "No workspace state to remove");
        }

        removed
    }

    /// List all tracked workspace hashes
    pub fn list_workspace_hashes(&self) -> Vec<String> {
        // Note: This would require extending the Cache trait to support listing keys
        // For now, we can implement a simpler approach by scanning the cache directory
        // This is acceptable since the cache is file-based

        let cache_dir = std::env::temp_dir().join("deacon-state").join("state");
        if !cache_dir.exists() {
            return Vec::new();
        }

        let mut workspace_hashes = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    // Remove the file extension to get the workspace hash
                    if let Some(hash) = file_name.strip_suffix(".bin") {
                        workspace_hashes.push(hash.to_string());
                    }
                }
            }
        }

        workspace_hashes
    }

    /// Clear all workspace states (for testing/cleanup)
    pub fn clear_all(&mut self) {
        self.cache.clear();
    }

    /// Get cache statistics
    pub fn stats(&self) -> crate::cache::CacheStats {
        self.cache.stats()
    }
}

// NOTE: deliberately no `Default` impl. `StateManager::new()` is fallible
// (it derives a cache directory and may hit IO errors), so a Default impl
// that calls `.expect(...)` would panic at runtime. Use `StateManager::new()`
// or `StateManager::new_with_cache_dir(path)` and propagate the `Result`.

// =============================================================================
// Lifecycle Phase Marker Helpers
// =============================================================================
//
// These functions provide centralized marker path generation and read/write
// operations for lifecycle phase markers. They support both normal and prebuild
// marker isolation per research.md Decision 1.

/// Directory name for devcontainer state files
/// Subdirectory of the host user-data folder that holds per-workspace lifecycle
/// markers (`~/.deacon/state/<workspace_hash>/`). Relocated out of the project
/// (`<workspace>/.devcontainer-state/`) per #280.
const STATE_SUBDIR: &str = "state";

/// Subdirectory for prebuild-isolated markers
const PREBUILD_SUBDIR: &str = "prebuild";

/// File extension for marker files
const MARKER_EXTENSION: &str = "json";

/// Get the base directory for lifecycle markers within a workspace.
///
/// Returns `{workspace}/.devcontainer-state/` for normal markers or
/// `{workspace}/.devcontainer-state/prebuild/` for prebuild markers.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `prebuild` - If true, returns the isolated prebuild marker directory
pub fn marker_base_dir(
    workspace: &Path,
    prebuild: bool,
    user_data_folder: Option<&Path>,
) -> Result<PathBuf> {
    // Markers live in the host user-data folder
    // (`~/.deacon/state/<workspace_hash>/[prebuild/]`), keyed by a stable
    // per-workspace hash, NOT inside the project (`<workspace>/.devcontainer-state/`).
    // This keeps resume state surviving `down && up` (the hash is stable across
    // container removal) while leaving no stray files in the user's repo (#280).
    let workspace_hash = crate::container::ContainerIdentity::hash_workspace_path(workspace);
    let mut base = crate::trust::user_data_root(user_data_folder)
        .map_err(|source| StateError::UserDataFolder {
            message: source.to_string(),
        })?
        .join(STATE_SUBDIR)
        .join(workspace_hash);
    if prebuild {
        base = base.join(PREBUILD_SUBDIR);
    }
    Ok(base)
}

/// Get the marker file path for a specific lifecycle phase.
///
/// Returns `{workspace}/.devcontainer-state/{phase}.json` for normal markers.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `phase` - The lifecycle phase to get the marker path for
///
/// # Example
///
/// ```
/// use std::path::Path;
/// use deacon_core::state::marker_path_for_phase;
/// use deacon_core::lifecycle::LifecyclePhase;
///
/// let workspace = Path::new("/workspace");
/// let path = marker_path_for_phase(workspace, LifecyclePhase::OnCreate, None).unwrap();
/// assert!(path.ends_with("onCreate.json"));
/// ```
pub fn marker_path_for_phase(
    workspace: &Path,
    phase: LifecyclePhase,
    user_data_folder: Option<&Path>,
) -> Result<PathBuf> {
    Ok(
        marker_base_dir(workspace, false, user_data_folder)?.join(format!(
            "{}.{}",
            phase.as_str(),
            MARKER_EXTENSION
        )),
    )
}

/// Get the prebuild marker file path for a specific lifecycle phase.
///
/// Returns `{workspace}/.devcontainer-state/prebuild/{phase}.json`.
///
/// Prebuild markers are isolated from normal markers per research.md Decision 1,
/// so a subsequent normal `up` will rerun onCreate and updateContent.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `phase` - The lifecycle phase to get the prebuild marker path for
///
/// # Example
///
/// ```
/// use std::path::Path;
/// use deacon_core::state::prebuild_marker_path_for_phase;
/// use deacon_core::lifecycle::LifecyclePhase;
///
/// let workspace = Path::new("/workspace");
/// let path = prebuild_marker_path_for_phase(workspace, LifecyclePhase::OnCreate, None).unwrap();
/// assert!(path.ends_with("prebuild/onCreate.json"));
/// ```
pub fn prebuild_marker_path_for_phase(
    workspace: &Path,
    phase: LifecyclePhase,
    user_data_folder: Option<&Path>,
) -> Result<PathBuf> {
    Ok(
        marker_base_dir(workspace, true, user_data_folder)?.join(format!(
            "{}.{}",
            phase.as_str(),
            MARKER_EXTENSION
        )),
    )
}

/// Validation result for phase markers.
///
/// Per research.md Decision 2, corrupted or invalid markers are treated
/// as missing to ensure rerun from the earliest phase.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkerValidation {
    /// Marker is valid and can be used for resume decisions
    Valid,
    /// Marker file does not exist
    Missing,
    /// Marker file is empty
    Empty,
    /// Marker contains invalid JSON
    InvalidJson(String),
    /// Marker is missing required fields (phase, status, marker_path)
    MissingFields(String),
    /// Marker has an unreadable file (permission error, etc.)
    Unreadable(String),
}

impl MarkerValidation {
    /// Returns true if the marker is valid
    pub fn is_valid(&self) -> bool {
        matches!(self, MarkerValidation::Valid)
    }

    /// Returns true if the marker should be treated as missing (for resume decisions)
    ///
    /// Per research.md Decision 2, all corruption scenarios are treated as missing,
    /// causing rerun from the earliest incomplete phase.
    pub fn treat_as_missing(&self) -> bool {
        !self.is_valid()
    }

    /// Returns a human-readable description of the validation issue
    pub fn description(&self) -> &str {
        match self {
            MarkerValidation::Valid => "valid",
            MarkerValidation::Missing => "file does not exist",
            MarkerValidation::Empty => "file is empty",
            MarkerValidation::InvalidJson(_) => "invalid JSON",
            MarkerValidation::MissingFields(_) => "missing required fields",
            MarkerValidation::Unreadable(_) => "file unreadable",
        }
    }
}

/// Validate a phase marker file without fully parsing it.
///
/// This function checks for various corruption scenarios:
/// - File does not exist
/// - File is empty
/// - File contains invalid JSON
/// - Marker is missing required fields
/// - File is unreadable due to permissions
///
/// Per research.md Decision 2, all corruption scenarios cause the marker
/// to be treated as missing, triggering rerun from the earliest phase.
///
/// # Arguments
///
/// * `path` - Path to the marker file to validate
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::{validate_phase_marker, MarkerValidation};
///
/// # async fn run() {
/// let path = Path::new("/workspace/.devcontainer-state/onCreate.json");
/// let validation = validate_phase_marker(path).await;
/// if validation.treat_as_missing() {
///     println!("Marker is invalid or missing: {}", validation.description());
/// }
/// # }
/// ```
#[instrument(skip_all, fields(path = %path.display()))]
pub async fn validate_phase_marker(path: &Path) -> MarkerValidation {
    // Try to read the file; NotFound is the canonical "missing" path.
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            debug!("Marker file does not exist: {}", path.display());
            return MarkerValidation::Missing;
        }
        Err(e) => {
            warn!(
                "Cannot read marker file at {}: {}. Will treat as missing.",
                path.display(),
                e
            );
            return MarkerValidation::Unreadable(e.to_string());
        }
    };

    // Check for empty file
    if content.trim().is_empty() {
        warn!(
            "Marker file at {} is empty. Will treat as missing.",
            path.display()
        );
        return MarkerValidation::Empty;
    }

    // Try to parse as JSON
    let json_value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            warn!(
                "Invalid JSON in marker file at {}: {}. Will treat as missing.",
                path.display(),
                e
            );
            return MarkerValidation::InvalidJson(e.to_string());
        }
    };

    // Validate required fields
    let obj = match json_value.as_object() {
        Some(o) => o,
        None => {
            warn!(
                "Marker file at {} is not a JSON object. Will treat as missing.",
                path.display()
            );
            return MarkerValidation::InvalidJson("not a JSON object".to_string());
        }
    };

    // Check for required fields: phase, status, marker_path (per data-model.md)
    let mut missing_fields = Vec::new();
    if !obj.contains_key("phase") {
        missing_fields.push("phase");
    }
    if !obj.contains_key("status") {
        missing_fields.push("status");
    }
    if !obj.contains_key("marker_path") && !obj.contains_key("markerPath") {
        missing_fields.push("marker_path");
    }

    if !missing_fields.is_empty() {
        let msg = format!("missing: {}", missing_fields.join(", "));
        warn!(
            "Marker file at {} has missing required fields: {}. Will treat as missing.",
            path.display(),
            msg
        );
        return MarkerValidation::MissingFields(msg);
    }

    // Validate phase field is a known value
    if let Some(phase_val) = obj.get("phase") {
        let phase_str = phase_val.as_str().unwrap_or("");
        let valid_phases = [
            "onCreate",
            "updateContent",
            "postCreate",
            "dotfiles",
            "postStart",
            "postAttach",
            "initialize",
        ];
        if !valid_phases.contains(&phase_str) {
            warn!(
                "Marker file at {} has invalid phase value '{}'. Will treat as missing.",
                path.display(),
                phase_str
            );
            return MarkerValidation::MissingFields(format!("invalid phase: {}", phase_str));
        }
    }

    // Validate status field is a known value
    if let Some(status_val) = obj.get("status") {
        let status_str = status_val.as_str().unwrap_or("");
        let valid_statuses = ["pending", "executed", "skipped", "failed"];
        if !valid_statuses.contains(&status_str) {
            warn!(
                "Marker file at {} has invalid status value '{}'. Will treat as missing.",
                path.display(),
                status_str
            );
            return MarkerValidation::MissingFields(format!("invalid status: {}", status_str));
        }
    }

    debug!("Marker file at {} is valid", path.display());
    MarkerValidation::Valid
}

/// Read a phase marker from disk.
///
/// Returns `Ok(Some(state))` if the marker exists and is valid,
/// `Ok(None)` if the marker does not exist or is corrupted.
///
/// Per research.md Decision 2, corrupted markers are treated as missing
/// to ensure rerun from the earliest phase. This includes:
/// - Empty files
/// - Invalid JSON
/// - Missing required fields
/// - Invalid phase/status values
///
/// # Arguments
///
/// * `path` - Path to the marker file to read
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::read_phase_marker;
///
/// # async fn run() {
/// let path = Path::new("/workspace/.devcontainer-state/onCreate.json");
/// match read_phase_marker(path).await {
///     Ok(Some(state)) => println!("Phase {} status: {:?}", state.phase.as_str(), state.status),
///     Ok(None) => println!("Marker not found or corrupted"),
///     Err(e) => eprintln!("Error reading marker: {}", e),
/// }
/// # }
/// ```
#[instrument(skip_all, fields(path = %path.display()))]
pub async fn read_phase_marker(path: &Path) -> Result<Option<LifecyclePhaseState>> {
    // First validate the marker file
    let validation = validate_phase_marker(path).await;
    if validation.treat_as_missing() {
        // Per Decision 2: all corruption scenarios treated as missing
        return Ok(None);
    }

    // Now read and parse the file (validation already confirmed it's valid JSON)
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(state_io(path.to_path_buf()))?;

    match serde_json::from_str::<LifecyclePhaseState>(&content) {
        Ok(state) => {
            debug!(
                "Read marker for phase {} with status {:?}",
                state.phase.as_str(),
                state.status
            );
            Ok(Some(state))
        }
        Err(e) => {
            // This should rarely happen since validation passed, but handle gracefully
            warn!(
                "Failed to deserialize marker at {}: {}. Will treat as missing.",
                path.display(),
                e
            );
            Ok(None)
        }
    }
}

/// Write a phase marker to disk.
///
/// Creates the parent directory if it does not exist.
/// The marker is written atomically by first writing to a temp file and renaming.
///
/// # Arguments
///
/// * `path` - Path where the marker file should be written
/// * `state` - The phase state to serialize and write
///
/// # Example
///
/// ```no_run
/// use std::path::PathBuf;
/// use deacon_core::state::write_phase_marker;
/// use deacon_core::lifecycle::{LifecyclePhase, LifecyclePhaseState};
///
/// # async fn run() {
/// let path = PathBuf::from("/workspace/.devcontainer-state/onCreate.json");
/// let state = LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, path.clone());
/// write_phase_marker(&path, &state).await.expect("Failed to write marker");
/// # }
/// ```
#[instrument(skip_all, fields(path = %path.display(), phase = %state.phase.as_str()))]
pub async fn write_phase_marker(path: &Path, state: &LifecyclePhaseState) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(state_io(parent.to_path_buf()))?;
    }

    let content = serde_json::to_string_pretty(state).map_err(|source| StateError::Serialize {
        kind: format!("LifecyclePhaseState({})", state.phase.as_str()),
        source,
    })?;

    // Write atomically via temp file + rename for crash safety
    let temp_path = path.with_extension("tmp");
    tokio::fs::write(&temp_path, &content)
        .await
        .map_err(state_io(temp_path.clone()))?;

    tokio::fs::rename(&temp_path, path)
        .await
        .map_err(state_io(path.to_path_buf()))?;

    debug!(
        "Wrote marker for phase {} with status {:?} to {}",
        state.phase.as_str(),
        state.status,
        path.display()
    );

    Ok(())
}

/// Read all phase markers from a workspace in spec-defined order.
///
/// Returns markers in lifecycle order: onCreate, updateContent, postCreate,
/// dotfiles, postStart, postAttach. Missing or corrupted markers are omitted.
///
/// Per research.md Decision 2, if markers are missing or corrupted, rerun
/// starts from the earliest incomplete phase.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `prebuild` - If true, reads from the isolated prebuild marker directory
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::read_all_markers;
///
/// # async fn run() {
/// let workspace = Path::new("/workspace");
/// let markers = read_all_markers(workspace, false, Some(workspace)).await.expect("Failed to read markers");
/// for marker in markers {
///     println!("{}: {:?}", marker.phase.as_str(), marker.status);
/// }
/// # }
/// ```
#[instrument(skip_all, fields(workspace = %workspace.display(), prebuild))]
pub async fn read_all_markers(
    workspace: &Path,
    prebuild: bool,
    user_data_folder: Option<&Path>,
) -> Result<Vec<LifecyclePhaseState>> {
    read_all_markers_for_config(workspace, prebuild, None, user_data_folder).await
}

/// Read every marker for `workspace`, then drop any whose recorded
/// `config_hash` doesn't match `current_config_hash`. Markers without a
/// recorded hash (legacy from older deacon builds) are kept so we don't
/// regress users upgrading deacon.
///
/// Caller passes `None` to skip the filter and read everything (#93).
#[instrument(skip_all, fields(workspace = %workspace.display(), prebuild))]
pub async fn read_all_markers_for_config(
    workspace: &Path,
    prebuild: bool,
    current_config_hash: Option<&str>,
    user_data_folder: Option<&Path>,
) -> Result<Vec<LifecyclePhaseState>> {
    let mut markers = Vec::new();
    let mut dropped = 0usize;

    for phase in LifecyclePhase::spec_order() {
        let path = if prebuild {
            prebuild_marker_path_for_phase(workspace, *phase, user_data_folder)?
        } else {
            marker_path_for_phase(workspace, *phase, user_data_folder)?
        };

        if let Some(state) = read_phase_marker(&path).await? {
            // Drop the marker when its recorded config hash differs from
            // the current config — this forces a rerun, matching what the
            // user expects after `--override-config` (or any other input
            // that changes the resolved config). Markers without a
            // recorded hash predate #93 and are kept as compatible.
            if let (Some(current), Some(recorded)) = (current_config_hash, &state.config_hash) {
                if current != recorded {
                    debug!(
                        "Dropping stale marker for phase {}: marker config_hash={} current={}",
                        state.phase.as_str(),
                        recorded,
                        current
                    );
                    dropped += 1;
                    continue;
                }
            }
            markers.push(state);
        }
    }

    debug!(
        "Read {} markers from {} (prebuild={}; {} dropped as stale)",
        markers.len(),
        workspace.display(),
        prebuild,
        dropped
    );

    Ok(markers)
}

/// Clear all phase markers for a workspace.
///
/// Removes all marker files from the specified marker directory.
/// Does not error if markers don't exist.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `prebuild` - If true, clears prebuild markers; otherwise clears normal markers
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::clear_markers;
///
/// # async fn run() {
/// let workspace = Path::new("/workspace");
/// clear_markers(workspace, false, None).await.expect("Failed to clear markers");
/// # }
/// ```
#[instrument(skip_all, fields(workspace = %workspace.display(), prebuild))]
pub async fn clear_markers(
    workspace: &Path,
    prebuild: bool,
    user_data_folder: Option<&Path>,
) -> Result<()> {
    let base_dir = marker_base_dir(workspace, prebuild, user_data_folder)?;

    if tokio::fs::metadata(&base_dir).await.is_err() {
        debug!("Marker directory does not exist, nothing to clear");
        return Ok(());
    }

    let mut cleared_count = 0;
    for phase in LifecyclePhase::spec_order() {
        let path = base_dir.join(format!("{}.{}", phase.as_str(), MARKER_EXTENSION));
        match tokio::fs::remove_file(&path).await {
            Ok(()) => cleared_count += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(state_io(path.clone())(e)),
        }
    }

    info!(
        "Cleared {} markers from {} (prebuild={})",
        cleared_count,
        base_dir.display(),
        prebuild
    );

    Ok(())
}

/// Find the earliest incomplete phase based on existing markers.
///
/// Returns the first phase in spec order that either has no marker or has a
/// non-executed status. This determines where resume should start.
///
/// Per research.md Decision 2, missing or corrupted markers cause rerun from
/// the earliest incomplete phase.
///
/// # Arguments
///
/// * `markers` - List of existing phase markers (in any order)
///
/// # Returns
///
/// The first incomplete phase, or `None` if all phases are complete.
///
/// # Example
///
/// ```
/// use deacon_core::state::find_earliest_incomplete_phase;
/// use deacon_core::lifecycle::{LifecyclePhase, LifecyclePhaseState};
/// use std::path::PathBuf;
///
/// // Only onCreate is complete
/// let markers = vec![
///     LifecyclePhaseState::new_executed(
///         LifecyclePhase::OnCreate,
///         PathBuf::from("/markers/onCreate.json")
///     ),
/// ];
/// let incomplete = find_earliest_incomplete_phase(&markers);
/// assert_eq!(incomplete, Some(LifecyclePhase::UpdateContent));
/// ```
pub fn find_earliest_incomplete_phase(markers: &[LifecyclePhaseState]) -> Option<LifecyclePhase> {
    for phase in LifecyclePhase::spec_order() {
        let is_complete = markers
            .iter()
            .any(|m| m.phase == *phase && m.status == PhaseStatus::Executed);

        if !is_complete {
            return Some(*phase);
        }
    }

    None
}

/// Check if all phases up to and including the specified phase are complete.
///
/// This is useful for determining if resume can skip to a later phase.
///
/// # Arguments
///
/// * `markers` - List of existing phase markers
/// * `up_to_phase` - The phase to check up to (inclusive)
///
/// # Returns
///
/// `true` if all phases from onCreate through `up_to_phase` have Executed status.
pub fn all_phases_complete_up_to(
    markers: &[LifecyclePhaseState],
    up_to_phase: LifecyclePhase,
) -> bool {
    for phase in LifecyclePhase::spec_order() {
        let is_complete = markers
            .iter()
            .any(|m| m.phase == *phase && m.status == PhaseStatus::Executed);

        if !is_complete {
            return false;
        }

        if *phase == up_to_phase {
            return true;
        }
    }

    false
}

/// Check if a marker file exists for a specific lifecycle phase.
///
/// This function checks whether a marker file exists at the appropriate location
/// without validating its contents. Use this for simple existence checks where
/// marker content validation is not required.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `phase` - The lifecycle phase to check for
/// * `prebuild` - If true, checks the isolated prebuild marker directory
///
/// # Returns
///
/// `true` if the marker file exists, `false` otherwise.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::marker_exists;
/// use deacon_core::lifecycle::LifecyclePhase;
///
/// # async fn run() {
/// let workspace = Path::new("/workspace");
/// if marker_exists(workspace, LifecyclePhase::OnCreate, false, None).await {
///     println!("onCreate marker exists");
/// }
/// # }
/// ```
pub async fn marker_exists(
    workspace: &Path,
    phase: LifecyclePhase,
    prebuild: bool,
    user_data_folder: Option<&Path>,
) -> bool {
    let marker_path = if prebuild {
        prebuild_marker_path_for_phase(workspace, phase, user_data_folder)
    } else {
        marker_path_for_phase(workspace, phase, user_data_folder)
    };
    match marker_path {
        Ok(p) => tokio::fs::metadata(&p).await.is_ok(),
        Err(_) => false,
    }
}

/// Record a phase as successfully executed by writing its marker to disk.
///
/// This function creates an executed phase state with a current timestamp and
/// writes it to the appropriate marker file. It is intended to be called after
/// each phase completes successfully during lifecycle execution.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `phase` - The lifecycle phase that completed successfully
/// * `prebuild` - If true, writes to the isolated prebuild marker directory
///
/// # Returns
///
/// The `LifecyclePhaseState` that was recorded, or an error if writing failed.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::record_phase_executed;
/// use deacon_core::lifecycle::LifecyclePhase;
///
/// # async fn run() {
/// let workspace = Path::new("/workspace");
/// let state = record_phase_executed(workspace, LifecyclePhase::OnCreate, false, None)
///     .await
///     .expect("Failed to record phase");
/// assert_eq!(state.phase, LifecyclePhase::OnCreate);
/// # }
/// ```
#[instrument(skip_all, fields(workspace = %workspace.display(), phase = %phase.as_str(), prebuild))]
pub async fn record_phase_executed(
    workspace: &Path,
    phase: LifecyclePhase,
    prebuild: bool,
    user_data_folder: Option<&Path>,
) -> Result<LifecyclePhaseState> {
    record_phase_executed_with_config_hash(workspace, phase, prebuild, None, user_data_folder).await
}

/// Record a phase as executed and stamp the marker with the resolved
/// configuration hash so future runs can detect a config drift (#93).
///
/// Callers that have a `ContainerIdentity` available (i.e. `up` and the
/// in-process lifecycle runner) should pass `Some(identity.config_hash)`.
/// Callers without identity context can pass `None`; the marker is
/// considered universally compatible in that case.
#[instrument(skip_all, fields(workspace = %workspace.display(), phase = %phase.as_str(), prebuild))]
pub async fn record_phase_executed_with_config_hash(
    workspace: &Path,
    phase: LifecyclePhase,
    prebuild: bool,
    config_hash: Option<&str>,
    user_data_folder: Option<&Path>,
) -> Result<LifecyclePhaseState> {
    let marker_path = if prebuild {
        prebuild_marker_path_for_phase(workspace, phase, user_data_folder)?
    } else {
        marker_path_for_phase(workspace, phase, user_data_folder)?
    };

    let mut state = LifecyclePhaseState::new_executed(phase, marker_path.clone());
    if let Some(hash) = config_hash {
        state = state.with_config_hash(hash);
    }
    write_phase_marker(&marker_path, &state).await?;

    info!(
        "Recorded phase {} as executed at {}",
        phase.as_str(),
        marker_path.display()
    );

    Ok(state)
}

/// Record a phase as skipped by writing its marker to disk.
///
/// This function creates a skipped phase state with the given reason and
/// writes it to the appropriate marker file. Skipped markers help track
/// which phases were intentionally bypassed (e.g., due to flags or mode).
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `phase` - The lifecycle phase that was skipped
/// * `reason` - Human-readable reason for skipping (e.g., "prebuild mode", "--skip-post-create flag")
/// * `prebuild` - If true, writes to the isolated prebuild marker directory
///
/// # Returns
///
/// The `LifecyclePhaseState` that was recorded, or an error if writing failed.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::record_phase_skipped;
/// use deacon_core::lifecycle::LifecyclePhase;
///
/// # async fn run() {
/// let workspace = Path::new("/workspace");
/// let state = record_phase_skipped(workspace, LifecyclePhase::PostCreate, "prebuild mode", true, None)
///     .await
///     .expect("Failed to record skipped phase");
/// assert_eq!(state.reason, Some("prebuild mode".to_string()));
/// # }
/// ```
#[instrument(skip_all, fields(workspace = %workspace.display(), phase = %phase.as_str(), reason = %reason, prebuild))]
pub async fn record_phase_skipped(
    workspace: &Path,
    phase: LifecyclePhase,
    reason: &str,
    prebuild: bool,
    user_data_folder: Option<&Path>,
) -> Result<LifecyclePhaseState> {
    let marker_path = if prebuild {
        prebuild_marker_path_for_phase(workspace, phase, user_data_folder)?
    } else {
        marker_path_for_phase(workspace, phase, user_data_folder)?
    };

    let state = LifecyclePhaseState::new_skipped(phase, marker_path.clone(), reason);
    write_phase_marker(&marker_path, &state).await?;

    info!(
        "Recorded phase {} as skipped (reason: {}) at {}",
        phase.as_str(),
        reason,
        marker_path.display()
    );

    Ok(state)
}

/// Record markers for all phases in a run summary.
///
/// This function iterates through the phases in the summary and writes markers
/// for each executed or skipped phase. It is typically called after lifecycle
/// execution completes to persist the final state.
///
/// # Arguments
///
/// * `workspace` - Path to the devcontainer workspace root
/// * `phases` - Slice of phase states to record
/// * `prebuild` - If true, writes to the isolated prebuild marker directory
///
/// # Returns
///
/// `Ok(())` if all markers were written successfully, or an error if any write failed.
///
/// # Example
///
/// ```no_run
/// use std::path::Path;
/// use deacon_core::state::record_all_phase_markers;
/// use deacon_core::lifecycle::{LifecyclePhase, LifecyclePhaseState, PhaseStatus};
/// use std::path::PathBuf;
///
/// # async fn run() {
/// let workspace = Path::new("/workspace");
/// let phases = vec![
///     LifecyclePhaseState::new_executed(
///         LifecyclePhase::OnCreate,
///         PathBuf::from("/workspace/.devcontainer-state/onCreate.json")
///     ),
///     LifecyclePhaseState::new_skipped(
///         LifecyclePhase::PostCreate,
///         PathBuf::from("/workspace/.devcontainer-state/postCreate.json"),
///         "prebuild mode"
///     ),
/// ];
/// record_all_phase_markers(workspace, &phases, false, None).await.expect("Failed to record markers");
/// # }
/// ```
#[instrument(skip_all, fields(workspace = %workspace.display(), phase_count = phases.len(), prebuild))]
pub async fn record_all_phase_markers(
    workspace: &Path,
    phases: &[LifecyclePhaseState],
    prebuild: bool,
    user_data_folder: Option<&Path>,
) -> Result<()> {
    let mut recorded = 0;

    for phase_state in phases {
        // Only record executed or skipped phases (not pending or failed)
        match phase_state.status {
            PhaseStatus::Executed | PhaseStatus::Skipped => {
                let marker_path = if prebuild {
                    prebuild_marker_path_for_phase(workspace, phase_state.phase, user_data_folder)?
                } else {
                    marker_path_for_phase(workspace, phase_state.phase, user_data_folder)?
                };

                // Create a new state with the correct marker path
                let state = match phase_state.status {
                    PhaseStatus::Executed => {
                        LifecyclePhaseState::new_executed(phase_state.phase, marker_path.clone())
                    }
                    PhaseStatus::Skipped => LifecyclePhaseState::new_skipped(
                        phase_state.phase,
                        marker_path.clone(),
                        phase_state.reason.as_deref().unwrap_or("unknown"),
                    ),
                    _ => unreachable!(),
                };

                write_phase_marker(&marker_path, &state).await?;
                recorded += 1;
            }
            PhaseStatus::Pending | PhaseStatus::Failed => {
                // Don't record pending or failed phases as completion markers
                debug!(
                    "Skipping marker recording for phase {} with status {:?}",
                    phase_state.phase.as_str(),
                    phase_state.status
                );
            }
        }
    }

    info!(
        "Recorded {} phase markers in {} (prebuild={})",
        recorded,
        workspace.display(),
        prebuild
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_state_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        // Should be able to create successfully
        assert_eq!(state_manager.stats().entries, 0);
    }

    #[test]
    fn test_container_state_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let container_state = ContainerState {
            container_id: "abc123".to_string(),
            container_name: Some("test-container".to_string()),
            image_id: "image123".to_string(),
            shutdown_action: Some("stopContainer".to_string()),
        };

        let workspace_hash = "test-workspace-hash";

        // Save state
        state_manager
            .save_container_state(workspace_hash, container_state.clone())
            .unwrap();

        // Retrieve state
        let retrieved = state_manager.get_workspace_state(workspace_hash).unwrap();

        match retrieved {
            WorkspaceState::Container(retrieved_container) => {
                assert_eq!(retrieved_container, container_state);
            }
            _ => panic!("Expected container state"),
        }
    }

    #[test]
    fn test_compose_state_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let compose_state = ComposeState {
            project_name: "test-project".to_string(),
            service_name: "app".to_string(),
            base_path: "/workspace".to_string(),
            compose_files: vec!["docker-compose.yml".to_string()],
            shutdown_action: Some("stopCompose".to_string()),
        };

        let workspace_hash = "test-workspace-hash";

        // Save state
        state_manager
            .save_compose_state(workspace_hash, compose_state.clone())
            .unwrap();

        // Retrieve state
        let retrieved = state_manager.get_workspace_state(workspace_hash).unwrap();

        match retrieved {
            WorkspaceState::Compose(retrieved_compose) => {
                assert_eq!(retrieved_compose, compose_state);
            }
            _ => panic!("Expected compose state"),
        }
    }

    #[test]
    fn test_remove_workspace_state() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let container_state = ContainerState {
            container_id: "abc123".to_string(),
            container_name: None,
            image_id: "image123".to_string(),
            shutdown_action: None,
        };

        let workspace_hash = "test-workspace-hash";

        // Save state
        state_manager
            .save_container_state(workspace_hash, container_state.clone())
            .unwrap();

        // Verify it exists
        assert!(state_manager.get_workspace_state(workspace_hash).is_some());

        // Remove state
        let removed = state_manager.remove_workspace_state(workspace_hash);
        assert!(removed.is_some());

        // Verify it's gone
        assert!(state_manager.get_workspace_state(workspace_hash).is_none());
    }

    #[test]
    fn test_nonexistent_workspace_state() {
        let temp_dir = TempDir::new().unwrap();
        let mut state_manager = StateManager::new_with_cache_dir(temp_dir.path()).unwrap();

        let result = state_manager.get_workspace_state("nonexistent");
        assert!(result.is_none());
    }

    // =========================================================================
    // Lifecycle Phase Marker Helper Tests
    // =========================================================================

    // Markers now live under the host user-data folder
    // (`<user_data>/state/<workspace_hash>/[prebuild/]<phase>.json`), NOT inside the
    // project (`<workspace>/.devcontainer-state/`). See #280.
    #[test]
    fn test_marker_base_dir_normal() {
        let workspace = Path::new("/workspace");
        let user_data = Path::new("/udf");
        let hash = crate::container::ContainerIdentity::hash_workspace_path(workspace);
        let base = marker_base_dir(workspace, false, Some(user_data)).unwrap();
        assert_eq!(base, PathBuf::from("/udf").join("state").join(&hash));
        assert!(!base.to_string_lossy().contains(".devcontainer-state"));
    }

    #[test]
    fn test_marker_base_dir_prebuild() {
        let workspace = Path::new("/workspace");
        let user_data = Path::new("/udf");
        let hash = crate::container::ContainerIdentity::hash_workspace_path(workspace);
        let base = marker_base_dir(workspace, true, Some(user_data)).unwrap();
        assert_eq!(
            base,
            PathBuf::from("/udf")
                .join("state")
                .join(&hash)
                .join("prebuild")
        );
    }

    #[test]
    fn test_marker_path_for_phase() {
        let workspace = Path::new("/workspace");
        let user_data = Path::new("/udf");
        let hash = crate::container::ContainerIdentity::hash_workspace_path(workspace);
        let expected_dir = PathBuf::from("/udf").join("state").join(&hash);

        for (phase, file) in [
            (LifecyclePhase::OnCreate, "onCreate.json"),
            (LifecyclePhase::UpdateContent, "updateContent.json"),
            (LifecyclePhase::PostCreate, "postCreate.json"),
            (LifecyclePhase::Dotfiles, "dotfiles.json"),
            (LifecyclePhase::PostStart, "postStart.json"),
            (LifecyclePhase::PostAttach, "postAttach.json"),
        ] {
            assert_eq!(
                marker_path_for_phase(workspace, phase, Some(user_data)).unwrap(),
                expected_dir.join(file)
            );
        }
    }

    #[test]
    fn test_prebuild_marker_path_for_phase() {
        let workspace = Path::new("/workspace");
        let user_data = Path::new("/udf");
        let hash = crate::container::ContainerIdentity::hash_workspace_path(workspace);
        let expected_dir = PathBuf::from("/udf")
            .join("state")
            .join(&hash)
            .join("prebuild");

        assert_eq!(
            prebuild_marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(user_data))
                .unwrap(),
            expected_dir.join("onCreate.json")
        );
        assert_eq!(
            prebuild_marker_path_for_phase(
                workspace,
                LifecyclePhase::UpdateContent,
                Some(user_data)
            )
            .unwrap(),
            expected_dir.join("updateContent.json")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_phase_marker_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.json");

        let result = read_phase_marker(&path).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_write_and_read_phase_marker() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();
        let path =
            marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace)).unwrap();

        // Write a marker
        let state = LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, path.clone());
        write_phase_marker(&path, &state).await.unwrap();

        // Read it back
        let read_state = read_phase_marker(&path).await.unwrap().unwrap();
        assert_eq!(read_state.phase, LifecyclePhase::OnCreate);
        assert_eq!(read_state.status, PhaseStatus::Executed);
        assert_eq!(read_state.marker_path, path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_write_phase_marker_creates_directory() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();
        let path =
            prebuild_marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace))
                .unwrap();

        // Parent directories should not exist yet
        assert!(!path.parent().unwrap().exists());

        // Write should create directories
        let state = LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, path.clone());
        write_phase_marker(&path, &state).await.unwrap();

        // Now the file should exist
        assert!(path.exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_phase_marker_corrupted_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("corrupted.json");

        // Write invalid JSON
        std::fs::write(&path, "not valid json {{{").unwrap();

        // Should return None (not error) per Decision 2
        let result = read_phase_marker(&path).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_all_markers_empty() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert!(markers.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_all_markers_partial() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Write markers for some phases
        let on_create_path =
            marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace)).unwrap();
        let on_create_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, on_create_path.clone());
        write_phase_marker(&on_create_path, &on_create_state)
            .await
            .unwrap();

        let update_content_path =
            marker_path_for_phase(workspace, LifecyclePhase::UpdateContent, Some(workspace))
                .unwrap();
        let update_content_state = LifecyclePhaseState::new_executed(
            LifecyclePhase::UpdateContent,
            update_content_path.clone(),
        );
        write_phase_marker(&update_content_path, &update_content_state)
            .await
            .unwrap();

        // Read all markers
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].phase, LifecyclePhase::OnCreate);
        assert_eq!(markers[1].phase, LifecyclePhase::UpdateContent);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_all_markers_prebuild_isolation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Write normal marker
        let normal_path =
            marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace)).unwrap();
        let normal_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, normal_path.clone());
        write_phase_marker(&normal_path, &normal_state)
            .await
            .unwrap();

        // Write prebuild marker
        let prebuild_path =
            prebuild_marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace))
                .unwrap();
        let prebuild_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, prebuild_path.clone());
        write_phase_marker(&prebuild_path, &prebuild_state)
            .await
            .unwrap();

        // Normal markers should not include prebuild
        let normal_markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert_eq!(normal_markers.len(), 1);
        assert_eq!(normal_markers[0].marker_path, normal_path);

        // Prebuild markers should not include normal
        let prebuild_markers = read_all_markers(workspace, true, Some(workspace))
            .await
            .unwrap();
        assert_eq!(prebuild_markers.len(), 1);
        assert_eq!(prebuild_markers[0].marker_path, prebuild_path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_clear_markers() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Write several markers
        for phase in &[
            LifecyclePhase::OnCreate,
            LifecyclePhase::UpdateContent,
            LifecyclePhase::PostCreate,
        ] {
            let path = marker_path_for_phase(workspace, *phase, Some(workspace)).unwrap();
            let state = LifecyclePhaseState::new_executed(*phase, path.clone());
            write_phase_marker(&path, &state).await.unwrap();
        }

        // Verify markers exist
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert_eq!(markers.len(), 3);

        // Clear markers
        clear_markers(workspace, false, Some(workspace))
            .await
            .unwrap();

        // Verify markers are gone
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert!(markers.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_clear_markers_prebuild_only() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Write normal marker
        let normal_path =
            marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace)).unwrap();
        let normal_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, normal_path.clone());
        write_phase_marker(&normal_path, &normal_state)
            .await
            .unwrap();

        // Write prebuild marker
        let prebuild_path =
            prebuild_marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace))
                .unwrap();
        let prebuild_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, prebuild_path.clone());
        write_phase_marker(&prebuild_path, &prebuild_state)
            .await
            .unwrap();

        // Clear only prebuild markers
        clear_markers(workspace, true, Some(workspace))
            .await
            .unwrap();

        // Normal markers should still exist
        let normal_markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert_eq!(normal_markers.len(), 1);

        // Prebuild markers should be gone
        let prebuild_markers = read_all_markers(workspace, true, Some(workspace))
            .await
            .unwrap();
        assert!(prebuild_markers.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_clear_markers_nonexistent_directory() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Should not error when directory doesn't exist
        clear_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        clear_markers(workspace, true, Some(workspace))
            .await
            .unwrap();
    }

    #[test]
    fn test_find_earliest_incomplete_phase_empty() {
        let markers: Vec<LifecyclePhaseState> = vec![];
        let result = find_earliest_incomplete_phase(&markers);
        assert_eq!(result, Some(LifecyclePhase::OnCreate));
    }

    #[test]
    fn test_find_earliest_incomplete_phase_partial() {
        let markers = vec![LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            PathBuf::from("/markers/onCreate.json"),
        )];
        let result = find_earliest_incomplete_phase(&markers);
        assert_eq!(result, Some(LifecyclePhase::UpdateContent));
    }

    #[test]
    fn test_find_earliest_incomplete_phase_gap() {
        // onCreate and postCreate complete, but updateContent missing
        let markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/markers/onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                PathBuf::from("/markers/postCreate.json"),
            ),
        ];
        let result = find_earliest_incomplete_phase(&markers);
        // Should return updateContent (the first missing phase)
        assert_eq!(result, Some(LifecyclePhase::UpdateContent));
    }

    #[test]
    fn test_find_earliest_incomplete_phase_all_complete() {
        let markers: Vec<LifecyclePhaseState> = LifecyclePhase::spec_order()
            .iter()
            .map(|phase| {
                LifecyclePhaseState::new_executed(
                    *phase,
                    PathBuf::from(format!("/markers/{}.json", phase.as_str())),
                )
            })
            .collect();
        let result = find_earliest_incomplete_phase(&markers);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_earliest_incomplete_phase_skipped_not_complete() {
        // Skipped phase is not considered complete
        let markers = vec![LifecyclePhaseState::new_skipped(
            LifecyclePhase::OnCreate,
            PathBuf::from("/markers/onCreate.json"),
            "test skip",
        )];
        let result = find_earliest_incomplete_phase(&markers);
        // Skipped != Executed, so onCreate is incomplete
        assert_eq!(result, Some(LifecyclePhase::OnCreate));
    }

    #[test]
    fn test_all_phases_complete_up_to() {
        let markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/markers/onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                PathBuf::from("/markers/updateContent.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                PathBuf::from("/markers/postCreate.json"),
            ),
        ];

        // Complete up to onCreate
        assert!(all_phases_complete_up_to(
            &markers,
            LifecyclePhase::OnCreate
        ));

        // Complete up to updateContent
        assert!(all_phases_complete_up_to(
            &markers,
            LifecyclePhase::UpdateContent
        ));

        // Complete up to postCreate
        assert!(all_phases_complete_up_to(
            &markers,
            LifecyclePhase::PostCreate
        ));

        // NOT complete up to dotfiles (missing)
        assert!(!all_phases_complete_up_to(
            &markers,
            LifecyclePhase::Dotfiles
        ));
    }

    #[test]
    fn test_all_phases_complete_up_to_gap() {
        // onCreate complete but updateContent missing
        let markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/markers/onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                PathBuf::from("/markers/postCreate.json"),
            ),
        ];

        // Complete up to onCreate
        assert!(all_phases_complete_up_to(
            &markers,
            LifecyclePhase::OnCreate
        ));

        // NOT complete up to updateContent (it's missing)
        assert!(!all_phases_complete_up_to(
            &markers,
            LifecyclePhase::UpdateContent
        ));

        // NOT complete up to postCreate (updateContent is missing before it)
        assert!(!all_phases_complete_up_to(
            &markers,
            LifecyclePhase::PostCreate
        ));
    }

    // =========================================================================
    // Record Phase Marker Tests
    // =========================================================================

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_phase_executed() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Record onCreate as executed
        let state =
            record_phase_executed(workspace, LifecyclePhase::OnCreate, false, Some(workspace))
                .await
                .unwrap();

        assert_eq!(state.phase, LifecyclePhase::OnCreate);
        assert_eq!(state.status, PhaseStatus::Executed);
        assert!(state.timestamp.is_some());

        // Verify the marker was written to disk
        let read_state = read_phase_marker(
            &marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace)).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(read_state.phase, LifecyclePhase::OnCreate);
        assert_eq!(read_state.status, PhaseStatus::Executed);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_phase_executed_prebuild() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Record onCreate as executed in prebuild mode
        let state =
            record_phase_executed(workspace, LifecyclePhase::OnCreate, true, Some(workspace))
                .await
                .unwrap();

        assert_eq!(state.phase, LifecyclePhase::OnCreate);
        assert_eq!(state.status, PhaseStatus::Executed);

        // Verify the marker was written to the prebuild directory
        let read_state = read_phase_marker(
            &prebuild_marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace))
                .unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(read_state.phase, LifecyclePhase::OnCreate);
        assert_eq!(read_state.status, PhaseStatus::Executed);

        // Verify normal marker directory does NOT have the marker
        assert!(
            read_phase_marker(
                &marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace))
                    .unwrap()
            )
            .await
            .unwrap()
            .is_none()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_phase_skipped() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Record postCreate as skipped
        let state = record_phase_skipped(
            workspace,
            LifecyclePhase::PostCreate,
            "prebuild mode",
            false,
            Some(workspace),
        )
        .await
        .unwrap();

        assert_eq!(state.phase, LifecyclePhase::PostCreate);
        assert_eq!(state.status, PhaseStatus::Skipped);
        assert_eq!(state.reason, Some("prebuild mode".to_string()));

        // Verify the marker was written to disk
        let read_state = read_phase_marker(
            &marker_path_for_phase(workspace, LifecyclePhase::PostCreate, Some(workspace)).unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(read_state.phase, LifecyclePhase::PostCreate);
        assert_eq!(read_state.status, PhaseStatus::Skipped);
        assert_eq!(read_state.reason, Some("prebuild mode".to_string()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_phase_skipped_prebuild() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Record postCreate as skipped in prebuild mode
        let state = record_phase_skipped(
            workspace,
            LifecyclePhase::PostCreate,
            "--skip-post-create flag",
            true,
            Some(workspace),
        )
        .await
        .unwrap();

        assert_eq!(state.phase, LifecyclePhase::PostCreate);
        assert_eq!(state.status, PhaseStatus::Skipped);

        // Verify the marker was written to the prebuild directory
        let read_state = read_phase_marker(
            &prebuild_marker_path_for_phase(workspace, LifecyclePhase::PostCreate, Some(workspace))
                .unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(read_state.status, PhaseStatus::Skipped);
        assert_eq!(
            read_state.reason,
            Some("--skip-post-create flag".to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_all_phase_markers() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Create a mix of executed and skipped phases
        let phases = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/dummy/onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                PathBuf::from("/dummy/updateContent.json"),
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostCreate,
                PathBuf::from("/dummy/postCreate.json"),
                "prebuild mode",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::Dotfiles,
                PathBuf::from("/dummy/dotfiles.json"),
                "prebuild mode",
            ),
        ];

        // Record all markers
        record_all_phase_markers(workspace, &phases, false, Some(workspace))
            .await
            .unwrap();

        // Verify all markers were written
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert_eq!(markers.len(), 4);

        // Check executed phases
        let on_create = markers.iter().find(|m| m.phase == LifecyclePhase::OnCreate);
        assert!(on_create.is_some());
        assert_eq!(on_create.unwrap().status, PhaseStatus::Executed);

        let update_content = markers
            .iter()
            .find(|m| m.phase == LifecyclePhase::UpdateContent);
        assert!(update_content.is_some());
        assert_eq!(update_content.unwrap().status, PhaseStatus::Executed);

        // Check skipped phases
        let post_create = markers
            .iter()
            .find(|m| m.phase == LifecyclePhase::PostCreate);
        assert!(post_create.is_some());
        assert_eq!(post_create.unwrap().status, PhaseStatus::Skipped);
        assert_eq!(
            post_create.unwrap().reason,
            Some("prebuild mode".to_string())
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_all_phase_markers_skips_pending_and_failed() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Create phases with different statuses including pending and failed
        let phases = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/dummy/onCreate.json"),
            ),
            LifecyclePhaseState::new_pending(
                LifecyclePhase::UpdateContent,
                PathBuf::from("/dummy/updateContent.json"),
            ),
            LifecyclePhaseState::new_failed(
                LifecyclePhase::PostCreate,
                PathBuf::from("/dummy/postCreate.json"),
                "command failed",
            ),
        ];

        // Record all markers
        record_all_phase_markers(workspace, &phases, false, Some(workspace))
            .await
            .unwrap();

        // Verify only executed phase was recorded
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].phase, LifecyclePhase::OnCreate);
        assert_eq!(markers[0].status, PhaseStatus::Executed);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_all_phase_markers_prebuild_isolation() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        let phases = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/dummy/onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                PathBuf::from("/dummy/updateContent.json"),
            ),
        ];

        // Record markers in prebuild mode
        record_all_phase_markers(workspace, &phases, true, Some(workspace))
            .await
            .unwrap();

        // Verify markers exist in prebuild directory
        let prebuild_markers = read_all_markers(workspace, true, Some(workspace))
            .await
            .unwrap();
        assert_eq!(prebuild_markers.len(), 2);

        // Verify normal marker directory is empty
        let normal_markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert!(normal_markers.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_record_phases_in_lifecycle_order() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Record phases in lifecycle order (simulating a fresh run)
        for phase in LifecyclePhase::spec_order() {
            record_phase_executed(workspace, *phase, false, Some(workspace))
                .await
                .unwrap();
        }

        // Read all markers and verify they are in order
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();
        assert_eq!(markers.len(), 6);

        // Verify order matches spec order
        let spec_order = LifecyclePhase::spec_order();
        for (i, marker) in markers.iter().enumerate() {
            assert_eq!(
                marker.phase, spec_order[i],
                "Marker at index {} should be {:?}",
                i, spec_order[i]
            );
        }
    }

    // =========================================================================
    // Marker Validation Tests (T015 - Corrupted/Missing Marker Handling)
    // =========================================================================

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.json");

        let validation = validate_phase_marker(&path).await;
        assert_eq!(validation, MarkerValidation::Missing);
        assert!(validation.treat_as_missing());
        assert_eq!(validation.description(), "file does not exist");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("empty.json");

        std::fs::write(&path, "").unwrap();

        let validation = validate_phase_marker(&path).await;
        assert_eq!(validation, MarkerValidation::Empty);
        assert!(validation.treat_as_missing());
        assert_eq!(validation.description(), "file is empty");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_whitespace_only() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("whitespace.json");

        std::fs::write(&path, "   \n  \t  ").unwrap();

        let validation = validate_phase_marker(&path).await;
        assert_eq!(validation, MarkerValidation::Empty);
        assert!(validation.treat_as_missing());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("invalid.json");

        std::fs::write(&path, "not valid json {{{").unwrap();

        let validation = validate_phase_marker(&path).await;
        assert!(matches!(validation, MarkerValidation::InvalidJson(_)));
        assert!(validation.treat_as_missing());
        assert_eq!(validation.description(), "invalid JSON");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_json_array() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("array.json");

        std::fs::write(&path, "[1, 2, 3]").unwrap();

        let validation = validate_phase_marker(&path).await;
        assert!(matches!(validation, MarkerValidation::InvalidJson(_)));
        assert!(validation.treat_as_missing());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_missing_phase_field() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("missing_phase.json");

        std::fs::write(
            &path,
            r#"{"status": "executed", "marker_path": "/path/to/marker"}"#,
        )
        .unwrap();

        let validation = validate_phase_marker(&path).await;
        assert!(matches!(validation, MarkerValidation::MissingFields(_)));
        assert!(validation.treat_as_missing());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_missing_status_field() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("missing_status.json");

        std::fs::write(
            &path,
            r#"{"phase": "onCreate", "marker_path": "/path/to/marker"}"#,
        )
        .unwrap();

        let validation = validate_phase_marker(&path).await;
        assert!(matches!(validation, MarkerValidation::MissingFields(_)));
        assert!(validation.treat_as_missing());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_missing_marker_path_field() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("missing_marker_path.json");

        std::fs::write(&path, r#"{"phase": "onCreate", "status": "executed"}"#).unwrap();

        let validation = validate_phase_marker(&path).await;
        assert!(matches!(validation, MarkerValidation::MissingFields(_)));
        assert!(validation.treat_as_missing());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_invalid_phase_value() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("invalid_phase.json");

        std::fs::write(
            &path,
            r#"{"phase": "unknownPhase", "status": "executed", "marker_path": "/path"}"#,
        )
        .unwrap();

        let validation = validate_phase_marker(&path).await;
        assert!(matches!(validation, MarkerValidation::MissingFields(_)));
        assert!(validation.treat_as_missing());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_invalid_status_value() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("invalid_status.json");

        std::fs::write(
            &path,
            r#"{"phase": "onCreate", "status": "unknownStatus", "marker_path": "/path"}"#,
        )
        .unwrap();

        let validation = validate_phase_marker(&path).await;
        assert!(matches!(validation, MarkerValidation::MissingFields(_)));
        assert!(validation.treat_as_missing());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_valid() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("valid.json");

        std::fs::write(
            &path,
            r#"{"phase": "onCreate", "status": "executed", "marker_path": "/path/to/marker"}"#,
        )
        .unwrap();

        let validation = validate_phase_marker(&path).await;
        assert_eq!(validation, MarkerValidation::Valid);
        assert!(validation.is_valid());
        assert!(!validation.treat_as_missing());
        assert_eq!(validation.description(), "valid");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_validate_phase_marker_valid_with_camel_case_marker_path() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("valid_camel.json");

        // markerPath is also accepted (serde rename_all = "camelCase")
        std::fs::write(
            &path,
            r#"{"phase": "postStart", "status": "skipped", "markerPath": "/path/to/marker", "reason": "prebuild mode"}"#,
        )
        .unwrap();

        let validation = validate_phase_marker(&path).await;
        assert_eq!(validation, MarkerValidation::Valid);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_phase_marker_empty_file_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("empty.json");

        std::fs::write(&path, "").unwrap();

        // Per Decision 2: empty file should return None (treat as missing)
        let result = read_phase_marker(&path).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_phase_marker_missing_fields_returns_none() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("incomplete.json");

        // Missing marker_path field
        std::fs::write(&path, r#"{"phase": "onCreate", "status": "executed"}"#).unwrap();

        // Per Decision 2: incomplete marker should return None
        let result = read_phase_marker(&path).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_read_all_markers_skips_corrupted() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Write a valid marker for onCreate
        let on_create_path =
            marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace)).unwrap();
        let on_create_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, on_create_path.clone());
        write_phase_marker(&on_create_path, &on_create_state)
            .await
            .unwrap();

        // Write a corrupted marker for updateContent
        let update_content_path =
            marker_path_for_phase(workspace, LifecyclePhase::UpdateContent, Some(workspace))
                .unwrap();
        std::fs::create_dir_all(update_content_path.parent().unwrap()).unwrap();
        std::fs::write(&update_content_path, "corrupted json {{").unwrap();

        // Write a valid marker for postCreate
        let post_create_path =
            marker_path_for_phase(workspace, LifecyclePhase::PostCreate, Some(workspace)).unwrap();
        let post_create_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::PostCreate, post_create_path.clone());
        write_phase_marker(&post_create_path, &post_create_state)
            .await
            .unwrap();

        // Read all markers - should skip the corrupted one
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();

        // Only 2 valid markers should be returned (onCreate and postCreate)
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].phase, LifecyclePhase::OnCreate);
        assert_eq!(markers[1].phase, LifecyclePhase::PostCreate);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_find_earliest_incomplete_with_corrupted_marker() {
        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path();

        // Write valid markers for onCreate
        let on_create_path =
            marker_path_for_phase(workspace, LifecyclePhase::OnCreate, Some(workspace)).unwrap();
        let on_create_state =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, on_create_path.clone());
        write_phase_marker(&on_create_path, &on_create_state)
            .await
            .unwrap();

        // Write corrupted marker for updateContent (simulating corruption)
        let update_content_path =
            marker_path_for_phase(workspace, LifecyclePhase::UpdateContent, Some(workspace))
                .unwrap();
        std::fs::create_dir_all(update_content_path.parent().unwrap()).unwrap();
        std::fs::write(&update_content_path, "").unwrap(); // Empty file = corrupted

        // Read markers - corrupted one will be skipped
        let markers = read_all_markers(workspace, false, Some(workspace))
            .await
            .unwrap();

        // Find earliest incomplete - should be updateContent (the corrupted one)
        let earliest = find_earliest_incomplete_phase(&markers);
        assert_eq!(earliest, Some(LifecyclePhase::UpdateContent));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_marker_validation_all_valid_phases() {
        let temp_dir = TempDir::new().unwrap();

        // Test all valid phase names
        let valid_phases = [
            "onCreate",
            "updateContent",
            "postCreate",
            "dotfiles",
            "postStart",
            "postAttach",
            "initialize",
        ];

        for phase_name in valid_phases {
            let path = temp_dir.path().join(format!("{}.json", phase_name));
            std::fs::write(
                &path,
                format!(
                    r#"{{"phase": "{}", "status": "executed", "marker_path": "/path"}}"#,
                    phase_name
                ),
            )
            .unwrap();

            let validation = validate_phase_marker(&path).await;
            assert_eq!(
                validation,
                MarkerValidation::Valid,
                "Phase '{}' should be valid",
                phase_name
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_marker_validation_all_valid_statuses() {
        let temp_dir = TempDir::new().unwrap();

        // Test all valid status values
        let valid_statuses = ["pending", "executed", "skipped", "failed"];

        for status_name in valid_statuses {
            let path = temp_dir.path().join(format!("{}.json", status_name));
            std::fs::write(
                &path,
                format!(
                    r#"{{"phase": "onCreate", "status": "{}", "marker_path": "/path"}}"#,
                    status_name
                ),
            )
            .unwrap();

            let validation = validate_phase_marker(&path).await;
            assert_eq!(
                validation,
                MarkerValidation::Valid,
                "Status '{}' should be valid",
                status_name
            );
        }
    }
}
