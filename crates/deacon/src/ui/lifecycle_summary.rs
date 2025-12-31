//! Lifecycle phase summary rendering for the up command.
//!
//! This module provides UI rendering for lifecycle phase summaries showing which
//! phases executed vs skipped, maintaining the spec-defined ordering:
//! onCreate -> updateContent -> postCreate -> dotfiles -> postStart -> postAttach
//!
//! Per FR-007: The command MUST present a summary indicating which phases executed
//! or were skipped (including dotfiles) so users can verify lifecycle behavior.
//!
//! Output contracts:
//! - JSON mode: Structured results to stdout, logs to stderr
//! - Text mode: Human-readable results to stdout, logs to stderr

// TODO: T030/T031 - These types will be used once lifecycle summary rendering is integrated
// into the up command flow. For now, allow dead_code to enable incremental development.
#![allow(dead_code)]

use console::style;
use deacon_core::lifecycle::{LifecyclePhase, LifecyclePhaseState, OutputMode, PhaseStatus};
use serde::{Deserialize, Serialize};

/// Summary of lifecycle phase execution for output rendering.
///
/// This structure represents the final state of all lifecycle phases after
/// execution, formatted for either JSON or text output. Phases are always
/// presented in spec-defined order.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleSummary {
    /// The invocation mode (fresh, resume, prebuild, skip_post_create)
    pub mode: String,

    /// Ordered list of phase execution results in spec order
    pub phases: Vec<PhaseExecutionResult>,

    /// Summary information
    pub summary: SummaryInfo,
}

/// Result of a single phase execution for display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhaseExecutionResult {
    /// Phase name (e.g., "onCreate", "updateContent", "dotfiles")
    pub phase: String,

    /// Execution status: "executed", "skipped", or "failed"
    pub status: String,

    /// Optional reason for skip/failure (e.g., "prebuild mode", "--skip-post-create flag")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Whether a completion marker was persisted for this phase
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_persisted: Option<bool>,

    /// Whether this phase was resumed (executed after being incomplete in a prior run)
    ///
    /// This is true when:
    /// - The mode is "resume"
    /// - The phase was executed (not skipped)
    /// - The phase would normally have been skipped if markers were complete
    #[serde(default, skip_serializing_if = "is_false")]
    pub resumed: bool,
}

/// Helper function for serde skip_serializing_if
fn is_false(b: &bool) -> bool {
    !*b
}

/// Summary information for the lifecycle run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SummaryInfo {
    /// Whether resume is required due to failure/interruption
    pub resume_required: bool,

    /// Number of phases that were resumed from an earlier incomplete run
    ///
    /// This indicates phases that were executed after being incomplete in a prior run
    /// (e.g., when markers were missing or corrupted).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub resumed_count: usize,

    /// Human-readable summary message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Helper function for serde skip_serializing_if
fn is_zero(n: &usize) -> bool {
    *n == 0
}

impl LifecycleSummary {
    /// Create a new lifecycle summary from phase states.
    ///
    /// # Arguments
    ///
    /// * `mode` - The invocation mode string (e.g., "fresh", "resume", "prebuild")
    /// * `phases` - List of phase states from the orchestrator (may be in any order)
    /// * `resume_required` - Whether resume is needed due to failure
    ///
    /// The phases will be reordered to match the spec-defined lifecycle order.
    pub fn from_phase_states(
        mode: &str,
        phases: &[LifecyclePhaseState],
        resume_required: bool,
    ) -> Self {
        Self::from_phase_states_with_context(mode, phases, resume_required, &[])
    }

    /// Create a new lifecycle summary from phase states with prior marker context.
    ///
    /// This variant allows tracking which phases were "resumed" - executed after
    /// being incomplete in a prior run due to missing or corrupted markers.
    ///
    /// # Arguments
    ///
    /// * `mode` - The invocation mode string (e.g., "fresh", "resume", "prebuild")
    /// * `phases` - List of phase states from the orchestrator (may be in any order)
    /// * `resume_required` - Whether resume is needed due to failure
    /// * `prior_markers` - List of phase states from prior markers (for resume tracking)
    ///
    /// The phases will be reordered to match the spec-defined lifecycle order.
    pub fn from_phase_states_with_context(
        mode: &str,
        phases: &[LifecyclePhaseState],
        resume_required: bool,
        prior_markers: &[LifecyclePhaseState],
    ) -> Self {
        // Build phase results in spec-defined order
        let mut phase_results = Vec::new();
        let is_resume_mode = mode == "resume";

        for spec_phase in LifecyclePhase::spec_order() {
            // Find this phase in the input states
            let phase_state = phases.iter().find(|p| p.phase == *spec_phase);

            // Check if this phase had a prior marker (for resume detection)
            let had_prior_marker = prior_markers
                .iter()
                .any(|m| m.phase == *spec_phase && m.status == PhaseStatus::Executed);

            let result = if let Some(state) = phase_state {
                // Determine if this phase was "resumed" - executed when in resume mode
                // but the marker was missing or corrupted (not in prior_markers)
                let is_resumed = is_resume_mode
                    && state.status == PhaseStatus::Executed
                    && !had_prior_marker
                    && !spec_phase.is_runtime_hook();

                PhaseExecutionResult {
                    phase: spec_phase.as_str().to_string(),
                    status: state.status.as_str().to_string(),
                    reason: state.reason.clone(),
                    marker_persisted: Some(state.status == PhaseStatus::Executed),
                    resumed: is_resumed,
                }
            } else {
                // Phase not in results - mark as pending/not run
                PhaseExecutionResult {
                    phase: spec_phase.as_str().to_string(),
                    status: "pending".to_string(),
                    reason: None,
                    marker_persisted: None,
                    resumed: false,
                }
            };

            phase_results.push(result);
        }

        // Generate counts for summary message
        let executed_count = phase_results
            .iter()
            .filter(|p| p.status == "executed")
            .count();
        let skipped_count = phase_results
            .iter()
            .filter(|p| p.status == "skipped")
            .count();
        let failed_count = phase_results
            .iter()
            .filter(|p| p.status == "failed")
            .count();
        let resumed_count = phase_results.iter().filter(|p| p.resumed).count();

        // Generate human-readable summary message
        // Per SC-003/SC-004: Include mode context for limited execution modes
        let message = if failed_count > 0 {
            Some(format!(
                "Lifecycle incomplete: {} executed, {} skipped, {} failed",
                executed_count, skipped_count, failed_count
            ))
        } else if resume_required {
            Some(format!(
                "Lifecycle interrupted: {} executed, {} skipped. Resume required.",
                executed_count, skipped_count
            ))
        } else if resumed_count > 0 {
            Some(format!(
                "Lifecycle resumed: {} executed ({} resumed from earlier), {} skipped",
                executed_count, resumed_count, skipped_count
            ))
        } else {
            // Mode-specific messages for limited execution modes
            match mode {
                "prebuild" => Some(format!(
                    "Prebuild complete: {} executed, {} skipped (post* hooks and dotfiles skipped by design)",
                    executed_count, skipped_count
                )),
                "skip_post_create" => Some(format!(
                    "Limited lifecycle complete: {} executed, {} skipped (--skip-post-create flag active)",
                    executed_count, skipped_count
                )),
                _ => Some(format!(
                    "Lifecycle complete: {} executed, {} skipped",
                    executed_count, skipped_count
                )),
            }
        };

        Self {
            mode: mode.to_string(),
            phases: phase_results,
            summary: SummaryInfo {
                resume_required,
                resumed_count,
                message,
            },
        }
    }

    /// Render the summary as JSON to stdout.
    ///
    /// Per the output contract, JSON mode writes exactly one JSON document to
    /// stdout. All logs and diagnostics go to stderr via tracing.
    pub fn render_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Render the summary as human-readable text.
    ///
    /// Per the output contract, text mode writes human-readable results to stdout.
    /// Uses color styling when the terminal supports it.
    ///
    /// Output format:
    /// ```text
    /// Lifecycle Summary (mode: fresh)
    ///   [checkmark] onCreate: executed
    ///   [checkmark] updateContent: executed
    ///   [checkmark] postCreate: executed
    ///   [checkmark] dotfiles: executed
    ///   [checkmark] postStart: executed
    ///   [checkmark] postAttach: executed
    /// ```
    ///
    /// For prebuild and skip_post_create modes, the header is styled differently
    /// to indicate limited execution scope per SC-003 and SC-004.
    pub fn render_text(&self) -> String {
        let mut output = String::new();

        // Header with mode - style differently for limited execution modes
        // Per SC-003/SC-004: prebuild and skip_post_create skip phases by design
        let header = format!("Lifecycle Summary (mode: {})", self.mode);
        let styled_header = match self.mode.as_str() {
            "prebuild" | "skip_post_create" => style(header).yellow().bold(),
            _ => style(header).bold(),
        };
        output.push_str(&format!("{}\n", styled_header));

        // Phase status lines in spec order
        for phase_result in &self.phases {
            let (icon, styled_status) = match phase_result.status.as_str() {
                "executed" if phase_result.resumed => (
                    // Resumed phases get a special indicator
                    style("[>>]").cyan().to_string(),
                    style("executed (resumed)").cyan().to_string(),
                ),
                "executed" => (
                    style("[OK]").green().to_string(),
                    style("executed").green().to_string(),
                ),
                "skipped" => (
                    style("[--]").yellow().to_string(),
                    style("skipped").yellow().to_string(),
                ),
                "failed" => (
                    style("[X]").red().to_string(),
                    style("FAILED").red().bold().to_string(),
                ),
                "pending" => (
                    style("[..]").dim().to_string(),
                    style("pending").dim().to_string(),
                ),
                _ => (
                    style("[?]").dim().to_string(),
                    style(&phase_result.status).dim().to_string(),
                ),
            };

            let reason_suffix = if let Some(ref reason) = phase_result.reason {
                format!(" ({})", style(reason).italic())
            } else {
                String::new()
            };

            output.push_str(&format!(
                "  {} {}: {}{}\n",
                icon, phase_result.phase, styled_status, reason_suffix
            ));
        }

        // Summary line
        if let Some(ref message) = self.summary.message {
            let styled_message = if self.summary.resume_required {
                style(message).yellow().to_string()
            } else {
                style(message).green().to_string()
            };
            output.push_str(&format!("\n{}\n", styled_message));
        }

        output
    }
}

/// Render a lifecycle summary to the appropriate output stream.
///
/// This function dispatches to either JSON or text rendering based on the
/// output mode, maintaining stdout/stderr separation per the output contract.
///
/// # Arguments
///
/// * `summary` - The lifecycle summary to render
/// * `output_mode` - Whether to use JSON or text output
///
/// # Returns
///
/// The formatted string. The caller is responsible for printing to stdout.
/// In JSON mode, the result should be the only output to stdout.
/// In text mode, the result is human-readable status information.
pub fn render_lifecycle_summary(summary: &LifecycleSummary, output_mode: OutputMode) -> String {
    match output_mode {
        OutputMode::Json => summary.render_json(),
        OutputMode::Text => summary.render_text(),
    }
}

/// Helper to create a lifecycle summary from a RunSummary.
///
/// This bridges the gap between the core RunSummary type and the UI-focused
/// LifecycleSummary type, handling the conversion of phase states and mode.
pub fn lifecycle_summary_from_run_summary(
    run_summary: &deacon_core::lifecycle::RunSummary,
    mode: &str,
) -> LifecycleSummary {
    LifecycleSummary::from_phase_states(mode, &run_summary.phases, run_summary.resume_required)
}

/// Helper to create a lifecycle summary from a RunSummary with prior marker context.
///
/// This variant includes prior marker information to properly track which phases
/// were "resumed" (executed after being incomplete in a prior run due to missing
/// or corrupted markers).
///
/// # Arguments
///
/// * `run_summary` - The run summary from the lifecycle orchestrator
/// * `mode` - The invocation mode string
/// * `prior_markers` - Prior marker states loaded from disk before this run
pub fn lifecycle_summary_from_run_summary_with_context(
    run_summary: &deacon_core::lifecycle::RunSummary,
    mode: &str,
    prior_markers: &[LifecyclePhaseState],
) -> LifecycleSummary {
    LifecycleSummary::from_phase_states_with_context(
        mode,
        &run_summary.phases,
        run_summary.resume_required,
        prior_markers,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use deacon_core::lifecycle::LifecyclePhaseState;
    use std::path::PathBuf;

    fn marker_path(phase: &str) -> PathBuf {
        PathBuf::from(format!("/workspace/.devcontainer-state/{}.json", phase))
    }

    #[test]
    fn test_lifecycle_summary_fresh_all_executed() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
            ),
            LifecyclePhaseState::new_executed(LifecyclePhase::Dotfiles, marker_path("dotfiles")),
            LifecyclePhaseState::new_executed(LifecyclePhase::PostStart, marker_path("postStart")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("fresh", &phases, false);

        assert_eq!(summary.mode, "fresh");
        assert_eq!(summary.phases.len(), 6);
        assert!(!summary.summary.resume_required);

        // Verify spec ordering
        assert_eq!(summary.phases[0].phase, "onCreate");
        assert_eq!(summary.phases[1].phase, "updateContent");
        assert_eq!(summary.phases[2].phase, "postCreate");
        assert_eq!(summary.phases[3].phase, "dotfiles");
        assert_eq!(summary.phases[4].phase, "postStart");
        assert_eq!(summary.phases[5].phase, "postAttach");

        // All should be executed
        for phase_result in &summary.phases {
            assert_eq!(phase_result.status, "executed");
            assert!(phase_result.reason.is_none());
            assert_eq!(phase_result.marker_persisted, Some(true));
        }
    }

    #[test]
    fn test_lifecycle_summary_prebuild_skipped_phases() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
                "prebuild mode",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::Dotfiles,
                marker_path("dotfiles"),
                "prebuild mode",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostStart,
                marker_path("postStart"),
                "prebuild mode",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
                "prebuild mode",
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("prebuild", &phases, false);

        assert_eq!(summary.mode, "prebuild");
        assert!(!summary.summary.resume_required);

        // Check executed phases
        assert_eq!(summary.phases[0].status, "executed");
        assert_eq!(summary.phases[1].status, "executed");

        // Check skipped phases with reasons
        for i in 2..6 {
            assert_eq!(summary.phases[i].status, "skipped");
            assert_eq!(summary.phases[i].reason, Some("prebuild mode".to_string()));
        }
    }

    #[test]
    fn test_lifecycle_summary_skip_post_create() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
                "--skip-post-create flag",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::Dotfiles,
                marker_path("dotfiles"),
                "--skip-post-create flag",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostStart,
                marker_path("postStart"),
                "--skip-post-create flag",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
                "--skip-post-create flag",
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("skip_post_create", &phases, false);

        assert_eq!(summary.mode, "skip_post_create");

        // Check skipped phases have correct reason
        assert_eq!(
            summary.phases[2].reason,
            Some("--skip-post-create flag".to_string())
        );
    }

    #[test]
    fn test_lifecycle_summary_resume_with_prior_markers() {
        let phases = vec![
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::OnCreate,
                marker_path("onCreate"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::Dotfiles,
                marker_path("dotfiles"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_executed(LifecyclePhase::PostStart, marker_path("postStart")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("resume", &phases, false);

        assert_eq!(summary.mode, "resume");

        // Non-runtime phases should be skipped
        for i in 0..4 {
            assert_eq!(summary.phases[i].status, "skipped");
            assert_eq!(
                summary.phases[i].reason,
                Some("prior completion marker".to_string())
            );
        }

        // Runtime hooks should execute
        assert_eq!(summary.phases[4].status, "executed");
        assert_eq!(summary.phases[5].status, "executed");
    }

    #[test]
    fn test_lifecycle_summary_with_failure() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
            LifecyclePhaseState::new_failed(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
                "command failed with exit code 1",
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("fresh", &phases, true);

        assert!(summary.summary.resume_required);
        assert_eq!(summary.phases[2].status, "failed");
        assert_eq!(
            summary.phases[2].reason,
            Some("command failed with exit code 1".to_string())
        );

        // Later phases should show as pending (not executed)
        assert_eq!(summary.phases[3].status, "pending");
        assert_eq!(summary.phases[4].status, "pending");
        assert_eq!(summary.phases[5].status, "pending");

        // Check summary message mentions failure
        assert!(summary.summary.message.is_some());
        assert!(summary.summary.message.as_ref().unwrap().contains("failed"));
    }

    #[test]
    fn test_lifecycle_summary_json_output() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
                "test skip",
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("fresh", &phases, false);
        let json_output = summary.render_json();

        // Verify it's valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();
        assert_eq!(parsed["mode"], "fresh");
        assert!(parsed["phases"].is_array());
        assert!(parsed["summary"]["resumeRequired"].is_boolean());
    }

    #[test]
    fn test_lifecycle_summary_text_output() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
                "test skip",
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("fresh", &phases, false);
        let text_output = summary.render_text();

        // Verify basic structure
        assert!(text_output.contains("Lifecycle Summary"));
        assert!(text_output.contains("mode: fresh"));
        assert!(text_output.contains("onCreate"));
        assert!(text_output.contains("postCreate"));
        assert!(text_output.contains("test skip"));
    }

    #[test]
    fn test_render_lifecycle_summary_json_mode() {
        let summary = LifecycleSummary::from_phase_states("fresh", &[], false);
        let output = render_lifecycle_summary(&summary, OutputMode::Json);

        // Should be valid JSON
        assert!(serde_json::from_str::<serde_json::Value>(&output).is_ok());
    }

    #[test]
    fn test_render_lifecycle_summary_text_mode() {
        let summary = LifecycleSummary::from_phase_states("fresh", &[], false);
        let output = render_lifecycle_summary(&summary, OutputMode::Text);

        // Should contain header
        assert!(output.contains("Lifecycle Summary"));
    }

    #[test]
    fn test_phases_out_of_order_are_reordered() {
        // Provide phases out of spec order
        let phases = vec![
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
            ),
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(LifecyclePhase::Dotfiles, marker_path("dotfiles")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
            ),
        ];

        let summary = LifecycleSummary::from_phase_states("fresh", &phases, false);

        // Output should be in spec order regardless of input order
        assert_eq!(summary.phases[0].phase, "onCreate");
        assert_eq!(summary.phases[1].phase, "updateContent"); // Not provided, shows as pending
        assert_eq!(summary.phases[2].phase, "postCreate");
        assert_eq!(summary.phases[3].phase, "dotfiles");
        assert_eq!(summary.phases[4].phase, "postStart"); // Not provided, shows as pending
        assert_eq!(summary.phases[5].phase, "postAttach");

        // Verify statuses
        assert_eq!(summary.phases[0].status, "executed"); // onCreate
        assert_eq!(summary.phases[1].status, "pending"); // updateContent - not provided
        assert_eq!(summary.phases[2].status, "executed"); // postCreate
        assert_eq!(summary.phases[3].status, "executed"); // dotfiles
        assert_eq!(summary.phases[4].status, "pending"); // postStart - not provided
        assert_eq!(summary.phases[5].status, "executed"); // postAttach
    }

    #[test]
    fn test_lifecycle_summary_from_run_summary() {
        use deacon_core::lifecycle::RunSummary;

        let mut run_summary = RunSummary::new(OutputMode::Text);
        run_summary.add_phase(LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path("onCreate"),
        ));
        run_summary.add_phase(LifecyclePhaseState::new_executed(
            LifecyclePhase::UpdateContent,
            marker_path("updateContent"),
        ));

        let lifecycle_summary = lifecycle_summary_from_run_summary(&run_summary, "fresh");

        assert_eq!(lifecycle_summary.mode, "fresh");
        assert!(!lifecycle_summary.summary.resume_required);
        assert_eq!(lifecycle_summary.phases[0].phase, "onCreate");
        assert_eq!(lifecycle_summary.phases[0].status, "executed");
    }

    #[test]
    fn test_summary_message_content() {
        // Test various summary message scenarios

        // All executed
        let all_executed = LifecycleSummary::from_phase_states(
            "fresh",
            &LifecyclePhase::spec_order()
                .iter()
                .map(|p| LifecyclePhaseState::new_executed(*p, marker_path(p.as_str())))
                .collect::<Vec<_>>(),
            false,
        );
        assert!(all_executed
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("complete"));
        assert!(all_executed
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("6 executed"));
        assert!(all_executed
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("0 skipped"));

        // Mixed executed and skipped
        let mixed = LifecycleSummary::from_phase_states(
            "prebuild",
            &[
                LifecyclePhaseState::new_executed(
                    LifecyclePhase::OnCreate,
                    marker_path("onCreate"),
                ),
                LifecyclePhaseState::new_executed(
                    LifecyclePhase::UpdateContent,
                    marker_path("updateContent"),
                ),
                LifecyclePhaseState::new_skipped(
                    LifecyclePhase::PostCreate,
                    marker_path("postCreate"),
                    "prebuild mode",
                ),
                LifecyclePhaseState::new_skipped(
                    LifecyclePhase::Dotfiles,
                    marker_path("dotfiles"),
                    "prebuild mode",
                ),
                LifecyclePhaseState::new_skipped(
                    LifecyclePhase::PostStart,
                    marker_path("postStart"),
                    "prebuild mode",
                ),
                LifecyclePhaseState::new_skipped(
                    LifecyclePhase::PostAttach,
                    marker_path("postAttach"),
                    "prebuild mode",
                ),
            ],
            false,
        );
        assert!(mixed
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("2 executed"));
        assert!(mixed
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("4 skipped"));

        // With failure
        let with_failure = LifecycleSummary::from_phase_states(
            "fresh",
            &[
                LifecyclePhaseState::new_executed(
                    LifecyclePhase::OnCreate,
                    marker_path("onCreate"),
                ),
                LifecyclePhaseState::new_failed(
                    LifecyclePhase::UpdateContent,
                    marker_path("updateContent"),
                    "error",
                ),
            ],
            true,
        );
        assert!(with_failure
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("incomplete"));
        assert!(with_failure
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("1 failed"));
    }

    // =========================================================================
    // Resumed Phase Tests (T015 - Corrupted/Missing Marker Handling)
    // =========================================================================

    #[test]
    fn test_lifecycle_summary_with_context_no_prior_markers() {
        // In resume mode with no prior markers, all executed non-runtime phases are "resumed"
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
            ),
            LifecyclePhaseState::new_executed(LifecyclePhase::Dotfiles, marker_path("dotfiles")),
            LifecyclePhaseState::new_executed(LifecyclePhase::PostStart, marker_path("postStart")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
            ),
        ];

        let summary =
            LifecycleSummary::from_phase_states_with_context("resume", &phases, false, &[]);

        assert_eq!(summary.mode, "resume");

        // Non-runtime phases should be marked as resumed (no prior markers)
        assert!(
            summary.phases[0].resumed,
            "onCreate should be resumed (no prior marker)"
        );
        assert!(
            summary.phases[1].resumed,
            "updateContent should be resumed (no prior marker)"
        );
        assert!(
            summary.phases[2].resumed,
            "postCreate should be resumed (no prior marker)"
        );
        assert!(
            summary.phases[3].resumed,
            "dotfiles should be resumed (no prior marker)"
        );

        // Runtime hooks should NOT be marked as resumed
        assert!(
            !summary.phases[4].resumed,
            "postStart is a runtime hook, should not be marked resumed"
        );
        assert!(
            !summary.phases[5].resumed,
            "postAttach is a runtime hook, should not be marked resumed"
        );

        // Summary should reflect resumed phases
        assert_eq!(summary.summary.resumed_count, 4);
        assert!(summary
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("resumed"));
    }

    #[test]
    fn test_lifecycle_summary_with_context_all_prior_markers() {
        // In resume mode with all prior markers, no phases are "resumed"
        let prior_markers = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
            ),
            LifecyclePhaseState::new_executed(LifecyclePhase::Dotfiles, marker_path("dotfiles")),
        ];

        // Current execution skips non-runtime phases (has markers) and runs runtime hooks
        let phases = vec![
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::OnCreate,
                marker_path("onCreate"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::Dotfiles,
                marker_path("dotfiles"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_executed(LifecyclePhase::PostStart, marker_path("postStart")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
            ),
        ];

        let summary = LifecycleSummary::from_phase_states_with_context(
            "resume",
            &phases,
            false,
            &prior_markers,
        );

        // No phases should be marked as resumed (all prior markers present)
        for phase_result in &summary.phases {
            assert!(
                !phase_result.resumed,
                "Phase {} should not be resumed when prior marker exists",
                phase_result.phase
            );
        }

        assert_eq!(summary.summary.resumed_count, 0);
        assert!(summary
            .summary
            .message
            .as_ref()
            .unwrap()
            .contains("complete"));
    }

    #[test]
    fn test_lifecycle_summary_with_context_partial_prior_markers() {
        // Some markers are missing (simulating corruption)
        let prior_markers = vec![LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path("onCreate"),
        )];
        // updateContent marker is missing/corrupted

        let phases = vec![
            LifecyclePhaseState::new_skipped(
                LifecyclePhase::OnCreate,
                marker_path("onCreate"),
                "prior completion marker",
            ),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ), // Resumed from corrupted marker
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostCreate,
                marker_path("postCreate"),
            ), // Resumed (no prior marker)
            LifecyclePhaseState::new_executed(LifecyclePhase::Dotfiles, marker_path("dotfiles")), // Resumed (no prior marker)
            LifecyclePhaseState::new_executed(LifecyclePhase::PostStart, marker_path("postStart")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::PostAttach,
                marker_path("postAttach"),
            ),
        ];

        let summary = LifecycleSummary::from_phase_states_with_context(
            "resume",
            &phases,
            false,
            &prior_markers,
        );

        // onCreate has prior marker, should not be resumed
        assert!(
            !summary.phases[0].resumed,
            "onCreate has prior marker, should not be resumed"
        );

        // updateContent, postCreate, dotfiles have no prior markers, should be resumed
        assert!(
            summary.phases[1].resumed,
            "updateContent should be resumed (no prior marker)"
        );
        assert!(
            summary.phases[2].resumed,
            "postCreate should be resumed (no prior marker)"
        );
        assert!(
            summary.phases[3].resumed,
            "dotfiles should be resumed (no prior marker)"
        );

        // Runtime hooks should not be marked resumed
        assert!(
            !summary.phases[4].resumed,
            "postStart is runtime hook, should not be resumed"
        );
        assert!(
            !summary.phases[5].resumed,
            "postAttach is runtime hook, should not be resumed"
        );

        assert_eq!(summary.summary.resumed_count, 3);
    }

    #[test]
    fn test_lifecycle_summary_fresh_mode_no_resumed() {
        // In fresh mode, nothing should be marked as resumed
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
        ];

        let summary =
            LifecycleSummary::from_phase_states_with_context("fresh", &phases, false, &[]);

        for phase_result in &summary.phases {
            assert!(
                !phase_result.resumed,
                "Phase {} should not be resumed in fresh mode",
                phase_result.phase
            );
        }

        assert_eq!(summary.summary.resumed_count, 0);
    }

    #[test]
    fn test_render_text_with_resumed_phases() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(
                LifecyclePhase::UpdateContent,
                marker_path("updateContent"),
            ),
            LifecyclePhaseState::new_executed(LifecyclePhase::PostStart, marker_path("postStart")),
        ];

        let summary =
            LifecycleSummary::from_phase_states_with_context("resume", &phases, false, &[]);

        let text = summary.render_text();

        // Resumed phases should be shown with "(resumed)" indicator
        assert!(
            text.contains("resumed"),
            "Text output should contain resumed indicator"
        );
    }

    #[test]
    fn test_render_json_with_resumed_phases() {
        let phases = vec![
            LifecyclePhaseState::new_executed(LifecyclePhase::OnCreate, marker_path("onCreate")),
            LifecyclePhaseState::new_executed(LifecyclePhase::PostStart, marker_path("postStart")),
        ];

        let summary =
            LifecycleSummary::from_phase_states_with_context("resume", &phases, false, &[]);

        let json_output = summary.render_json();
        let parsed: serde_json::Value = serde_json::from_str(&json_output).unwrap();

        // Check resumed field is present for non-runtime phases
        assert_eq!(
            parsed["phases"][0]["resumed"], true,
            "onCreate should have resumed=true in JSON"
        );

        // Runtime hooks should not have resumed=true (skipped in serialization when false)
        // Note: serde will skip serializing `resumed: false` due to skip_serializing_if
        assert!(
            parsed["phases"][4]["resumed"].is_null() || parsed["phases"][4]["resumed"] == false,
            "postStart should not have resumed=true"
        );

        // Check summary includes resumed_count
        assert_eq!(parsed["summary"]["resumedCount"], 1);
    }

    #[test]
    fn test_lifecycle_summary_from_run_summary_with_context() {
        use deacon_core::lifecycle::RunSummary;

        let mut run_summary = RunSummary::new(OutputMode::Text);
        run_summary.add_phase(LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path("onCreate"),
        ));
        run_summary.add_phase(LifecyclePhaseState::new_executed(
            LifecyclePhase::UpdateContent,
            marker_path("updateContent"),
        ));

        // No prior markers - simulating corrupted/missing markers
        let prior_markers = vec![];

        let summary =
            lifecycle_summary_from_run_summary_with_context(&run_summary, "resume", &prior_markers);

        assert_eq!(summary.mode, "resume");
        assert!(summary.phases[0].resumed, "onCreate should be resumed");
        assert!(summary.phases[1].resumed, "updateContent should be resumed");
        assert_eq!(summary.summary.resumed_count, 2);
    }
}
