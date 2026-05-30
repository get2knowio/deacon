//! Command implementations
//!
//! This module contains implementations for all CLI subcommands.

pub mod build;
pub mod config;
pub mod down;
pub mod exec;
pub mod outdated;
pub mod read_configuration;
pub mod run_user_commands;
pub mod set_up;
pub mod shared;
pub mod templates;
pub mod up;
pub mod upgrade;

/// Re-export the UpResult type to preserve the stdout JSON contract for the up command.
pub use up::UpResult;
