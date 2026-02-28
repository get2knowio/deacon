//! Shared test helpers for core integration tests.

use deacon_core::container_lifecycle::{
    AggregatedLifecycleCommand, LifecycleCommandList, LifecycleCommandSource, LifecycleCommandValue,
};

/// Helper to create a LifecycleCommandList from shell command strings
pub fn make_shell_command_list(cmds: &[&str]) -> LifecycleCommandList {
    LifecycleCommandList {
        commands: cmds
            .iter()
            .map(|cmd| AggregatedLifecycleCommand {
                command: LifecycleCommandValue::Shell(cmd.to_string()),
                source: LifecycleCommandSource::Config,
            })
            .collect(),
    }
}
