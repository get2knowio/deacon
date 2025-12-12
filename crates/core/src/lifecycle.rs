//! Lifecycle command execution harness
//!
//! This module provides execution harness for lifecycle commands (initialize, onCreate,
//! postCreate, postStart, postAttach) with host-only simulation for phases before
//! container support.
//!
//! References: subcommand-specs/*/SPEC.md "Container Lifecycle Management"

use crate::errors::{DeaconError, Result};
use crate::redaction::{redact_if_enabled, RedactionConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;
use tracing::{debug, error, info, instrument};

/// Lifecycle phases representing different stages of container setup
///
/// The spec-defined execution order is:
/// onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach
///
/// References: subcommand-specs/*/SPEC.md "Lifecycle Commands"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LifecyclePhase {
    /// Host-side initialization (internal, not part of spec phases)
    Initialize,
    /// Container creation setup
    OnCreate,
    /// Content synchronization
    UpdateContent,
    /// Post-creation configuration
    PostCreate,
    /// Dotfiles application (between postCreate and postStart)
    Dotfiles,
    /// Container startup tasks
    PostStart,
    /// Attachment preparation
    PostAttach,
}

impl LifecyclePhase {
    /// Get the phase name as string
    pub fn as_str(&self) -> &'static str {
        match self {
            LifecyclePhase::Initialize => "initialize",
            LifecyclePhase::OnCreate => "onCreate",
            LifecyclePhase::UpdateContent => "updateContent",
            LifecyclePhase::PostCreate => "postCreate",
            LifecyclePhase::Dotfiles => "dotfiles",
            LifecyclePhase::PostStart => "postStart",
            LifecyclePhase::PostAttach => "postAttach",
        }
    }

    /// Returns the spec-defined lifecycle phases in execution order.
    ///
    /// The order is: onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach
    ///
    /// Note: Initialize is not included as it's an internal phase, not part of the spec.
    pub fn spec_order() -> &'static [LifecyclePhase] {
        &[
            LifecyclePhase::OnCreate,
            LifecyclePhase::UpdateContent,
            LifecyclePhase::PostCreate,
            LifecyclePhase::Dotfiles,
            LifecyclePhase::PostStart,
            LifecyclePhase::PostAttach,
        ]
    }

    /// Returns whether this phase is a runtime hook (postStart or postAttach).
    ///
    /// Runtime hooks are rerun on resume, while other phases are skipped if their
    /// markers are present.
    pub fn is_runtime_hook(&self) -> bool {
        matches!(self, LifecyclePhase::PostStart | LifecyclePhase::PostAttach)
    }

    /// Returns whether this phase is skipped in prebuild mode.
    ///
    /// In prebuild mode, only onCreate and updateContent run; dotfiles and all
    /// post* hooks are skipped.
    pub fn is_skipped_in_prebuild(&self) -> bool {
        matches!(
            self,
            LifecyclePhase::PostCreate
                | LifecyclePhase::Dotfiles
                | LifecyclePhase::PostStart
                | LifecyclePhase::PostAttach
        )
    }

    /// Returns whether this phase is skipped when --skip-post-create is set.
    ///
    /// With --skip-post-create, postCreate, dotfiles, postStart, and postAttach are all skipped.
    pub fn is_skipped_with_skip_post_create(&self) -> bool {
        matches!(
            self,
            LifecyclePhase::PostCreate
                | LifecyclePhase::Dotfiles
                | LifecyclePhase::PostStart
                | LifecyclePhase::PostAttach
        )
    }
}

/// Status of a lifecycle phase execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhaseStatus {
    /// Phase has not yet been executed
    Pending,
    /// Phase was successfully executed
    Executed,
    /// Phase was skipped (due to flag, mode, or prior completion)
    Skipped,
    /// Phase execution failed
    Failed,
}

impl PhaseStatus {
    /// Get the status as string
    pub fn as_str(&self) -> &'static str {
        match self {
            PhaseStatus::Pending => "pending",
            PhaseStatus::Executed => "executed",
            PhaseStatus::Skipped => "skipped",
            PhaseStatus::Failed => "failed",
        }
    }
}

/// State of a lifecycle phase including execution status and metadata
///
/// This structure tracks the status of each lifecycle phase, including
/// why it was skipped or failed, and where its completion marker is stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LifecyclePhaseState {
    /// The lifecycle phase this state represents
    pub phase: LifecyclePhase,
    /// Current status of the phase
    pub status: PhaseStatus,
    /// Optional reason for skip or failure (e.g., "flag", "prebuild mode", error message)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Filesystem path to the completion marker for this phase
    pub marker_path: PathBuf,
    /// Timestamp when the marker was written (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

impl LifecyclePhaseState {
    /// Create a new pending phase state
    pub fn new_pending(phase: LifecyclePhase, marker_path: PathBuf) -> Self {
        Self {
            phase,
            status: PhaseStatus::Pending,
            reason: None,
            marker_path,
            timestamp: None,
        }
    }

    /// Create a phase state marked as executed
    pub fn new_executed(phase: LifecyclePhase, marker_path: PathBuf) -> Self {
        Self {
            phase,
            status: PhaseStatus::Executed,
            reason: None,
            marker_path,
            timestamp: Some(chrono::Utc::now()),
        }
    }

    /// Create a phase state marked as skipped with a reason
    pub fn new_skipped(
        phase: LifecyclePhase,
        marker_path: PathBuf,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            phase,
            status: PhaseStatus::Skipped,
            reason: Some(reason.into()),
            marker_path,
            timestamp: None,
        }
    }

    /// Create a phase state marked as failed with an error message
    pub fn new_failed(
        phase: LifecyclePhase,
        marker_path: PathBuf,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            phase,
            status: PhaseStatus::Failed,
            reason: Some(reason.into()),
            marker_path,
            timestamp: None,
        }
    }

    /// Mark this phase as executed (updates status and sets timestamp)
    pub fn mark_executed(&mut self) {
        self.status = PhaseStatus::Executed;
        self.reason = None;
        self.timestamp = Some(chrono::Utc::now());
    }

    /// Mark this phase as skipped with a reason
    pub fn mark_skipped(&mut self, reason: impl Into<String>) {
        self.status = PhaseStatus::Skipped;
        self.reason = Some(reason.into());
        self.timestamp = None;
    }

    /// Mark this phase as failed with an error message
    pub fn mark_failed(&mut self, reason: impl Into<String>) {
        self.status = PhaseStatus::Failed;
        self.reason = Some(reason.into());
        self.timestamp = None;
    }
}

/// Mode of invocation that determines lifecycle behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationMode {
    /// Fresh run: execute all phases in order
    Fresh,
    /// Resume: only rerun runtime hooks (postStart, postAttach)
    Resume,
    /// Prebuild: stop after updateContent, skip dotfiles and post* hooks
    Prebuild,
    /// Skip post-create: run base setup but skip postCreate, dotfiles, and all runtime hooks
    SkipPostCreate,
}

impl InvocationMode {
    /// Get the mode as string
    pub fn as_str(&self) -> &'static str {
        match self {
            InvocationMode::Fresh => "fresh",
            InvocationMode::Resume => "resume",
            InvocationMode::Prebuild => "prebuild",
            InvocationMode::SkipPostCreate => "skip_post_create",
        }
    }
}

/// Flags that affect lifecycle execution
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvocationFlags {
    /// Whether --skip-post-create was specified
    #[serde(default)]
    pub skip_post_create: bool,
    /// Whether prebuild mode is active
    #[serde(default)]
    pub prebuild: bool,
}

/// Context for a lifecycle invocation
///
/// Captures the mode, flags, and prior state needed to determine which
/// phases should run during this invocation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvocationContext {
    /// The invocation mode (fresh, resume, prebuild, skip_post_create)
    pub mode: InvocationMode,
    /// Flags affecting lifecycle execution
    pub flags: InvocationFlags,
    /// Path to the devcontainer workspace root
    pub workspace_root: PathBuf,
    /// Prior phase states loaded from disk (markers present before this run)
    pub prior_markers: Vec<LifecyclePhaseState>,
}

impl InvocationContext {
    /// Create a new invocation context for a fresh run
    pub fn new_fresh(workspace_root: PathBuf) -> Self {
        Self {
            mode: InvocationMode::Fresh,
            flags: InvocationFlags::default(),
            workspace_root,
            prior_markers: Vec::new(),
        }
    }

    /// Determine the appropriate invocation mode based on prior markers.
    ///
    /// This function analyzes the prior markers to determine whether to:
    /// - Run in Fresh mode (no markers exist)
    /// - Run in Resume mode (all non-runtime phases complete)
    /// - Run in partial-resume mode (some non-runtime phases incomplete - treat as Fresh with markers)
    ///
    /// Per FR-003: On resume after successful initial run, skip onCreate, updateContent,
    /// postCreate, and dotfiles; only rerun postStart and postAttach.
    ///
    /// Per FR-004: If prior run ended before postStart completed, rerun any incomplete
    /// earlier phases in order before executing postStart and postAttach.
    ///
    /// # Arguments
    ///
    /// * `markers` - Prior phase states loaded from disk
    ///
    /// # Returns
    ///
    /// `InvocationMode::Fresh` if no markers or if any non-runtime phase is incomplete.
    /// `InvocationMode::Resume` if all non-runtime phases (onCreate, updateContent, postCreate, dotfiles) are complete.
    pub fn determine_mode_from_markers(markers: &[LifecyclePhaseState]) -> InvocationMode {
        if markers.is_empty() {
            return InvocationMode::Fresh;
        }

        // Check if all non-runtime phases are complete
        // Non-runtime phases: onCreate, updateContent, postCreate, dotfiles
        let non_runtime_phases = [
            LifecyclePhase::OnCreate,
            LifecyclePhase::UpdateContent,
            LifecyclePhase::PostCreate,
            LifecyclePhase::Dotfiles,
        ];

        let all_non_runtime_complete = non_runtime_phases.iter().all(|phase| {
            markers
                .iter()
                .any(|m| m.phase == *phase && m.status == PhaseStatus::Executed)
        });

        if all_non_runtime_complete {
            // All non-runtime phases complete -> Resume mode (only run postStart/postAttach)
            InvocationMode::Resume
        } else {
            // Some non-runtime phases incomplete -> Fresh mode, but markers will inform
            // which phases to skip (through the prior_markers field)
            // This is effectively "partial resume" per FR-004
            InvocationMode::Fresh
        }
    }

    /// Create an invocation context by analyzing existing markers.
    ///
    /// This is a convenience method that:
    /// 1. Determines the appropriate mode based on marker completeness
    /// 2. Preserves the prior markers for per-phase skip decisions
    ///
    /// Use this when you want automatic mode detection for resume scenarios.
    ///
    /// # Arguments
    ///
    /// * `workspace_root` - Path to the workspace
    /// * `prior_markers` - Markers loaded from disk
    /// * `flags` - CLI flags that may override the mode
    ///
    /// # Returns
    ///
    /// An InvocationContext with the appropriate mode set based on markers and flags.
    pub fn from_markers_with_flags(
        workspace_root: PathBuf,
        prior_markers: Vec<LifecyclePhaseState>,
        flags: InvocationFlags,
    ) -> Self {
        // Flags take precedence over marker-based mode detection
        let mode = if flags.prebuild {
            InvocationMode::Prebuild
        } else if flags.skip_post_create {
            InvocationMode::SkipPostCreate
        } else {
            Self::determine_mode_from_markers(&prior_markers)
        };

        Self {
            mode,
            flags,
            workspace_root,
            prior_markers,
        }
    }

    /// Create a new invocation context for resume
    pub fn new_resume(workspace_root: PathBuf, prior_markers: Vec<LifecyclePhaseState>) -> Self {
        Self {
            mode: InvocationMode::Resume,
            flags: InvocationFlags::default(),
            workspace_root,
            prior_markers,
        }
    }

    /// Create a new invocation context for prebuild mode
    pub fn new_prebuild(workspace_root: PathBuf) -> Self {
        Self {
            mode: InvocationMode::Prebuild,
            flags: InvocationFlags {
                prebuild: true,
                ..Default::default()
            },
            workspace_root,
            prior_markers: Vec::new(),
        }
    }

    /// Create a new invocation context with skip-post-create flag
    pub fn new_skip_post_create(workspace_root: PathBuf) -> Self {
        Self {
            mode: InvocationMode::SkipPostCreate,
            flags: InvocationFlags {
                skip_post_create: true,
                ..Default::default()
            },
            workspace_root,
            prior_markers: Vec::new(),
        }
    }

    /// Set prior markers (for resume scenarios)
    pub fn with_prior_markers(mut self, prior_markers: Vec<LifecyclePhaseState>) -> Self {
        self.prior_markers = prior_markers;
        self
    }

    /// Check if a phase should be skipped based on mode and flags.
    ///
    /// Per SC-002 and FR-004:
    /// - In Resume mode: Skip non-runtime phases that have markers, always run postStart/postAttach
    /// - In Fresh mode with markers (partial resume): Skip phases that already completed
    /// - In Prebuild/SkipPostCreate modes: Skip based on mode rules
    pub fn should_skip_phase(&self, phase: LifecyclePhase) -> Option<&'static str> {
        match self.mode {
            InvocationMode::Prebuild => {
                if phase.is_skipped_in_prebuild() {
                    return Some("prebuild mode");
                }
            }
            InvocationMode::SkipPostCreate => {
                if phase.is_skipped_with_skip_post_create() {
                    return Some("--skip-post-create flag");
                }
            }
            InvocationMode::Resume => {
                // In resume mode, skip non-runtime phases that have markers
                // Per SC-002: Skip onCreate, updateContent, postCreate, dotfiles; run postStart, postAttach
                if !phase.is_runtime_hook() {
                    // Check if marker exists in prior_markers
                    if self
                        .prior_markers
                        .iter()
                        .any(|m| m.phase == phase && m.status == PhaseStatus::Executed)
                    {
                        return Some("prior completion marker");
                    }
                }
            }
            InvocationMode::Fresh => {
                // Per FR-004: In Fresh mode with prior markers (partial resume),
                // skip non-runtime phases that already completed.
                // This enables recovery from partial runs by only running incomplete phases.
                if !self.prior_markers.is_empty()
                    && !phase.is_runtime_hook()
                    && self
                        .prior_markers
                        .iter()
                        .any(|m| m.phase == phase && m.status == PhaseStatus::Executed)
                {
                    return Some("prior completion marker");
                }
            }
        }
        None
    }
}

/// Output mode for lifecycle summaries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Human-readable text output
    #[default]
    Text,
    /// JSON output
    Json,
}

impl OutputMode {
    /// Get the mode as string
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputMode::Text => "text",
            OutputMode::Json => "json",
        }
    }
}

/// Summary of a lifecycle run
///
/// Contains the final state of all phases after execution, including
/// which phases ran, which were skipped, and whether further phases
/// remain due to failure or interruption.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSummary {
    /// Ordered list of phase states after execution
    pub phases: Vec<LifecyclePhaseState>,
    /// Whether further phases remain due to failure/interrupt
    pub resume_required: bool,
    /// Output mode for reporting
    pub output_mode: OutputMode,
}

/// Determines whether a phase should execute based on context and prior state.
///
/// This enum captures the decision made for each phase during orchestration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseDecision {
    /// Phase should execute (no prior marker or is a runtime hook in resume mode)
    Execute,
    /// Phase should be skipped with a reason
    Skip(&'static str),
}

/// A lifecycle orchestrator that enforces strict phase ordering and single-run guards.
///
/// The orchestrator ensures:
/// 1. Phases execute in spec-defined order: onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach
/// 2. Each phase runs at most once per fresh invocation (checked via markers)
/// 3. Runtime hooks (postStart, postAttach) rerun on resume
/// 4. Phases skipped by mode/flags are recorded with reasons
///
/// # Example
///
/// ```
/// use std::path::PathBuf;
/// use deacon_core::lifecycle::{LifecycleOrchestrator, InvocationContext, LifecyclePhase};
///
/// // Create orchestrator for a fresh run
/// let workspace = PathBuf::from("/workspace");
/// let context = InvocationContext::new_fresh(workspace);
/// let orchestrator = LifecycleOrchestrator::new(context);
///
/// // Get phases to execute in order
/// for (phase, decision) in orchestrator.phases_with_decisions() {
///     println!("{}: {:?}", phase.as_str(), decision);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LifecycleOrchestrator {
    /// The invocation context determining mode and prior state
    context: InvocationContext,
    /// Set of phases that have been executed in this orchestration run
    executed_phases: std::collections::HashSet<LifecyclePhase>,
}

impl LifecycleOrchestrator {
    /// Create a new lifecycle orchestrator with the given invocation context.
    pub fn new(context: InvocationContext) -> Self {
        Self {
            context,
            executed_phases: std::collections::HashSet::new(),
        }
    }

    /// Get the invocation context.
    pub fn context(&self) -> &InvocationContext {
        &self.context
    }

    /// Returns phases in spec-defined order with their execution decisions.
    ///
    /// The order is always: onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach
    ///
    /// Each phase is paired with a decision indicating whether it should execute or be skipped.
    pub fn phases_with_decisions(&self) -> Vec<(LifecyclePhase, PhaseDecision)> {
        LifecyclePhase::spec_order()
            .iter()
            .map(|&phase| (phase, self.decide_phase(phase)))
            .collect()
    }

    /// Decide whether a specific phase should execute.
    ///
    /// The decision is based on:
    /// 1. Mode-based skipping (prebuild, skip_post_create)
    /// 2. Prior marker state (for resume mode)
    /// 3. Single-run guard (phase already executed this run)
    pub fn decide_phase(&self, phase: LifecyclePhase) -> PhaseDecision {
        // Check if already executed this run (single-run guard)
        if self.executed_phases.contains(&phase) {
            return PhaseDecision::Skip("already executed this run");
        }

        // Check mode/flag-based skipping
        if let Some(reason) = self.context.should_skip_phase(phase) {
            return PhaseDecision::Skip(reason);
        }

        PhaseDecision::Execute
    }

    /// Mark a phase as executed in this orchestration run.
    ///
    /// This enforces the single-run guard: once marked, subsequent calls to
    /// `decide_phase` for this phase will return `Skip("already executed this run")`.
    pub fn mark_phase_executed(&mut self, phase: LifecyclePhase) {
        self.executed_phases.insert(phase);
    }

    /// Check if a phase has been executed in this orchestration run.
    pub fn is_phase_executed(&self, phase: LifecyclePhase) -> bool {
        self.executed_phases.contains(&phase)
    }

    /// Returns phases that should execute (not skipped) in order.
    ///
    /// This is a convenience method that filters `phases_with_decisions` to only
    /// include phases with `PhaseDecision::Execute`.
    pub fn phases_to_execute(&self) -> Vec<LifecyclePhase> {
        self.phases_with_decisions()
            .into_iter()
            .filter_map(|(phase, decision)| {
                if decision == PhaseDecision::Execute {
                    Some(phase)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Execute phases in order, calling the provided executor for each.
    ///
    /// The executor receives each phase that should execute and returns a Result.
    /// Execution stops on the first error.
    ///
    /// For each phase:
    /// 1. Checks if phase should execute (not skipped)
    /// 2. Calls the executor
    /// 3. Marks the phase as executed on success
    ///
    /// Returns a summary of all phases with their final states.
    pub fn execute_in_order<F, E>(&mut self, mut executor: F) -> std::result::Result<RunSummary, E>
    where
        F: FnMut(LifecyclePhase) -> std::result::Result<(), E>,
    {
        let mut summary = RunSummary::new(OutputMode::Text);

        for &phase in LifecyclePhase::spec_order() {
            let marker_path = std::path::PathBuf::from(format!(
                "{}/.devcontainer-state/{}.json",
                self.context.workspace_root.display(),
                phase.as_str()
            ));

            match self.decide_phase(phase) {
                PhaseDecision::Execute => match executor(phase) {
                    Ok(()) => {
                        self.mark_phase_executed(phase);
                        summary.add_phase(LifecyclePhaseState::new_executed(phase, marker_path));
                    }
                    Err(e) => {
                        summary.add_phase(LifecyclePhaseState::new_failed(
                            phase,
                            marker_path,
                            "execution failed",
                        ));
                        return Err(e);
                    }
                },
                PhaseDecision::Skip(reason) => {
                    summary.add_phase(LifecyclePhaseState::new_skipped(phase, marker_path, reason));
                }
            }
        }

        Ok(summary)
    }

    /// Execute phases in order asynchronously, calling the provided async executor for each.
    ///
    /// Similar to `execute_in_order` but supports async executors.
    pub async fn execute_in_order_async<F, Fut, E>(
        &mut self,
        mut executor: F,
    ) -> std::result::Result<RunSummary, E>
    where
        F: FnMut(LifecyclePhase) -> Fut,
        Fut: std::future::Future<Output = std::result::Result<(), E>>,
    {
        let mut summary = RunSummary::new(OutputMode::Text);

        for &phase in LifecyclePhase::spec_order() {
            let marker_path = std::path::PathBuf::from(format!(
                "{}/.devcontainer-state/{}.json",
                self.context.workspace_root.display(),
                phase.as_str()
            ));

            match self.decide_phase(phase) {
                PhaseDecision::Execute => match executor(phase).await {
                    Ok(()) => {
                        self.mark_phase_executed(phase);
                        summary.add_phase(LifecyclePhaseState::new_executed(phase, marker_path));
                    }
                    Err(e) => {
                        summary.add_phase(LifecyclePhaseState::new_failed(
                            phase,
                            marker_path,
                            "execution failed",
                        ));
                        return Err(e);
                    }
                },
                PhaseDecision::Skip(reason) => {
                    summary.add_phase(LifecyclePhaseState::new_skipped(phase, marker_path, reason));
                }
            }
        }

        Ok(summary)
    }

    /// Execute phases in order with marker persistence, calling the provided executor for each.
    ///
    /// Similar to `execute_in_order` but writes completion markers to disk after each
    /// successful phase execution. This is the preferred method for fresh runs where
    /// marker recording is required per FR-002.
    ///
    /// For each phase:
    /// 1. Checks if phase should execute (not skipped)
    /// 2. Calls the executor
    /// 3. Writes the phase marker to disk (executed or skipped)
    /// 4. Marks the phase as executed in the orchestrator
    ///
    /// Returns a summary of all phases with their final states.
    ///
    /// # Arguments
    ///
    /// * `executor` - Closure that executes the phase commands
    /// * `prebuild` - If true, writes markers to the isolated prebuild directory
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::path::PathBuf;
    /// use deacon_core::lifecycle::{LifecycleOrchestrator, InvocationContext, LifecyclePhase};
    ///
    /// let workspace = PathBuf::from("/workspace");
    /// let context = InvocationContext::new_fresh(workspace.clone());
    /// let mut orchestrator = LifecycleOrchestrator::new(context);
    ///
    /// let summary = orchestrator.execute_with_markers(|phase| {
    ///     println!("Executing phase: {}", phase.as_str());
    ///     Ok::<(), String>(())
    /// }, false).unwrap();
    /// ```
    pub fn execute_with_markers<F, E>(
        &mut self,
        mut executor: F,
        prebuild: bool,
    ) -> std::result::Result<RunSummary, E>
    where
        F: FnMut(LifecyclePhase) -> std::result::Result<(), E>,
        E: std::fmt::Display,
    {
        use crate::state::{
            marker_path_for_phase, prebuild_marker_path_for_phase, write_phase_marker,
        };

        let mut summary = RunSummary::new(OutputMode::Text);

        for &phase in LifecyclePhase::spec_order() {
            let marker_path = if prebuild {
                prebuild_marker_path_for_phase(&self.context.workspace_root, phase)
            } else {
                marker_path_for_phase(&self.context.workspace_root, phase)
            };

            match self.decide_phase(phase) {
                PhaseDecision::Execute => match executor(phase) {
                    Ok(()) => {
                        self.mark_phase_executed(phase);
                        let state = LifecyclePhaseState::new_executed(phase, marker_path.clone());

                        // Write marker to disk - log errors but don't fail the execution
                        if let Err(e) = write_phase_marker(&marker_path, &state) {
                            tracing::warn!(
                                "Failed to write marker for phase {}: {}",
                                phase.as_str(),
                                e
                            );
                        }

                        summary.add_phase(state);
                    }
                    Err(e) => {
                        let state = LifecyclePhaseState::new_failed(
                            phase,
                            marker_path,
                            format!("execution failed: {}", e),
                        );
                        summary.add_phase(state);
                        return Err(e);
                    }
                },
                PhaseDecision::Skip(reason) => {
                    let state =
                        LifecyclePhaseState::new_skipped(phase, marker_path.clone(), reason);

                    // Write skipped marker to disk - log errors but don't fail
                    if let Err(e) = write_phase_marker(&marker_path, &state) {
                        tracing::warn!(
                            "Failed to write skip marker for phase {}: {}",
                            phase.as_str(),
                            e
                        );
                    }

                    summary.add_phase(state);
                }
            }
        }

        Ok(summary)
    }

    /// Execute phases in order asynchronously with marker persistence.
    ///
    /// Similar to `execute_in_order_async` but writes completion markers to disk after
    /// each successful phase execution. This is the preferred method for fresh runs
    /// where marker recording is required per FR-002.
    ///
    /// # Arguments
    ///
    /// * `executor` - Async closure that executes the phase commands
    /// * `prebuild` - If true, writes markers to the isolated prebuild directory
    pub async fn execute_with_markers_async<F, Fut, E>(
        &mut self,
        mut executor: F,
        prebuild: bool,
    ) -> std::result::Result<RunSummary, E>
    where
        F: FnMut(LifecyclePhase) -> Fut,
        Fut: std::future::Future<Output = std::result::Result<(), E>>,
        E: std::fmt::Display,
    {
        use crate::state::{
            marker_path_for_phase, prebuild_marker_path_for_phase, write_phase_marker,
        };

        let mut summary = RunSummary::new(OutputMode::Text);

        for &phase in LifecyclePhase::spec_order() {
            let marker_path = if prebuild {
                prebuild_marker_path_for_phase(&self.context.workspace_root, phase)
            } else {
                marker_path_for_phase(&self.context.workspace_root, phase)
            };

            match self.decide_phase(phase) {
                PhaseDecision::Execute => match executor(phase).await {
                    Ok(()) => {
                        self.mark_phase_executed(phase);
                        let state = LifecyclePhaseState::new_executed(phase, marker_path.clone());

                        // Write marker to disk - log errors but don't fail the execution
                        if let Err(e) = write_phase_marker(&marker_path, &state) {
                            tracing::warn!(
                                "Failed to write marker for phase {}: {}",
                                phase.as_str(),
                                e
                            );
                        }

                        summary.add_phase(state);
                    }
                    Err(e) => {
                        let state = LifecyclePhaseState::new_failed(
                            phase,
                            marker_path,
                            format!("execution failed: {}", e),
                        );
                        summary.add_phase(state);
                        return Err(e);
                    }
                },
                PhaseDecision::Skip(reason) => {
                    let state =
                        LifecyclePhaseState::new_skipped(phase, marker_path.clone(), reason);

                    // Write skipped marker to disk - log errors but don't fail
                    if let Err(e) = write_phase_marker(&marker_path, &state) {
                        tracing::warn!(
                            "Failed to write skip marker for phase {}: {}",
                            phase.as_str(),
                            e
                        );
                    }

                    summary.add_phase(state);
                }
            }
        }

        Ok(summary)
    }
}

impl RunSummary {
    /// Create a new empty run summary
    pub fn new(output_mode: OutputMode) -> Self {
        Self {
            phases: Vec::new(),
            resume_required: false,
            output_mode,
        }
    }

    /// Add a phase state to the summary
    pub fn add_phase(&mut self, phase_state: LifecyclePhaseState) {
        // If this phase failed, mark resume as required
        if phase_state.status == PhaseStatus::Failed {
            self.resume_required = true;
        }
        self.phases.push(phase_state);
    }

    /// Get all executed phases
    pub fn executed_phases(&self) -> Vec<&LifecyclePhaseState> {
        self.phases
            .iter()
            .filter(|p| p.status == PhaseStatus::Executed)
            .collect()
    }

    /// Get all skipped phases
    pub fn skipped_phases(&self) -> Vec<&LifecyclePhaseState> {
        self.phases
            .iter()
            .filter(|p| p.status == PhaseStatus::Skipped)
            .collect()
    }

    /// Get all failed phases
    pub fn failed_phases(&self) -> Vec<&LifecyclePhaseState> {
        self.phases
            .iter()
            .filter(|p| p.status == PhaseStatus::Failed)
            .collect()
    }

    /// Check if all phases completed successfully (executed or skipped, none failed)
    pub fn all_complete(&self) -> bool {
        !self.resume_required
            && self
                .phases
                .iter()
                .all(|p| p.status == PhaseStatus::Executed || p.status == PhaseStatus::Skipped)
    }
}

/// Commands to execute for lifecycle phases
#[derive(Debug, Clone)]
pub struct LifecycleCommands {
    /// Command strings and environment variables
    pub commands: Vec<CommandTemplate>,
}

/// Template for creating commands with environment
#[derive(Debug, Clone)]
pub struct CommandTemplate {
    /// The command string to execute
    pub command: String,
    /// Environment variables for this command
    pub env_vars: HashMap<String, String>,
}

impl LifecycleCommands {
    /// Create new lifecycle commands from JSON value (string or array of strings)
    pub fn from_json_value(value: &Value, env_vars: &HashMap<String, String>) -> Result<Self> {
        let commands = match value {
            Value::String(cmd) => {
                vec![CommandTemplate {
                    command: crate::platform::normalize_line_endings(cmd),
                    env_vars: env_vars.clone(),
                }]
            }
            Value::Array(cmds) => {
                let mut commands = Vec::new();
                for cmd_value in cmds {
                    if let Value::String(cmd) = cmd_value {
                        commands.push(CommandTemplate {
                            command: crate::platform::normalize_line_endings(cmd),
                            env_vars: env_vars.clone(),
                        });
                    } else {
                        return Err(DeaconError::Lifecycle(format!(
                            "Invalid command in array: expected string, got {:?}",
                            cmd_value
                        )));
                    }
                }
                commands
            }
            _ => {
                return Err(DeaconError::Lifecycle(format!(
                    "Invalid command format: expected string or array of strings, got {:?}",
                    value
                )));
            }
        };

        Ok(Self { commands })
    }
}

/// Execution mode for lifecycle commands
#[derive(Debug, Clone)]
pub enum ExecutionMode {
    /// Execute commands on the host system
    Host,
    /// Execute commands in a container
    Container {
        /// Container ID to execute commands in
        container_id: String,
        /// User to run commands as (optional, defaults to root)
        user: Option<String>,
        /// Working directory in the container
        working_dir: Option<String>,
    },
}

/// Execution context for lifecycle commands
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Environment variables to pass to commands
    pub environment: HashMap<String, String>,
    /// Working directory for command execution (host mode only)
    pub working_directory: Option<std::path::PathBuf>,
    /// Timeout for command execution (placeholder, not enforced yet)
    pub timeout: Option<std::time::Duration>,
    /// Redaction configuration for sensitive output filtering
    pub redaction_config: RedactionConfig,
    /// Execution mode (host or container)
    pub execution_mode: ExecutionMode,
}

impl ExecutionContext {
    /// Create new execution context for host execution
    pub fn new() -> Self {
        Self {
            environment: HashMap::new(),
            working_directory: None,
            timeout: None, // TODO: Implement timeout enforcement
            redaction_config: RedactionConfig::default(),
            execution_mode: ExecutionMode::Host,
        }
    }

    /// Create new execution context for container execution
    pub fn new_container(container_id: String) -> Self {
        Self {
            environment: HashMap::new(),
            working_directory: None,
            timeout: None,
            redaction_config: RedactionConfig::default(),
            execution_mode: ExecutionMode::Container {
                container_id,
                user: None,
                working_dir: None,
            },
        }
    }

    /// Add environment variable
    pub fn with_env(mut self, key: String, value: String) -> Self {
        self.environment.insert(key, value);
        self
    }

    /// Set working directory
    pub fn with_working_directory(mut self, dir: std::path::PathBuf) -> Self {
        self.working_directory = Some(dir);
        self
    }

    /// Set timeout (placeholder for future implementation)
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set redaction configuration
    pub fn with_redaction_config(mut self, config: RedactionConfig) -> Self {
        self.redaction_config = config;
        self
    }

    /// Set container user for container execution
    pub fn with_container_user(mut self, user: String) -> Self {
        if let ExecutionMode::Container {
            container_id,
            working_dir,
            ..
        } = self.execution_mode
        {
            self.execution_mode = ExecutionMode::Container {
                container_id,
                user: Some(user),
                working_dir,
            };
        }
        self
    }

    /// Set container working directory for container execution
    pub fn with_container_working_dir(mut self, working_dir: String) -> Self {
        if let ExecutionMode::Container {
            container_id, user, ..
        } = self.execution_mode
        {
            self.execution_mode = ExecutionMode::Container {
                container_id,
                user,
                working_dir: Some(working_dir),
            };
        }
        self
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of lifecycle command execution
#[derive(Debug, Clone)]
pub struct LifecycleResult {
    /// Exit codes from executed commands
    pub exit_codes: Vec<i32>,
    /// Combined stdout from all commands
    pub stdout: String,
    /// Combined stderr from all commands  
    pub stderr: String,
    /// Whether all commands succeeded
    pub success: bool,
    /// Duration of each command execution
    pub durations: Vec<std::time::Duration>,
}

impl LifecycleResult {
    /// Create new result
    pub fn new() -> Self {
        Self {
            exit_codes: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            durations: Vec::new(),
        }
    }

    /// Add command result with duration
    pub fn add_command_result(
        &mut self,
        exit_code: i32,
        stdout: String,
        stderr: String,
        duration: std::time::Duration,
    ) {
        self.exit_codes.push(exit_code);
        self.durations.push(duration);
        if !stdout.is_empty() {
            if !self.stdout.is_empty() {
                self.stdout.push('\n');
            }
            self.stdout.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !self.stderr.is_empty() {
                self.stderr.push('\n');
            }
            self.stderr.push_str(&stderr);
        }
        if exit_code != 0 {
            self.success = false;
        }
    }
}

impl Default for LifecycleResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Execute a lifecycle phase with the given commands and context
///
/// This function runs commands sequentially and captures their output.
/// If any command fails, execution halts and returns an error with phase context.
/// This version only supports host execution.
#[instrument(skip(commands, ctx), fields(phase = %phase.as_str()))]
pub fn run_phase(
    phase: LifecyclePhase,
    commands: &LifecycleCommands,
    ctx: &ExecutionContext,
) -> Result<LifecycleResult> {
    match &ctx.execution_mode {
        ExecutionMode::Host => run_phase_host_sync(phase, commands, ctx),
        ExecutionMode::Container { .. } => Err(DeaconError::Lifecycle(
            "Container execution not supported in sync version - use container_lifecycle module"
                .to_string(),
        )),
    }
}

/// Execute a lifecycle phase on the host system (synchronous)
#[instrument(skip(commands, ctx), fields(phase = %phase.as_str()))]
fn run_phase_host_sync(
    phase: LifecyclePhase,
    commands: &LifecycleCommands,
    ctx: &ExecutionContext,
) -> Result<LifecycleResult> {
    let mut result = LifecycleResult::new();

    for (i, command_template) in commands.commands.iter().enumerate() {
        debug!(
            "Executing command {} of {} for phase {}: {}",
            i + 1,
            commands.commands.len(),
            phase.as_str(),
            command_template.command
        );

        let start_time = Instant::now();

        // Create the actual command from the template
        let mut command = if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", &command_template.command]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", &command_template.command]);
            cmd
        };

        // Set working directory if specified
        if let Some(ref dir) = ctx.working_directory {
            command.current_dir(dir);
        }

        // Execute command in blocking task to use sync stdio handling
        // Add environment variables from template and context
        for (key, value) in &command_template.env_vars {
            command.env(key, value);
        }
        for (key, value) in &ctx.environment {
            command.env(key, value);
        }

        // Configure stdio
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        // Execute command
        let mut child = command.spawn().map_err(|e| {
            DeaconError::Lifecycle(format!(
                "Failed to spawn command for phase {}: {}",
                phase.as_str(),
                e
            ))
        })?;

        // Capture stdout line by line
        let stdout_reader = BufReader::new(child.stdout.take().unwrap());
        let stderr_reader = BufReader::new(child.stderr.take().unwrap());

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        // Read stdout
        for line in stdout_reader.lines() {
            let line =
                line.map_err(|e| DeaconError::Lifecycle(format!("Failed to read stdout: {}", e)))?;

            // Apply redaction to the line before logging
            let redacted_line = redact_if_enabled(&line, &ctx.redaction_config);
            info!("[{}] stdout: {}", phase.as_str(), redacted_line);
            stdout_lines.push(line); // Store original for result, log redacted
        }

        // Read stderr
        for line in stderr_reader.lines() {
            let line =
                line.map_err(|e| DeaconError::Lifecycle(format!("Failed to read stderr: {}", e)))?;

            // Apply redaction to the line before logging
            let redacted_line = redact_if_enabled(&line, &ctx.redaction_config);
            info!("[{}] stderr: {}", phase.as_str(), redacted_line);
            stderr_lines.push(line); // Store original for result, log redacted
        }

        // Wait for command to complete
        let exit_status = child.wait().map_err(|e| {
            DeaconError::Lifecycle(format!(
                "Failed to wait for command in phase {}: {}",
                phase.as_str(),
                e
            ))
        })?;

        let exit_code = exit_status.code().unwrap_or(-1);
        let duration = start_time.elapsed();
        let stdout = stdout_lines.join("\n");
        let stderr = stderr_lines.join("\n");

        // Apply redaction to the combined output for the result
        let redacted_stdout = redact_if_enabled(&stdout, &ctx.redaction_config);
        let redacted_stderr = redact_if_enabled(&stderr, &ctx.redaction_config);

        debug!(
            "Command completed with exit code: {} in {:?}",
            exit_code, duration
        );

        result.add_command_result(exit_code, redacted_stdout, redacted_stderr, duration);

        // If command failed, halt execution and return error with phase context
        if exit_code != 0 {
            error!(
                "Command failed in phase {} with exit code {}",
                phase.as_str(),
                exit_code
            );
            return Err(DeaconError::Lifecycle(format!(
                "Command failed in phase {} with exit code {}: Command: {}",
                phase.as_str(),
                exit_code,
                command_template.command
            )));
        }
    }

    info!("Completed lifecycle phase: {}", phase.as_str());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_lifecycle_phase_as_str() {
        assert_eq!(LifecyclePhase::Initialize.as_str(), "initialize");
        assert_eq!(LifecyclePhase::OnCreate.as_str(), "onCreate");
        assert_eq!(LifecyclePhase::UpdateContent.as_str(), "updateContent");
        assert_eq!(LifecyclePhase::PostCreate.as_str(), "postCreate");
        assert_eq!(LifecyclePhase::Dotfiles.as_str(), "dotfiles");
        assert_eq!(LifecyclePhase::PostStart.as_str(), "postStart");
        assert_eq!(LifecyclePhase::PostAttach.as_str(), "postAttach");
    }

    #[test]
    fn test_lifecycle_phase_spec_order() {
        let order = LifecyclePhase::spec_order();
        assert_eq!(order.len(), 6);
        assert_eq!(order[0], LifecyclePhase::OnCreate);
        assert_eq!(order[1], LifecyclePhase::UpdateContent);
        assert_eq!(order[2], LifecyclePhase::PostCreate);
        assert_eq!(order[3], LifecyclePhase::Dotfiles);
        assert_eq!(order[4], LifecyclePhase::PostStart);
        assert_eq!(order[5], LifecyclePhase::PostAttach);
    }

    #[test]
    fn test_lifecycle_phase_predicates() {
        // Runtime hooks
        assert!(!LifecyclePhase::OnCreate.is_runtime_hook());
        assert!(!LifecyclePhase::UpdateContent.is_runtime_hook());
        assert!(!LifecyclePhase::PostCreate.is_runtime_hook());
        assert!(!LifecyclePhase::Dotfiles.is_runtime_hook());
        assert!(LifecyclePhase::PostStart.is_runtime_hook());
        assert!(LifecyclePhase::PostAttach.is_runtime_hook());

        // Skipped in prebuild
        assert!(!LifecyclePhase::OnCreate.is_skipped_in_prebuild());
        assert!(!LifecyclePhase::UpdateContent.is_skipped_in_prebuild());
        assert!(LifecyclePhase::PostCreate.is_skipped_in_prebuild());
        assert!(LifecyclePhase::Dotfiles.is_skipped_in_prebuild());
        assert!(LifecyclePhase::PostStart.is_skipped_in_prebuild());
        assert!(LifecyclePhase::PostAttach.is_skipped_in_prebuild());

        // Skipped with --skip-post-create
        assert!(!LifecyclePhase::OnCreate.is_skipped_with_skip_post_create());
        assert!(!LifecyclePhase::UpdateContent.is_skipped_with_skip_post_create());
        assert!(LifecyclePhase::PostCreate.is_skipped_with_skip_post_create());
        assert!(LifecyclePhase::Dotfiles.is_skipped_with_skip_post_create());
        assert!(LifecyclePhase::PostStart.is_skipped_with_skip_post_create());
        assert!(LifecyclePhase::PostAttach.is_skipped_with_skip_post_create());
    }

    #[test]
    fn test_phase_status_as_str() {
        assert_eq!(PhaseStatus::Pending.as_str(), "pending");
        assert_eq!(PhaseStatus::Executed.as_str(), "executed");
        assert_eq!(PhaseStatus::Skipped.as_str(), "skipped");
        assert_eq!(PhaseStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn test_lifecycle_phase_state_creation() {
        let marker_path = PathBuf::from("/tmp/markers/onCreate");

        let pending =
            LifecyclePhaseState::new_pending(LifecyclePhase::OnCreate, marker_path.clone());
        assert_eq!(pending.phase, LifecyclePhase::OnCreate);
        assert_eq!(pending.status, PhaseStatus::Pending);
        assert!(pending.reason.is_none());
        assert!(pending.timestamp.is_none());

        let executed =
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path.clone());
        assert_eq!(executed.status, PhaseStatus::Executed);
        assert!(executed.timestamp.is_some());

        let skipped = LifecyclePhaseState::new_skipped(
            LifecyclePhase::PostCreate,
            marker_path.clone(),
            "prebuild mode",
        );
        assert_eq!(skipped.status, PhaseStatus::Skipped);
        assert_eq!(skipped.reason, Some("prebuild mode".to_string()));

        let failed = LifecyclePhaseState::new_failed(
            LifecyclePhase::PostStart,
            marker_path,
            "command exited with code 1",
        );
        assert_eq!(failed.status, PhaseStatus::Failed);
        assert_eq!(
            failed.reason,
            Some("command exited with code 1".to_string())
        );
    }

    #[test]
    fn test_lifecycle_phase_state_mutations() {
        let marker_path = PathBuf::from("/tmp/markers/onCreate");
        let mut state = LifecyclePhaseState::new_pending(LifecyclePhase::OnCreate, marker_path);

        state.mark_executed();
        assert_eq!(state.status, PhaseStatus::Executed);
        assert!(state.timestamp.is_some());
        assert!(state.reason.is_none());

        state.mark_skipped("test skip");
        assert_eq!(state.status, PhaseStatus::Skipped);
        assert_eq!(state.reason, Some("test skip".to_string()));
        assert!(state.timestamp.is_none());

        state.mark_failed("test failure");
        assert_eq!(state.status, PhaseStatus::Failed);
        assert_eq!(state.reason, Some("test failure".to_string()));
    }

    #[test]
    fn test_invocation_mode_as_str() {
        assert_eq!(InvocationMode::Fresh.as_str(), "fresh");
        assert_eq!(InvocationMode::Resume.as_str(), "resume");
        assert_eq!(InvocationMode::Prebuild.as_str(), "prebuild");
        assert_eq!(InvocationMode::SkipPostCreate.as_str(), "skip_post_create");
    }

    #[test]
    fn test_invocation_context_fresh() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        assert_eq!(ctx.mode, InvocationMode::Fresh);
        assert!(!ctx.flags.skip_post_create);
        assert!(!ctx.flags.prebuild);
        assert!(ctx.prior_markers.is_empty());

        // Fresh mode skips nothing
        assert!(ctx.should_skip_phase(LifecyclePhase::OnCreate).is_none());
        assert!(ctx.should_skip_phase(LifecyclePhase::PostCreate).is_none());
        assert!(ctx.should_skip_phase(LifecyclePhase::Dotfiles).is_none());
        assert!(ctx.should_skip_phase(LifecyclePhase::PostStart).is_none());
    }

    #[test]
    fn test_invocation_context_prebuild() {
        let ctx = InvocationContext::new_prebuild(PathBuf::from("/workspace"));
        assert_eq!(ctx.mode, InvocationMode::Prebuild);
        assert!(ctx.flags.prebuild);

        // Prebuild skips postCreate, dotfiles, postStart, postAttach
        assert!(ctx.should_skip_phase(LifecyclePhase::OnCreate).is_none());
        assert!(ctx
            .should_skip_phase(LifecyclePhase::UpdateContent)
            .is_none());
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::PostCreate),
            Some("prebuild mode")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::Dotfiles),
            Some("prebuild mode")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::PostStart),
            Some("prebuild mode")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::PostAttach),
            Some("prebuild mode")
        );
    }

    #[test]
    fn test_invocation_context_skip_post_create() {
        let ctx = InvocationContext::new_skip_post_create(PathBuf::from("/workspace"));
        assert_eq!(ctx.mode, InvocationMode::SkipPostCreate);
        assert!(ctx.flags.skip_post_create);

        // skip-post-create skips postCreate, dotfiles, postStart, postAttach
        assert!(ctx.should_skip_phase(LifecyclePhase::OnCreate).is_none());
        assert!(ctx
            .should_skip_phase(LifecyclePhase::UpdateContent)
            .is_none());
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::PostCreate),
            Some("--skip-post-create flag")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::Dotfiles),
            Some("--skip-post-create flag")
        );
    }

    #[test]
    fn test_invocation_context_resume() {
        let marker_path = PathBuf::from("/tmp/markers");
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                marker_path.join("onCreate"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path.join("updateContent"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path.join("postCreate"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                marker_path.join("dotfiles"),
            ),
        ];

        let ctx = InvocationContext::new_resume(PathBuf::from("/workspace"), prior_markers);
        assert_eq!(ctx.mode, InvocationMode::Resume);

        // Resume skips phases with prior markers (non-runtime)
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::OnCreate),
            Some("prior completion marker")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::UpdateContent),
            Some("prior completion marker")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::PostCreate),
            Some("prior completion marker")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::Dotfiles),
            Some("prior completion marker")
        );

        // Runtime hooks always run in resume mode
        assert!(ctx.should_skip_phase(LifecyclePhase::PostStart).is_none());
        assert!(ctx.should_skip_phase(LifecyclePhase::PostAttach).is_none());
    }

    #[test]
    fn test_output_mode_as_str() {
        assert_eq!(OutputMode::Text.as_str(), "text");
        assert_eq!(OutputMode::Json.as_str(), "json");
    }

    #[test]
    fn test_run_summary() {
        let marker_path = PathBuf::from("/tmp/markers");
        let mut summary = RunSummary::new(OutputMode::Text);

        assert!(summary.phases.is_empty());
        assert!(!summary.resume_required);
        assert!(summary.all_complete());

        // Add executed phase
        summary.add_phase(LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path.join("onCreate"),
        ));
        assert_eq!(summary.executed_phases().len(), 1);
        assert!(summary.all_complete());

        // Add skipped phase
        summary.add_phase(LifecyclePhaseState::new_skipped(
            LifecyclePhase::PostCreate,
            marker_path.join("postCreate"),
            "prebuild mode",
        ));
        assert_eq!(summary.skipped_phases().len(), 1);
        assert!(summary.all_complete());

        // Add failed phase
        summary.add_phase(LifecyclePhaseState::new_failed(
            LifecyclePhase::PostStart,
            marker_path.join("postStart"),
            "command failed",
        ));
        assert_eq!(summary.failed_phases().len(), 1);
        assert!(summary.resume_required);
        assert!(!summary.all_complete());
    }

    #[test]
    fn test_lifecycle_commands_from_string() {
        let env = HashMap::new();
        let value = json!("echo 'hello world'");
        let commands = LifecycleCommands::from_json_value(&value, &env).unwrap();
        assert_eq!(commands.commands.len(), 1);
        assert_eq!(commands.commands[0].command, "echo 'hello world'");
    }

    #[test]
    fn test_lifecycle_commands_from_array() {
        let env = HashMap::new();
        let value = json!(["echo 'hello'", "echo 'world'"]);
        let commands = LifecycleCommands::from_json_value(&value, &env).unwrap();
        assert_eq!(commands.commands.len(), 2);
        assert_eq!(commands.commands[0].command, "echo 'hello'");
        assert_eq!(commands.commands[1].command, "echo 'world'");
    }

    #[test]
    fn test_lifecycle_commands_invalid_format() {
        let env = HashMap::new();
        let value = json!(42);
        let result = LifecycleCommands::from_json_value(&value, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_context_creation() {
        let ctx = ExecutionContext::new()
            .with_env("TEST_VAR".to_string(), "test_value".to_string())
            .with_working_directory("/tmp".into());

        assert_eq!(
            ctx.environment.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
        assert_eq!(ctx.working_directory, Some("/tmp".into()));
    }

    #[test]
    fn test_lifecycle_result_creation() {
        let mut result = LifecycleResult::new();
        assert!(result.success);
        assert!(result.exit_codes.is_empty());

        result.add_command_result(
            0,
            "output".to_string(),
            "".to_string(),
            std::time::Duration::from_millis(100),
        );
        assert!(result.success);
        assert_eq!(result.exit_codes, vec![0]);
        assert_eq!(result.stdout, "output");
        assert_eq!(result.durations.len(), 1);

        result.add_command_result(
            1,
            "".to_string(),
            "error".to_string(),
            std::time::Duration::from_millis(200),
        );
        assert!(!result.success);
        assert_eq!(result.exit_codes, vec![0, 1]);
        assert_eq!(result.stderr, "error");
        assert_eq!(result.durations.len(), 2);
    }

    // =========================================================================
    // LifecycleOrchestrator Tests
    // =========================================================================

    #[test]
    fn test_orchestrator_fresh_mode_executes_all_phases() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        let orchestrator = LifecycleOrchestrator::new(ctx);

        let phases_to_execute = orchestrator.phases_to_execute();
        assert_eq!(phases_to_execute.len(), 6);
        assert_eq!(phases_to_execute[0], LifecyclePhase::OnCreate);
        assert_eq!(phases_to_execute[1], LifecyclePhase::UpdateContent);
        assert_eq!(phases_to_execute[2], LifecyclePhase::PostCreate);
        assert_eq!(phases_to_execute[3], LifecyclePhase::Dotfiles);
        assert_eq!(phases_to_execute[4], LifecyclePhase::PostStart);
        assert_eq!(phases_to_execute[5], LifecyclePhase::PostAttach);
    }

    #[test]
    fn test_orchestrator_fresh_mode_phases_in_spec_order() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        let orchestrator = LifecycleOrchestrator::new(ctx);

        let decisions = orchestrator.phases_with_decisions();
        assert_eq!(decisions.len(), 6);

        // Verify order matches spec order exactly
        let expected_order = LifecyclePhase::spec_order();
        for (i, (phase, decision)) in decisions.iter().enumerate() {
            assert_eq!(
                *phase, expected_order[i],
                "Phase at index {} should be {:?}",
                i, expected_order[i]
            );
            assert_eq!(
                *decision,
                PhaseDecision::Execute,
                "Phase {:?} should execute in fresh mode",
                phase
            );
        }
    }

    #[test]
    fn test_orchestrator_prebuild_mode_skips_post_phases() {
        let ctx = InvocationContext::new_prebuild(PathBuf::from("/workspace"));
        let orchestrator = LifecycleOrchestrator::new(ctx);

        let decisions = orchestrator.phases_with_decisions();

        // onCreate and updateContent should execute
        assert_eq!(
            decisions[0],
            (LifecyclePhase::OnCreate, PhaseDecision::Execute)
        );
        assert_eq!(
            decisions[1],
            (LifecyclePhase::UpdateContent, PhaseDecision::Execute)
        );

        // postCreate, dotfiles, postStart, postAttach should be skipped
        assert_eq!(
            decisions[2],
            (
                LifecyclePhase::PostCreate,
                PhaseDecision::Skip("prebuild mode")
            )
        );
        assert_eq!(
            decisions[3],
            (
                LifecyclePhase::Dotfiles,
                PhaseDecision::Skip("prebuild mode")
            )
        );
        assert_eq!(
            decisions[4],
            (
                LifecyclePhase::PostStart,
                PhaseDecision::Skip("prebuild mode")
            )
        );
        assert_eq!(
            decisions[5],
            (
                LifecyclePhase::PostAttach,
                PhaseDecision::Skip("prebuild mode")
            )
        );
    }

    #[test]
    fn test_orchestrator_skip_post_create_mode() {
        let ctx = InvocationContext::new_skip_post_create(PathBuf::from("/workspace"));
        let orchestrator = LifecycleOrchestrator::new(ctx);

        let decisions = orchestrator.phases_with_decisions();

        // onCreate and updateContent should execute
        assert_eq!(
            decisions[0],
            (LifecyclePhase::OnCreate, PhaseDecision::Execute)
        );
        assert_eq!(
            decisions[1],
            (LifecyclePhase::UpdateContent, PhaseDecision::Execute)
        );

        // Everything else should be skipped
        assert_eq!(
            decisions[2],
            (
                LifecyclePhase::PostCreate,
                PhaseDecision::Skip("--skip-post-create flag")
            )
        );
        assert_eq!(
            decisions[3],
            (
                LifecyclePhase::Dotfiles,
                PhaseDecision::Skip("--skip-post-create flag")
            )
        );
        assert_eq!(
            decisions[4],
            (
                LifecyclePhase::PostStart,
                PhaseDecision::Skip("--skip-post-create flag")
            )
        );
        assert_eq!(
            decisions[5],
            (
                LifecyclePhase::PostAttach,
                PhaseDecision::Skip("--skip-post-create flag")
            )
        );
    }

    #[test]
    fn test_orchestrator_resume_mode_skips_non_runtime_phases() {
        let marker_path = PathBuf::from("/workspace/.devcontainer-state");
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                marker_path.join("onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path.join("updateContent.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path.join("postCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                marker_path.join("dotfiles.json"),
            ),
        ];

        let ctx = InvocationContext::new_resume(PathBuf::from("/workspace"), prior_markers);
        let orchestrator = LifecycleOrchestrator::new(ctx);

        let decisions = orchestrator.phases_with_decisions();

        // Non-runtime phases should be skipped due to prior markers
        assert_eq!(
            decisions[0],
            (
                LifecyclePhase::OnCreate,
                PhaseDecision::Skip("prior completion marker")
            )
        );
        assert_eq!(
            decisions[1],
            (
                LifecyclePhase::UpdateContent,
                PhaseDecision::Skip("prior completion marker")
            )
        );
        assert_eq!(
            decisions[2],
            (
                LifecyclePhase::PostCreate,
                PhaseDecision::Skip("prior completion marker")
            )
        );
        assert_eq!(
            decisions[3],
            (
                LifecyclePhase::Dotfiles,
                PhaseDecision::Skip("prior completion marker")
            )
        );

        // Runtime hooks should execute
        assert_eq!(
            decisions[4],
            (LifecyclePhase::PostStart, PhaseDecision::Execute)
        );
        assert_eq!(
            decisions[5],
            (LifecyclePhase::PostAttach, PhaseDecision::Execute)
        );
    }

    #[test]
    fn test_orchestrator_single_run_guard() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        // Initially, onCreate should execute
        assert_eq!(
            orchestrator.decide_phase(LifecyclePhase::OnCreate),
            PhaseDecision::Execute
        );
        assert!(!orchestrator.is_phase_executed(LifecyclePhase::OnCreate));

        // Mark it as executed
        orchestrator.mark_phase_executed(LifecyclePhase::OnCreate);

        // Now it should be skipped
        assert_eq!(
            orchestrator.decide_phase(LifecyclePhase::OnCreate),
            PhaseDecision::Skip("already executed this run")
        );
        assert!(orchestrator.is_phase_executed(LifecyclePhase::OnCreate));

        // Other phases should still be executable
        assert_eq!(
            orchestrator.decide_phase(LifecyclePhase::UpdateContent),
            PhaseDecision::Execute
        );
    }

    #[test]
    fn test_orchestrator_execute_in_order_success() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();

        let summary = orchestrator
            .execute_in_order(|phase| {
                executed_phases.push(phase);
                Ok::<(), String>(())
            })
            .unwrap();

        // All 6 phases should have been executed in order
        assert_eq!(executed_phases.len(), 6);
        assert_eq!(executed_phases[0], LifecyclePhase::OnCreate);
        assert_eq!(executed_phases[1], LifecyclePhase::UpdateContent);
        assert_eq!(executed_phases[2], LifecyclePhase::PostCreate);
        assert_eq!(executed_phases[3], LifecyclePhase::Dotfiles);
        assert_eq!(executed_phases[4], LifecyclePhase::PostStart);
        assert_eq!(executed_phases[5], LifecyclePhase::PostAttach);

        // Summary should reflect all phases as executed
        assert_eq!(summary.phases.len(), 6);
        assert!(summary.all_complete());
        assert!(!summary.resume_required);

        for phase_state in &summary.phases {
            assert_eq!(
                phase_state.status,
                PhaseStatus::Executed,
                "Phase {:?} should be executed",
                phase_state.phase
            );
        }
    }

    #[test]
    fn test_orchestrator_execute_in_order_with_skip() {
        let ctx = InvocationContext::new_prebuild(PathBuf::from("/workspace"));
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();

        let summary = orchestrator
            .execute_in_order(|phase| {
                executed_phases.push(phase);
                Ok::<(), String>(())
            })
            .unwrap();

        // Only onCreate and updateContent should have been executed
        assert_eq!(executed_phases.len(), 2);
        assert_eq!(executed_phases[0], LifecyclePhase::OnCreate);
        assert_eq!(executed_phases[1], LifecyclePhase::UpdateContent);

        // Summary should have all 6 phases (2 executed, 4 skipped)
        assert_eq!(summary.phases.len(), 6);
        assert!(summary.all_complete()); // Skipped phases are still "complete"
        assert!(!summary.resume_required);

        // Check executed phases
        assert_eq!(summary.phases[0].status, PhaseStatus::Executed);
        assert_eq!(summary.phases[1].status, PhaseStatus::Executed);

        // Check skipped phases
        for phase_state in &summary.phases[2..] {
            assert_eq!(
                phase_state.status,
                PhaseStatus::Skipped,
                "Phase {:?} should be skipped",
                phase_state.phase
            );
            assert!(phase_state.reason.is_some());
        }
    }

    #[test]
    fn test_orchestrator_execute_in_order_stops_on_error() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();

        let result = orchestrator.execute_in_order(|phase| {
            executed_phases.push(phase);
            if phase == LifecyclePhase::PostCreate {
                Err("postCreate failed".to_string())
            } else {
                Ok(())
            }
        });

        // Should have executed up to and including postCreate (where it failed)
        assert_eq!(executed_phases.len(), 3);
        assert_eq!(executed_phases[0], LifecyclePhase::OnCreate);
        assert_eq!(executed_phases[1], LifecyclePhase::UpdateContent);
        assert_eq!(executed_phases[2], LifecyclePhase::PostCreate);

        // Should return the error
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "postCreate failed");
    }

    #[test]
    fn test_orchestrator_phases_to_execute_respects_mode() {
        // Fresh mode
        let fresh_ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        let fresh_orchestrator = LifecycleOrchestrator::new(fresh_ctx);
        assert_eq!(fresh_orchestrator.phases_to_execute().len(), 6);

        // Prebuild mode
        let prebuild_ctx = InvocationContext::new_prebuild(PathBuf::from("/workspace"));
        let prebuild_orchestrator = LifecycleOrchestrator::new(prebuild_ctx);
        let prebuild_phases = prebuild_orchestrator.phases_to_execute();
        assert_eq!(prebuild_phases.len(), 2);
        assert_eq!(prebuild_phases[0], LifecyclePhase::OnCreate);
        assert_eq!(prebuild_phases[1], LifecyclePhase::UpdateContent);

        // Skip-post-create mode
        let skip_ctx = InvocationContext::new_skip_post_create(PathBuf::from("/workspace"));
        let skip_orchestrator = LifecycleOrchestrator::new(skip_ctx);
        let skip_phases = skip_orchestrator.phases_to_execute();
        assert_eq!(skip_phases.len(), 2);
        assert_eq!(skip_phases[0], LifecyclePhase::OnCreate);
        assert_eq!(skip_phases[1], LifecyclePhase::UpdateContent);
    }

    #[test]
    fn test_orchestrator_resume_with_incomplete_markers() {
        // Only onCreate completed - should resume from updateContent
        let marker_path = PathBuf::from("/workspace/.devcontainer-state");
        let prior_markers = vec![LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path.join("onCreate.json"),
        )];

        let ctx = InvocationContext::new_resume(PathBuf::from("/workspace"), prior_markers);
        let orchestrator = LifecycleOrchestrator::new(ctx);

        let decisions = orchestrator.phases_with_decisions();

        // onCreate should be skipped (has marker)
        assert_eq!(
            decisions[0],
            (
                LifecyclePhase::OnCreate,
                PhaseDecision::Skip("prior completion marker")
            )
        );

        // updateContent should execute (no marker)
        assert_eq!(
            decisions[1],
            (LifecyclePhase::UpdateContent, PhaseDecision::Execute)
        );

        // postCreate should execute (no marker)
        assert_eq!(
            decisions[2],
            (LifecyclePhase::PostCreate, PhaseDecision::Execute)
        );

        // dotfiles should execute (no marker)
        assert_eq!(
            decisions[3],
            (LifecyclePhase::Dotfiles, PhaseDecision::Execute)
        );

        // Runtime hooks always execute in resume mode
        assert_eq!(
            decisions[4],
            (LifecyclePhase::PostStart, PhaseDecision::Execute)
        );
        assert_eq!(
            decisions[5],
            (LifecyclePhase::PostAttach, PhaseDecision::Execute)
        );
    }

    #[test]
    fn test_orchestrator_context_accessor() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/test/workspace"));
        let orchestrator = LifecycleOrchestrator::new(ctx);

        assert_eq!(orchestrator.context().mode, InvocationMode::Fresh);
        assert_eq!(
            orchestrator.context().workspace_root,
            PathBuf::from("/test/workspace")
        );
    }

    #[tokio::test]
    async fn test_orchestrator_execute_in_order_async() {
        let ctx = InvocationContext::new_fresh(PathBuf::from("/workspace"));
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();

        let summary = orchestrator
            .execute_in_order_async(|phase| {
                executed_phases.push(phase);
                async move { Ok::<(), String>(()) }
            })
            .await
            .unwrap();

        // All 6 phases should have been executed in order
        assert_eq!(executed_phases.len(), 6);
        assert_eq!(summary.phases.len(), 6);
        assert!(summary.all_complete());
    }

    // =========================================================================
    // execute_with_markers Tests
    // =========================================================================

    #[test]
    fn test_orchestrator_execute_with_markers_writes_markers() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().to_path_buf();

        let ctx = InvocationContext::new_fresh(workspace.clone());
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();

        let summary = orchestrator
            .execute_with_markers(
                |phase| {
                    executed_phases.push(phase);
                    Ok::<(), String>(())
                },
                false,
            )
            .unwrap();

        // All 6 phases should have been executed in order
        assert_eq!(executed_phases.len(), 6);
        assert_eq!(summary.phases.len(), 6);
        assert!(summary.all_complete());

        // Verify markers were written to disk
        use crate::state::read_all_markers;
        let markers = read_all_markers(&workspace, false).unwrap();
        assert_eq!(markers.len(), 6);

        // Verify markers are in spec order
        let spec_order = LifecyclePhase::spec_order();
        for (i, marker) in markers.iter().enumerate() {
            assert_eq!(marker.phase, spec_order[i]);
            assert_eq!(marker.status, PhaseStatus::Executed);
        }
    }

    #[test]
    fn test_orchestrator_execute_with_markers_prebuild_isolation() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().to_path_buf();

        let ctx = InvocationContext::new_prebuild(workspace.clone());
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let summary = orchestrator
            .execute_with_markers(|_phase| Ok::<(), String>(()), true)
            .unwrap();

        // 2 executed (onCreate, updateContent), 4 skipped (postCreate, dotfiles, postStart, postAttach)
        assert_eq!(summary.phases.len(), 6);
        assert!(summary.all_complete());

        // Verify markers were written to prebuild directory
        use crate::state::read_all_markers;
        let prebuild_markers = read_all_markers(&workspace, true).unwrap();
        assert_eq!(prebuild_markers.len(), 6); // All phases get markers (executed or skipped)

        // Verify normal directory is empty
        let normal_markers = read_all_markers(&workspace, false).unwrap();
        assert!(normal_markers.is_empty());

        // Verify only onCreate and updateContent are executed
        let executed: Vec<_> = prebuild_markers
            .iter()
            .filter(|m| m.status == PhaseStatus::Executed)
            .collect();
        assert_eq!(executed.len(), 2);
        assert_eq!(executed[0].phase, LifecyclePhase::OnCreate);
        assert_eq!(executed[1].phase, LifecyclePhase::UpdateContent);

        // Verify the rest are skipped
        let skipped: Vec<_> = prebuild_markers
            .iter()
            .filter(|m| m.status == PhaseStatus::Skipped)
            .collect();
        assert_eq!(skipped.len(), 4);
    }

    #[test]
    fn test_orchestrator_execute_with_markers_stops_on_error() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().to_path_buf();

        let ctx = InvocationContext::new_fresh(workspace.clone());
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();

        let result = orchestrator.execute_with_markers(
            |phase| {
                executed_phases.push(phase);
                if phase == LifecyclePhase::PostCreate {
                    Err("postCreate failed".to_string())
                } else {
                    Ok(())
                }
            },
            false,
        );

        // Should have executed up to and including postCreate (where it failed)
        assert_eq!(executed_phases.len(), 3);
        assert!(result.is_err());

        // Verify only successfully executed phases have markers
        use crate::state::read_all_markers;
        let markers = read_all_markers(&workspace, false).unwrap();
        assert_eq!(markers.len(), 2); // Only onCreate and updateContent should have markers

        // Verify phases
        assert_eq!(markers[0].phase, LifecyclePhase::OnCreate);
        assert_eq!(markers[0].status, PhaseStatus::Executed);
        assert_eq!(markers[1].phase, LifecyclePhase::UpdateContent);
        assert_eq!(markers[1].status, PhaseStatus::Executed);
    }

    #[tokio::test]
    async fn test_orchestrator_execute_with_markers_async() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().to_path_buf();

        let ctx = InvocationContext::new_fresh(workspace.clone());
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let summary = orchestrator
            .execute_with_markers_async(|_phase| async move { Ok::<(), String>(()) }, false)
            .await
            .unwrap();

        // All 6 phases should have been executed in order
        assert_eq!(summary.phases.len(), 6);
        assert!(summary.all_complete());

        // Verify markers were written to disk
        use crate::state::read_all_markers;
        let markers = read_all_markers(&workspace, false).unwrap();
        assert_eq!(markers.len(), 6);
    }

    #[test]
    fn test_orchestrator_execute_with_markers_dotfiles_in_order() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let workspace = temp_dir.path().to_path_buf();

        let ctx = InvocationContext::new_fresh(workspace.clone());
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut execution_order = Vec::new();

        let _ = orchestrator
            .execute_with_markers(
                |phase| {
                    execution_order.push(phase);
                    Ok::<(), String>(())
                },
                false,
            )
            .unwrap();

        // Verify dotfiles is executed between postCreate and postStart
        let post_create_idx = execution_order
            .iter()
            .position(|p| *p == LifecyclePhase::PostCreate)
            .unwrap();
        let dotfiles_idx = execution_order
            .iter()
            .position(|p| *p == LifecyclePhase::Dotfiles)
            .unwrap();
        let post_start_idx = execution_order
            .iter()
            .position(|p| *p == LifecyclePhase::PostStart)
            .unwrap();

        assert!(
            post_create_idx < dotfiles_idx,
            "postCreate should come before dotfiles"
        );
        assert!(
            dotfiles_idx < post_start_idx,
            "dotfiles should come before postStart"
        );

        // Verify markers reflect this order
        use crate::state::read_all_markers;
        let markers = read_all_markers(&workspace, false).unwrap();

        let marker_order: Vec<_> = markers.iter().map(|m| m.phase).collect();
        assert_eq!(
            marker_order,
            vec![
                LifecyclePhase::OnCreate,
                LifecyclePhase::UpdateContent,
                LifecyclePhase::PostCreate,
                LifecyclePhase::Dotfiles,
                LifecyclePhase::PostStart,
                LifecyclePhase::PostAttach,
            ]
        );
    }

    // =========================================================================
    // Resume Decision Logic Tests (T014 - SC-002 and FR-004)
    // =========================================================================

    #[test]
    fn test_determine_mode_from_markers_empty() {
        // No markers -> Fresh mode
        let markers: Vec<LifecyclePhaseState> = vec![];
        let mode = InvocationContext::determine_mode_from_markers(&markers);
        assert_eq!(mode, InvocationMode::Fresh);
    }

    #[test]
    fn test_determine_mode_from_markers_partial() {
        // Only onCreate complete -> Fresh mode (partial resume)
        let markers = vec![LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            PathBuf::from("/markers/onCreate.json"),
        )];
        let mode = InvocationContext::determine_mode_from_markers(&markers);
        assert_eq!(mode, InvocationMode::Fresh);
    }

    #[test]
    fn test_determine_mode_from_markers_all_non_runtime_complete() {
        // All non-runtime phases complete -> Resume mode
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
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                PathBuf::from("/markers/dotfiles.json"),
            ),
        ];
        let mode = InvocationContext::determine_mode_from_markers(&markers);
        assert_eq!(mode, InvocationMode::Resume);
    }

    #[test]
    fn test_determine_mode_from_markers_with_runtime_hooks_complete() {
        // All phases complete (including runtime hooks) -> Resume mode
        let markers: Vec<LifecyclePhaseState> = LifecyclePhase::spec_order()
            .iter()
            .map(|phase| {
                LifecyclePhaseState::new_executed(
                    *phase,
                    PathBuf::from(format!("/markers/{}.json", phase.as_str())),
                )
            })
            .collect();
        let mode = InvocationContext::determine_mode_from_markers(&markers);
        assert_eq!(mode, InvocationMode::Resume);
    }

    #[test]
    fn test_determine_mode_from_markers_missing_middle_phase() {
        // Missing updateContent -> Fresh mode (partial resume)
        let markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                PathBuf::from("/markers/onCreate.json"),
            ),
            // updateContent missing
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                PathBuf::from("/markers/postCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                PathBuf::from("/markers/dotfiles.json"),
            ),
        ];
        let mode = InvocationContext::determine_mode_from_markers(&markers);
        assert_eq!(mode, InvocationMode::Fresh);
    }

    #[test]
    fn test_from_markers_with_flags_prebuild_takes_precedence() {
        // Prebuild flag should override marker-based mode detection
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
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                PathBuf::from("/markers/dotfiles.json"),
            ),
        ];
        let flags = InvocationFlags {
            prebuild: true,
            skip_post_create: false,
        };
        let ctx =
            InvocationContext::from_markers_with_flags(PathBuf::from("/workspace"), markers, flags);
        assert_eq!(ctx.mode, InvocationMode::Prebuild);
    }

    #[test]
    fn test_from_markers_with_flags_skip_post_create_takes_precedence() {
        // skip_post_create flag should override marker-based mode detection (but not prebuild)
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
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                PathBuf::from("/markers/dotfiles.json"),
            ),
        ];
        let flags = InvocationFlags {
            prebuild: false,
            skip_post_create: true,
        };
        let ctx =
            InvocationContext::from_markers_with_flags(PathBuf::from("/workspace"), markers, flags);
        assert_eq!(ctx.mode, InvocationMode::SkipPostCreate);
    }

    #[test]
    fn test_from_markers_with_flags_no_flags_uses_markers() {
        // Without special flags, mode should be determined by markers
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
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                PathBuf::from("/markers/dotfiles.json"),
            ),
        ];
        let flags = InvocationFlags::default();
        let ctx =
            InvocationContext::from_markers_with_flags(PathBuf::from("/workspace"), markers, flags);
        assert_eq!(ctx.mode, InvocationMode::Resume);
    }

    #[test]
    fn test_fresh_mode_with_markers_skips_completed_phases_fr004() {
        // FR-004: Fresh mode with prior markers should skip completed non-runtime phases
        let marker_path = PathBuf::from("/workspace/.devcontainer-state");
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                marker_path.join("onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path.join("updateContent.json"),
            ),
            // postCreate not complete - should be executed
        ];

        let ctx = InvocationContext {
            mode: InvocationMode::Fresh,
            flags: InvocationFlags::default(),
            workspace_root: PathBuf::from("/workspace"),
            prior_markers,
        };

        // Completed non-runtime phases should be skipped
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::OnCreate),
            Some("prior completion marker")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::UpdateContent),
            Some("prior completion marker")
        );

        // Incomplete non-runtime phases should execute
        assert!(ctx.should_skip_phase(LifecyclePhase::PostCreate).is_none());
        assert!(ctx.should_skip_phase(LifecyclePhase::Dotfiles).is_none());

        // Runtime hooks should always execute (even in Fresh mode with markers)
        assert!(ctx.should_skip_phase(LifecyclePhase::PostStart).is_none());
        assert!(ctx.should_skip_phase(LifecyclePhase::PostAttach).is_none());
    }

    #[test]
    fn test_resume_mode_only_runs_runtime_hooks_sc002() {
        // SC-002: Resume mode should skip all non-runtime phases and only run postStart/postAttach
        let marker_path = PathBuf::from("/workspace/.devcontainer-state");
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                marker_path.join("onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path.join("updateContent.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path.join("postCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                marker_path.join("dotfiles.json"),
            ),
        ];

        let ctx = InvocationContext::new_resume(PathBuf::from("/workspace"), prior_markers);

        // All non-runtime phases should be skipped
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::OnCreate),
            Some("prior completion marker")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::UpdateContent),
            Some("prior completion marker")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::PostCreate),
            Some("prior completion marker")
        );
        assert_eq!(
            ctx.should_skip_phase(LifecyclePhase::Dotfiles),
            Some("prior completion marker")
        );

        // Runtime hooks should execute
        assert!(ctx.should_skip_phase(LifecyclePhase::PostStart).is_none());
        assert!(ctx.should_skip_phase(LifecyclePhase::PostAttach).is_none());
    }

    #[test]
    fn test_orchestrator_resume_full_completion_only_runs_runtime_hooks() {
        // Full integration test: orchestrator with all non-runtime phases complete
        let marker_path = PathBuf::from("/workspace/.devcontainer-state");
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                marker_path.join("onCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path.join("updateContent.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path.join("postCreate.json"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::Dotfiles,
                marker_path.join("dotfiles.json"),
            ),
        ];

        let flags = InvocationFlags::default();
        let ctx = InvocationContext::from_markers_with_flags(
            PathBuf::from("/workspace"),
            prior_markers,
            flags,
        );
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();
        let _ = orchestrator.execute_in_order(|phase| {
            executed_phases.push(phase);
            Ok::<(), String>(())
        });

        // Only runtime hooks should have been executed
        assert_eq!(executed_phases.len(), 2);
        assert_eq!(executed_phases[0], LifecyclePhase::PostStart);
        assert_eq!(executed_phases[1], LifecyclePhase::PostAttach);
    }

    #[test]
    fn test_orchestrator_partial_resume_runs_from_incomplete_phase() {
        // FR-004 integration test: orchestrator with partial completion resumes from incomplete phase
        let marker_path = PathBuf::from("/workspace/.devcontainer-state");
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::OnCreate,
                marker_path.join("onCreate.json"),
            ),
            // updateContent, postCreate, dotfiles not complete
        ];

        let flags = InvocationFlags::default();
        let ctx = InvocationContext::from_markers_with_flags(
            PathBuf::from("/workspace"),
            prior_markers,
            flags,
        );
        let mut orchestrator = LifecycleOrchestrator::new(ctx);

        let mut executed_phases = Vec::new();
        let _ = orchestrator.execute_in_order(|phase| {
            executed_phases.push(phase);
            Ok::<(), String>(())
        });

        // Should skip onCreate, execute everything else
        assert_eq!(executed_phases.len(), 5);
        assert_eq!(executed_phases[0], LifecyclePhase::UpdateContent);
        assert_eq!(executed_phases[1], LifecyclePhase::PostCreate);
        assert_eq!(executed_phases[2], LifecyclePhase::Dotfiles);
        assert_eq!(executed_phases[3], LifecyclePhase::PostStart);
        assert_eq!(executed_phases[4], LifecyclePhase::PostAttach);
    }
}
