//! Command implementations
//!
//! This module contains implementations for all CLI subcommands.

#[cfg(feature = "full")]
pub mod build;
#[cfg(feature = "full")]
pub mod config;
pub mod down;
pub mod exec;
#[cfg(feature = "full")]
pub mod features;
#[cfg(feature = "full")]
pub mod features_monolith;
#[cfg(feature = "full")]
pub mod features_publish_output;
#[cfg(feature = "full")]
pub mod outdated;
pub mod read_configuration;
#[cfg(feature = "full")]
pub mod run_user_commands;
pub mod shared;
#[cfg(feature = "full")]
pub mod templates;
pub mod up;

/// Re-export the UpResult type to preserve the stdout JSON contract for the up command.
pub use up::UpResult;
