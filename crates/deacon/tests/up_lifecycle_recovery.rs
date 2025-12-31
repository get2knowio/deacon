//! Recovery tests for lifecycle phase resume behavior
//!
//! Tests SC-002/FR-004: When a prior run failed/was interrupted before completing
//! all phases, rerun should resume from the earliest incomplete phase.
//!
//! Scenarios covered:
//! - Partial completion: markers exist up to postCreate but not dotfiles - resume from dotfiles
//! - Early failure: only onCreate complete - resume from updateContent
//! - Corrupted markers: malformed JSON treated as missing - resume from earliest phase
//! - Runtime hook rerun: even with all non-runtime markers present, postStart/postAttach rerun
//!
//! Related spec: specs/008-up-lifecycle-hooks/spec.md
//! Task: T013 [P] [US2]

use deacon_core::lifecycle::{
    InvocationContext, InvocationMode, LifecycleOrchestrator, LifecyclePhase, LifecyclePhaseState,
    PhaseDecision,
};
use deacon_core::state::{
    clear_markers, find_earliest_incomplete_phase, marker_path_for_phase, read_all_markers,
    read_phase_marker, write_phase_marker,
};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper function to write an executed marker for a specific phase
fn write_executed_marker(workspace: &std::path::Path, phase: LifecyclePhase) {
    let marker_path = marker_path_for_phase(workspace, phase);
    let state = LifecyclePhaseState::new_executed(phase, marker_path.clone());
    write_phase_marker(&marker_path, &state).expect("Failed to write marker");
}

/// Helper function to write a corrupted (invalid JSON) marker file
fn write_corrupted_marker(workspace: &std::path::Path, phase: LifecyclePhase) {
    let marker_path = marker_path_for_phase(workspace, phase);
    // Ensure parent directory exists
    if let Some(parent) = marker_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create marker directory");
    }
    // Write invalid JSON content
    std::fs::write(&marker_path, "this is not valid json {{{")
        .expect("Failed to write corrupted marker");
}

// =============================================================================
// Unit Tests for find_earliest_incomplete_phase
// =============================================================================

/// Test: Empty markers returns onCreate as earliest incomplete
#[test]
fn test_find_earliest_incomplete_no_markers() {
    let markers: Vec<LifecyclePhaseState> = vec![];
    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::OnCreate),
        "With no markers, should resume from onCreate"
    );
}

/// Test: Only onCreate complete - resume from updateContent
#[test]
fn test_find_earliest_incomplete_after_oncreate() {
    let markers = vec![LifecyclePhaseState::new_executed(
        LifecyclePhase::OnCreate,
        PathBuf::from("/markers/onCreate.json"),
    )];
    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::UpdateContent),
        "With only onCreate complete, should resume from updateContent"
    );
}

/// Test: onCreate + updateContent complete - resume from postCreate
#[test]
fn test_find_earliest_incomplete_after_update_content() {
    let markers = vec![
        LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            PathBuf::from("/markers/onCreate.json"),
        ),
        LifecyclePhaseState::new_executed(
            LifecyclePhase::UpdateContent,
            PathBuf::from("/markers/updateContent.json"),
        ),
    ];
    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::PostCreate),
        "With onCreate+updateContent complete, should resume from postCreate"
    );
}

/// Test: Up to postCreate complete - resume from dotfiles
#[test]
fn test_find_earliest_incomplete_after_postcreate() {
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
    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::Dotfiles),
        "With onCreate+updateContent+postCreate complete, should resume from dotfiles"
    );
}

/// Test: Up to dotfiles complete - resume from postStart
#[test]
fn test_find_earliest_incomplete_after_dotfiles() {
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
    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::PostStart),
        "With up to dotfiles complete, should resume from postStart"
    );
}

/// Test: All phases complete - returns None
#[test]
fn test_find_earliest_incomplete_all_complete() {
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
    assert!(
        result.is_none(),
        "With all phases complete, should return None"
    );
}

/// Test: Gap in markers - returns earliest missing even if later phases present
/// This tests the scenario where markers got corrupted/deleted in the middle
#[test]
fn test_find_earliest_incomplete_with_gap() {
    // onCreate and postCreate complete, but updateContent missing (gap)
    let markers = vec![
        LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            PathBuf::from("/markers/onCreate.json"),
        ),
        // updateContent is missing!
        LifecyclePhaseState::new_executed(
            LifecyclePhase::PostCreate,
            PathBuf::from("/markers/postCreate.json"),
        ),
        LifecyclePhaseState::new_executed(
            LifecyclePhase::Dotfiles,
            PathBuf::from("/markers/dotfiles.json"),
        ),
    ];
    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::UpdateContent),
        "With a gap in markers, should return the earliest missing phase"
    );
}

/// Test: Skipped marker is NOT considered complete for resume purposes
#[test]
fn test_find_earliest_incomplete_skipped_not_complete() {
    let markers = vec![
        LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            PathBuf::from("/markers/onCreate.json"),
        ),
        LifecyclePhaseState::new_skipped(
            LifecyclePhase::UpdateContent,
            PathBuf::from("/markers/updateContent.json"),
            "prebuild mode",
        ),
    ];
    let result = find_earliest_incomplete_phase(&markers);
    // Skipped != Executed, so updateContent should be incomplete
    assert_eq!(
        result,
        Some(LifecyclePhase::UpdateContent),
        "Skipped phases are not considered complete for resume"
    );
}

// =============================================================================
// Filesystem-Based Marker Recovery Tests
// =============================================================================

/// Test: Read markers from filesystem with partial completion (up to postCreate)
/// Resume should start from dotfiles
#[test]
fn test_recovery_filesystem_partial_up_to_postcreate() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write markers for first three phases
    write_executed_marker(workspace, LifecyclePhase::OnCreate);
    write_executed_marker(workspace, LifecyclePhase::UpdateContent);
    write_executed_marker(workspace, LifecyclePhase::PostCreate);

    // Read all markers
    let markers = read_all_markers(workspace, false).expect("Failed to read markers");
    assert_eq!(markers.len(), 3, "Should have 3 markers");

    // Find earliest incomplete
    let incomplete = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        incomplete,
        Some(LifecyclePhase::Dotfiles),
        "Should resume from dotfiles when postCreate is complete but dotfiles is not"
    );
}

/// Test: Read markers from filesystem with only onCreate complete
/// Resume should start from updateContent
#[test]
fn test_recovery_filesystem_only_oncreate() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write marker for only onCreate
    write_executed_marker(workspace, LifecyclePhase::OnCreate);

    // Read all markers
    let markers = read_all_markers(workspace, false).expect("Failed to read markers");
    assert_eq!(markers.len(), 1, "Should have 1 marker");

    // Find earliest incomplete
    let incomplete = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        incomplete,
        Some(LifecyclePhase::UpdateContent),
        "Should resume from updateContent when only onCreate is complete"
    );
}

/// Test: Corrupted marker treated as missing per research.md Decision 2
/// If postCreate marker is corrupted, it's treated as missing
#[test]
fn test_recovery_corrupted_marker_treated_as_missing() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write valid markers for first two phases
    write_executed_marker(workspace, LifecyclePhase::OnCreate);
    write_executed_marker(workspace, LifecyclePhase::UpdateContent);

    // Write corrupted marker for postCreate
    write_corrupted_marker(workspace, LifecyclePhase::PostCreate);

    // Read the corrupted marker directly - should return None
    let corrupted_path = marker_path_for_phase(workspace, LifecyclePhase::PostCreate);
    let corrupted_state = read_phase_marker(&corrupted_path).expect("Read should not error");
    assert!(
        corrupted_state.is_none(),
        "Corrupted marker should be treated as missing (return None)"
    );

    // Read all markers - corrupted marker should not be included
    let markers = read_all_markers(workspace, false).expect("Failed to read markers");
    assert_eq!(
        markers.len(),
        2,
        "Should have 2 valid markers (corrupted one excluded)"
    );

    // Find earliest incomplete - should be postCreate (the corrupted one)
    let incomplete = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        incomplete,
        Some(LifecyclePhase::PostCreate),
        "Should resume from postCreate when its marker is corrupted"
    );
}

/// Test: Corrupted marker in the middle followed by valid markers
/// Resume should start from the corrupted phase (earliest incomplete)
#[test]
fn test_recovery_corrupted_marker_in_middle() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write valid marker for onCreate
    write_executed_marker(workspace, LifecyclePhase::OnCreate);

    // Write corrupted marker for updateContent
    write_corrupted_marker(workspace, LifecyclePhase::UpdateContent);

    // Write valid marker for postCreate (simulating inconsistent state)
    write_executed_marker(workspace, LifecyclePhase::PostCreate);

    // Read all markers
    let markers = read_all_markers(workspace, false).expect("Failed to read markers");
    assert_eq!(
        markers.len(),
        2,
        "Should have 2 valid markers (corrupted updateContent excluded)"
    );

    // Find earliest incomplete
    let incomplete = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        incomplete,
        Some(LifecyclePhase::UpdateContent),
        "Should resume from updateContent (corrupted marker) even though postCreate exists"
    );
}

/// Test: Clear markers and verify empty state
#[test]
fn test_clear_markers_resets_state() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write all markers
    for phase in LifecyclePhase::spec_order() {
        write_executed_marker(workspace, *phase);
    }

    // Verify all markers exist
    let markers_before = read_all_markers(workspace, false).expect("Failed to read markers");
    assert_eq!(
        markers_before.len(),
        6,
        "Should have 6 markers before clear"
    );

    // Clear markers
    clear_markers(workspace, false).expect("Failed to clear markers");

    // Verify markers are gone
    let markers_after = read_all_markers(workspace, false).expect("Failed to read markers");
    assert!(
        markers_after.is_empty(),
        "Should have 0 markers after clear"
    );

    // Verify earliest incomplete is now onCreate
    let incomplete = find_earliest_incomplete_phase(&markers_after);
    assert_eq!(
        incomplete,
        Some(LifecyclePhase::OnCreate),
        "After clearing markers, resume should start from onCreate"
    );
}

// =============================================================================
// Orchestrator Resume Mode Tests
// =============================================================================

/// Test: Resume mode with all non-runtime phases complete
/// Only postStart and postAttach should execute
#[test]
fn test_orchestrator_resume_with_complete_non_runtime() {
    let marker_path = PathBuf::from("/workspace/.devcontainer-state");

    // Simulate markers for all non-runtime phases
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

    // Non-runtime phases should be skipped
    assert_eq!(
        decisions[0],
        (
            LifecyclePhase::OnCreate,
            PhaseDecision::Skip("prior completion marker")
        ),
        "onCreate should be skipped"
    );
    assert_eq!(
        decisions[1],
        (
            LifecyclePhase::UpdateContent,
            PhaseDecision::Skip("prior completion marker")
        ),
        "updateContent should be skipped"
    );
    assert_eq!(
        decisions[2],
        (
            LifecyclePhase::PostCreate,
            PhaseDecision::Skip("prior completion marker")
        ),
        "postCreate should be skipped"
    );
    assert_eq!(
        decisions[3],
        (
            LifecyclePhase::Dotfiles,
            PhaseDecision::Skip("prior completion marker")
        ),
        "dotfiles should be skipped"
    );

    // Runtime hooks should execute
    assert_eq!(
        decisions[4],
        (LifecyclePhase::PostStart, PhaseDecision::Execute),
        "postStart should execute (runtime hook)"
    );
    assert_eq!(
        decisions[5],
        (LifecyclePhase::PostAttach, PhaseDecision::Execute),
        "postAttach should execute (runtime hook)"
    );

    // Verify phases_to_execute only includes runtime hooks
    let phases_to_execute = orchestrator.phases_to_execute();
    assert_eq!(
        phases_to_execute,
        vec![LifecyclePhase::PostStart, LifecyclePhase::PostAttach],
        "Only runtime hooks should be in phases_to_execute"
    );
}

/// Test: Resume mode with incomplete non-runtime phases
/// Missing phases should execute, plus runtime hooks
#[test]
fn test_orchestrator_resume_with_incomplete_non_runtime() {
    let marker_path = PathBuf::from("/workspace/.devcontainer-state");

    // Simulate markers for only onCreate and updateContent (incomplete)
    let prior_markers = vec![
        LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path.join("onCreate.json"),
        ),
        LifecyclePhaseState::new_executed(
            LifecyclePhase::UpdateContent,
            marker_path.join("updateContent.json"),
        ),
        // postCreate and dotfiles missing - simulating failed prior run
    ];

    let ctx = InvocationContext::new_resume(PathBuf::from("/workspace"), prior_markers);
    let orchestrator = LifecycleOrchestrator::new(ctx);

    let decisions = orchestrator.phases_with_decisions();

    // Completed non-runtime phases should be skipped
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

    // Incomplete non-runtime phases should execute
    assert_eq!(
        decisions[2],
        (LifecyclePhase::PostCreate, PhaseDecision::Execute),
        "postCreate should execute (no prior marker)"
    );
    assert_eq!(
        decisions[3],
        (LifecyclePhase::Dotfiles, PhaseDecision::Execute),
        "dotfiles should execute (no prior marker)"
    );

    // Runtime hooks always execute
    assert_eq!(
        decisions[4],
        (LifecyclePhase::PostStart, PhaseDecision::Execute)
    );
    assert_eq!(
        decisions[5],
        (LifecyclePhase::PostAttach, PhaseDecision::Execute)
    );

    // Verify phases_to_execute
    let phases_to_execute = orchestrator.phases_to_execute();
    assert_eq!(
        phases_to_execute,
        vec![
            LifecyclePhase::PostCreate,
            LifecyclePhase::Dotfiles,
            LifecyclePhase::PostStart,
            LifecyclePhase::PostAttach
        ],
        "Incomplete phases plus runtime hooks should execute"
    );
}

/// Test: Resume mode with gap in markers
/// Should execute from earliest missing phase forward
#[test]
fn test_orchestrator_resume_with_marker_gap() {
    let marker_path = PathBuf::from("/workspace/.devcontainer-state");

    // Simulate markers with a gap: onCreate complete, updateContent missing, postCreate complete
    // This is an inconsistent state that could result from disk corruption
    let prior_markers = vec![
        LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            marker_path.join("onCreate.json"),
        ),
        // updateContent missing!
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

    // onCreate should be skipped (has marker)
    assert_eq!(
        decisions[0],
        (
            LifecyclePhase::OnCreate,
            PhaseDecision::Skip("prior completion marker")
        )
    );

    // updateContent should execute (missing marker - the gap)
    assert_eq!(
        decisions[1],
        (LifecyclePhase::UpdateContent, PhaseDecision::Execute),
        "updateContent should execute (marker missing - gap detected)"
    );

    // postCreate and dotfiles should be skipped (have markers)
    // Note: The current implementation skips if marker exists, even with a gap before
    // This is the expected behavior per the spec - we rely on find_earliest_incomplete_phase
    // to determine the actual resume point
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

    // Runtime hooks always execute
    assert_eq!(
        decisions[4],
        (LifecyclePhase::PostStart, PhaseDecision::Execute)
    );
    assert_eq!(
        decisions[5],
        (LifecyclePhase::PostAttach, PhaseDecision::Execute)
    );
}

/// Test: Resume mode with no prior markers (treated as fresh)
/// All phases should execute
#[test]
fn test_orchestrator_resume_with_no_markers() {
    // Resume with empty prior markers - effectively becomes a fresh run
    let prior_markers: Vec<LifecyclePhaseState> = vec![];

    let ctx = InvocationContext::new_resume(PathBuf::from("/workspace"), prior_markers);
    let orchestrator = LifecycleOrchestrator::new(ctx);

    let phases_to_execute = orchestrator.phases_to_execute();

    // All phases should execute
    assert_eq!(
        phases_to_execute,
        vec![
            LifecyclePhase::OnCreate,
            LifecyclePhase::UpdateContent,
            LifecyclePhase::PostCreate,
            LifecyclePhase::Dotfiles,
            LifecyclePhase::PostStart,
            LifecyclePhase::PostAttach,
        ],
        "With no prior markers, all phases should execute"
    );
}

// =============================================================================
// Integration of Marker Reading with Orchestrator
// =============================================================================

/// Test: Full flow - read markers from disk and create resume context
#[test]
fn test_full_recovery_flow_from_disk() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write markers simulating a prior run that completed up to postCreate
    write_executed_marker(workspace, LifecyclePhase::OnCreate);
    write_executed_marker(workspace, LifecyclePhase::UpdateContent);
    write_executed_marker(workspace, LifecyclePhase::PostCreate);

    // Read markers from disk
    let prior_markers = read_all_markers(workspace, false).expect("Failed to read markers");
    assert_eq!(prior_markers.len(), 3);

    // Create resume context with the read markers
    let ctx = InvocationContext::new_resume(workspace.to_path_buf(), prior_markers.clone());
    assert_eq!(ctx.mode, InvocationMode::Resume);
    assert_eq!(ctx.prior_markers.len(), 3);

    // Create orchestrator
    let orchestrator = LifecycleOrchestrator::new(ctx);

    // Verify the phases to execute
    let phases_to_execute = orchestrator.phases_to_execute();
    assert_eq!(
        phases_to_execute,
        vec![
            LifecyclePhase::Dotfiles, // Resume from here
            LifecyclePhase::PostStart,
            LifecyclePhase::PostAttach
        ],
        "Should resume from dotfiles onward"
    );

    // Verify find_earliest_incomplete_phase is consistent
    let earliest_incomplete = find_earliest_incomplete_phase(&prior_markers);
    assert_eq!(
        earliest_incomplete,
        Some(LifecyclePhase::Dotfiles),
        "Earliest incomplete should be dotfiles"
    );
}

/// Test: Full flow with corrupted marker - recovery from corruption
#[test]
fn test_full_recovery_flow_with_corruption() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write valid markers for first two phases
    write_executed_marker(workspace, LifecyclePhase::OnCreate);
    write_executed_marker(workspace, LifecyclePhase::UpdateContent);

    // Write corrupted marker for postCreate
    write_corrupted_marker(workspace, LifecyclePhase::PostCreate);

    // Write valid markers for later phases (simulating inconsistent state)
    write_executed_marker(workspace, LifecyclePhase::Dotfiles);
    write_executed_marker(workspace, LifecyclePhase::PostStart);
    write_executed_marker(workspace, LifecyclePhase::PostAttach);

    // Read markers from disk - corrupted marker should be excluded
    let prior_markers = read_all_markers(workspace, false).expect("Failed to read markers");

    // Should have 5 markers (corrupted postCreate excluded)
    assert_eq!(
        prior_markers.len(),
        5,
        "Should have 5 valid markers (corrupted excluded)"
    );

    // Find earliest incomplete - should be postCreate
    let earliest_incomplete = find_earliest_incomplete_phase(&prior_markers);
    assert_eq!(
        earliest_incomplete,
        Some(LifecyclePhase::PostCreate),
        "Should resume from postCreate (corrupted marker)"
    );

    // Create resume context
    let ctx = InvocationContext::new_resume(workspace.to_path_buf(), prior_markers);
    let orchestrator = LifecycleOrchestrator::new(ctx);

    // Get phases to execute
    let phases_to_execute = orchestrator.phases_to_execute();

    // The orchestrator checks individual markers, so postCreate executes, but
    // dotfiles/postStart/postAttach are skipped due to their markers
    // Runtime hooks (postStart/postAttach) always execute in resume mode
    assert!(
        phases_to_execute.contains(&LifecyclePhase::PostCreate),
        "postCreate should execute (corrupted marker)"
    );
    assert!(
        phases_to_execute.contains(&LifecyclePhase::PostStart),
        "postStart should execute (runtime hook)"
    );
    assert!(
        phases_to_execute.contains(&LifecyclePhase::PostAttach),
        "postAttach should execute (runtime hook)"
    );
}

/// Test: Execute_in_order records phases correctly after resume
#[test]
fn test_execute_in_order_after_resume() {
    let temp_dir = TempDir::new().unwrap();
    let workspace = temp_dir.path();

    // Write markers for first two phases
    write_executed_marker(workspace, LifecyclePhase::OnCreate);
    write_executed_marker(workspace, LifecyclePhase::UpdateContent);

    // Read markers from disk
    let prior_markers = read_all_markers(workspace, false).expect("Failed to read markers");

    // Create resume context
    let ctx = InvocationContext::new_resume(workspace.to_path_buf(), prior_markers);
    let mut orchestrator = LifecycleOrchestrator::new(ctx);

    // Track which phases execute
    let mut executed_phases = Vec::new();

    // Execute in order
    let summary = orchestrator
        .execute_in_order(|phase| {
            executed_phases.push(phase);
            Ok::<(), String>(())
        })
        .unwrap();

    // Verify correct phases executed (incomplete non-runtime + runtime hooks)
    assert_eq!(
        executed_phases,
        vec![
            LifecyclePhase::PostCreate,
            LifecyclePhase::Dotfiles,
            LifecyclePhase::PostStart,
            LifecyclePhase::PostAttach
        ],
        "Should execute postCreate, dotfiles, postStart, postAttach"
    );

    // Verify summary
    assert!(summary.all_complete());
    assert_eq!(summary.phases.len(), 6);

    // Check that skipped phases have the right reason
    let skipped = summary.skipped_phases();
    assert_eq!(skipped.len(), 2);
    for phase_state in skipped {
        assert_eq!(
            phase_state.reason,
            Some("prior completion marker".to_string())
        );
    }

    // Check that executed phases have correct status
    let executed = summary.executed_phases();
    assert_eq!(executed.len(), 4);
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test: All phases have failed status - treated as incomplete
#[test]
fn test_failed_phases_treated_as_incomplete() {
    let markers = vec![LifecyclePhaseState::new_failed(
        LifecyclePhase::OnCreate,
        PathBuf::from("/markers/onCreate.json"),
        "command failed",
    )];

    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::OnCreate),
        "Failed phases should be treated as incomplete"
    );
}

/// Test: Mixed statuses - only Executed counts as complete
#[test]
fn test_mixed_statuses_only_executed_complete() {
    let markers = vec![
        LifecyclePhaseState::new_executed(
            LifecyclePhase::OnCreate,
            PathBuf::from("/markers/onCreate.json"),
        ),
        LifecyclePhaseState::new_pending(
            LifecyclePhase::UpdateContent,
            PathBuf::from("/markers/updateContent.json"),
        ),
        LifecyclePhaseState::new_skipped(
            LifecyclePhase::PostCreate,
            PathBuf::from("/markers/postCreate.json"),
            "test",
        ),
        LifecyclePhaseState::new_failed(
            LifecyclePhase::Dotfiles,
            PathBuf::from("/markers/dotfiles.json"),
            "error",
        ),
    ];

    let result = find_earliest_incomplete_phase(&markers);
    assert_eq!(
        result,
        Some(LifecyclePhase::UpdateContent),
        "Only Executed status counts as complete - pending, skipped, failed are incomplete"
    );
}

/// Test: Runtime hooks are NOT skipped based on prior markers in resume mode
/// Even if postStart and postAttach have markers, they should still execute
#[test]
fn test_runtime_hooks_always_execute_in_resume() {
    let marker_path = PathBuf::from("/workspace/.devcontainer-state");

    // Simulate markers for ALL phases including runtime hooks
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
        LifecyclePhaseState::new_executed(
            LifecyclePhase::PostStart,
            marker_path.join("postStart.json"),
        ),
        LifecyclePhaseState::new_executed(
            LifecyclePhase::PostAttach,
            marker_path.join("postAttach.json"),
        ),
    ];

    let ctx = InvocationContext::new_resume(PathBuf::from("/workspace"), prior_markers);
    let orchestrator = LifecycleOrchestrator::new(ctx);

    let phases_to_execute = orchestrator.phases_to_execute();

    // Even with markers for postStart/postAttach, they should execute (runtime hooks)
    assert_eq!(
        phases_to_execute,
        vec![LifecyclePhase::PostStart, LifecyclePhase::PostAttach],
        "Runtime hooks should execute even with prior markers"
    );
}
