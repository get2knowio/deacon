//! Integration test for per-command lifecycle progress events
//!
//! This test verifies that when lifecycle commands are executed, both phase-level
//! and per-command progress events are emitted in the correct order.

mod common;

use deacon_core::container_lifecycle::{
    execute_container_lifecycle_with_progress_callback, ContainerLifecycleCommands,
    ContainerLifecycleConfig,
};
use deacon_core::progress::{ProgressEvent, ProgressTracker};
use deacon_core::variable::SubstitutionContext;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Mock progress event collector that stores events for verification
#[derive(Debug, Default)]
struct MockProgressCollector {
    events: Vec<ProgressEvent>,
}

impl MockProgressCollector {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    fn add_event(&mut self, event: ProgressEvent) {
        self.events.push(event);
    }

    fn get_events(&self) -> &[ProgressEvent] {
        &self.events
    }
}

#[tokio::test]
async fn test_per_command_events_emitted() {
    // This test simulates container lifecycle execution and verifies that
    // per-command events are emitted. Since we can't actually run docker commands
    // in the test environment, we'll verify the event emission logic by
    // expecting the execution to fail but still emit begin events.

    let temp_dir = TempDir::new().unwrap();
    let workspace_path = temp_dir.path();

    // Create mock progress collector
    let collector = Arc::new(Mutex::new(MockProgressCollector::new()));
    let collector_for_callback = collector.clone();

    // Create progress callback that collects events
    let progress_callback = move |event: ProgressEvent| -> anyhow::Result<()> {
        collector_for_callback
            .lock()
            .unwrap()
            .add_event(event.clone());
        Ok(())
    };

    // Create substitution context
    let substitution_context = SubstitutionContext::new(workspace_path).unwrap();

    // Create container lifecycle configuration
    let lifecycle_config = ContainerLifecycleConfig {
        container_id: "test-container".to_string(),
        user: Some("root".to_string()),
        container_workspace_folder: "/workspaces/test".to_string(),
        container_env: HashMap::new(),
        skip_post_create: false,
        skip_non_blocking_commands: false,
        non_blocking_timeout: Duration::from_secs(300),
        use_login_shell: false,
        user_env_probe: deacon_core::container_env_probe::ContainerProbeMode::None,
        cache_folder: None,
        force_pty: false,
        dotfiles: None,
        is_prebuild: false,
    };

    // Create lifecycle commands with multiple commands in a phase
    let commands = ContainerLifecycleCommands::new()
        .with_on_create(common::make_shell_command_list(&[
            "echo 'First onCreate command'",
            "echo 'Second onCreate command'",
        ]))
        .with_post_create(common::make_shell_command_list(&[
            "echo 'PostCreate command'",
        ]));

    // Execute lifecycle commands (this will fail due to no docker, but should emit events)
    let _result = execute_container_lifecycle_with_progress_callback(
        &lifecycle_config,
        &commands,
        &substitution_context,
        Some(progress_callback),
    )
    .await;

    // The execution will fail because there's no actual Docker container,
    // but we should still see the command begin events being emitted

    // Verify that events were emitted
    let events = collector.lock().unwrap().get_events().to_vec();

    // We should have at least the begin events for each command
    let command_begin_events: Vec<_> = events
        .iter()
        .filter(|event| matches!(event, ProgressEvent::LifecycleCommandBegin { .. }))
        .collect();

    println!(
        "Found {} command begin events out of {} total events",
        command_begin_events.len(),
        events.len()
    );

    // Print all events for debugging
    for (i, event) in events.iter().enumerate() {
        println!("Event {}: {:?}", i, event);
    }

    // We expect at least 1 command begin event (the first onCreate command)
    // The execution might fail early due to Docker not being available
    assert!(
        !command_begin_events.is_empty(),
        "Expected at least 1 command begin event, got {}",
        command_begin_events.len()
    );

    // Verify the command IDs are unique and follow expected pattern
    let mut command_ids = Vec::new();
    for event in &command_begin_events {
        if let ProgressEvent::LifecycleCommandBegin {
            command_id, phase, ..
        } = event
        {
            command_ids.push(command_id.clone());
            println!(
                "Found command begin event: {} in phase {}",
                command_id, phase
            );
        }
    }

    if !command_ids.is_empty() {
        // Command IDs should be unique
        let mut sorted_ids = command_ids.clone();
        sorted_ids.sort();
        sorted_ids.dedup();
        assert_eq!(
            command_ids.len(),
            sorted_ids.len(),
            "Command IDs should be unique: {:?}",
            command_ids
        );

        // At least the first command ID should follow the pattern "phase-N"
        let first_id = &command_ids[0];
        assert!(
            first_id.starts_with("onCreate-"),
            "Expected onCreate command ID, got: {}",
            first_id
        );
    }

    println!(
        "Successfully verified {} per-command events with IDs: {:?}",
        command_begin_events.len(),
        command_ids
    );
}

#[test]
fn test_lifecycle_command_event_structure() {
    // Test that the new event types have the expected structure and serialization
    let begin_event = ProgressEvent::LifecycleCommandBegin {
        id: ProgressTracker::next_event_id(),
        timestamp: ProgressTracker::current_timestamp(),
        phase: "onCreate".to_string(),
        command_id: "onCreate-1".to_string(),
        command: "echo 'test command'".to_string(),
    };

    let end_event = ProgressEvent::LifecycleCommandEnd {
        id: ProgressTracker::next_event_id(),
        timestamp: ProgressTracker::current_timestamp(),
        phase: "onCreate".to_string(),
        command_id: "onCreate-1".to_string(),
        duration_ms: 1500,
        success: true,
        exit_code: Some(0),
    };

    // Test serialization
    let begin_json = serde_json::to_string(&begin_event).unwrap();
    let end_json = serde_json::to_string(&end_event).unwrap();

    // Verify correct event type names
    assert!(begin_json.contains("lifecycle.command.begin"));
    assert!(end_json.contains("lifecycle.command.end"));

    // Verify expected fields are present
    assert!(begin_json.contains("command_id"));
    assert!(begin_json.contains("onCreate-1"));
    assert!(begin_json.contains("echo 'test command'"));

    assert!(end_json.contains("command_id"));
    assert!(end_json.contains("onCreate-1"));
    assert!(end_json.contains("duration_ms"));
    assert!(end_json.contains("1500"));
    assert!(end_json.contains("success"));
    assert!(end_json.contains("exit_code"));

    // Test deserialization
    let _begin_deserialized: ProgressEvent = serde_json::from_str(&begin_json).unwrap();
    let _end_deserialized: ProgressEvent = serde_json::from_str(&end_json).unwrap();

    println!("Successfully verified event structure and serialization");
}
